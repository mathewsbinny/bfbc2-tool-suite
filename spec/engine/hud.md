# The HUD: minimap, markers, nametags, screen effects, spectator and kill cam

**Sources.** Round SPG4 (2026-11-23): the census of DICE's partitions (`spec/census.json`, re-measured by the game
tier) and `BFBC2Game.exe`'s names. The HUD's Flash movies and their action script are `engine/menus.md` (MENU-HUD,
MENU-RT); this file is the data that configures them and the in-world HUD elements. The tables list the members
DICE's instances carry, less `Name`, `MemoryReportGroup`, `Components`, `Enabled`, `Enumeration` and `Transform`.

## 1. Configuration

### HUD-CFG-1: Two HUD configurations and the shared UI configuration

Status: **verified** · Variants: PC

`Menu/HudConfig` and `Menu/Nam_HudConfig` (Vietnam's) are the HUD's `HudConfigAsset`s: the victory / defeat sounds per
faction, objective sounds, vignettes and health vignette states, the artillery and laser-designator aiming states,
collectibles, the out-of-area voice-over and `ShowSpawnPointsOnMinimap`. `Menu/Shared_Config`'s `UISharedConfigAsset`
holds the input schemes, the HUD render configuration, the unlock and weapon statistics data, the kill message
mapping and the feature hints, with 338 `UIInputNodeDescription`s (an input and its description string id: the
controls screen's labels, INP-UI-1). The HUD's nametag and minimap distances are `UIHudConfig` (MENU-HUD); its
nametags per kind are `UINametag` (84 in `Menu/Hud_Root` and `Menu/Nam_Hud_Root`: icon, text and health-bar
offsets, fade and maximum distances, text colours with colour-blind variants `TextColor_CB`).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| HudConfigAsset (GameSharedResources) | 2 | 2 | `menu/hudconfig.dbx`, `menu/nam_hudconfig.dbx` | AmmoPickupSound, AntiSpam, ArtilleryAiming, ArtilleryAimingOOA, Collectibles, HealthVignetteStates, LaserDesignatorLocked, LaserDesignatorLocking, MPDefeatSoundMec, MPDefeatSoundRu, MPDefeatSoundUs, MPVictorySoundMec, MPVictorySoundRu, MPVictorySoundUs, MortarStrikeFiring, MortarStrikeLocking, NewObjectiveSound, Objective, ObjectiveCompletedSound, ObjectiveUpdatedSound, OutOfAreaVO, SPMissionPresentationConfig, ShowSpawnPointsOnMinimap, Vignettes, WeaponPickupSound |
| UISharedConfigAsset (GameSharedResources) | 1 | 1 | `menu/shared_config.dbx` | CollectiblesGrid, FeatureHints, HudRenderConfig, InputSchemes, KillMsgSIDMapping, UnlocksData, WeaponData, WeaponStatsData |
| UIInputNodeDescription (GameSharedResources) | 338 | 1 | `menu/shared_config.dbx` | DescriptionSID, Input |
| UINametag (UI) | 84 | 2 | `menu/hud_root.dbx`, `menu/nam_hud_root.dbx` | HealthFadeDistance, HealthMaxDistance, HealthbarOffset, Icon, IconFadeDistance, IconMaxDistance, IconOffset, TextColor, TextColor_CB, TextFadeDistance, TextMaxDistance, TextOffset |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json (DICE's partitions, round SPG4, 2026-11-23): the classes above]

### HUD-MENU-1: The menu configuration, movies, tips and the 3D menu scene

Status: **verified** · Variants: PC

`Menu/MenuConfig`'s `MenuConfigAsset` names the front end's movies (intro, multiplayer intro, attract movie and idle
time, game-over and pre-game-over movies, extra movies), the background music, credits, EULAs, the award grid and the
transition configuration; its `UIMovieAsset`s (110: a movie with its texture, music, subtitle movie, language,
looping and special-edition flag) are the Bink videos of BINK-OUT-1 as the menus know them. `GameTipAsset` (2) holds the
loading tips; `UI3DSystemAsset` the main menu's 3D scene (camera and geometry animations); `UIShaderAsset` the UI's
shaders. In a level, `MenuEntityData` (288, campaign levels mostly) opens a menu by id with a header and body string.

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| MenuConfigAsset (GameSharedResources) | 1 | 1 | `menu/menuconfig.dbx` | AttractionIdleTime, AttractionMovie, AwardGrid, BackgroundMusic, Credits, Eulas, ExtraMovies, GameOverMovies, IntroMovie, IntroMovieMultiplayer, IntroMovies, MenuTransitionConfig, PreGameOverMovie, VehicleStatsData |
| UIMovieAsset (UI) | 110 | 24 | `menu/menuconfig.dbx`, `movies/introtransitions`, `movies/uimalogos.dbx` | Description, IsSpecialEdition, Language, Looping, Movie, Music, OverrideBackgroundMusic, SubtitleMovie, Texture |
| GameTipAsset (GameSharedResources) | 2 | 2 | `menu/gametips` | Tips |
| UI3DSystemAsset (UI) | 1 | 1 | `menu/3dsystem` | CameraAnimations, GeometryAnimations, SourceFiles |
| UIShaderAsset (UI) | 1 | 1 | `systems/ui` |  |
| MenuEntityData (GameSharedResources) | 288 | 74 | `levels/` | BodySid, HeaderSid, MenuId |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

## 2. The minimap and markers

### HUD-MAP-1: The minimap: the level's settings, the icon atlas and the zoom states

Status: **verified** · Variants: PC

A level's minimap is `LevelData.MinimapSettings`; its icons are `Menu/Minimap`'s two `UIAnimatedTextureManagerAsset`s
(each builds an output atlas from `Groups`): 526 `UIAnimatedTextureGroup`s (an icon: size, colours, rotation, blink
frames, text anchors, whether it sticks to the border) of 952 `UIAnimatedTextureFrame`s (a texture, its UV rectangle,
timing). The executable's minimap states are `UIMinimapZoomState_None`, `_Default_Nam`, `_Overview`, `_Mission`,
`_CombatArea`, `_Artillery` with shapes `UIMinimapShapeType_Circle`, `_Diamond`, `_Rectangle`, `_Square`; the
menus' externs `enableMinimap`, `hideMinimap`, `setMinimapState`, `zoomMinimap` drive it. A capture point shows on it
when `ShowOnMinimap` is set and draws its `MapMarker` (GAME-CQ).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| UIAnimatedTextureManagerAsset (UI) | 2 | 2 | `menu/minimap` | Groups, OutputAtlasFile |
| UIAnimatedTextureGroup (UI) | 526 | 4 | `menu/minimap` | BlinkFrames, Color, FrameRate, Frames, Height, HorizontalTextAnchor, Icon, IconColor, MaxTime, MaxTimeBlink, Rotation, RotationSpeed, StickOnBorder, VerticalTextAnchor, Width |
| UIAnimatedTextureFrame (UI) | 952 | 4 | `menu/minimap` | Delay, Texture, Time, UVRect |
| UIAnimatedTextureData (UI) | 2 | 2 | `menu/minimap` | Groups |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above; LevelData member MinimapSettings; CapturePointEntityData members MapMarker, ShowOnMinimap]
[exe: BFBC2Game.exe strings UIMinimapZoomState_None ... UIMinimapZoomState_Artillery, UIMinimapShapeType_Circle ... _Square, enableMinimap, hideMinimap, setMinimapState, zoomMinimap, MinimapCameraOffsetData]

### HUD-MARK-1: Map markers, tooltips, counters and objective cameras

Status: **verified** · Variants: PC

`MapMarkerEntityData` (653, almost all in the campaign) is a 3D / minimap marker: its HUD icon, marker type, colour,
radius, the team that sees it, blink time and whether it is a focus point; a `MissionObjectiveEntityData` lists its
markers (`MapMarkers`). A `TooltipEntityData` (44) shows a hint string with an input action; a `HudCounterEntityData`
(1, SP_004_B) a counter; a `MissionObjectiveHudData` (2, SP_006) the objective screen's camera.

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| MapMarkerEntityData (GameSharedResources) | 653 | 74 | `levels/` | BlinkTime, ColorType, Controllable, FocusPointRadius, HudIcon, IsFocusPoint, IsVisible, MarkerType, MultipleTrigger, Radius, ShowAirTargetBox, Sid, StaticModel, VisibleForTeam |
| TooltipEntityData (GameSharedResources) | 44 | 9 | `levels/` | EntryInputAction, TooltipSid |
| HudCounterEntityData (GameSharedResources) | 1 | 1 | `levels/` | CounterType, DecreaseStep, IncreaseStep, MaxValue, MinValue, Position, StartValue |
| MissionObjectiveHudData (GameSharedResources) | 2 | 1 | `levels/` | CameraDistance, CameraFov, CameraLookDistance, CameraPosition, CameraRotation, HorizontalBoundaries, VerticalBoundaries |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above; MissionObjectiveEntityData member MapMarkers]

## 3. Screen effects

### HUD-FX-1: Camera screen effects: CRT views, scope filters, level screen effects

Status: **verified** · Variants: PC

The UAV, TOW and helicopter gunner views draw through `CrtHudFxData` (scan lines, noise, a rolling bar, colour tint,
brightness; 33) and the analog-link variant `CrtAnalogLinkHudFxData` (21); sniper scopes through
`SniperLensScopeFilterData` (blur, chromatic aberration, radial blend; 3) and the laser designator / binoculars
through `ColorTintScopeFilterData` (3). A level fires full-screen effects through `ScreenEffectEntityData` (a frame
effect texture, blend mode, fades; 23) and fades through `ScreenFadeEntityData` (CAMP-FLOW-2). A vehicle's radar
(`RadarSweepComponentData`, 27: radius, pulse delay, which kinds it shows) spots for the minimap; lock-on is
`LockingControllerData` (28: lock and release times, ray length, visible targets only).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| CrtHudFxData (Render) | 33 | 20 | `objects/vehicles`, `objects/weapons` | ColorTint, InterpolatedBrightness, Noise, RollingBar, ScanLines, ScissorRegion, SegmentHeight |
| CrtAnalogLinkHudFxData (Render) | 21 | 21 | `objects/vehicles`, `levels/` | ColorTint, Ghosts, InterpolatedBrightness, Noise, RollingBar, ScanLines, ScissorRegion, SegmentHeight |
| SniperLensScopeFilterData (Render) | 3 | 3 | `objects/weapons` | BlurCenter, BlurScale, ChromaticAberrationColor1, ChromaticAberrationColor2, ChromaticAberrationDisplacement1, ChromaticAberrationDisplacement2, ChromaticAberrationStrengths, ColorTintData, RadialBlendDistanceCoefficients, ScissorRegion |
| ColorTintScopeFilterData (Render) | 3 | 3 | `objects/weapons` | ColorTintData, ScissorRegion |
| ScreenEffectEntityData (GameSharedResources) | 23 | 9 | `levels/` | BlendMode, EndState, FadeInTime, FadeOutTime, FrameEffectTexture, InitialState, TriggerFrameEffect |
| RadarSweepComponentData (GameSharedResources) | 27 | 27 | `objects/vehicles` | OnlyShowEnemies, PulseDelay, Radius, ShowExplosives, ShowSoldiers, ShowVehicles, SoldierVehicleMinSpeed, SpottingSound, TimeToActivate, TimeToShowEnemies |
| LockingControllerData (GameSharedResources) | 28 | 28 | `objects/vehicles`, `objects/weapons` | HoldStillThreshold, LockOnVisibleTargetsOnly, LockOnWorldSpacePos, LockTime, MinimumLockTime, PositionOnly, RayLength, ReleaseOnNewTargetTime, ReleaseTime, SampleRate, ZoomLevelLock |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

## 4. Spectator and kill cam

### HUD-SPEC-1: The spectator is a level entity with camera indices per target kind

Status: **verified** · Variants: PC

Each multiplayer level places `SpectateEntityData` (227 in DICE's levels: 4 - 5 per mode layer): a mode, a target,
whether to target a controllable, its `Cameras` and which of them to use for a soldier, a vehicle, an air vehicle and
a capture point (`SoldierCameraIndex`, `VehicleCameraIndex`, `AirVehicleCameraIndex`, `CapturePointCameraIndex`),
and `UseInHardcoreMode`. The executable has `ClientSpectateEntity` / `ServerSpectateEntity` and the menus' extern
`doSpectate`.

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| SpectateEntityData (GameSharedResources) | 227 | 62 | `levels/` | AirVehicleCameraIndex, Cameras, CapturePointCameraIndex, Mode, ShouldTargetControllable, SoldierCameraIndex, Target, UseInHardcoreMode, VehicleCameraIndex |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: SpectateEntityData 227 instances]
[exe: BFBC2Game.exe strings ClientSpectateEntity, ServerSpectateEntity, doSpectate, MieSpectate]

### HUD-KILL-1: The kill cam has no data class

Status: **observed** · Variants: PC

No DBX class of DICE's partitions and no typeinfo record is a kill camera: the executable names `KillCam` once (a
string), and the death view is the killed soldier's camera turning to the killer, driven by code. Its only data is
the HUD art: the `Menu/MP_Killcam_Textures_Win32` texture atlas (a `UITextureAtlasData`, TEXT-ATL-1) the kill cam's
Flash movie draws from. A tool that wants to change the camera has to patch the executable (not traced).

[data: spec/census.json, 2026-11-23: no class or member name containing KillCam; a UITextureAtlasData in Menu/MP_Killcam_Textures_Win32]
[exe: BFBC2Game.exe one string "KillCam"; no typeinfo record of that name]

## Open questions

- The minimap's settings value type (`LevelData.MinimapSettings`: the typeinfo's `MinimapData`, 80 bytes, and
  `UIMinimapZoomConfig`) member by member.
- The kill cam's code path.
