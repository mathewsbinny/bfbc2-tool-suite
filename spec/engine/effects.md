# Effects: effect blueprints, emitters, emitter atlases, placed and area effects

How BFBC2 describes particle effects: an effect blueprint (`Entity.EffectAsset`) and its entity, the emitters it
spawns (`Emitter.EmitterDocument` and its template), each level's own copies of its emitters and the two emitter
atlases they draw from, placed effects and camera-proximity area effects (the rain and snow grids), emitters that act
as point lights, and what moving an emitter to another level takes (the merged atlas OPFOR builds for Wake Island).
Lights are `engine/lighting.md` (LIGHT-PT-*); weather particles and sounds are `engine/weather.md` (WTH-FX-*);
destruction effects are `engine/destruction.md`; impact effects per material pair are `engine/materials.md`
(MAT-FX-*); the texture header is `formats/texture.md`.

**Corpus.** The level bundles of Project OPFOR BFBC2's game copy (501 bundles, 444 DICE's: dated before 2013), the
partitions whose names hold `fx/` or `emitters/`, each name once with its `levels/<lv>/` prefix taken off (the first
copy, DICE's first, Package before Dist), and the level partitions' placements; counted by `spec/probes/lighting.py
emitters` and `lights` on 2026-11-22. The atlas facts are the suite's `emitteratlas.py` (measured on DICE's atlases)
and OPFOR's Wake Island build (`opfor:research/EMITTER_ATLAS.md`). Nothing here was measured in game.

## 1. The classes

### FX-CLS-1: An effect is an EffectAsset; an emitter is an EmitterDocument

Status: **observed** · Variants: PC, BF1943

An effect partition's primary instance is an `Entity.EffectAsset` whose object is an `EffectEntityData`; its
components are `EmitterEntityData` (each naming an emitter partition through `Emitter`, at its own `Transform` in the
effect) and sound entities. An emitter partition's primary instance is an `Emitter.EmitterDocument` holding one
`EmitterTemplateData` (the emitter's settings: `MaxCount`, `LifeTime`, `Emissive`, the point-light fields of
LIGHT-PT-2 ...) and its processor chain (`Emitter.Update*Data`, `SpawnPositionData`, `SpawnAnimationFrameData` ...).
Distinct partitions over the game copy: 2,882 emitter documents, 998 effect assets, 322 wave assets and 69 sound
templates beside them (the effects' sounds), and a few meshes and blueprints (5 rigid meshes: factory and background
smoke).

[data: spec/probes/lighting.py emitters on the game copy, 2026-11-22: 4,285 partitions; primary classes EmitterDocument 2,882, EffectAsset 998, WaveAsset 322, SoundTemplate 69, RigidMeshAsset 5, ObjectBlueprint 5, MeshShaderSetAsset 3; 2,882 EmitterTemplateData]
[code: rust/bc2-editor/src/overlays.rs:568-600]

### FX-CLS-2: An emitter's spawn volume is its SpawnPositionData's evaluator

Status: **implemented** · Variants: PC

Where an emitter spawns its particles is its `SpawnPositionData`'s `Pre` evaluator: a `SphereEvaluatorData`
(`Radius`, `Scale`, `Pivot`) or a `BoxEvaluatorData` (`Dimensions`, `Pivot`), in the emitter's space (the effect's
transform times the emitter entity's). The editor's Effects overlay draws these volumes.

[code: rust/bc2-editor/src/overlays.rs:582-600]
[test: rust/bc2-editor/src/overlays.rs::shapes_have_their_sizes]

### FX-CLS-3: Effect instance and culling settings

Status: **observed** · Variants: PC

An `EffectEntityData` caps its live instances with `MaxInstanceCount` (1 - 100 over DICE's 998 effects: 10 x 237,
50 x 214, 30 x 172, 20 x 147) and culls by `CullDistance` (0, no distance cull, on 977; 300 m on 11, 350 m on 3).
What the cap does when more placements are alive than it allows is open (LIGHT-PT-7).

[data: spec/probes/lighting.py emitters: MaxInstanceCount and CullDistance over 998 EffectEntityData]

## 2. Emitters belong to a level

### FX-NS-1: Each level keeps its own copy of every emitter it uses

Status: **observed** · Variants: PC

Emitter and effect partitions are named under the level that uses them (`levels/<lv>/fx/...`,
`levels/<lv>/emitters/...`), one copy per level bundle: 28,932 entries under `emitters/` and 72,550 under `fx/` in the
game copy's bundles for 2,882 + 998 distinct emitters and effects. The copies of one emitter are the same document but
for its texture fields (FX-ATL-2), which point into that level's own atlas: SP_03's and SP_007's copies of the rain
streak emitter are the same 11,417 bytes because the two levels' atlases place the image alike.

[data: the bundle entry index of the game copy (research round WX0's entries.json): 28,932 entries with emitters/, 72,550 with fx/; spec/probes/lighting.py emitters: 4,285 distinct names after the level prefix]
[data: research round WX0: em_amb_sp3_rain_streaks, 11,417 bytes in both levels]

## 3. Emitter atlases

### FX-ATL-1: Two atlases per level: L8 alpha and BC5 normals

Status: **verified** · Variants: PC

Every particle of a level is drawn from two per-level textures, `Levels/<level>/emittertexturealpha.res` (format 10,
L8) and `Levels/<level>/emittertexturenormals.res` (format 20, BC5). Alpha: 2048 x 2048 on 64 of the game copy's 83
level atlases, 2048 x 1024 on 18, 1024 x 2048 on 1; normals 1024 x 512 on 74, 1024 x 1024 on 9. DICE's alpha levels
are each the rounded 2 x 2 mean of the level above (`(a + b + c + d + 2) // 4`, exact on every texel of MP_005's and
BC1 Harvest Day's 11 levels), down to a shorter side of 2; empty normal blocks are all zero.

[data: spec/probes/lighting.py emitters on the game copy, 2026-11-22: 83 alpha and 83 normals atlases, (format, width, height) as given]
[code: bfbc2-bf1943-import/emitteratlas.py:16-30]
[test: bfbc2-bf1943-import/tests/test_emitteratlas.py::main]

### FX-ATL-2: An emitter's TextureInfo places its image in the atlases

Status: **verified** · Variants: PC, BF1943

An emitter names its image by the `TextureInfo` of its `Emitter.UpdateTextureCoordsData`: `TextureOriginU` / `V` (the
top left of frame 0 in the alpha atlas, 0 - 1), `AnimationStepU` / `V` (one frame's size; frame f at column
f % `AnimationFrameColumnCount`, row f // it), the `Nm...` equivalents in the normals atlas (origin (0, 0) with step
(1, 1) or (0, 0): no normal map), `AnimationFrameCount`. All frames are drawn when `EnableAnimation` is true or an
enabled `SpawnAnimationFrameData` picks the first frame, else frame 0 only. Nothing else in an emitter names an atlas
position. DICE's atlas images sit in slots on an 8-texel grid, sampled 3 or 6 texels inside the slot.

[code: bfbc2-bf1943-import/emitteratlas.py:16-30] [code: bfbc2-bf1943-import/emitteratlas.py:185-232]
[test: bfbc2-bf1943-import/tests/test_emitteratlas.py::main]

### FX-ATL-3: An emitter moved to another level must be re-pointed into that level's atlas

Status: **verified** · Variants: PC, BF1943

Because the texture fields are per level (FX-NS-1), an emitter copied from level A into level B draws whatever B's
atlas holds at A's coordinates. On OPFOR's Wake Island (shipping MP_005's atlases) 361 of the 441 emitters with an
MP_005 copy pointed elsewhere than that copy, and 168 had no MP_005 copy (wood splinters drawn as blobs, concrete
sparks as glass shards). The fix is a merged atlas pair holding every image the level's emitters were authored with,
the emitters re-pointed into it, proved image for image: OPFOR's Wake build merges 607 emitters from 13 source atlases
into one 2048 x 2048 alpha (82 % used) and one 1024 x 1024 normals atlas; 801 of 801 alpha images equal texel for
texel, 780 of 780 normal images pass (602 exactly, the rest correlating at least 0.9999).

[test: bfbc2-bf1943-import/tests/test_emitteratlas.py::main]
[test: opfor:src/tools/tests/test_emitter_atlas.py::test_install_swaps_the_atlases_and_emitters]
[code: opfor:src/tools/ww2/emitter_atlas.py:136-195]
[doc: opfor:research/EMITTER_ATLAS.md:23-45] [doc: opfor:research/EMITTER_ATLAS.md:132-150]

### FX-ATL-4: The same rule binds every cross-level copy

Status: **inferred** · Variants: PC

Any tool that places another level's effect - the lamp tool (VW2c, DICE's night lamp from SP_02 / ONS_MP_005GR), rain
and snow grids (VW2d, SP_03 / MP_007 donors), E2b's "+bundle" placements - must carry the effects, the emitters under
the target's `levels/<lv>/` names, and the images into the target's atlas (merged as in FX-ATL-3, or appended to the
target's atlas where it has room), and register the new partitions in the level's bundle manifest on server and client
(REG-NEW-1).

[inferred: FX-NS-1 and FX-ATL-3 hold for every emitter, not only the WW2 ports; the map export's cross-level copy (E3d) moves emitters but no export yet merges atlases]

## 4. Placed effects

### FX-PLACE-1: Effects are placed with GameEffectEntityData

Status: **observed** · Variants: PC

A level places an effect with a `GameEffectEntityData` (in its level partition or a sub-blueprint) naming the effect
blueprint: 13,083 placements in 82 of the game copy's 83 level partitions. `IsCameraProximityTriggered` true (3,024 in
62 levels: rain and snow grids, dust, ambient smoke and insects) makes the effect play only while the camera is near
it; false (DICE's night lamps) plays it everywhere.

[data: spec/probes/lighting.py lights on the game copy, 2026-11-22: 13,083 placed GameEffectEntityData in 82 level partitions; 3,024 camera-proximity in 62]

### FX-PLACE-2: A weather grid is many camera-proximity area effects

Status: **inferred** · Variants: PC

DICE covers a playable area with rain or snow by a grid of camera-proximity area effects (SP_03: 32 rain placements;
MP_012: 48 snowstorms; WTH-FX-1). A generated grid (VW2d) over a level's combat area should keep DICE's spacing per
donor effect (the donor's spawn-volume size, FX-CLS-2) and its count within DICE's (up to 48 for one effect on a
multiplayer level) until a playtest measures the cost.

[inferred: DICE's grids measured by WX0; the cost of a denser grid is unknown]

### FX-PLACE-3: Emitters can act as point lights

Status: **observed** · Variants: PC

35 of the 2,882 emitter templates act as point lights (LIGHT-PT-2): 25 light-source effects (`fx/misc/lightsource/`),
3 muzzle flashes, detonations and ambient fires; 8 of the 35 are `Emissive`. `MaxCount` (live particles) on them is 1 -
201 (6 x 22).

[data: spec/probes/lighting.py emitters on the game copy, 2026-11-22: 35 point-light templates by folder; Emissive true x 8; MaxCount 1 .. 201]

## 5. The processor chain

### FX-PROC-1: One linked chain per emitter, spawn processors first, from SpawnRateData

Status: **verified** · Variants: PC

An emitter's behaviour is a linked list of processors (`Emitter.*Data`, each naming the next through
`NextProcessor`). The template's `RootProcessor` is a `SpawnRateData` in every DICE emitter; the chain runs through the
spawn processors (`SpawnPositionData`, `SpawnSizeData`, `SpawnDirectionData`, `SpawnSpeedData`,
`SpawnRotationData`, `SpawnRotationSpeedData`, `SpawnAnimationData`, `SpawnAnimationFrameData`) and then the update
and force processors (`UpdateAgeData`, size, colour, transparency, alpha levels, texture coordinates, rotation,
orientation, linear velocity, clip scale, `GravityData`, `AirResistanceData`, `TurbulanceData`, `LocalForceData`),
every spawn processor before every update processor (55,904 of 55,904 chains that have both). The document's own
`RootProcessor` points into the same chain (at its first update processor, `UpdateAgeData`, on 41,148 documents; the
whole chain on 3,095); 1,216 start from a `BaseEmitterData` outside it. Template chains are 6 - 26 processors long
(most 18 - 21). `SpawnRateData`, `UpdateAgeData` and `UpdateBoundingBoxData` are on every emitter.

[test: core/tests/test_spec_probes.py::emitters]
[data: spec/probes/effects_chain.py, 2026-11-22: 50,141 emitter documents (the fx partitions of DICE's bundles, each per-level name once); template root SpawnRateData on 50,141; the document chain the same chain 3,095, inside the template chain 45,830, other 1,216; 31 processor classes; spawn before update on 55,904 of 55,904]

### FX-PROC-2: A processor is its class, a type tag, an enable flag and Pre / Post evaluators

Status: **verified** · Variants: PC

Every processor carries `ProcessorType` = `Pt` + its class name without `Data` (966,139 of 966,139), `Enable` (false
on 4,228: 2,694 turbulences, 1,182 base emitters, 303 rotations ...), `Deprecated` (never true), `EvaluatorInput`
(`EfNormTime`, the particle's normalised age, on all but 40, which take `EfVelocity`) and `DefaultValues`. Its value
over the particle's life comes from its `Pre` evaluator: `PolynomialData` (189,165), `RandomEvaluatorData`
(180,318), `RotateVectorData` (28,953), `PolynomialColorInterpData` (24,111), `BoxEvaluatorData` (18,994),
`RandomXYZEvaluatorData` (17,733), `SphereEvaluatorData` (10,679: the spawn volumes of FX-CLS-2), `SampleTextureData`
(15); no processor uses `Post`. A particle previewer therefore needs these 31 processors and 8 evaluators.

[test: core/tests/test_spec_probes.py::emitters]
[data: spec/probes/effects_chain.py, 2026-11-22: the counts as listed over 966,139 chained processors]

### FX-PROC-3: What an emitter draws

Status: **observed** · Variants: PC

The template's `EmittableType` is `ScreenAlignedQuad` (30,443 emitters), `DirectionAlignedQuad` (7,830),
`ParticleMesh` (7,339: a `Mesh`, with `CompositeMeshPartNames`) or `Quad` (4,529).

[data: spec/probes/effects_chain.py, 2026-11-22: EmittableType over 50,141 templates]

## Open questions

- **The effect instance cap** (FX-CLS-3, LIGHT-PT-7).
- **Appending images to a target's own atlas** (FX-ATL-4) instead of a merge: room in DICE's atlases is not measured
  (the merged Wake atlas is 82 % full).
- **The processor chain's semantics** (FX-PROC-1, FX-PROC-2): the chain's structure and classes are counted; what each
  processor computes from its evaluator (the polynomial's form, the random range) is not traced in the executable.
- **The 1,092 `BaseEmitterData` outside any chain** and the documents whose root starts there.
- **Cost** of camera-proximity grids and of many lights: a playtest.
