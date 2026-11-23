# The remaining DBX classes, by system

**Sources.** Round SPG4 (2026-11-23): the census of DICE's partitions (`spec/census.json`, re-measured by the game
tier). Round SPG4 made the specification name every DBX instance class DICE's partitions hold (`spec/CENSUS.md`);
the classes below belong to systems that already have a file (effects, animation, vehicles, weapons, the AI,
networking, destruction, the level) but that file did not name them. Each rule says what the classes are for and
where DICE keeps them; the tables list the members DICE's instances carry, less `Name`, `MemoryReportGroup`,
`Components`, `Enabled`, `Enumeration` and `Transform`. A rule here is a starting point: a round that works on the
system moves the classes into its own file's rules with their semantics.

## 1. Effects

### DATA-FX-1: The emitter update processors

Status: **verified** · Variants: PC

After FX-PROC-1's spawn processors an emitter's chain holds its update processors, one class per property the
processor animates over the particle's life (each with FX-PROC-2's `ProcessorType`, `Enable`, `Pre` / `Post`
evaluators and `NextProcessor`): size (uniform, x, y, z), linear velocity, rotation, orientation, colour,
transparency, clip scale, and the alpha level's scale, minimum and maximum. `EmitterData` (1,194) spawns a child
emitter from a particle and `SpawnCompositeMeshPartsData` (309) the parts of a destroyed composite mesh. Each level's
`EmitterSystem` partition holds its `EmitterSystemAsset` (74: the Havok meshes particles collide with). An object
spawns effects through `SpawnableEffectComponentData` (239: effects linked to a health state, a spawn probability)
and `EffectComponentData` (68: an effect with start and stop triggers). The run-time side is ENG-REND-1's
`EmitterRender` module.

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| UpdateSizeData (Emitter) | 54,775 | 54,775 | `levels/<level>` | AcceptsEvaluator, DefaultValues, Deprecated, Edited, Enable, EvaluatorInput, NextProcessor, Pivot, Post, Pre, ProcessorType |
| UpdateSizeXData (Emitter) | 13,511 | 13,511 | `levels/<level>` | AcceptsEvaluator, DefaultValues, Deprecated, Edited, Enable, EvaluatorInput, NextProcessor, Post, Pre, ProcessorType, WaitingForUbbe |
| UpdateSizeYData (Emitter) | 10,266 | 10,266 | `levels/<level>` | AcceptsEvaluator, DefaultValues, Deprecated, Edited, Enable, EvaluatorInput, NextProcessor, Post, Pre, ProcessorType, WaitingForUbbe |
| UpdateSizeZData (Emitter) | 309 | 309 | `levels/<level>` | AcceptsEvaluator, DefaultValues, Deprecated, Edited, Enable, EvaluatorInput, NextProcessor, Post, Pre, ProcessorType, WaitingForUbbe |
| UpdateLinearVelocityData (Emitter) | 51,467 | 51,467 | `levels/<level>` | AcceptsEvaluator, DefaultValues, Deprecated, Edited, Enable, EvaluatorInput, NextProcessor, Post, Pre, ProcessorType, WaitingForUbbe |
| UpdateRotationData (Emitter) | 48,123 | 48,123 | `levels/<level>` | AcceptsEvaluator, DefaultValues, Deprecated, Edited, Enable, EvaluatorInput, NextProcessor, Post, Pre, ProcessorType, TextureInfo1 |
| UpdateOrientationData (Emitter) | 41,803 | 41,803 | `levels/<level>` | AcceptsEvaluator, DefaultValues, Deprecated, Edited, Enable, EvaluatorInput, NextProcessor, Post, Pre, ProcessorType, WaitingForUbbe |
| UpdateColorData (Emitter) | 46,982 | 46,982 | `levels/<level>` | AcceptsEvaluator, Color, DefaultValues, Deprecated, Edited, Enable, EvaluatorInput, NextProcessor, Post, Pre, ProcessorType |
| UpdateTransparencyData (Emitter) | 46,982 | 46,982 | `levels/<level>` | AcceptsEvaluator, CullThreshold, DefaultValues, Deprecated, Edited, Enable, EvaluatorInput, NextProcessor, Post, Pre, ProcessorType |
| UpdateClipScaleData (Emitter) | 42,618 | 42,618 | `levels/<level>` | AcceptsEvaluator, DefaultValues, Deprecated, Edited, Enable, EvaluatorInput, Lookup, NextProcessor, Post, Pre, ProcessorType |
| UpdateAlphaLevelScaleData (Emitter) | 33,748 | 33,748 | `levels/<level>` | AcceptsEvaluator, DefaultValues, Deprecated, Edited, Enable, EvaluatorInput, Exponent, NextProcessor, Post, Pre, ProcessorType |
| UpdateAlphaLevelMinData (Emitter) | 6,268 | 6,268 | `levels/<level>` | AcceptsEvaluator, DefaultValues, Deprecated, Edited, Enable, EvaluatorInput, MinLevel, NextProcessor, Post, Pre, ProcessorType |
| UpdateAlphaLevelMaxData (Emitter) | 13,984 | 13,984 | `levels/<level>` | AcceptsEvaluator, DefaultValues, Deprecated, Edited, Enable, EvaluatorInput, MaxLevel, NextProcessor, Post, Pre, ProcessorType |
| EmitterData (Emitter) | 1,194 | 1,194 | `levels/<level>` | AcceptsEvaluator, DefaultValues, Deprecated, Edited, Emitter, Enable, EvaluatorInput, Mesh, NextProcessor, Post, Pre, ProcessorType |
| SpawnCompositeMeshPartsData (Emitter) | 309 | 309 | `levels/<level>` | AcceptsEvaluator, DefaultValues, Deprecated, Edited, Enable, EvaluatorInput, NextProcessor, Post, Pre, ProcessorType, WaitingForUbbe |
| EmitterSystemAsset (Emitter) | 74 | 74 | `levels/<level>` | HavokMeshCount, HavokMeshes |
| SpawnableEffectComponentData (GameSharedResources) | 239 | 40 | `objects/vietnam`, `objects/buildings`, `objects/objects` | Effects, LinkToHealthState, SpawnProbabilityFactor |
| EffectComponentData (GameSharedResources) | 68 | 43 | `objects/vehicles`, `objects/buildings` | Effect, StartTrigger, StopTrigger |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json (DICE's partitions, round SPG4, 2026-11-23): the classes above]

## 2. Animation

### DATA-ANIM-1: Tree parameters, expressions and specific trees

Status: **verified** · Variants: PC

An animation tree's inputs are typed parameters - `FloatParameter` (6,833, most in the cutscenes' generated trees),
`BoolParameter`, `IntParameter`, `Vec3Parameter`, `Mat4Parameter` (each a data index and an initial value) - and
its conditions expressions over them (`BoolParameterExpressionData`, `IntParameterExpressionData`, `AndExpressionData`,
`OrExpressionData`); `DisablePropertyCurve_*` switch a property off over time. `StrafePreprocessorData` turns the
movement angle into the strafe blend. A `SpecificAnimTreeData` (310: weapons and vehicle seats) is a tree with its
replacement animations, result trees, blend times and priority, compiled to an `AnimTreeInfo` (ATREE-HDR-1).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| FloatParameter (Animation) | 6,833 | 1,690 | `animations/cutscenes`, `levels/<level>` | DataIndex, InitialValue |
| BoolParameter (Animation) | 546 | 374 | `characters/animtree`, `objects/weapons`, `levels/<level>` | DataIndex, InitialValue |
| IntParameter (Animation) | 70 | 48 | `characters/animtree`, `objects/weapons`, `animations/animals` | DataIndex, InitialValue |
| Vec3Parameter (Animation) | 4 | 4 | `characters/animtree`, `objects/weapons` | DataIndex, InitialValue |
| Mat4Parameter (Animation) | 29 | 29 | `objects/vehicles`, `objects/weapons` | DataIndex, InitialValue |
| BoolParameterExpressionData (Animation) | 26 | 3 | `characters/animtree`, `objects/weapons` | Parameter, Value |
| IntParameterExpressionData (Animation) | 1 | 1 | `characters/animtree` | Comparison, Parameter, Value |
| AndExpressionData (Animation) | 8 | 2 | `characters/animtree`, `objects/weapons` | SubExpressions |
| OrExpressionData (Animation) | 4 | 3 | `objects/weapons`, `characters/animtree` | SubExpressions |
| DisablePropertyCurve_ManualInput_OrAnimated (Animation) | 6 | 1 | `objects/weapons` | Curve |
| DisablePropertyCurve_ManualInput_EndsAbsoluteTime (Animation) | 4 | 1 | `objects/weapons` | DisableInterval |
| DisablePropertyCurve_ManualInput_NormalizedTime (Animation) | 3 | 1 | `objects/weapons` | DisableIntervals |
| StrafePreprocessorData (Animation) | 1 | 1 | `characters/animtree` | FrontParameter, LeftBackAngle, LeftFrontAngle, MovementAngleParameter, RightBackAngle, RightFrontAngle |
| SpecificAnimTreeData (Animation) | 310 | 310 | `objects/weapons`, `objects/vehicles`, `characters/animtree` | AnimTree, BlendInTime, BlendOutTime, Priority, ReplacmentAnimations, ResultAnimTrees, Skeleton |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

### DATA-ANIM-2: Influence maps, aim bones and the AI's animations

Status: **verified** · Variants: PC

Which bones an animation layer drives is an influence map (`InfluenceMapData`, 54, under
`Characters/InfluenceMaps` and `Animations/Settings/ExportMaps`: per-bone weights `InfluenceMapBoneData`, 8,559,
and their results `ResultInfluenceMapData`). An aim node binds aim directions to bones (`AimNode2AimBoneData`, 1,518,
mostly weapons' third-person trees: AIM-HDR-1's aim animations). `AI/Animations` lists the bots' special moves
(`AnimationData`, 31: start / end states, obstacle height and distance, fire; `AnimationMovementAsset`).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| InfluenceMapData (Animation) | 54 | 54 | `characters/influencemaps`, `animations/settings` | BoneWeightMap, Dynamic, InfluenceMapEffects, ResultInfluenceMaps, Skeleton |
| InfluenceMapBoneData (Animation) | 8,559 | 54 | `characters/influencemaps`, `animations/settings` | BoneName, Weight |
| ResultInfluenceMapData (Animation) | 54 | 54 | `characters/influencemaps`, `animations/settings` | Skeleton, Weights |
| AimNode2AimBoneData (Animation) | 1,518 | 111 | `objects/weapons`, `characters/animtree` | AimBoneIndex, AimBoneName, AimBoneWorldRotation, AimDirections, AimPositionBaseBoneIndex, AimPositionBaseBoneName, AimPositionBoneBaseTransforms, AimPositionBoneIndex, AimPositionBoneName, AlwaysAnimate, CorrectDirection |
| AnimationData (GameSharedResources) | 31 | 1 | `ai/animations.dbx` | Animation, Duration, EndAngle, EndState, Fire, Index, Movement, NodeName, ObstacleDistance, ObstacleHeight, Readiness, StartState |
| AnimationMovementAsset (GameSharedResources) | 1 | 1 | `ai/animations.dbx` | Animations |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

## 3. Vehicles

### DATA-VEH-1: Vehicle physics components beyond the chassis

Status: **verified** · Variants: PC

VEH-PHYS-1's chassis is completed by: wheels (`WheelConfigData`, 66), anti-roll bars (`AntiRollBar`), stabilisers
(`StabilizerData`, 64), motion damping (`MotionDampingData`, 39), aerodynamics (`AeroDynamicPhysicsData`, 30; wings
`WingPhysicsData`, 28; helicopter rotors `RotorParameters`, 17; their curves `Curve2D`), floating (`BoxFloatPhysicsData`,
96; boats' `BoatFloatPhysicsData`, 9) and input tweaks (`VehicleInputTweakData`). Moving parts are
`ChildRotationBodyData` (235: turrets, rotors) and `ChildMovingBodyData` (49); loose parts that fall off are
`LoosePartPhysicsData` (182; networked `NetworkableLoosePartPhysicsData`, 185) and a destroyed part's banger impulse
`BangerSpawnImpulseParams` (44).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| WheelConfigData (Physics) | 66 | 35 | `objects/vehicles` | AckermanDeviceType, AdjustWheelRotation, AlignMomScale, AngularVelocityMinSlipCondition, BrakeFactor, BrakeForce, CollisionMaterial, DiffGearRatio, DrivingType, EngineBrakeMaxFactor, EngineBrakeMinFactor, EngineBrakeVelocityFactor, EngineIndex, FrictionMethod, FrictionMomentMaxFactor, FrictionMomentMultiplier, FrictionMomentVelocityMax, FrictionMomentVelocityMin, FxTorqueRadiusMultiplier, HandBrakeFactor, HandBrakeForce, HasSteeringInverted, HighSpeedSteeringSensitivity, HighSpeedSteeringSensitivityLimit, Inertia, IsAllowedToSpin, LateralFrictionForceMaxFactor, LateralNegK, LateralPosK, LongitudinalFrictionForceMaxFactor, LongitudinalNegK, LongitudinalPosK, LowSpeedSteeringSensitivity, LowSpeedSteeringSensitivityLimit, Mass, MidSpeedSteeringSensitivity, MidSpeedSteeringSensitivityLimit, OffGroundGravityModifier, Offset, PacejkaConfigIndex, Radius, RollingResistanceBaseFactor, RollingResistanceInternalBaseFactor, RollingResistanceVelocityFactor, RotationBody, RotationDirectionIndex, SensitivityRangeScale, SideSlipAngleMaxSlipCondition, SlopeGripExponent, SlopeGripMaxAngle, SlopeGripMinAngle, SphereCollision, Spring, SteerInertia, SteeringAngleIndex, SteeringSensitivity, SteeringType, TrackedForwardSpeedLimit, TrackedTurnSpeedLimit, UseEngineBrake, UseFrictionMoment, UseLowSpeedAutoBrake, UseRollingResistanceBaseFactor, UseRollingResistanceVelocityFactor, WheelBaseLateral, WheelBaseLongitudinal, WheelSlipRatioMaxSlipCondition, WheelVelocityXMinSlipCondition, WheelVelocityZMinSlipCondition, Width |
| AntiRollBar (Physics) | 6 | 3 | `objects/vehicles` | Stiffness, TorqueActivationLimit |
| StabilizerData (Physics) | 64 | 64 | `objects/vehicles` | Advanced, AdvancedYaw, PitchStrength, RollStrength, StabilizeUnoccupiedVehicle, VerticalVelocityStrength, YawStrength |
| MotionDampingData (Physics) | 39 | 39 | `objects/vehicles` | Linear, LinearModifier, Pitch, Roll, Yaw |
| AeroDynamicPhysicsData (Physics) | 30 | 30 | `objects/vehicles`, `objects/weapons` | BodyDrag, BodyDragOffsetXY, BodyDragOffsetXZ, BodyDragOffsetYZ |
| WingPhysicsData (Physics) | 28 | 8 | `objects/vehicles` | BaseAngleOfAttack, Drag, DragCoefficient, FlapDrag, FlapLift, FlapTurnSpeed, InputForFlap, Lift, LiftCoefficient, MaxFlapAngleScaleFactor, VisualFlapTurnSpeed |
| RotorParameters (Physics) | 17 | 17 | `objects/vehicles` | AdditionalGravityModifier, AngleOfAttack, CollectiveBrakeInputScale, CollectiveInputIdle, CollectiveThrottleInputScale, CyclicFadeOutOffset, CyclicInputScalePitch, CyclicInputScaleRoll, HorizontalForceModifier |
| Curve2D (Physics) | 29 | 11 | `objects/vehicles` | Curve |
| BoxFloatPhysicsData (Physics) | 96 | 83 | `objects/vehicles`, `objects/objects`, `objects/street_objects` | Density, FilledDensity |
| BoatFloatPhysicsData (Physics) | 9 | 9 | `objects/vehicles` | AngularDampening, Density, Depth, FilledDensity, FrictionThrottleModifier, FrontCurveDegree, FrontRatio, Length, LiftModifier, NonEngineSteer, NonEngineSteerMaxSpeed, NonEngineSteerMinSpeed, Offset, SideCurveDegree, SubSurfaceSplits, SupportSizeMod, WaterDampeningMod, WaterFrictionAxisMod, WaterResistanceAxisMod, Width |
| VehicleInputTweakData (Physics) | 1 | 1 | `objects/weapons` | InputTweakType, MaxSpeed, MaxSpeedScale, MinSpeed, MinSpeedScale |
| ChildRotationBodyData (GameSharedResources) | 235 | 78 | `objects/vehicles`, `objects/weapons` | AngularConstraintMax, AngularConstraintMin, AngularMomentumDampingAcc, AngularMomentumDampingDeacc, AngularMomentumMultiplier, AngularMomentumThreshold, ForceModifier, Inertia, Mass, ResetForceInputThreshold, ResetForceModifier, RotationAxis, RotationInput, UseAngularConstraint, UseAngularMomentumDamping, UseAngularMomentumThreshold, UseLinearInput, UsePostSatisfyConstraints, WorldSpaceLock, WorldSpaceLockEfficiency |
| ChildMovingBodyData (GameSharedResources) | 49 | 44 | `objects/vehicles`, `objects/weapons` | Axis, End, Inertia, InputAction, InputDelay, IsOneShotInput, Mass, Speed, Start, TriggerImpulse |
| LoosePartPhysicsData (Physics) | 182 | 33 | `objects/vehicles` | FakePhysics |
| NetworkableLoosePartPhysicsData (GameSharedResources) | 185 | 31 | `objects/vehicles` | FakePhysics, Networked |
| BangerSpawnImpulseParams (GameSharedResources) | 44 | 33 | `objects/vehicles`, `objects/objects`, `terrains/bc1_oasis` | MaxHorizontalAngle, MaxStrength, MaxVerticalAngle, MinHorizontalAngle, MinStrength, MinVerticalAngle, WaterStrengthModifier |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

### DATA-VEH-2: Vehicle components: effects, weapons routing, remote entries

Status: **verified** · Variants: PC

A vehicle's other components: `WorldInteractionComponentData` (41: `MiddlegrowthSpeedThreshold`, inferred: the speed above which it pushes middle-growth aside),
`VehicleWaterEffectData` (22), `WeaponEffectsData` (21) and `LockingWeaponEffectsData` (24: a weapon's spotting /
order cones and its lock-on controller, HUD-FX-1), `WeaponInputRouterComponentData` (19: one input firing several
barrels in rotation), `SkinnedMeshComponentData` (17) and `RemoteEntryComponentData` (2: a seat entered from outside
the vehicle, VEH-SEAT-1's entry with a remote dummy).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| WorldInteractionComponentData (GameSharedResources) | 41 | 41 | `objects/vehicles` | MiddlegrowthSpeedThreshold |
| VehicleWaterEffectData (GameSharedResources) | 22 | 6 | `objects/vehicles` | Effect, Position |
| WeaponEffectsData (GameSharedResources) | 21 | 20 | `objects/vehicles`, `objects/weapons` | ActiveSpotConeAngle, CanSpotAndOrder, FocusConeAngle, OrderConeAngle |
| LockingWeaponEffectsData (GameSharedResources) | 24 | 24 | `objects/vehicles`, `objects/weapons` | ActiveSpotConeAngle, CanSpotAndOrder, FocusConeAngle, IsGuided, IsHoming, LockingController, OrderConeAngle, WarnLock |
| WeaponInputRouterComponentData (GameSharedResources) | 19 | 19 | `objects/vehicles`, `objects/weapons` | MaxFireRate, RotationCount |
| SkinnedMeshComponentData (GameSharedResources) | 17 | 11 | `objects/vehicles` | BoneFakePhysics, Mesh, Skeleton |
| RemoteEntryComponentData (GameSharedResources) | 2 | 2 | `objects/vehicles`, `objects/weapons` | AIData, AnimationOffset, AnimationParametersFromEntry, Dummy, EnterImpulse, EntryClass, EntryOrderNumber, EntryRadius, HasDeathAnimation, HudData, InputConceptDefinition, InputMapping, IsAllowedToExitInAir, IsShielded, LockSoldierAimingToEntry, PoseConstraints, RagdollCollideWithVehicles, ShowSoldierGearInEntry, ShowSoldierInEntry, ShowSoldierWeaponInEntry, SoldierOffset, VOTypes |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

## 4. Weapons and gadgets

### DATA-WPN-1: Gadgets, thrown objects and the weapon effects that hold the spot cones

Status: **verified** · Variants: PC

The deployables are projectile entities of their own: `AmmoBoxEntityData` and `MedicBagEntityData` (2 each:
resupply / healer, health, attach sound), `MotionSensorEntityData` (5: shown on the minimap) and
`TracerDartEntityData` (3: attachable to soldiers or vehicles); the campaign's `AmmoCrateEntityData` (71) is a
placed resupply. Each hand-held weapon's `...WeaponEffectsData` (melee, detonated, mortar strike, ammo box, medic bag,
defibrillator, binoculars, power tool) carries the spotting and ordering cones SOLD-SPOT-1 asks about:
`ActiveSpotConeAngle`, `FocusConeAngle`, `OrderConeAngle`, `CanSpotAndOrder`, with the gadget's own range or
strike values.

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| AmmoBoxEntityData (GameSharedResources) | 2 | 2 | `objects/weapons` | ActiveSpottedTime, AmmoResupply, AttachSound, DetonateOnTimeout, Explosion, ExtraDamping, HasRemoteDetonator, HasVehicleDetonation, Health, ImpulseScale, InitMeshHideTime, InitialAngularVelocity, InitialSpeed, IsAttachable, IsAttachableToSoldiers, IsAttachableToVehicles, IsDestructible, IsObsolete, Material, MaxCount, Mesh, PhysicsData, ReceivesExplosionDamage, ReplacementObject, TimeToLive, TimeToLiveOnPlayerDeath, TrailEffect, VehicleDetonationActivationDelay, VehicleDetonationRadius, VisualConvergeDistance |
| MedicBagEntityData (GameSharedResources) | 2 | 2 | `objects/weapons` | ActiveSpottedTime, AttachSound, DetonateOnTimeout, Explosion, ExtraDamping, HasRemoteDetonator, HasVehicleDetonation, Healer, Health, ImpulseScale, InitMeshHideTime, InitialAngularVelocity, InitialSpeed, IsAttachable, IsAttachableToSoldiers, IsAttachableToVehicles, IsDestructible, IsObsolete, Material, MaxCount, Mesh, PhysicsData, ReceivesExplosionDamage, ReplacementObject, TimeToLive, TimeToLiveOnPlayerDeath, TrailEffect, VehicleDetonationActivationDelay, VehicleDetonationRadius, VisualConvergeDistance |
| MotionSensorEntityData (GameSharedResources) | 5 | 5 | `objects/weapons`, `levels/` | CollisionSpeedMultiplier, DetonateOnTimeout, DisplayInMiniMap, Explosion, ExtraDamping, Gravity, GrenadeProximityRadius, GrenadeProximityTime, InitMeshHideTime, InitialAngularVelocity, InitialSpeed, IsAttachable, IsAttachableToSoldiers, IsAttachableToVehicles, IsObsolete, Material, MaxCount, Mesh, MinBounceSpeed, OnlyShowEnemies, PhysicsData, PulseDelay, Radius, ReplacementObject, ShowExplosives, ShowSoldiers, ShowVehicles, SoldierVehicleMinSpeed, SpottingSound, TeamToEffect, TimeToActivate, TimeToLive, TimeToShowEnemies, TrailEffect, VisualConvergeDistance |
| TracerDartEntityData (GameSharedResources) | 3 | 3 | `objects/weapons`, `objects/vehicles` | DetonateOnTimeout, Explosion, ExtraDamping, Gravity, InitMeshHideTime, InitialAngularVelocity, InitialSpeed, IsAttachable, IsAttachableToSoldiers, IsAttachableToVehicles, IsObsolete, Material, MaxCount, Mesh, PhysicsData, ReplacementObject, TimeToLive, TrailEffect, TransmissionEffect, VisualConvergeDistance |
| AmmoCrateEntityData (GameSharedResources) | 71 | 12 | `levels/` | EnableReplenish, Marker, Model, Radius, RefillDelay |
| MeleeWeaponEffectsData (GameSharedResources) | 5 | 5 | `objects/weapons` | ActiveSpotConeAngle, AttackMaterial, CanSpotAndOrder, FocusConeAngle, KeepStickyAimTime, OrderConeAngle, StickToTargetDelay |
| DetonatedWeaponEffectsData (GameSharedResources) | 3 | 3 | `objects/weapons` | ActiveSpotConeAngle, CanSpotAndOrder, FocusConeAngle, LockingController, OrderConeAngle, Range |
| MortarStrikeFiringEffectsData (GameSharedResources) | 3 | 3 | `objects/weapons` | ActiveSpotConeAngle, CanSpotAndOrder, FocusConeAngle, HoldingTolerance, MaxRandomSpawnHeight, MaxStrikeDistance, OrderConeAngle, StrikeRadius |
| AmmoBoxWeaponEffectsData (GameSharedResources) | 2 | 2 | `objects/weapons` | ActiveSpotConeAngle, CanSpotAndOrder, FocusConeAngle, OrderConeAngle |
| MedicBagWeaponEffectsData (GameSharedResources) | 2 | 2 | `objects/weapons` | ActiveSpotConeAngle, CanSpotAndOrder, FocusConeAngle, OrderConeAngle |
| DefibrillatorWeaponEffectsData (GameSharedResources) | 2 | 2 | `objects/weapons` | ActiveSpotConeAngle, CanSpotAndOrder, FocusConeAngle, OrderConeAngle, Range |
| BinocularsWeaponEffectsData (GameSharedResources) | 2 | 2 | `objects/weapons` | ActiveSpotConeAngle, CanSpotAndOrder, FOV, FocusConeAngle, MaxDistance, OrderConeAngle |
| PowerToolWeaponEffectsData (GameSharedResources) | 2 | 2 | `objects/weapons` | ActiveSpotConeAngle, CanSpotAndOrder, DamageSound, FocusConeAngle, OrderConeAngle, RepairCompletedSound, RepairMaterial, RepairSound |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

## 5. The AI's constants

### DATA-AI-1: `AI/Settings`' constants, vehicle types and the path data's sector offsets

Status: **verified** · Variants: PC

`AI/Settings` holds one partition of tuning constants: look (eye targeter offsets, look-around angles), turret, tank
and car control, search and destroy, position evaluation (front line search), environment (banger distance) and
traversal (`TraversalConstantData` with four `TraversalConfigData`); `AI/Behaviours`' `AIVehicleTypes` lists the vehicle
types, goals and squad behaviours (BOT-SQD-2). `AI/Pathfinding`'s `PathfindingZoneSettings` sets the graph accuracy and
sector count, and each campaign level's `Precomputed/<infantry|vehicle>/SectorOffsets` partition a
`PathdataSectorOffsets` (25: the x / z offset of its path data's sectors, HPD-USE-1).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| LookConstantData (GameSharedResources) | 1 | 1 | `ai/settings.dbx` | EyeTargeterMaxOffsetTime2, EyeTargeterMaxPitchOffset, EyeTargeterMaxYawOffset, EyeTargeterMinOffsetTime2, EyeTargeterMinPitchOffset, EyeTargeterMinYawOffset, EyeTargeterSpringModifier, LookAroundClampAngle, LookAroundMaxRandomWaitTime, LookAroundMinRandomWaitTime, LookAroundRandomAfterObjectModifier |
| TurretControlConstantData (GameSharedResources) | 1 | 1 | `ai/settings.dbx` | FastLookSpeed, NormalLookSpeed, SearchLookSpeed, SlowLookSpeed |
| TankControlConstantData (GameSharedResources) | 1 | 1 | `ai/settings.dbx` | SmoothTurnSpeedAngle, SmoothTurnSpeedLimit, TurnSpeedLimit, YawStrength |
| CarControlConstantData (GameSharedResources) | 1 | 1 | `ai/settings.dbx` | YawStrength |
| SearchAndDestroyConstantData (GameSharedResources) | 1 | 1 | `ai/settings.dbx` | DefaultSearchAndDestroyRadius, KnownTargetRadius, TimeToWaitMax, TimeToWaitMin |
| PositionEvaluationConstantData (GameSharedResources) | 1 | 1 | `ai/settings.dbx` | FrontLineAccuracy, FrontLineSearchDepth, FrontLineSearchSensitivity, MaxCachedNodeIds, OwnRandomPositionsRadius, RandomPositionCount |
| EnvironmentConstantData (GameSharedResources) | 1 | 1 | `ai/settings.dbx` | BangerIncludeDistance |
| TraversalConstantData (GameSharedResources) | 1 | 1 | `ai/settings.dbx` | BehaviourTestTraversalConstants, CoverSearchTraversalConstants, EnvironmentTraversalConstants, FrontLineTraversalConstants |
| TraversalConfigData (GameSharedResources) | 4 | 1 | `ai/settings.dbx` | MaxDistance, TimePart |
| AIVehicleTypes (GameSharedResources) | 1 | 1 | `ai/behaviours.dbx` | Goals, SquadBehaviours, VehicleTypes |
| PathfindingZoneSettings (GameSharedResources) | 1 | 1 | `ai/pathfinding` | GraphAccuracy, NbSectors, Pitch |
| PathdataSectorOffsets (GameSharedResources) | 25 | 25 | `levels/<level>` | XOffset, ZOffset |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

## 6. Matchmaking

### DATA-MM-1: The match maker's parameters

Status: **verified** · Variants: PC

`Awards/MatchMakerParams` holds the `MatchMakerAsset`: ELO parameters, the maps (`MatchMakingMap`, 20), playlists
(4: maps, rounds per map, a rule), sessions (6: type, filters, string and integer preferences, the player pool's
thresholds and time-out, server listing limits), filters and preferences. It is the online back-end's quick-match
(NET-ONL-1); OPFOR's offline play does not use it.

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| MatchMakerAsset (GameSharedResources) | 1 | 1 | `awards/matchmakerparams.dbx` | EloParams, Maps, Playlists, PoolOverride, Sessions |
| MatchMakingSession (GameSharedResources) | 6 | 1 | `awards/matchmakerparams.dbx` | CountServersAvailableForReset, Filters, HostSetupTimeout, IntegerPreferences, ListServersMaxCount, PoolMaxPlayers, PoolPlayerCountThresholds, PoolPlayerTimeout, SessionIdentifier, SessionType, StringPreferences |
| MatchMakingMap (GameSharedResources) | 20 | 1 | `awards/matchmakerparams.dbx` | GameMode, KickIfLicenseMissing, Licenses, Map, MapName, Revision, UIName |
| MatchMakingPlaylist (GameSharedResources) | 4 | 1 | `awards/matchmakerparams.dbx` | IsDefault, Maps, RoundsPerMap, Rule |
| MatchMakingFilter (GameSharedResources) | 3 | 1 | `awards/matchmakerparams.dbx` | DefaultValues, HideOnRuleVisible, Match, PossibleValues, Rule, SkipIfDefault |
| MatchMakingStringPreference (GameSharedResources) | 8 | 1 | `awards/matchmakerparams.dbx` | DefaultValues, Fit, FitThresholds, HideOnRuleVisible, Labels, Match, MatchingOption, Rule, SkipIfDefault, Variant, VotingMethod, Weight |
| MatchMakingIntegerPreference (GameSharedResources) | 2 | 1 | `awards/matchmakerparams.dbx` | DefaultValue, FitThresholds, HideOnRuleVisible, Match, MatchingOption, Rule, Scale, SkipIfDefault, Weight |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

## 7. Destruction, vegetation, decals, meshes

### DATA-DSTR-1: Edge models, tree health, decal templates, the mesh asset's build settings

Status: **verified** · Variants: PC

A destructible building's broken edges are edge models: `Objects/Edge_Models/`' `EdgeModelMeshData` (26: a mesh and
settings) placed along the building by its `EdgeModelGenerationSettings` (22: orientation volumes, a volume map) and
`EdgeModelInstances` (21: meshes and links). A destructible tree's states are `TreeHealthStateData` (97: health,
effects, the banger it spawns, loose parts). `Decals/`' `DecalTemplateData` (16: size, rotation, randomness, time to
live, shaders) are the impact decals' templates (DECAL-SRC). The one `MeshAsset` instance (SP_006) shows the mesh
assets' build settings every mesh asset class inherits: LODs, occlusion texture generation, the occluder mesh
(`OccluderMeshEnable`, `HighPriorityOccluderMesh`, `OccluderMeshNode`: OCC-HDR-1), irradiance volumes (LIGHT-IRR-1),
and the PS3's edge and soot switches.

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| EdgeModelMeshData (GameSharedResources) | 26 | 26 | `objects/edge_models` | Mesh, Settings |
| EdgeModelGenerationSettings (GameSharedResources) | 22 | 22 | `objects/buildings` | OrientationVolumes, SharedSettings, VolumeMap |
| EdgeModelInstances (GameSharedResources) | 21 | 21 | `objects/buildings` | Links, Meshes, MultiLinks |
| TreeHealthStateData (GameSharedResources) | 97 | 17 | `objects/vegetation`, `vegetation/nam`, `vegetation/jungle` | BranchEffects, CopyDamageToBanger, Effects, Explosion, Health, LoosePartPhysics, LoosePartPhysicsArray, MeshName, PreferedShapeType, ShakeEffects, SpawnedBanger, SpawnedBangerImpulseParams |
| DecalTemplateData (Decal) | 16 | 16 | `decals/metal`, `decals/concrete`, `decals/scorchmark` | IsStatic, RandomRotation, RandomSize, Rotation, ShaderAssets, Size, TimeToLive |
| MeshAsset (Render) | 1 | 1 | `levels/` | CompressPositionsEnable, CulledOcclusionTextureShaders, ExcludeZOnlyMaterials, GenerateOcclusionTexture, HighPriorityOccluderMesh, IrradianceVolumeEnable, KeepZOnlyVertexElements, LodCount, LodSettings, ManualDrawOrderEnable, OccluderMeshEnable, OccluderMeshNode, OcclusionContrast, OcclusionFallOff, OcclusionMaxDistance, OcclusionTexture, OcclusionTextureQuality, OcclusionTextureResolution, Ps3EdgeEnable, Ps3EdgeScoreErrorWorkaroundEnable, Ps3SootEdgeEnable, Ps3SootEdgeScoreErrorWorkaroundEnable, RadiosityAlbedoTexture, RadiosityMesh, RootNode, ShaderSets, SourceFile, StreamInstancingEnable, StreamingEnable, SurfaceShaders, TangentSpaceBasisEnable, ZOnlyMesh |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

## 8. The level

### DATA-LVL-1: Transform modifiers, award entities, the level list query, the empty input mapping

Status: **verified** · Variants: PC

Objects that sway or spin without physics carry visual transform modifiers (`PingPongRotationVisualTransformModifierData`,
303, most in Vietnam's levels; `RotationVisualTransformModifierData`, 13; `LinearMovementVisualTransformModifierData`,
12). Rush levels place `ArmDisarmAwardEntityData` (146, no members set; inferred: the M-COM arm / disarm award hook, PERS-AWD).
`LevelListQuery`'s `FileQueryData` (1) is a file query (path, recursion, include files / directories) whose results
list the levels (MENU-CAT-1). `InputActionMappingData` (1, `Input/InputMappings`) has no members (the base of
INP-DATA-2's mapping classes).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| PingPongRotationVisualTransformModifierData (Entity) | 303 | 54 | `levels/` | Frequency, MaxAngleInDegrees, MinAngleInDegrees, RotationAxis, UseRandomTiming |
| RotationVisualTransformModifierData (Entity) | 13 | 10 | `objects/objects`, `levels/`, `objects/street_objects` | DegreesPerSecond, IsStaticRotation, RotationAxis, UseRandomTiming |
| LinearMovementVisualTransformModifierData (Entity) | 12 | 9 | `objects/objects`, `levels/` | Frequency, MaxOffset, MinOffset, UseRandomTiming |
| ArmDisarmAwardEntityData (GameSharedResources) | 146 | 30 | `levels/` |  |
| FileQueryData (GameSharedResources) | 1 | 1 | `levellistquery.dbx` | IncludeDirectories, IncludeFiles, Path, Recursive, Results |
| InputActionMappingData (GameSharedResources) | 1 | 1 | `input/inputmappings` |  |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]
