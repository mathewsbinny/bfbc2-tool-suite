# Sound graphs, voice-over trees and the mix

**Sources.** Round SPG4 (2026-11-23): the census of DICE's partitions (`spec/census.json`, re-measured by the game
tier) and `BFBC2Game.exe`'s names. The Wave resources are `formats/audio.md`; how the engine loads and plays sounds,
the voice-over events and the streaming are `engine/audio.md` (SND-*) and `engine/streaming.md`. This file is the
data side of a sound: the node graph a `SoundPatchAsset` holds, the voice-over trees and the global mix. The tables
list the members DICE's instances carry, less `Name`, `MemoryReportGroup`, `Components`, `Enabled`, `Enumeration` and
`Transform`.

## 1. The sound graph

### SGR-WIRE-1: A sound is a node graph wired by ports and connections

Status: **verified** · Variants: PC

Every sound partition under `Sound/` (weapons, vehicles, ambients, character, music ...) is a graph of nodes. A node
exposes `SoundNodePort`s (59,593: a port type, a wire index, the value used when unconnected); a
`SoundNodeConnection` (17,908) wires a source node's port to a target node's port. The graph's inputs are
`SoundGraphParameter`s (1,032: id, default value, wire) set by the game (speed, rpm, distance ...) and
`SoundGraphEvent`s (2,143) it fires (start, stop, fire, reload ...); a `ReceiveNode` with its `ReceiveEntry`s reads
parameters into the graph. These graphs are what the engine's sound event code drives (SND-EVT-1).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| SoundNodePort (Audio) | 59,593 | 1,312 | `sound/weapons`, `sound/vehicles`, `sound/ambients` | PortType, UnconnectedValue, WireIndex |
| SoundNodeConnection (Audio) | 17,908 | 1,312 | `sound/weapons`, `sound/vehicles`, `sound/ambients` | SourceNode, SourcePort, TargetNode, TargetPort |
| SoundGraphParameter (Audio) | 1,032 | 234 | `sound/vehicles`, `sound/weapons`, `sound/character` | DefaultValue, Id, WireIndex |
| SoundGraphEvent (Audio) | 2,143 | 1,312 | `sound/ambients`, `sound/weapons`, `sound/character` | DefaultValue, Id, WireIndex |
| ReceiveNode (Audio) | 2,759 | 1,312 | `sound/ambients`, `sound/weapons`, `sound/character` | EffectDescCachedIndex, Entries |
| ReceiveEntry (Audio) | 3,265 | 1,312 | `sound/weapons`, `sound/ambients`, `sound/vehicles` | Out, Parameter, SavedValue, Source |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json (DICE's partitions, round SPG4, 2026-11-23): the classes above]

### SGR-NODE-1: Samplers play waves; processing nodes shape them

Status: **verified** · Variants: PC

A `SamplerNode` (2,357) holds `SamplerGroup`s (2,373: the waves or a wave map, pitch, amplitude, delay, random
variation, trigger and release, channel count) and their `SamplerWaveMapEntry`s (9,801: a wave with base pitch and
loop flag); monophonic samplers (`MonophonicSamplerNode` / `-Group`) are the single-voice variant. The processing
nodes: scale / clamp, simple transform, delta (rate of change), condition, number and sequence generators, LFO, ADSR
envelope, crossfaders (two-way, multi), mixer, demultiplexer, delay, low- and high-pass IIR filters and parameter
filters, a physics node (impact speeds), a camera node (listener position), a rumble node (pad feedback), car / tank
engine models (`CarNode`, `SimpleCarNode`, `TankNode`), the user-music arbiter, and debug nodes.

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| SamplerNode (Audio) | 2,357 | 1,301 | `sound/weapons`, `sound/ambients`, `sound/vehicles` | EffectDescCachedIndex, Groups |
| SamplerGroup (Audio) | 2,373 | 1,302 | `sound/weapons`, `sound/ambients`, `sound/vehicles` | Amplitude, ChannelCount, Delay, Finished, Offset, Output, Pitch, Random, RandomOnDemandVariationCount, Release, Trigger, Wave, WaveMap, ZeroLatency |
| SamplerWaveMapEntry (Audio) | 9,801 | 1,306 | `sound/weapons`, `sound/character`, `sound/vehicles` | BasePitch, Loop, Wave |
| MonophonicSamplerNode (Audio) | 5 | 5 | `sound/weapons`, `sound/maoism` | EffectDescCachedIndex, Groups |
| MonophonicSamplerGroup (Audio) | 4 | 4 | `sound/weapons`, `sound/maoism` | Amplitude, ChannelCount, Delay, Finished, Offset, Out, Output, Pitch, Random, RandomOnDemandVariationCount, Release, Trigger, Wave, WaveMap, ZeroLatency |
| ScaleClampNode (Audio) | 1,917 | 324 | `sound/vehicles`, `sound/weapons`, `sound/character` | EffectDescCachedIndex, In, InMax, InMin, Out, OutMax, OutMin |
| SimpleTransformNode (Audio) | 1,709 | 186 | `sound/vehicles`, `sound/weapons`, `sound/action_bubbles` | EffectDescCachedIndex, Operation, X, Y, Z |
| DeltaNode (Audio) | 74 | 44 | `sound/vehicles`, `sound/weapons`, `sound/anaconda` | Deltas, EffectDescCachedIndex |
| DeltaGroup (Audio) | 74 | 44 | `sound/vehicles`, `sound/weapons`, `sound/anaconda` | Delta, In |
| ConditionNode (Audio) | 379 | 137 | `sound/vehicles`, `sound/explosions`, `sound/weapons` | Conditions, EffectDescCachedIndex |
| ConditionGroup (Audio) | 379 | 137 | `sound/vehicles`, `sound/explosions`, `sound/weapons` | Condition, True, X, Y |
| NumberGeneratorNode (Audio) | 512 | 424 | `sound/ambients`, `sound/weapons`, `sound/character` | EffectDescCachedIndex, Max, Min, NumberType, Trigger, Y |
| SequenceGeneratorNode (Audio) | 1 | 1 | `sound/music` | EffectDescCachedIndex, Max, Min, Reset, Step, Trigger, Value |
| LfoNode (Audio) | 319 | 133 | `sound/action_bubbles`, `sound/weapons`, `sound/explosions` | EffectDescCachedIndex, LfoGroups |
| LfoGroup (Audio) | 319 | 133 | `sound/action_bubbles`, `sound/weapons`, `sound/explosions` | Amplitude, Hz, Max, Min, Mode, Out |
| AdsrNode (Audio) | 214 | 96 | `sound/action_bubbles`, `sound/explosions`, `sound/music` | A, D, EffectDescCachedIndex, InitialValue, R, Release, S, Trigger, Value |
| CrossfaderNode (Audio) | 192 | 108 | `sound/weapons`, `sound/explosions`, `sound/maoism` | Ctrl, EffectDescCachedIndex, In1, In2, Out |
| Crossfader2Node (Audio) | 44 | 36 | `sound/vehicles`, `sound/weapons` | Ctrl, CtrlOut1, CtrlOut2, EffectDescCachedIndex |
| MultiCrossfaderNode (Audio) | 32 | 23 | `sound/vehicles`, `sound/weapons`, `sound/maoism` | Control, CrossfaderGroups, EffectDescCachedIndex, LockControlValue, Start, Stop |
| MultiCrossfaderGroup (Audio) | 51 | 23 | `sound/vehicles`, `sound/weapons`, `sound/maoism` | Amplitude, FadeAmplitude, FadeBegin, FadeEnd, FadeType, Start, Stop |
| MixerNode (Audio) | 212 | 111 | `sound/weapons`, `sound/vehicles`, `sound/music` | EffectDescCachedIndex, MixerGroups, Out |
| MixerGroup (Audio) | 422 | 111 | `sound/weapons`, `sound/vehicles`, `sound/music` | Amplitude, In |
| DeMuxNode (Audio) | 334 | 146 | `sound/weapons`, `sound/explosions`, `sound/vehicles` | EffectDescCachedIndex, Outputs, Trigger, Value, WrapValue |
| DeMuxOutput (Audio) | 670 | 146 | `sound/weapons`, `sound/explosions`, `sound/character` | Trigger |
| DelayNode (Audio) | 152 | 108 | `sound/weapons`, `sound/vehicles`, `sound/radio` | DelayTime, EffectDescCachedIndex, Feedback, In, Out |
| LowPassIir2Node (Audio) | 189 | 119 | `sound/weapons`, `sound/vehicles`, `sound/radio` | EffectDescCachedIndex, Frequency, In, Out |
| HighPassIir2Node (Audio) | 199 | 155 | `sound/weapons`, `sound/character`, `sound/vehicles` | EffectDescCachedIndex, Frequency, In, Out |
| ParameterFilterLinearNode (Audio) | 209 | 51 | `sound/vehicles`, `sound/beach_sfx`, `sound/maoism` | AttackSpeed, EffectDescCachedIndex, In, Out, ReleaseSpeed |
| ParameterFilterLpNode (Audio) | 7 | 5 | `sound/vehicles`, `sound/beach_sfx`, `sound/maoism` | EffectDescCachedIndex, Hz, In, Out |
| PhysicsNode (Audio) | 358 | 283 | `sound/weapons`, `sound/vehicles`, `sound/character` | Distance, EffectDescCachedIndex, Velocity |
| CameraNode (Audio) | 69 | 51 | `sound/action_bubbles`, `sound/explosions`, `sound/weapons` | EffectDescCachedIndex, Pitch, Roll, Yaw |
| RumbleNode (Audio) | 78 | 61 | `sound/action_bubbles`, `sound/explosions`, `sound/vehicles` | EffectDescCachedIndex, Left, Right |
| CarNode (Audio) | 2 | 2 | `sound/beach_sfx`, `sound/vehicles` | AccellerateOut, Accellerate_AngularVel_Pitch, Accellerate_Slip_Pitch, Accellerate_Tilt_Pitch, AngularVelocity, CruiseLoop, Cruise_Rpm_Pitch, DeaccellerateOut, EffectDescCachedIndex, Gear, Gear1, Gear1MaxVelocity, Gear1Release, Gear2, Gear2MaxVelocity, Gear2Release, Gear3, Gear3MaxVelocity, Gear3Release, IdleLoop, IdleOut, Idle_Rpm_Pitch, Idle_Tilt_Pitch, InDriverPos, InteriorCam, InteriorLoop, InteriorOut, InteriorStrainLoop, InteriorStrainOut, InteriorStrain_Vel_Amp, InteriorStrain_Vel_Pitch, PipeAtatckThrottleDeltaThreshold, PipeAttackMaxVelocity, PipeAttackOut, PipeAttacks, RPM, Release, ReleaseEvent, Release_Tilt_Pitch, Reverse_Idle_Rpm_Amp, Reverse_Idle_Rpm_Pitch, Reverse_Idle_Tilt_Pitch, Slip, SpringCompression, Start, StartOut, Stop, StopOut, StuckIdleTimeThreshold, StuckLoop, StuckOut, StuckVelocityThreshold, Stuck_Rpm_Amp, Stuck_Vel_Amp, SuspensionBounceOut, SuspensionBounces, Throttle, Tilt, Trans_Reverse_Amp_Factor, Trans_Vel_Amp, Trans_Vel_Pitch, TransmissionLoop, TransmissionOut, Velocity |
| SimpleCarNode (Audio) | 6 | 6 | `sound/vehicles`, `sound/beach_sfx` | Drive, DriveOut, Drive_Tilt_Pitch, Drive_Vel_Pitch, EffectDescCachedIndex, Idle, IdleOut, Idle_Drive_Vel_Fade, Idle_Vel_Pitch, InDriverPos, ReleaseEvent, Start, StartOut, Stop, StopOut, Tilt, Velocity |
| TankNode (Audio) | 2 | 2 | `sound/vehicles` | AngularVelocity, AngularVelocityToThrottleScale, AngularVelocityToVelocityScale, EffectDescCachedIndex, Idle, IdleOut, Idle_RPM_Pitch, Idle_TheRest_Vel_Fade, InDriverPos, IntStrain_Vel_Pitch, Int_IntStrain_Vel_Fade, Interior, InteriorCam, InteriorOut, InteriorStrain, InteriorStrainOut, Load_UnLoad_RPM_Pitch, Load_UnLoad_Throttle_Fade, Load_UnLoad_Vel_Pitch, Loaded, LoadedOut, PipeAttackOut, PipeAttacks, RPM, ReleaseEvent, Start, StartOut, Stop, StopOut, Throttle, TrackSlow_Fast_Vel_Fade, TrackSlow_Fast_Vel_Pitch, TracksFast, TracksFastOut, TracksSlow, TracksSlowOut, UnLoaded, UnLoadedOut, Velocity |
| UserMusicArbiterNode (Audio) | 81 | 81 | `sound/music`, `sound/radio`, `sound/action_bubbles` | EffectDescCachedIndex, In, Out |
| DebugValueInput (Audio) | 57 | 17 | `sound/vehicles`, `sound/weapons`, `sound/beach_sfx` | Max, Min, RenderType, V |
| ValueDebugNode (Audio) | 18 | 17 | `sound/vehicles`, `sound/weapons`, `sound/maoism` | EffectDescCachedIndex, Values |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

### SGR-OUT-1: Outputs and emitters place the sound in the world

Status: **verified** · Variants: PC

An `OutputNode` (1,312) sends the graph's signal to `SoundOutput`s (1,829, via `OutputEntry`s) on `SoundSource`s (1,312),
each with emitters: a cone (`SoundSourceConeEmitter`, 1,032), a flat / unpositioned one (`SoundSourceFlatEmitter`,
797), the low-frequency channel (`SoundSourceLfeEmitter`, 134) or a dialog emitter (`SoundSourceDialogEmitter`, 12).
In a level or an effect a `SoundEntityData` (661) places a sound template with a cull distance, an instance cap and
optional post-effects (a brightness / contrast flash).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| OutputNode (Audio) | 1,312 | 1,312 | `sound/ambients`, `sound/weapons`, `sound/character` | BypassHeadroom, ConnectTo, Dialog, DialogSoundEmitter, EffectDescCachedIndex, Entries, Lfe, LfeSoundEmitter |
| OutputEntry (Audio) | 1,832 | 1,308 | `sound/weapons`, `sound/ambients`, `sound/vehicles` | In, Output |
| SoundOutput (Audio) | 1,829 | 1,305 | `sound/weapons`, `sound/ambients`, `sound/vehicles` | Emitter, Source |
| SoundSource (Audio) | 1,312 | 1,312 | `sound/ambients`, `sound/weapons`, `sound/character` | Emitters |
| SoundSourceConeEmitter (Audio) | 1,032 | 767 | `sound/weapons`, `sound/vehicles`, `sound/character` | AttenuationType, Direction, DistanceSlope, Gain, HFDampingAngle, HFDampingDistance, HFDampingObstruction, HFDampingOcclusion, InnerAngle, MinDistance, OuterAngle, OutsideVolume, Position, Solo |
| SoundSourceFlatEmitter (Audio) | 797 | 677 | `sound/ambients`, `sound/weapons`, `sound/action_bubbles` | Angle, AttenuationType, Bleed, DistanceSlope, Gain, HFDampingAngle, HFDampingDistance, HFDampingObstruction, HFDampingOcclusion, MinDistance, Solo |
| SoundSourceLfeEmitter (Audio) | 134 | 134 | `sound/weapons`, `sound/action_bubbles`, `sound/explosions` | AttenuationType, DistanceSlope, Gain, HFDampingAngle, HFDampingDistance, HFDampingObstruction, HFDampingOcclusion, LfeLoudness, MinDistance, Solo |
| SoundSourceDialogEmitter (Audio) | 12 | 12 | `sound/vo`, `sound/character`, `sound/action_bubbles` | AttenuationType, DialogLoudness, DistanceSlope, Gain, HFDampingAngle, HFDampingDistance, HFDampingObstruction, HFDampingOcclusion, MinDistance, Solo |
| SoundEntityData (Audio) | 661 | 586 | `effects/weapons`, `fx/ambient`, `fx/vietnam` | CullDistance, MaxInstanceCount, PostEffectBrightnessOffset, PostEffectContrastOffset, PostEffectDistanceFallof, PostEffectFadeInTime, PostEffectFadeOutTime, PostEffectSizeScale, ResetInstanceWhenStarted, SoundTemplate, TriggerPostEffect |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

## 2. Voice-over

### SGR-VO-1: Voice-over trees, dialog lines and the trigger tables

Status: **verified** · Variants: PC

The spoken lines are eight `VoiceOverTree`s under `Sound/VO` (per language: sound templates for talk, radio, scream
and scripted lines) of `DialogLineNode`s (15,734: a node name, priority, children, talk times). `Sound/VO_Triggers`'
tables pick a line for a voice-over event (SND-VO): `VORow`s (1,206) of `VOGroup`s (1,419: chance, pause, lines) of
`VOLine`s (1,653: talk / radio / scream lines, urgency, animations), filtered by parameters on the person, listener,
team, vehicle, weapon, target distance and readiness (`VO*Param`). `Sound/VO`'s `VOTypesAsset` lists the persons,
vehicles and weapons those parameters name (`VOPerson` 26, `VOVehicle` 61, `VOWeapon` 33). A level's scripted lines
are `VoiceOverLineAsset`s (5,221: a dialog line node, the speaking soldier, over the radio or not); the AI's voice-over
constants are `AI/Settings`' `VOConstantData`.

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| VoiceOverTree (Audio) | 8 | 8 | `sound/vo` | Children, Language, RadioEffectSoundTemplate, RadioSoundTemplate, ScreamSoundTemplate, ScriptedSoundTemplate, ScriptedUnpositionedSoundTemplate, TalkSoundTemplate, TalkTimeFileNameMatchStrings, VoiceOverEntries |
| DialogLineNode (Audio) | 15,734 | 8 | `sound/vo` | BlockAllSoundsWithLowerPriority, Children, NodeName, Priority, TalkTimes |
| VORow (GameSharedResources) | 1,206 | 28 | `sound/vo_triggers` | NodeName, RowData |
| VOGroup (GameSharedResources) | 1,419 | 28 | `sound/vo_triggers` | Chance, Lines, NodeName, PauseTime |
| VOLine (GameSharedResources) | 1,653 | 28 | `sound/vo_triggers` | CanSkip, CrouchAnimation, Delay, NodeName, RadioDialogLine, ScreamDialogLine, Source, SourcePerson, StandAnimation, TalkDialogLine, UrgencyLevel |
| VOPersonParam (GameSharedResources) | 2,689 | 25 | `sound/vo_triggers` | NodeName, ParameterType, Person |
| VOListenerParam (GameSharedResources) | 494 | 10 | `sound/vo_triggers` | Listener, NodeName, ParameterType |
| VOTeamParam (GameSharedResources) | 289 | 9 | `sound/vo_triggers` | NodeName, ParameterType, Team |
| VOVehicleParam (GameSharedResources) | 450 | 17 | `sound/vo_triggers` | NodeName, ParameterType, Vehicle |
| VOWeaponParam (GameSharedResources) | 155 | 9 | `sound/vo_triggers` | NodeName, ParameterType, Weapon |
| VOTargetDistanceParam (GameSharedResources) | 136 | 5 | `sound/vo_triggers` | MaxDistance, NodeName, ParameterType |
| VOReadinessParam (GameSharedResources) | 60 | 5 | `sound/vo_triggers` | NodeName, ParameterType, Readiness |
| VOTypesAsset (GameSharedResources) | 1 | 1 | `sound/vo` | Persons, Vehicles, Weapons |
| VOPerson (GameSharedResources) | 26 | 1 | `sound/vo` |  |
| VOVehicle (GameSharedResources) | 61 | 1 | `sound/vo` |  |
| VOWeapon (GameSharedResources) | 33 | 2 | `sound/vo`, `objects/vehicles` |  |
| VoiceOverLineAsset (GameSharedResources) | 5,221 | 74 | `levels/` | DialogLineNode, OverRadio, Soldier |
| VOConstantData (GameSharedResources) | 1 | 1 | `ai/settings.dbx` | EnemyBehindPlayerArc, EnemyBehindPlayerRadius, LoiterActivityDistance, LoiterAllowedEnemyDistance, LoiterDistanceSquadToFriendlies, LoiterHumanActivityTime, LoiterMemberActivityTime, LoiterPauseTime, MaxEnemyBehindPlayerHeightDiff, MaxPlayerHurtDistance, MinTimeAfterInterludeBeforeLookAtPlayer, PlayerHurtThreshold, TimeBeforeFirstLookAtPlayerMax, TimeBeforeFirstLookAtPlayerMin, TimeBetweenLookAtPlayersMax, TimeBetweenLookAtPlayersMin |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

## 3. The mix and the sound system

### SGR-MIX-1: Sound states, the HDR mix, mixer groups, stream pools, master settings

Status: **verified** · Variants: PC

`Sound/SoundStates` and `Sound/Nam_SoundStates` hold a `SoundStateSettingsAsset` each (scream and impaired-hearing
thresholds) with their `SoundState`s (11: fades, a low-pass cutoff, an HDR setting: the shell-shock and menu states)
and `HdrSetting`s (the HDR audio's headroom, compression, RMS and window times). `Sound/Mixer`'s `MixerAsset` has five
`MixGroup`s (a path or wildcard and a gain). `Sound/StreamPools`' `StreamPools` holds four `StreamPool`s (stream
counts and read block counts / sizes per platform: Win32, Xenon, PS3), the streamed sounds' budget (SND-STRM-1).
`Sound/AudioMasterSettings` holds the five `SoundSystemSettings` (master, dialog, LFE and reverb volumes, an
equalizer and a compressor: `EqualizerSettings`, `CompressorSettings`). `Sound/AudioMaterialDefinitions` names the
on-road materials (vehicle tyre sounds); `Sound/Big_World`'s 62 `BigWorldSettingsAsset`s and a level's
`AmbientSoundsAsset` (with `IndoorDampingSetting`s) are the ambience; `AnimationSoundMapData` maps animation
categories to sounds per level.

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| SoundStateSettingsAsset (Audio) | 2 | 2 | `sound/soundstates.dbx`, `sound/nam_soundstates.dbx` | ImpairedHearingImpulseThreshold, ScreamThreshold, SoundStates |
| SoundState (Audio) | 11 | 2 | `sound/nam_soundstates.dbx`, `sound/soundstates.dbx` | BypassSound, Duration, FadeInTime, FadeOutTime, HdrSetting, LpCutoffFrequency, SendLpCutoffFrequencyToVo |
| HdrSetting (Audio) | 2 | 2 | `sound/soundstates.dbx`, `sound/nam_soundstates.dbx` | CompressFactor, Headroom, RmsAttackTime, RmsReleaseTime, RmsSmoothFactor, WindowAttackTime, WindowMinBottom, WindowMinTop, WindowReleaseTime, WindowSize |
| MixerAsset (Audio) | 1 | 1 | `sound/mixer.dbx` | Groups |
| MixGroup (Audio) | 5 | 1 | `sound/mixer.dbx` | Gain, IsWildcard, Parent, Path, Wildcard |
| StreamPools (Audio) | 1 | 1 | `sound/streampools.dbx` | StreamPool |
| StreamPool (Audio) | 4 | 1 | `sound/streampools.dbx` | StreamCountPs3, StreamCountWin32, StreamCountXenon, StreamPoolName, StreamReadBlockCountPs3, StreamReadBlockCountWin32, StreamReadBlockCountXenon, StreamReadBlockSizePs3, StreamReadBlockSizeWin32, StreamReadBlockSizeXenon |
| AudioMasterSettingsAsset (Audio) | 1 | 1 | `sound/audiomastersettings.dbx` | SoundSystemSettings |
| SoundSystemSettings (Audio) | 5 | 1 | `sound/audiomastersettings.dbx` | Compressor, DistortionClipLevel, Equalizer, MainMixVolume, MasterDialogGain, MasterLfeGain, MasterVolume, PostEffectsGain, ReverbVolume |
| EqualizerSettings (Audio) | 5 | 1 | `sound/audiomastersettings.dbx` | HighShelfFrequency, HighShelfGain, HpCutoffFrequency, LowShelfFrequency, LowShelfGain |
| CompressorSettings (Audio) | 5 | 1 | `sound/audiomastersettings.dbx` | Attack, Ratio, Release, Threshold |
| AudioMaterialDefinitions (Audio) | 1 | 1 | `sound/audiomaterialdefinitions.dbx` | OnRoadMaterialNameHashes, OnRoadMaterials |
| BigWorldSettingsAsset (GameSharedResources) | 62 | 62 | `sound/big_world` | Settings |
| AmbientSoundsAsset (GameSharedResources) | 28 | 28 | `levels/<level>` | BigWorld, GlobalAmbientSounds, IndoorDampings, MaterialBigWorld, MaterialSounds, SoundMaxHeight |
| IndoorDampingSetting (GameSharedResources) | 85 | 28 | `levels/<level>` | Damping, MaxArea |
| AnimationSoundMapData (GameSharedResources) | 74 | 74 | `levels/<level>` | Categories |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

## Open questions

- The node types' run-time semantics (the port order a node expects, what each parameter id means).
- How the game sets the graphs' parameters (the names of the ids per entity kind).
