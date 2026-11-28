# Lighting: baked data, the outdoor light, reflection cubes, point lights, sky props

What lights a BFBC2 frame and where each part of it comes from: what DICE baked (the terrain's colour / occlusion
map, the objects' occlusion and sky-occlusion textures, irradiance volumes, the level's reflection cube), what the
engine computes every frame from the weather state (the sun or moon, the sky's hemisphere light, shadows), and the
dynamic point lights (emitters that act as point lights, and `WorldRender.PointLightEntityData`); then the sky props
(moon, god rays) and DICE's rules for a night. The weather state's data is `engine/weather.md`; how the renderer
draws (databases, draws, sky dome, post-process) is `engine/rendering.md` (RNDR-WTHR-*, RNDR-LIGHT-1, RNDR-SKY-*,
RNDR-POST-*, RNDR-SHAD-1); effects and emitters as such are `engine/effects.md`.

**Corpus.** The level bundles of Project OPFOR BFBC2's game copy (`game/**/levels/*/*.fbrb`: 501 bundles, 444 of them
DICE's - dated before 2013 as the install leaves them), each partition once from the first bundle that holds it,
DICE's first and Package before Dist; counted by `spec/probes/lighting.py` on 2026-11-22 unless a rule says
otherwise. Research rounds WX0 (weather and baked lighting, 83 levels, DICE's Dx11 databases disassembled) and WX1
(light fixtures, DICE's day / night pair: PC MP_002 against the Xbox 360 Onslaught DLC's ONS_MP_002 partition, which
OPFOR keeps unpacked under `work\xbox_dlc`) measured the shader-side facts; their reports were not kept (a session
scratchpad, gone by round KB2: `bfo:docs/research/rounds/README.md`), so their findings are cited as data with the round named. Nothing here was measured
in game.

## 1. What is baked

### LIGHT-BAKE-1: Nothing DICE baked depends on the sun

Status: **observed** · Variants: PC

Every baked input a Dx11 shader reads is independent of the weather state's sun: the terrain map's red channel fits
sky visibility and slope, not a shadow from any sun direction; `_SkyOc` is grey and multiplies only the sky term;
irradiance volumes hold one scalar per voxel; and per-asset data cannot know a level's sun (the same building stands
rotated in many levels). Direct light, shadows (three cascades, RNDR-SHAD-1), hemisphere ambient, fog, sky, exposure
and point lights are computed per frame. So a level drawn under another state (a night, RNDR-WTHR-9) needs no
re-bake; the one day-coloured leftover is the reflection cube (LIGHT-ENV-2).

[data: research round WX0 (2026-11-21): MP_005's terrain map R against computed cast-shadow visibility for every azimuth (6 degree steps) and elevation: a flat azimuth profile, best correlation 0.36; the 51 Dx11 terrain shaders tallied sample R and B only; `_SkyOc` channel difference 0.01]
[inferred: a per-asset bake shared by every level that places the asset rotated cannot encode one level's sun]

### LIGHT-BAKE-2: The terrain's colour / occlusion map multiplies the albedo

Status: **observed** · Variants: PC

A level's terrain carries one colour / occlusion map (`mp05_AmbientOcclusion`, `MP008_Occlusion`,
`mp01_ambientocclusion`, `SP04_Occlusion` ...: 2048 texels over 2048 m, DXT1 or L8), sampled by the patch shaders at
`world / 2048 + 0.5`. Its red channel overlays the albedo, which then multiplies both the sun and the ambient term
(`(hemisphere + key x N.L x shadow) x albedo'`); the blue channel is a layer mask; MP_005's green channel holds a
directional lobe that no Dx11 terrain shader reads. The red channel is largely painted: the best linear fit of
computed sky visibility and slope explains 36 % of MP_005's (RMS 22 of 255), so a re-bake cannot reproduce it texel
for texel (the E5 paint layer's ratio method keeps DICE's painting: LIGHT-BAKE-6).

[data: research round WX0: 12 or more levels' maps, 1-4 MB each; 39 patch shaders on MP_005 sample R and B; MP_008's albedo scaled 0.556 .. 0.983 by R; the fit in WX0's ao_model.py]

### LIGHT-BAKE-3: Objects carry occlusion textures by DICE's suffixes

Status: **verified** · Variants: PC

Objects' baked lighting is per asset, shared by every level that places the asset, and named by suffix: `_Oc` (red:
ambient occlusion, multiplying diffuse and key light alike; green: a material mask, MAT-TEX-3), `_OcRGB`,
`_Occlusion`, `_AmbientOcclusion` and `_AO` (occlusion), `_SkyOc` (sky visibility in the mesh's second UV set,
multiplying only the hemisphere ambient term through `texture_OcclusionTexture`). Distinct texture names by suffix
over the game copy: `_Oc` 65, `_OcRGB` 43, `_Occlusion` 31, `_AO` 20, `_SkyOc` 14, `_Envmap` 13 (reflection cubes,
LIGHT-ENV-1), `_AmbientOcclusion` 6, `_Lightmap` 1, `_LightmapColor` 1 (LIGHT-BAKE-5).

[test: core/tests/test_spec_probes.py::spg_lighting]
[data: spec/probes/lighting.py baked on the game copy, 2026-11-22: distinct ITexture names by suffix as listed]
[data: research round WX0: 267 `_Oc` shader-parameter uses (70 MB of texture), 44 `_SkyOc` uses, 256-512 texels, DXT1; MP_005's 587 textures by suffix]

### LIGHT-BAKE-4: There are no classic lightmaps and no Enlighten data

Status: **observed** · Variants: PC

No level ships a per-level lightmap. The shader databases hold Enlighten's entries
(`Systems/Enlighten/DirectIllumination`, `DiffuseDuster`) and every weather state has `EnlightenSkyColor` fields,
but no Enlighten data ships: the fields are dormant. See RNDR-LIGHT-1.

[data: research round WX0: no Enlighten resource in any bundle; MP_005's EnlightenSkyColorIntensity 0; the fields vary over the 355 states (21 colours) without anything that reads them]

### LIGHT-BAKE-5: Interior lightmaps are multipliers, not added light

Status: **observed** · Variants: PC

The two lightmap textures (`SubPen_Lightmap`, L8 1024 texels, with `SubPen_LightmapColor`; SP_01's submarine pen,
also placed on MP_008; SP_006's `Ship_wreck/LightMap`) multiply the already lit result (`2 x L8 x colour x 3.69`,
with a time-animated UV wobble): they darken and tint, they add no light, so they need no change for a night.

[data: research round WX0: the Sub_Pen pixel shaders disassembled]
[data: spec/probes/lighting.py baked: one `_Lightmap` and one `_LightmapColor` texture name in the game copy]

### LIGHT-BAKE-6: A re-bake keeps DICE's painting (the suite's rule)

Status: **inferred** · Variants: PC

When a level's terrain is edited (E5) or a new asset arrives without occlusion data, the suite re-bakes only what the
edit invalidates and keeps what DICE painted: the terrain map's red channel is scaled by `new / old_computed` sky
visibility (unedited texels stay byte for byte DICE's), green and blue are kept; a new asset's `_SkyOc` and
irradiance volume are computed (cosine-weighted sky visibility per texel or voxel) and proved against DICE's own
assets first (`IND_Warehouse_SkyOc`, `Barn_SkyOc`, `IND_VehicleShed_SkyOc`: mean error at most 6 of 255 after DXT1;
irradiance volumes: 90 % of voxels within 8 of DICE's byte).

[inferred: WX0's re-bake design (B.4, B.5); nothing implements it yet; the ROADMAP's E5 row folds it into terrain paint]

## 2. Irradiance volumes

### LIGHT-IRR-1: An IrradianceVolume is a 36-byte header and one byte per voxel

Status: **verified** · Variants: PC

An `IrradianceVolume` resource (`<mesh>_Irradiance.res`) is a little-endian header - the box's minimum and maximum
(6 x f32), then the voxel counts nx, ny, nz (3 x u32) - followed by nx x ny x nz bytes, nothing else. Voxels are
about 0.5 m apart (extent / (n - 1) 0.506 - 1.49 m, most within 0.52 m; the larger steps are thin axes of small
volumes).

[test: core/tests/test_spec_probes.py::spg_lighting]
[data: spec/probes/lighting.py baked on the game copy, 2026-11-22: 25 distinct IrradianceVolume resources, 25 of 25 exactly 36 + nx x ny x nz bytes; 8 .. 612,256 voxels a volume; cell size 0.506 .. 1.489 m]

### LIGHT-IRR-2: The voxels are scalar sky visibility, 51 .. 255

Status: **verified** · Variants: PC

Each voxel byte is a sky visibility from 51 (0.2, the floor: inside walls) to 255 (open sky); no colour and no
direction. The 25 volumes belong to buildings (houses, factories, hangars, the lighthouse) and are shared by every
level that places the building.

[test: core/tests/test_spec_probes.py::spg_lighting]
[data: spec/probes/lighting.py baked: 1,703,118 voxels, minimum 51 (152,495 voxels), maximum 255 (979,692 voxels)]
[data: research round WX0: the 25 volumes' owners]

### LIGHT-IRR-3: Objects inside a volume read it as the hemisphere visibility

Status: **inferred** · Variants: PC

The engine samples a building's volume at an object's position and passes it as
`outdoorLightKeySpecularColorAndHemisphereVisibility.w` (engine parameter 33): object shaders scale the hemisphere
ambient and the reflection cube by it. Which objects read which volume (the building itself, objects and soldiers
inside its box) is the engine's.

[inferred: no shader samples a volume texture; the only per-draw scalar visibility input is the .w of parameter 33, which the object pixel shaders multiply into the ambient and envmap terms (WX0's disassembly)]
[code: bfbc2-shader-tools/pcbuild.py:29-35]

## 3. Reflection cubes

### LIGHT-ENV-1: One StaticEnvmapEntityData per level names its reflection cube

Status: **verified** · Variants: PC

A level's reflection cube is a `WorldRender.StaticEnvmapEntityData` in its level partition: `StaticEnvmap` names a
cube texture (`mp05_envmap`, `mp02_envmap`; 64 KB) captured from the day sky; `InfluenceRadiusInner` /
`InfluenceRadiusOuter` set its reach (5,000 / 10,000 m on most levels; 20 / 25 m for local cubes). 73 of the 83
level partitions have one (70 one, 3 three); 7 of the 79 name no texture. The engine binds it as
`outdoorLightReflectionEnvmap`. The suite's game-accurate view reads it from the placement.

[test: core/tests/test_spec_probes.py::spg_lighting]
[data: spec/probes/lighting.py lights on the game copy, 2026-11-22: 79 StaticEnvmapEntityData in 73 of 83 level partitions; InfluenceRadiusInner 5,000 x 62, 20 x 6, 20,000 x 4, 10,000 x 3; InfluenceRadiusOuter 10,000 x 66, 25 x 6, 30,000 x 4, 20,000 x 2; StaticEnvmap null x 7]
[code: rust/bc2-render/src/placement.rs:117-118]

### LIGHT-ENV-2: The cube is sampled at level 5, faded by the key light and the state's scale

Status: **observed** · Variants: PC

Object shaders sample `outdoorLightReflectionEnvmap` at mip level 5, times the hemisphere visibility (LIGHT-IRR-3)
and the key specular term, so the cube fades with the key light; the weather state's `ReflectionEnvmapScale`
(values over DICE's states: 1 x 321, 0 x 10, 2 x 10, 10 x 7) scales it. DICE's night reuses the day cube
(`mp02_envmap`) with the night state's values; the suite's night export scales `ReflectionEnvmapScale` down instead of
re-rendering a cube (RNDR-WTHR-9).

[data: research round WX0: the Dx11 object shaders' envmap sample disassembled; WX1: the Xbox ONS_MP_002 places a StaticEnvmapEntityData with the same `mp02_envmap`]
[data: spec/probes/lighting.py weather: ReflectionEnvmapScale over DICE's 348 outdoor-light partitions]
[code: rust/bc2-levelwrite/src/weather.rs:856-890]

## 4. The outdoor light

### LIGHT-OUT-1: The outdoor light reaches the shaders as engine parameters 30 - 37

Status: **implemented** · Variants: PC, BF1943

The sun or moon (`outdoorLightDir`, `outdoorLightKeyColor`), the hemisphere ambient (`outdoorLightHemisphereDir`,
`outdoorLightTopColor`, `outdoorLightBottomColor`), the key specular colour with the hemisphere visibility, and the
shadow transforms (parameter 36, indexed by cascade) are engine parameters 30 - 37 of a program's binding table,
filled per frame from the active weather state (RNDR-WTHR-1, RNDR-WTHR-2) and the engine's sun direction
(RNDR-WTHR-4). The suite's shader translation binds BF1943's programs to the same ids.

[code: bfbc2-shader-tools/pcbuild.py:29-35]
[data: BFBC2's Dx11 databases: the binding tables' engine parameter ids, decoded in OPFOR's Xenos translation (2026-11-07)]

### LIGHT-OUT-2: Hemisphere ambient by the normal, key light by N.L and the shadow

Status: **observed** · Variants: PC

A lit pixel is `(hemisphere(N) x skyOcclusion + key x N.L x shadow) x albedo x occlusion`, plus the specular and
envmap terms: the hemisphere lerps the bottom colour to the top colour by the normal against
`outdoorLightHemisphereDir` (scaled by the state's `HemisphereLightAngleFactor`), the key light is the state's key
colour times its intensity, shadowed by the three cascades scaled by `ShadowScale` (0.01 - 1 over DICE's states). The
suite's game-accurate view reproduces this for the weather state it draws.

[data: research round WX0: the Dx11 object and terrain pixel shaders disassembled]
[data: spec/probes/lighting.py weather: ShadowScale 0.01 .. 1 over DICE's 348 outdoor-light partitions]
[code: rust/bc2-render/src/weather.rs:315-357]

## 5. Point lights

### LIGHT-PT-1: One point light per draw

Status: **observed** · Variants: PC

Dx11 programs carry a single point-light permutation (`pointLight0`: engine parameters 27 - 29, position, colour,
radius); no program reflects a second light. On MP_005, 1,648 of 1,714 database entries have it, terrain patches,
vegetation, characters and the translated WW2 objects included. Two light pools overlapping on one draw therefore do
not add: the engine picks one, by a rule not yet known (LIGHT-PT-8). DICE spaced its night lamps 10 - 30 m apart at
radius 10 m.

[data: research round WX0: MP_005's Dx11 database, 1,648 of 1,714 entries with the pointLight0 permutation, none with a second light; WX1: the permutation on 40 of MP_002's 68 terrain patches and on the WW2 entries]
[code: bfbc2-shader-tools/pcbuild.py:29-35]

### LIGHT-PT-2: An emitter acts as a point light through its template

Status: **verified** · Variants: PC

A dynamic light is usually an emitter: its `Emitter.EmitterTemplateData` sets `ActAsPointLight` true and
`PointLightColor`, `PointLightRadius`, `PointLightIntensity`, `PointLightPivot` (an offset from the emitter),
`PointLightFadeStartDistance` / `PointLightFadeEndDistance` (the camera distance over which the light fades out),
`PointLightRandomIntensityMin` / `Max` (flicker), `PointLightMinClamp` / `MaxClamp`. 35 of the 2,882 emitter
templates do (25 under `fx/misc/lightsource/`, the rest muzzle flashes, detonations and ambient fires): radius 1 -
300 m (10 x 12, 20 x 7), fade end 5 - 100 m (30 x 14), random intensity 0.1 - 20, clamps always 0 and 1;
`PointLightColor` is HDR (components up to 200) and `PointLightIntensity` a vector (0 .. 26 per component). The editor's
Lights overlay draws each as a sphere of `PointLightRadius` at the emitter plus its pivot, in its colour.

[data: spec/probes/lighting.py emitters on the game copy, 2026-11-22: 2,882 EmitterTemplateData, 35 with ActAsPointLight, the field ranges as given]
[code: rust/bc2-editor/src/overlays.rs:568-581]
[test: rust/bc2-editor/tests/view_toggles.rs::sp_01_lights_collision_states_walk_and_modes]

### LIGHT-PT-3: DICE's night lamp: radius 10 m, intensity 26, cut at 100 m

Status: **observed** · Variants: Xbox 360

DICE's one multiplayer night (ONS_MP_002 on the Xbox 360) lights its street lamps with
`fx/misc/lightsource/fx_lightsource_sp02_cs_14_01_fire` (an SP_02 cutscene light, re-saved for the DLC): emitter
`em_sp02_cs_14_01_fire_p`, `ActAsPointLight` true, colour (0.996, 0.354, 0.004), radius 10 m, intensity 26, fade start
= end = 100 m (a hard cut), pivot 0, `MaxCount` 2, 0.1 m flame quads (effectively invisible); the effect's
`MaxInstanceCount` 30, `CullDistance` 0. The PC copies are SP_02's and ONS_MP_005GR's; the game copy's Package
ONS_MP_002 lacks all 53 lamp lights. That level is OPFOR's console port (its stage markers, `gamemodes/onslaught`),
not Napisal's as WX1 wrote: Napisal's backed-up port carries the 53 lamp effects; the console port's first stage drops
them (ONS-PORT-3).

[data: research round WX1 (2026-11-21): the Xbox ONS_MP_002 partition under OPFOR's work\xbox_dlc\ons_all\x_002, its 53 GameEffectEntityData named Lightpost<n>; the Package port's level partition: none]
[data: spec/probes/lighting.py emitters: one template of colour (0.996, 0.354, 0.004) among the 35 point-light templates]
[data: round SPG2, 2026-11-22: the game copy's Package ONS_MP_002 level partition (2,861 instances): 0 lamp effects, 2 GameEffectEntityData, OPFOR's stage markers beside it; Napisal's backed-up ONS_MP_002 (4,320 instances): 53 references to the lamp effect]

### LIGHT-PT-4: DICE places a lamp's light 2.4 m below and 1.6 m in front of its bulb

Status: **observed** · Variants: Xbox 360

Each of the 53 lights is a separate placed `GameEffectEntityData` beside its post (no fixture blueprint in 15 levels
carries its own light): 22 of 53 at exactly (0.08, 3.98, 2.04) m in the post's frame (right, up, forward), the rest
within about 1 m of it, while the post's self-lit part's centroid is (0.05, 6.37, 0.42): DICE lit the street, not
the post. `IsCameraProximityTriggered` is false on all of them, so every lamp plays everywhere.

[data: research round WX1: lamp_offsets.py and bulbpos.py over the Xbox ONS_MP_002 placements and the lamppost mesh's LOD0]

### LIGHT-PT-5: WorldRender.PointLightEntityData exists, on the Vietnam levels only

Status: **verified** · Variants: PC

A point light can also be a `WorldRender.PointLightEntityData` placed in the level partition: `Color`, `Intensity`
(1 x 60, 0.5 x 8), `Radius` 4 - 20 m (5 x 20, 8 x 20, 10 x 16), `FadeStartDistance` 5 - 25 m, `FadeEndDistance` 10 -
50 m, `DiffuseEnable` true, `SpecularEnable` false, `Shadowmap` (true on 16) with `ShadowmapResolution` 256,
`Shadowbuffer` false with `ShadowbufferResolution` 512 and `ShadowbufferOffset` 0.2, `Visible` true. DICE placed 68,
all in the Vietnam levels (NAM_MP_002 2, NAM_MP_003 10, NAM_MP_005 5, in each of their mode levels), as fill lights in
trenches and tunnels. No other level has one, and the editor's Lights overlay does not show them yet (VW2e-2).

[test: core/tests/test_spec_probes.py::spg_lighting]
[data: spec/probes/lighting.py lights on the game copy, 2026-11-22: 68 PointLightEntityData in 12 of 83 level partitions, the field values as given]

### LIGHT-PT-6: Lamp glass is self-lit and glows in every state

Status: **observed** · Variants: PC

A BFBC2 lamp's glass is a self-lit material: a 4-instruction pixel shader, `o.rgb = tex(Light_Atlas_001) x 15`,
`o.a = 1`, reading no light constant. It glows by day and by night alike, so a night needs the point light
(LIGHT-PT-2), not a new material, wherever the fixture has such a slot. DICE's off switch is a mesh shader set chosen
per placement (`lamppost_wood_tobanger_lightsoff`, an ordinary lit material, on all 21 of SP_01's posts). The
recognition rule: a mesh slot whose Dx11 entry reads no `outdoorLight*` constant and samples
`Textures/_ATLAS/Light_Atlas_001` or has a light-like shader name.

[data: research round WX1: shaderclass.py over 15 levels' Dx11 databases; the lamppost's three MeshShaderSetAssets]

### LIGHT-PT-7: The effect's instance cap against 53 lamps is open

Status: **inferred** · Variants: PC, Xbox 360

DICE's night effect has `MaxInstanceCount` 30 but 53 placements on ONS_MP_002. Either the engine plays only 30 (the
nearest, or the first spawned) or the cap counts something else (per spawn batch). Until the executable says, the
suite splits light effects into clones of at most 30 placements each (VW2e-6). Over DICE's 998 effects
`MaxInstanceCount` is 1 - 100 (10 x 237, 50 x 214, 30 x 172, 20 x 147).

[inferred: the data contradicts a hard per-effect cap only if all 53 lights are seen; no capture or RE yet]
[data: spec/probes/lighting.py emitters: MaxInstanceCount over 998 EffectEntityData]

### LIGHT-PT-8: Which light wins on a draw is open

Status: **inferred** · Variants: PC

With one point-light slot per draw (LIGHT-PT-1) the engine must choose among the lights reaching an object; the rule
(nearest, brightest, first registered) is not known. Objects longer than DICE's lamp spacing (terrain patches 16 m,
LIGHT-PT-1) can show a pool cut at a patch edge.

[inferred: the single permutation forces a choice; to be settled by RE of the light assignment (RE-a in WX1's plan) or a capture]

### LIGHT-PT-9: The suite's views draw no point light

Status: **implemented** · Variants: PC

The game-accurate view fills the point-light constants with an out-of-reach placeholder (position 1,000 m below the
origin, colour 0, radius 0.001), so no draw is lit by a point light; the Lights overlay shows where lights are. A night
preview therefore shows lamp glass glowing (LIGHT-PT-6) without light pools.

[code: rust/bc2-render/src/weather.rs:523-525] [code: bfbc2-shader-tools/renderqa.py:121-122]

## 6. Sky props and DICE's night rules

### LIGHT-SKY-1: The moon is a mesh prop, MP_008's

Status: **verified** · Variants: PC, Xbox 360

BFBC2 draws no moon from the weather state (the sun disc is switched off with `Sun.Gain` 0 at night: 35 of DICE's 348
sun partitions). The moon is a placed mesh prop, `terrains/mp_008/props/moon/moon`: placed on PC only in MP_008 and
MP_008SDM, and reused by DICE in the Xbox ONS_MP_002 night at (-1851, 2166, -1662), about 3.3 km from the map's
centre.

[test: core/tests/test_spec_probes.py::spg_lighting]
[data: spec/probes/lighting.py lights on the game copy, 2026-11-22: moon placements in mp_008 and mp_008sdm only]
[data: spec/probes/lighting.py weather: DynamicSunData Gain 0 x 35 of 348]
[data: research round WX1: the Xbox ONS_MP_002 placement]

### LIGHT-SKY-2: God rays are props, removed for a night

Status: **verified** · Variants: PC, Xbox 360

Daylight shafts are placed god-ray meshes (`sp11_godrays` and kin): 40 placements in 7 level partitions of the game
copy. DICE's night removed MP_002's 4.

[test: core/tests/test_spec_probes.py::spg_lighting]
[data: spec/probes/lighting.py lights: 40 god-ray placements in 7 levels]
[data: research round WX1: the Xbox ONS_MP_002 partition lacks MP_002's 4 sp11_godrays]

### LIGHT-SKY-3: DICE's night recipe is data only

Status: **observed** · Variants: Xbox 360

DICE made ONS_MP_002's night without new meshes, materials, LOD or culling changes: the weather state switched on
spawn (`ons_mp_002_night`, RNDR-WTHR-9), 51 more of the same wooden lamppost (53 in all) each with one point-light
effect (LIGHT-PT-3, LIGHT-PT-4), MP_008's moon added (LIGHT-SKY-1), the god rays removed (LIGHT-SKY-2), 10 ambience
sound areas re-pointed to `*_night_*` sounds, a reflection cube with the day texture (LIGHT-ENV-2), the lighthouse
swapped for SP_006's `lighthousenoentry` (likely gameplay). The 6 wall lamps (`outdoorlamp_t02v01_br`, no emissive
slot) stayed dark. PC MP_002 and the Xbox MP_002 match placement for placement (2,981 of 2,981), so the Xbox
partition is the template for the PC.

[data: research round WX1: diff_pair.py over the Xbox MP_002 and ONS_MP_002 partitions (1,460 matched, 82 swapped, 1,439 removed, 645 added; the night's share as listed)]

### LIGHT-SKY-4: BF1943 shipped no night and no light

Status: **observed** · Variants: BF1943

No WW2 level has a night state, a light effect or a point light; BF1943's `lamppost_01` "light" slot is translated as
an ordinary sun-lit surface and the bomb lamp's bulb reflects a fixed day-sky cube (`Sky_Beach_Generic_RGBA16f`).
A WW2 night needs a built self-lit entry and shader set per fixture kind (lamppost 92 on Wake, 130 on Guadalcanal, 74
on Iwo Jima; the bomb lamp; Iwo Jima's lighthouse) and point lights.

[data: research round WX1: inventory.py over the four WW2 levels; spec/probes/lighting.py lights: no PointLightEntityData and no camera-proximity or light effect beyond Wake's 21 placed effects]

## Open questions

- **The light chosen per draw** (LIGHT-PT-8) and **the effect instance cap** (LIGHT-PT-7): RE of the engine's light
  assignment and of `MaxInstanceCount`, or a RenderDoc capture of a night with dense lamps.
- **PointLightEntityData outside the Vietnam levels** (LIGHT-PT-5): registered engine-wide (`WorldRender`), but only
  seen in Vietnam levels; a playtest on another level, and the meaning of `Shadowmap` true on 16 of them.
- **Irradiance volume sampling** (LIGHT-IRR-3): which objects read which volume; RE of the volume lookup or a capture
  of parameter 33's .w inside and outside a building.
- **The `_Oc` green channel** (LIGHT-BAKE-3, MAT-TEX-3): one pixel shader that reads it, disassembled.
- **The re-bake** (LIGHT-BAKE-6): only a design; proved when E5 implements it against DICE's `_SkyOc` and volumes.
- In game, nothing here is proved: light pools on terrain and objects, the fade, the moon's look, the self-lit glass
  under a night exposure.
