# Weather: the weather system's data, its 355 states, the switches, weather effects

The data side of BFBC2's weather: the classes of a weather system (system, setup, settings, states and their six
component partitions), the compiled `WeatherSystem` resource, how many systems and states DICE shipped and what kinds
they are, which state a level shows (`ActiveState` and the spawn switch), the value ranges DICE used (the ranges the
weather editor, VW3, holds a state to), weather particles and sounds (rain, snow, lightning), and what the suite's
exports write. How the renderer uses a state (light, sky dome, fog, post-process) is `engine/rendering.md`
(RNDR-WTHR-1 - RNDR-WTHR-9, RNDR-SKY-*, RNDR-POST-*); light itself is `engine/lighting.md`; effect and emitter data is
`engine/effects.md`.

**Corpus.** The level bundles of Project OPFOR BFBC2's game copy (501 bundles, 444 DICE's: dated before 2013), every
`weather/` partition once from the first bundle that holds it, DICE's first, counted by `spec/probes/lighting.py
weather` on 2026-11-22; per-level facts (83 levels: stock MP and campaign, Package, Vietnam, Onslaught, BFBC1, OPFOR's
four WW2 levels) from research round WX0 (2026-11-21, `weather_inventory.json` in the coordinator's scratchpad).
Nothing here was measured in game.

## 1. The classes

### WTH-CLS-1: System, setup, settings, states

Status: **observed** · Variants: PC, BF1943

A weather system is a `Weather.WeatherSystemData` partition: its `States` array (references to
`Weather.WeatherSystemState` partitions) and its `Setup` (a `Weather.WeatherSystemSetup`: the data all states share -
a `StaticCloudLayerData` naming the panoramic sky texture and the cloud alpha, a `StaticFogData`, a
`StaticOutdoorLightData`, the lens-flare atlas). A `Weather.WeatherSystemSettings` partition picks a state
(`ActiveState`, an index into `States`) and holds `StateFadeTimeSeconds`, `InterpolationMode`, `SkydomeEnable`,
`VolumetricCloudsEnable`, `PanoramicCloudBlendMode`, `FogType`. A level names a system and a settings partition through
its `WeatherSystemEntityData` (RNDR-WTHR-5). DICE's data: 28 systems, 28 setups, 28 static cloud layers, 30 settings,
27 static outdoor lights and static fogs (the setups' static Sun / Sky partitions are named but exist in no bundle:
the game runs without them), 24 weather entities as partitions and 3 weather blueprints.

[data: spec/probes/lighting.py weather on the game copy, 2026-11-22: primary classes of the weather/ partitions - WeatherSystemState 355, DynamicOutdoorLightData / DynamicSkyData / DynamicPostProcessData / DynamicSunData / DynamicFogData 353 each, DynamicCloudLayerData 352, DynamicLensFlareData 35, WeatherSystemSettings 30, StaticCloudLayerData / WeatherSystemSetup / WeatherSystemData 28, StaticOutdoorLightData / StaticFogData 27, WeatherSystemEntityData 24, ObjectBlueprint 3, TextureAsset 2, StaticLensFlareData 1]
[code: rust/bc2-render/src/weather.rs:231-265]

### WTH-CLS-2: A state is values and six component partitions, no textures

Status: **observed** · Variants: PC, BF1943

A `WeatherSystemState` holds its own values (`ViewDistance`, `WorldViewDistance`, `ShadowViewDistance`, `Wind`,
`ApplyFog`, LOD scales, fade distances) and references to six component partitions - `Sky`
(`DynamicSkyData`), `Sun` (`DynamicSunData`), `Clouds` (`DynamicCloudLayerData`), `OutdoorLight`
(`DynamicOutdoorLightData`), `Fog` (`DynamicFogData`), `PostProcess` (`Render.DynamicPostProcessData`) - and
optionally a `LensFlare`. No state names a texture: a state's closure is its own partition and its 6 - 7 components
(8 partitions for `ons_mp_002_night`), which is what makes a state cheap to borrow (RNDR-WTHR-8). Two state
partitions lack some components (355 states against 353 of each component class).

[data: spec/probes/lighting.py weather: 355 WeatherSystemState, 353 of each component class, 352 cloud layers]
[data: research round WX0: the closure of ons_mp_002_night, 8 partitions under weather/, no resources]
[test: rust/bc2-levelwrite/tests/vw2.rs::the_night_template_resolves]

### WTH-RES-1: Each system has a 4-byte WeatherSystem resource holding 72

Status: **verified** · Variants: PC

Beside each system's partition is a compiled `WeatherSystem` resource of 4 bytes, the little-endian value 72 in all
28 systems whatever their state count (2 to 53): it does not describe the states. A system the suite's export writes
gets the same resource (RNDR-WTHR-8). The two other resources under `weather/` are textures (ITexture version 3:
524,380 and 8,284 bytes), the two `TextureAsset`s.

[data: spec/probes/lighting.py weather: resources (bytes, first u32) (4, 72) x 28, (524,380, 3) x 1, (8,284, 3) x 1]
[test: rust/bc2-levelwrite/tests/vw2.rs::levels_weather_as_the_export_reads_it]
[code: rust/bc2-levelwrite/src/weather.rs:969-1060]

## 2. DICE's states

### WTH-STATE-1: 28 systems, 355 states, 457 slots

Status: **observed** · Variants: PC

DICE's systems list 457 state slots over 355 distinct state partitions (systems share states: `weather/default/states/default`
is slot 0 of nine systems), 2 to 53 per system. Every system, setting and state is used by some level: there is no
orphan. Systems are shared between a map's mode levels (MP_005's serves MP_005, MP_005GR, MP_005SR, ONS_MP_005 and
ONS_MP_005GR) and OPFOR's `weather/wake_island/weathersystem_wake` (5 states) serves all four WW2 levels, each
choosing its state by its own settings.

[data: spec/probes/lighting.py weather: 28 systems, 457 states listed, 2 .. 53 per system]
[data: research round WX0: every system used by at least one of 83 levels; the shared systems as listed]

### WTH-STATE-2: Most states are scripted; one night, two rain, four snow

Status: **observed** · Variants: PC

By DICE's names and values: cutscene 182, other 59, placeholder 29, win 18 (switched at round end), indoor 15,
effect 13 (flashes, shock waves), day 13, dark by value 7, dawn / dusk 5, snow / cold 4 (SP_04's frostbite
whiteouts), fog 3, fire 3, rain 2 (`r_sp03/afterrain`, `r_sp07/prison_rain`), storm 2 (SP_03's lightning states), night
1 (`r_mp02/ons_mp_002_night`, reachable on MP_002, MP_002SR, ONS_MP_002 and ONS_MP_005GR). There is no sandstorm
state. "Rain" states are darker, flatter, foggier values: no state or shader has a wetness parameter.

[data: research round WX0: classify.py over the 355 states (names and values), the counts as listed]

### WTH-STATE-3: The value ranges of DICE's states

Status: **observed** · Variants: PC

Over DICE's 348 component partitions of each kind (350 states): key light intensity 0 - 10 (3 x 60), top 0 - 3,
bottom 0 - 5, key colour components 0 - 10, top / bottom colours 0 - 1; `KeySpecularScale` 0 - 10;
`ShadowScale` 0.01 - 1; `ReflectionEnvmapScale` 0, 1, 2 or 10; `DiffuseEnvmapScale` 0 - 10; sun `Theta` 0 - 360,
`Phi` 14.7 - 180, `Gain` 0 - 200 (0 in 35: night and whiteout), `Size` 0 - 0.025, colour components up to 10; fog
`Start` 0 or 100 and `End` 100 or 200 (barely used: the look is in the three `LinearPoint`s), fog colour 0 - 0.98;
`MiddleGray` 0.004 - 1, `MinExposure` 0 - 20, `MaxExposure` 0.4 - 20, bloom 0 - 1, brightness 0 - 10, contrast
0.1 - 2, saturation 0 - 1.2; `ViewDistance` 1 - 10,000, `WorldViewDistance` 10 - 20,000, `ShadowViewDistance` -1 -
315 (-1 in 155). `ApplyFog` is false in all 350. These are the ranges a state edited in the suite is held to, with a
warning outside them (VW3).

[data: spec/probes/lighting.py weather on the game copy, 2026-11-22: the ranges as given, over DICE's state and component partitions]

### WTH-STATE-4: Placeholder ActiveStates are dark by value

Status: **observed** · Variants: PC

Several multiplayer settings point `ActiveState` at a placeholder (`default`, `state1`, MP_009's `deafult`) whose
values are dark or unlit (the exposure cannot reach middle grey: MP_005's `default` reaches 0.33) and which is never
seen, because the level's spawn switch moves to another state at once (RNDR-WTHR-6): 70 of 83 levels have an
enabled switch and in 45 it names another state than `ActiveState`. 13 levels have none (MP_008, the MP_012 modes,
SP_006, SP_008, SP_02, Wake Island, Iwo Jima, Guadalcanal ...).

[data: research round WX0: per level ActiveState and spawn switches (inventory_tables), the exposure reach of the placeholder states]

### WTH-STATE-5: Campaign levels switch states by script

Status: **observed** · Variants: PC

Campaign levels place dozens of further `WeatherStateEntityData` (cutscenes, indoor areas, lightning flashes; SP_03
lists 50 or more) switched by their event graphs, and the win states (`*_us_win`, `*_ru_win`) are switched at round
end on MP levels. Lightning (SP_03) is a state switch to `lightning_new` / `river_run_lightning` plus thunder sounds,
not a weather feature.

[data: research round WX0: the other state entities per level (inventory_tables "other state entities")]

## 3. Weather effects and sounds

### WTH-FX-1: Rain and snow are camera-proximity area effects

Status: **observed** · Variants: PC

Precipitation is ordinary placed `GameEffectEntityData`, not a camera-attached system: rain on SP_03 is a grid of
`fx_amb_sp3_rain_area20` (12) and `rainfallground_compound_area` (20), reused on SP_007 (7); snow on MP_007 is
`fx_amb_mp07_snowflakes` (30) and `snowstorm_l` (28), on MP_012 `fx_amb_mp12_snowstorm_l` (48). With
`IsCameraProximityTriggered` true an effect plays only near the camera, so a grid of area emitters over the
playable space reads as weather everywhere. Over the game copy, 3,024 of 13,083 placed effects in 62 of 83 level
partitions are camera-proximity ones (rain, snow, dust and ambient effects together). SP_04 adds 2 snow effects
attached to the chopper (`EntitySpace` and a bone).

[data: research round WX0: the weather effects per level (A.5)]
[data: spec/probes/lighting.py lights on the game copy, 2026-11-22: 13,083 placed GameEffectEntityData in 82 level partitions, 3,024 with IsCameraProximityTriggered in 62]

### WTH-FX-2: Weather sounds are sound areas

Status: **observed** · Variants: PC

Rain, snow, storm and wind sounds are `SoundAreaEntityData` with polygon geometry and a `Sound` partition (MP_007 3
snow areas, MP_012 4, SP_04 14, SP_03 4 rain and 5 thunder); SP_03's rain sound closure is 9 partitions and 6 small
Wave resources across the level and `sp_common` bundles. DICE's night swapped 10 ambience areas for `*_night_*`
sounds (LIGHT-SKY-3). Moving a level's weather sound to another level needs the sound partitions and their Wave data
(SND-OD-1 for on-demand waves).

[data: research round WX0: sound areas per level; the closure sp03_rain_sound]

## 4. What the suite writes

### WTH-SUITE-1: The export sets a state; borrowing appends or swaps

Status: **verified** · Variants: PC

The suite's weather export (round VW2) sets `ActiveState` and the first enabled spawn switch together, in copies
under `weather/<level>/` (RNDR-WTHR-8); another level's state is appended to a copy of the level's system (the level
keeps its sky) or the donor's whole system is swapped in (57 - 61 partitions with the panorama texture); every new
system has its 4-byte resource (WTH-RES-1). Night (RNDR-WTHR-9) and rain (SP_03's `afterrain`, the state only) are
refused unless "Build it anyway" is set; rain and snow effects (WTH-FX-1) and sounds (WTH-FX-2) are not written yet
(VW2d).

[code: rust/bc2-levelwrite/src/weather.rs:546-815]
[test: rust/bc2-levelwrite/tests/vw2.rs::levels_weather_as_the_export_reads_it]
[doc: rust/BENCHMARKS.md:6419-6450]

### WTH-SUITE-2: A borrowed state keeps the target's sky

Status: **inferred** · Variants: PC

A state appended to the target's system draws with the target's Setup: its panoramic sky and cloud textures, static
fog and lens flare, with the donor's light, sky colours, fog and exposure values. So DICE's night appended to MP_005
shows MP_005's day panorama darkened by the night values and the export's lowered panoramic transparency; a night
sky needs the system swap or a texture swap of the static cloud layer (no setup names a night panorama).

[inferred: the state holds no texture (WTH-CLS-2) and the static cloud layer is the setup's (WTH-CLS-1); not seen in game]

## Open questions

- **Which state the engine starts in** (RNDR-WTHR-6): ActiveState, then the spawn switch, or the switch's state at
  once; and which switch wins when several are enabled. A playtest or RE of the state entity's spawn handler.
- **The `WeatherSystem` resource's 72** (WTH-RES-1): what the engine reads it as (a size, a version); harmless while
  every system carries the same value.
- **The two states missing components** (WTH-CLS-2): which, and whether the engine tolerates a null component.
- **A borrowed state's look** (WTH-SUITE-2) and the dark placeholder states (WTH-STATE-4) are unproven in game.
