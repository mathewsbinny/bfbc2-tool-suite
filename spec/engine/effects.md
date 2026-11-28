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
and OPFOR's Wake Island build (`bfo:docs/research/opfor/EMITTER_ATLAS.md`). Nothing here was measured in game.

## 1. The classes

### FX-CLS-1: An effect is an EffectAsset; an emitter is an EmitterDocument

Status: **verified** · Variants: PC, BF1943

An effect partition's primary instance is an `Entity.EffectAsset` whose object is an `EffectEntityData`; its
components are `EmitterEntityData` (each naming an emitter partition through `Emitter`, at its own `Transform` in the
effect) and sound entities. An emitter partition's primary instance is an `Emitter.EmitterDocument` holding one
`EmitterTemplateData` (the emitter's settings: `MaxCount`, `LifeTime`, `Emissive`, the point-light fields of
LIGHT-PT-2 ...) and its processor chain (`Emitter.Update*Data`, `SpawnPositionData`, `SpawnAnimationFrameData` ...).
Distinct partitions over the game copy: 2,882 emitter documents, 998 effect assets, 322 wave assets and 69 sound
templates beside them (the effects' sounds), and a few meshes and blueprints (5 rigid meshes: factory and background
smoke).

[test: core/tests/test_spec_probes.py::spg_lighting]
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

Status: **verified** · Variants: PC

An `EffectEntityData` caps its live instances with `MaxInstanceCount` (1 - 100 over DICE's 998 effects: 10 x 237,
50 x 214, 30 x 172, 20 x 147) and culls by `CullDistance` (0, no distance cull, on 977; 300 m on 11, 350 m on 3).
What the cap does when more placements are alive than it allows is open (LIGHT-PT-7).

[test: core/tests/test_spec_probes.py::spg_lighting]
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
[doc: bfo:docs/research/opfor/EMITTER_ATLAS.md:27-49] [doc: bfo:docs/research/opfor/EMITTER_ATLAS.md:136-154]

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

Status: **verified** · Variants: PC

A level places an effect with a `GameEffectEntityData` (in its level partition or a sub-blueprint) naming the effect
blueprint: 13,083 placements in 82 of the game copy's 83 level partitions. `IsCameraProximityTriggered` true (3,024 in
62 levels: rain and snow grids, dust, ambient smoke and insects) makes the effect play only while the camera is near
it; false (DICE's night lamps) plays it everywhere.

[test: core/tests/test_spec_probes.py::spg_lighting]
[data: spec/probes/lighting.py lights on the game copy, 2026-11-22: 13,083 placed GameEffectEntityData in 82 level partitions; 3,024 camera-proximity in 62]

### FX-PLACE-2: A weather grid is many camera-proximity area effects

Status: **inferred** · Variants: PC

DICE covers a playable area with rain or snow by a grid of camera-proximity area effects (SP_03: 32 rain placements;
MP_012: 48 snowstorms; WTH-FX-1). A generated grid (VW2d) over a level's combat area should keep DICE's spacing per
donor effect (the donor's spawn-volume size, FX-CLS-2) and its count within DICE's (up to 48 for one effect on a
multiplayer level) until a playtest measures the cost.

[inferred: DICE's grids measured by WX0; the cost of a denser grid is unknown]

### FX-PLACE-3: Emitters can act as point lights

Status: **verified** · Variants: PC

35 of the 2,882 emitter templates act as point lights (LIGHT-PT-2): 25 light-source effects (`fx/misc/lightsource/`),
3 muzzle flashes, detonations and ambient fires; 8 of the 35 are `Emissive`. `MaxCount` (live particles) on them is 1 -
201 (6 x 22).

[test: core/tests/test_spec_probes.py::spg_lighting]
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

Status: **verified** · Variants: PC

The template's `EmittableType` is `ScreenAlignedQuad` (30,443 emitters), `DirectionAlignedQuad` (7,830),
`ParticleMesh` (7,339: a `Mesh`, with `CompositeMeshPartNames`) or `Quad` (4,529).

[test: core/tests/test_spec_probes.py::emitters]
[data: spec/probes/effects_chain.py, 2026-11-22: EmittableType over 50,141 templates]

### FX-PROC-4: The executable's processor and evaluator types

Status: **observed** · Variants: PC

The type information enumerates 37 processor types, `ProcessorType` 0 - 0x24: `PtBaseEmitter`, `PtSpawnRate`,
`PtPreSpawn`, `PtSpawnCompositeMeshParts`, `PtSpawnSpeed`, `PtSpawnPosition`, `PtSpawnDirection`, `PtSpawnSize`,
`PtSpawnAnimation`, `PtSpawnAnimationFrame`, `PtSpawnRotation`, `PtSpawnRotationSpeed`, `PtUpdatePosition`,
`PtUpdateAge`, `PtTurbulance`, `PtGravity`, `PtLocalForce`, `PtAirResistance`, `PtUpdateLinearVelocity`,
`PtUpdateCollision`, `PtUpdateOrientation`, `PtEmitter`, `PtUpdateBoundingBox`, `PtUpdateColor`, `PtUpdateColorLeaf`,
`PtUpdateTransparency`, `PtUpdateTextureCoords`, `PtCycleAnimations`, `PtUpdateRotation`, `PtUpdateSizeX` / `Y` / `Z`,
`PtUpdateSize`, `PtUpdateAlphaLevelMin` / `Max` / `Scale`, `PtUpdateClipScale`; DICE's emitters use 31 of them
(FX-PROC-1), so a previewer that implements those 31 draws every DICE emitter. The evaluators are 12 types,
`EvaluatorType` 0 - 0xB: `EtDefault`, `EtBox`, `EtMesh`, `EtPolynomial`, `EtPolynomialOperator`, `EtRandom`,
`EtRandomXYZ`, `EtRotateVector`, `EtSampleTexture`, `EtSphere`, `EtConstant`, `EtPolynomialColorInterp` (DICE uses 8,
FX-PROC-2). A polynomial evaluator's fields are `Coefficients` (one Vec4), `ScaleValue`, `MinClamp`, `MaxClamp`, and
the operator classes add `ExpressionType` (`PolynomialExpression`, `SineExpression`, `CosineExpression`) and
`Operator` (none, add, sub; `Multiplication` / `Addition` / `Subtraction` for the polynomial operator). The
`EvaluatorInput` values step like byte offsets into a particle record (`EfVelocity` 0x50, `EfNormTime` 0x70,
`EfSpeed` 0x78, `EfNone` 0x7C): they are the particle record's offsets (`EmittableField`, FX-PROC-7, FX-PROC-8). The evaluation is
FX-PROC-5 (round SPG2-rest4: an 11-way dispatch, 0xB268E0, not a 12- or 37-way switch). The 37-way switch at 0x11DD76A, left unread by round
SPG2-rest2, is the C runtime's code-page upper-casing (`toupper` for bytes 0x80 and up: 0x9A -> 0x8A, 0xE0 - 0xFE ->
0xC0 - 0xDE), not the particle update. Round SPG2-rest3's census of the switches with 36 or more cases found 16; of
those read none is the particle update's (0x98B7F0, 38 cases, copies dirty render states; 0x77BC8C is the main menu's
command dispatcher; 0x1363390 and 0x11B31C0 as before), and 0x9AF940 (41 cases) and the rest were not read.

[exe: BFBC2Game.exe enum records 0x1C1D500 - 0x1C1DB48 (EvaluatorInput, the particle vertex layout, EvaluatorType, ProcessorType, name and value each); field descriptors 0x1C1E328 - 0x1C1E3A0 and 0x1C1E898 - 0x1C1E988 (ExpressionType, Operator, Coefficients with the Vec4 type 0x1561CE0, ScaleValue, MinClamp, MaxClamp; Multiplication, Addition, Subtraction); round SPG2-rest2's switch scan (Ghidra, 12- to 38-target jump tables): 0xA5D370, 0xA5D480 (13 targets) are vertex-format conversions and 0x1363390 (37) a settings switch, 0x11B31C0 (37) Kynapse's; round SPG2-rest3: 0x11DD76A (37) is `toupper` for code-page bytes (low half through the CRT's `toupper`, the switch maps 0x9A / 0x9C / 0x9E / 0xE0 - 0xFE, 0xFF -> 0x9F), Ghidra's switch census of 36+ targets: 0x76ECC1, 0x776569, 0x77BC8C, 0x83B9BD, 0x85D53A, 0x95A2B7, 0x95E743, 0x98B8A1, 0x9AF960, 0x1198550, 0x11B39B3, 0x11B3A29, 0x11DD7BF, 0x11FD505, 0x1205000, 0x13633AB]
[exe: BFBC2Game.exe the enumeration EmittableField (round SPG2-rest5) names the same values, and the update writes the particle record at those offsets (FX-PROC-8)]

### FX-PROC-5: The evaluator dispatch and the polynomial: a cubic in the input, clamped, then scaled

Status: **observed** · Variants: PC

The particle update reads a processor's value through one dispatcher (0xB268E0, called by the processors' update
functions: the update 0xB2C1A0, the vertex fill 0xB280C0, the spawn 0xB328D0; FX-PROC-8 - 11). The processor builds a
small record for it: its `DefaultValues` (+0x00, a Vec4), its `Pre` evaluator (+0x10), the evaluator's component
selector (+0x14: 0 - 3 pick x, y, z or w of the value vector the processor passes, anything else gives 1.0), the
evaluator's type (+0x18) and the processor's `EvaluatorInput` (+0x1C, `EfNone` 0x7C when it has none). The dispatcher
switches on the type (1 - 0xB through the table at 0xB26E98; 0, `EtDefault`, and anything else return nothing) to one
function per evaluator: box 0xB23890, mesh 0xB26830, polynomial 0xB24FD0, polynomial operator 0xB266D0, random 0xB21300,
random XYZ 0xB23780, rotate vector 0xB23500, sample texture 0xB265F0, sphere 0xB25090, constant 0xB23990, colour
interpolation from 0xB26D92 on.

For a polynomial (`PolynomialData`, run-time `PolynomialTempData`: `Coefficients` c at +0x00, `MaxClamp` +0x10,
`MinClamp` +0x14, `ScaleValue` +0x18, after a 0x20-byte header in the evaluator object), with the input t and the
selected component v:

    value = min(MaxClamp, max(MinClamp, c.x t^3 + c.y t^2 + c.z t + c.w)) x ScaleValue x v

So the clamp applies to the polynomial before the scale, and `Coefficients` runs from the cubic term down to the
constant. The input t is the vector the update passes for the particle when the processor's `EvaluatorInput` is
`EfNormTime` (0x70, the particle's normalised age: FX-PROC-2, all but 40 of DICE's processors); for any other input
this dispatcher evaluates the polynomial at t = 1.0 (FX-PROC-7: DICE's 40 `EfVelocity` processors have no evaluator).

[exe: BFBC2Game.exe dispatcher 0xB268E0 (type at record +0x18 minus 1 against 0xA at 0xB268F6 - 0xB26905, table 0xB26E98; component switch at +0x14 with the default 1.0 from 0x14D94F0; the input test `cmp ecx, 0x70` at 0xB26A2E); its wrapper 0xB26F80; the record built at 0xB27CB0 - 0xB27D1E in 0xB27BF0 (the processor's DefaultValues +0x10, Pre +0x0C, EvaluatorInput +0x24 by the field descriptors 0x1C1DEC0, 0x1C1DB60, 0x1C1DE90; the evaluator's +0x14 the type, +0x10 minus 1 the component); polynomial 0xB24FD0 (coefficients +0x20, MaxClamp +0x30, MinClamp +0x34, ScaleValue +0x38; the powers and their shuffles 0xB24FFF - 0xB25047, `maxps` with MinClamp 0xB25052, `minps` with MaxClamp 0xB25058, the product with ScaleValue x v 0xB2505B); PolynomialTempData's field descriptors 0x1C1E8F8 - 0x1C1E954 (Coefficients +0x0, MaxClamp +0x10, MinClamp +0x14, ScaleValue +0x18; record 0x1BFA934, 0x20 bytes)]

### FX-PROC-6: The other evaluators

Status: **observed** · Variants: PC

Each evaluator returns a Vec4 from its data object (the field offsets below are the `*Data` classes'), the input t
(FX-PROC-5: the particle's normalised age for `EfNormTime`, else 1.0), the selected parameter v (FX-PROC-7) and the
emitter's random numbers r (0x4E45C0: ((a random word | 0x80) >> 7) + 1, times 2^-24, so 0 < r <= 1; a fresh r per
draw). "w = 1" means the result's fourth component is forced to 1.

| Type | Class | Result |
|---|---|---|
| 1 box | `BoxEvaluatorData` (`Pivot` +0x20, `Dimensions` +0x30) | v x D x (2r - 1) + `Pivot` x D, one r per axis, D the dimensions (so a box of half-extents D about `Pivot` x D); w = 1 |
| 3 polynomial | `PolynomialData` | FX-PROC-5 |
| 4 polynomial operator | `PolynomialOperatorData` (`Operation` +0x20, `MinClampResult` +0x18, `MaxClampResult` +0x1C, `FirstOperand` +0x30 and `SecondOperand` +0x50, each a `PolynomialTempData`) | each operand o = clamp(c.x t + c.y t^2 + c.z t^3 + c.w, MinClamp, MaxClamp) x ScaleValue x v; then o1 x o2 (`Multiplication`, and any other value), o1 + o2 (`Addition`) or o1 - o2 (`Subtraction`), clamped to the result's limits |
| 5 random | `RandomEvaluatorData` (`Min` +0x18, `Max` +0x1C) | (Min + (Max - Min) r) x v, one r for all four components |
| 6 random XYZ | `RandomXYZEvaluatorData` (`MinX` +0x28, `MaxX` +0x24, `MinY` +0x18, `MaxY` +0x2C, `MinZ` +0x20, `MaxZ` +0x1C) | (MinX + (MaxX - MinX) r1, the same for y, z) x v; w = 1 |
| 7 rotate vector | `RotateVectorData` (`Angle` +0x18, `RotateWithinPlane` +0x1C) | the unit vector (-cos p sin h, cos h, sin p sin h, 1): h uniform in +-(`Angle` x v) / 2 degrees from the y axis, p uniform in +-90 degrees, or 0 with `RotateWithinPlane` (a cone of opening `Angle` about y, or a fan in the x-y plane) |
| 9 sphere | `SphereEvaluatorData` (`Pivot` +0x20, `Scale` +0x30, `Radius` +0x40) | (`Pivot` x R + p) x `Scale` x v, p uniform in the ball of radius R (cube draws, rejected outside); w = 1 |
| 10 constant | `ConstantEvaluatorData` (`Scale` +0x18) | t x v x `Scale`; w = 1 |
| 11 colour interpolation | `PolynomialColorInterpData` (`Coefficients` +0x20, `Color1` +0x30, `Color0` +0x40) | s = clamp(c.x t^3 + c.y t^2 + c.z t + c.w, 0, 1) per component (w: 0); (1 - s) x `Color1` + s x `Color0`, times v; w = 1 |
| 2 mesh | `MeshEvaluatorData` (`DistributionData` +0x24, a list of Vec4) | the entry i = trunc(r' x n) of `DistributionData` (n entries, r' = 0.9999999 r), times v; w = 1. `MeshAsset`, `UniformPointCount`, the iso values and `UseDirectionData` are not read here (presumably the editor's, which bakes the point list from the mesh) |
| 8 sample texture | `SampleTextureData` (`ColorIntensityMin` +0x20, `ColorIntensityMax` +0x30, `TextureDimensions` +0x40, `GradientData` +0x50, a list of Vec4) | g = `GradientData`[trunc(clamp(t, 0, 0.9999999) x `TextureDimensions`.x)]; ((1 - g.w) x `ColorIntensityMin` + g.w x `ColorIntensityMax`) x g x v per component; w = 1. So a texture row baked into `GradientData` is sampled at t, and its alpha picks the intensity between the two limits; `Texture` and `TextureOriginU` / `V` are not read here |

So the polynomial operator's operands run from the linear term up, the reverse of `PolynomialData`'s (FX-PROC-5);
DICE's data uses neither the operator nor the constant (FX-PROC-2), but an editor that writes them must keep the two
orders apart. `ExpressionType` (sine / cosine) and the `Expression*` classes are read by none of these.

DICE's data has no mesh evaluator and 15 sample-texture evaluators, every one the `Pre` of an `UpdateColor` with the
input `EfNormTime` and a gradient of 512 entries for a `TextureDimensions`.x of 512, so the lookup never runs past the
list (the gradients' components are multiples of 1/255 from 0 to 0.95). The evaluator does not check the length: a
writer keeps `GradientData` at least `TextureDimensions`.x entries long.

[exe: BFBC2Game.exe box 0xB23890 (-D at 0xB238AD, three draws 0xB238BF - 0xB23942, Pivot x D 0xB2396A - 0xB23972, w 0xB23979 - 0xB2397D); random 0xB21300; random XYZ 0xB23780; rotate vector 0xB23500 (pi/2 x (RotateWithinPlane == 0) 0xB2350F - 0xB23534, the half-degree factor 0.0087266 0x15165BC, the sine and cosine polynomials 0x145A3AC - 0x145A3C4, the result 0xB23758 - 0xB23770); sphere 0xB25090 (the rejection loop 0xB250E1 - 0xB251A4 against R^2); constant 0xB23990; polynomial operator 0xB266D0 (the powers in the order t, t^2, t^3 at 0xB26732 - 0xB26757, the operation +0x20 at 0xB267A9 - 0xB267EF, the result's clamp 0xB2680E - 0xB2681E); colour interpolation inline at 0xB26DF5 - 0xB26E86; the random number 0x4E45C0 (2^-24 at 0x14112F0); the classes' field descriptors as classes.json of round SPG2-rest5 read them (BoxEvaluatorData 0x1BFAA14, SphereEvaluatorData 0x1BFAA2C, RandomEvaluatorData 0x1BFA9E4, RandomXYZEvaluatorData 0x1BFA9FC, RotateVectorData 0x1BFA99C, ConstantEvaluatorData 0x1BFAA5C, PolynomialOperatorData 0x1BFA96C, PolynomialColorInterpData 0x1BFAA74)]
[exe: BFBC2Game.exe round SPG2-rest6: mesh 0xB26830 (DistributionData's list through 0x1398E90, this + 0x24; its entries (end - begin) >> 4 at 0xB26850 - 0xB26871; the draw 0x4E45F0 over (0, 0.9999999 at 0x145A4D0) at 0xB2687C; the index 0xB26887 - 0xB268B5; the entry times v 0xB268BC - 0xB268C6; w = 1 0xB268CF - 0xB268D3); sample texture 0xB265F0 (GradientData's list through 0x1398E70, this + 0x50; t clamped to 0 and 0.9999999 (0x142BEF0) at 0xB2660C - 0xB26647; times TextureDimensions.x (+0x40) 0xB2664F; the entry 0xB26690; (1 - g.w) x +0x20 + g.w x +0x30 then x g x v 0xB2669D - 0xB266BA); field descriptors MeshEvaluatorData 0x1BFAA44 (DistributionData +0x24), SampleTextureData 0x1BFA984 (GradientData +0x50, ColorIntensityMin +0x20, ColorIntensityMax +0x30, TextureDimensions +0x40)]
[data: spec/probes/effects_chain.py, round SPG2-rest6 2026-11-27: sample-texture evaluators 15, all `UpdateColorData Pre EfNormTime: gradient 512 x 512`; mesh evaluators 0; the gradients' 30,720 components each a multiple of 1/255 in 0 - 0.949 (a scratch read)]

### FX-PROC-7: The evaluator's inputs: the particle's age, and one of four emitter parameters

Status: **verified** · Variants: PC

`EvaluatorInput` takes its values from the enumeration `EmittableField`, the particle record's fields (FX-PROC-8):
`EfSpawnAnimationSpeed` 0x48, `EfSpawnAnimationFrameIndex` 0x4C, `EfVelocity` 0x50, `EfParameters` 0x60,
`EfNormTime` 0x70, `EfRotation` 0x74, `EfSpeed` 0x78, `EfUserDefined` 0x79, `EfConstantFloat` 0x7A, `EfConstantVec`
0x7B, `EfNone` 0x7C. The update always passes the particle's new normalised age as the input vector, and the
dispatcher uses it only when the input is `EfNormTime`: any other input, `EfVelocity` included, evaluates at t = 1.0.
DICE's 40 `EfVelocity` processors are all `TurbulanceData` with no `Pre` evaluator, and a processor with no evaluator
returns its `DefaultValues` whatever the input, so the field changes nothing in DICE's data.

The evaluator's `EmitterParameter` picks v, the value the evaluator scales by: `EmitterParameter1` - `4` are the x - w
of the particle's `Parameters` (record +0x60, copied from the emitter when the particle is spawned: the values game
code sets on an emitter, such as a vehicle's speed), `EmitterParameterNone` gives 1.0. DICE's evaluators:
`EmitterParameterNone` 442,864, `EmitterParameter4` 20,478, `EmitterParameter1` 6,120, `EmitterParameter2` 506.

[test: core/tests/test_spec_probes.py::emitters]
[data: spec/probes/effects_chain.py, round SPG2-rest5 2026-11-27: EfVelocity on 40 processors, all TurbulanceData without Pre; EmitterParameter over the template chains' evaluators as listed]
[exe: BFBC2Game.exe EmittableField's records from 0x1BF99C0 (EvaluatorInput's type); the dispatcher's evaluator test 0xB268EC (no evaluator: DefaultValues at 0xB26E8E) and the component switch from evaluator +0x10 minus 1 (0xB2690C - 0xB26947); the update 0xB2C1A0 passes the age vector [esp+0x80] (+0x70 plus the step, 0xB2DCB2 - 0xB2DD17) and the particle's +0x60 (0xB2DD23) to every evaluation (0xB2DE52 - 0xB2DE6B and the calls after it)]

### FX-PROC-8: The particle record and what the processors write

Status: **observed** · Variants: PC

A CPU particle is a 0x80-byte record: its axes and position as four rows (+0x00, +0x10, +0x20, +0x30: the position
the fourth), +0x40 a vector holding the animation speed (+0x48) and frame index (+0x4C), the velocity (+0x50), the
`Parameters` (+0x60), the normalised age (+0x70), the rotation (+0x74), the size (+0x78; the enumeration calls it
`EfSpeed`) and the rotation speed (+0x7C). The emitter's update (0xB2C1A0) finds each processor by its type (a byte
per `ProcessorType` at +0x20 of the emitter's processor set, an index into its list, 0xFF for none), so the chain's
order (FX-PROC-1) does not decide the order of evaluation. Read so far:

- **Spawn**: `SpawnPosition`'s value, moved into the emitter's space, is the position; `SpawnDirection`'s, rotated
  and normalised, times `SpawnSpeed`'s is the velocity, plus the emitter's own velocity with
  `InheritSpeedAndDirectionFromEmitter`; `SpawnSize` sets +0x78, `SpawnRotation` +0x74, `SpawnRotationSpeed` +0x7C,
  `SpawnAnimation` / `SpawnAnimationFrame` the +0x40 vector; `UpdateAge`'s `Lifetime` gives the age step 1 /
  `Lifetime`.
- **Update**, each frame at the new age: `Turbulance`, `Gravity`, `LocalForce` and `AirResistance` change the
  velocity, each added as its value times a vector the emitter set up before the loop (turbulence: three noise
  draws; air resistance: the velocity itself, the factor limited from below so it cannot reverse the velocity);
  `UpdateLinearVelocity` moves the position by velocity x the time step; `UpdateOrientation` rebuilds the axes
  from the velocity's direction when it is long enough; `UpdateRotation` adds value x step x the rotation speed to the
  rotation; the forces' vectors are FX-PROC-10. The update also adds the time step to +0x40 each frame, so +0x40 runs
  in seconds (the texture frame of FX-PROC-9 multiplies it).
- **Size** (corrected by round SPG2-rest6): `UpdateSize`'s value x the spawn size, and the larger of that and
  `UpdateSizeX` / `Y`'s value x the spawn size, go to a temporary of the update, not to the record (+0x78 keeps the
  spawn size): the box of position +- that size goes to the bounds the update gathers. The drawn width and height are
  the vertex fill's (FX-PROC-9), where `UpdateSizeX` / `Y` replace `UpdateSize`'s value per axis instead of taking the
  larger.

The colour, transparency, alpha-level, texture-coordinate and clip-scale processors are not evaluated in this CPU
update: the quad vertex fill evaluates them (FX-PROC-9).

[exe: BFBC2Game.exe the update 0xB2C1A0: processor lookups by type byte 0xB2C1B4 (UpdateAge, 1 / Lifetime at 0xB2C1D6 - 0xB2C1E4), 0xB2C6C7 - 0xB2C771 (spawn), 0xB2D664 - 0xB2D791 (update); spawn evaluations 0xB2CE06 (position, then 0x403870), 0xB2CEF3 (speed), 0xB2CF35 (direction, 0x403970 and 0x4182E0; InheritSpeedAndDirectionFromEmitter +0x30 at 0xB2D046), 0xB2D100 (size), 0xB2D146 / 0xB2D186 (animation), 0xB2D214 (rotation), 0xB2D25C (rotation speed); the record written 0xB2D27C - 0xB2D3F5; update evaluations 0xB2DE6B (turbulence after three 0xB20A90 draws), 0xB2DEE6 (gravity), 0xB2DF34 (local force), 0xB2DF82 (air resistance, `maxps` 0xB2DFA9), 0xB2DFD8 (linear velocity), 0xB2DFFD - 0xB2E13C (orientation), 0xB2E172 (rotation), 0xB2E1BD (size), 0xB2E1FF / 0xB2E24C (size x / y)]
[exe: BFBC2Game.exe round SPG2-rest6: +0x40 loaded 0xB2DD1F, the time step (xmm4, the update's argument broadcast at 0xB2CBAF - 0xB2CBCD) added to its x 0xB2DD9A - 0xB2DDA0, written back 0xB2E29E; the sizes into the temporary [esp+0xA0] 0xB2E1C2 - 0xB2E26F (`maxps` for x / y), then position -+ it at 0xB2E476 - 0xB2E4DE; no store to +0x78 in 0xB2DCA0 - 0xB2E611]

### FX-PROC-9: The quad vertex fill and the processors it evaluates

Status: **observed** · Variants: PC

The quads (`EmittableType` `Quad` 1, `ScreenAlignedQuad` 2, `DirectionAlignedQuad` 3) are drawn from vertices the CPU
fills each frame (0xB292E0, for each live particle in age order). A set-up per emitter (0xB26FB0) finds the
processors by type (FX-PROC-8) and keeps each enabled one's `DefaultValues`, evaluator and input; it skips the
rotation and size processors for `ParticleMesh` (6), whose particles are not quads. Then per particle (0xB280C0),
each value evaluated at the particle's normalised age with its `Parameters` (FX-PROC-5, 7), x its first component:

| Processor | Vertex value |
|---|---|
| `UpdateColor` | the colour (x, y, z of the value); without it (0, 0, 0) |
| `UpdateTransparency` | a = clamp(value, 0, 1) x clamp(1.25 x d, 0, 1), d the particle's distance in front of the camera's plane: every particle fades out over the last 0.8 m before the camera; without the processor value 1 |
| `UpdateAlphaLevelScale` / `Min` / `Max` | value x the processor's `Exponent` / `MinLevel` / `MaxLevel`; without them 1 / 0 / 1 |
| `UpdateTextureCoords` | frame = (+0x40) x (+0x48) x value + (+0x4C), the particle's animation words (FX-PROC-8: +0x40 runs in seconds); without it 0 |
| `UpdateRotation` | the record's rotation (+0x74) as it is (the processor only changes it in the update) |
| `UpdateSize` / `UpdateSizeX` / `UpdateSizeY` | width = height = value x the spawn size (+0x78); then `UpdateSizeX` sets the width and `UpdateSizeY` the height to their value x the spawn size; without any, both are the spawn size |
| `UpdateClipScale` | u = clamp(age, 0, 0.999) x (n - 1) over the n Vec4 of `Lookup`; L = lerp(`Lookup`[floor u], `Lookup`[floor u + 1], u - floor u); the quad's four corners get L.x, L.y, L.z, L.w (one each) |

A particle writes four 0x40-byte vertices, one per corner, all eight half-float Vec4 alike but for the corner: +0x00 the
colour (its w a per-draw byte the caller passes), +0x08 a vector of the draw's, +0x10 (0, 0, 0,
`VertexPixelLightingBlendFactor`), +0x18 (frame, rotation, width, height), +0x20 (a, alpha scale, alpha min, alpha max),
+0x28 the velocity with the corner's index 0 - 3 as w, +0x30 and +0x38 the quad's two axes with the corner's clip-scale
value as w. The axes are the particle's own (its first two rows) for a `Quad`, the camera's for a `ScreenAlignedQuad`,
and the caller's for the others; the position goes to a separate stream with the height. So the quad's shape, the
texture lookup and the alpha levels' use are the shaders' (not read). The fill also adds each particle's screen share
(width x height / distance^2 over a camera factor, at most 1, for particles in the view cone and the distance range)
times 10,000 to a counter. 0xB27BF0, which sums width x height over the particles (0 for a `ParticleMesh`), has no
caller or reference in the executable.

In DICE's data all 42,802 quad templates have `UpdateColor`, `UpdateTransparency` and `UpdateSize`, and all but 45
`UpdateTextureCoords`; no `ParticleMesh` template has any of the colour, transparency, alpha-level, texture-coordinate
or clip-scale processors. Every clip-scale `Lookup` (38,811) holds 33 Vec4.

[exe: BFBC2Game.exe the batch 0xB292E0 (set-up 0xB29362, per particle 0xB293B4, 0x100 bytes of vertices and 0x10 of position per particle, the screen share x 10,000.0 (0x142F360) by `lock xadd` 0xB29426 - 0xB2943C); set-up 0xB26FB0 (type bytes +0x37 colour, +0x39 transparency, +0x3A texture coordinates, +0x41 - +0x44 alpha min, max, scale, clip scale at 0xB273E3 - 0xB27487; EmittableType 6 skips +0x3C rotation, +0x40 size, +0x3D / +0x3E size x / y at 0xB27480 - 0xB274F1; the records with the `Enable` test +0x2C 0xB274F4 - 0xB27916; (1, Exponent, MinLevel, MaxLevel) at +0x1A0 0xB27783 - 0xB278BC; Lookup's Vec4 count 0xB27937 - 0xB2795C; the camera plane (-axis 2, w) and 1.25 (0x145A4D4) 0xB27ABD - 0xB27B5F; the camera rows for ScreenAlignedQuad 0xB270E9 - 0xB27108; VertexPixelLightingBlendFactor (template +0xD0) as half w 0xB27028 - 0xB270B7); per particle 0xB280C0 (colour 0xB281D4 into +0x190; transparency 0xB2822F, clamp and the plane x 1.25 0xB28253 - 0xB282B1; alpha scale / min / max 0xB282D0 / 0xB2831D / 0xB2836A into y / z / w; texture frame 0xB283DF - 0xB28411; size 0xB28434, x 0xB28478, y 0xB284C7 into z / w; clip scale 0xB2854B - 0xB286E3 (0.999 at 0x1434DE8, floor 0x428B20, the lerp 0xB2860D - 0xB28635, halves into the corners' +0x36 / +0x3E words); the screen share 0xB286EB - 0xB28809; alpha x +0x1A0 0xB28931; the four vertices 0xB28BE5 - 0xB28E12, corner halves +0x1D0 - +0x1D6 (0, 1, 2, 3 from 0xB27332 - 0xB273DB); the axes for EmittableType 1 from the particle's rows 0xB2899E - 0xB28BDD); 0xB27BF0 (EmittableType 6 returns 0 at 0xB27C2F, the sum 0xB27F86 - 0xB27FB5): no call, jump or 4-byte reference anywhere in the file; EmittableType's values from its field records 0x1C1EE38 - 0x1C1EEB0 (Point 0, Quad 1, ScreenAlignedQuad 2, DirectionAlignedQuad 3, Trail 4, ParticleMesh 6)]
[data: spec/probes/effects_chain.py, round SPG2-rest6 2026-11-27: vertex-fill processors by emittable type (UpdateColorData / UpdateTransparencyData: ScreenAlignedQuad 30,443, DirectionAlignedQuad 7,830, Quad 4,529; UpdateTextureCoordsData 30,443 / 7,785 / 4,529; none on ParticleMesh); clip-scale lookups `{33: 38811}`; UpdateSizeData on every quad template (a scratch read)]

### FX-PROC-10: The force vectors

Status: **observed** · Variants: PC

Before its particle loop the update (FX-PROC-8) builds one vector per force processor from the processor's own field
and the frame's time step dt; per particle, velocity += value x vector (value the processor's evaluation):

| Processor | Vector |
|---|---|
| `Turbulance` | three uniform draws in [-K, K], K = `Intensity` x dt x 30 (per particle, one component from each) |
| `Gravity` | (0, -`Gravity` x dt, 0): a positive `Gravity` pulls down the world's y |
| `LocalForce` | -dt x `LocalForce` (x, y, z) |
| `AirResistance` | the velocity itself times max(-value x `DragFactor` x dt, -1): the velocity shrinks by that fraction and never reverses |

So gravity and air resistance follow the usual signs, and a `LocalForce` vector pushes the particles along its
negative. DICE's data: `Gravity` positive on 21,616 processors, negative (rising smoke and the like) on 9,465, zero on
4,408; `LocalForce`'s y zero on 10,867, negative on 902, positive on 406; `DragFactor` 0 - 80, `Intensity` 0 - 15.

[exe: BFBC2Game.exe 0xB2C1A0: dt broadcast into [esp+0x90] 0xB2CBAF - 0xB2CBCD, 30.0 (0x141D840) into [esp+0x580] 0xB2CBE1 - 0xB2CBF5, the zero vector [esp+0xB0] 0xB2C206 and ones [esp+0x110] 0xB2C20E; type bytes +0x2E - +0x32 0xB2D664 - 0xB2D6E9; turbulence Intensity x dt x 30 0xB2D7D5 - 0xB2D7EC, the draws 0xB20A90 (lo + r (hi - lo)) between -K and K 0xB2DDB7 - 0xB2DE4A; gravity (0 - dt) x Gravity into y 0xB2D873 - 0xB2D88D; local force (0 - dt) x LocalForce 0xB2D903 - 0xB2D936; drag (0 - dt) x DragFactor 0xB2D98D - 0xB2D9BA; the per-particle sums 0xB2DEB1 (turbulence), 0xB2DEF7 (gravity), 0xB2DF4D (local force), 0xB2DF96 - 0xB2DFB7 (drag, `maxps` with 0 - 1)]
[data: spec/probes/effects_chain.py, round SPG2-rest6 2026-11-27: force fields by sign over the template chains]

### FX-PROC-11: The mesh particles' spawn and the emitter's lifetime

Status: **observed** · Variants: PC

0xB328C0, which round SPG2-rest5 took for the mesh particles' update, is a two-instruction wrapper of 0xB32730, the
emitter's lifetime step for every emittable type: it counts the frames and adds the time step to the emitter's time,
stops emitting once the time passes the template's `Lifetime` (when not 0) or the frames its `LifetimeFrameCount`
(when not 0), frees the particle buffer once nothing is emitting or alive, and then queues the emitter for removal.

The mesh particles' own code is the spawn 0xB328D0 (called from 0xB34394 in a loop over emitters, with the time step
and a count): with a `SpawnCompositeMeshParts` processor it makes one particle per part of the effect's mesh, else the
count asked; it evaluates the spawn processors as FX-PROC-8 does (position, direction x speed, rotation, rotation
speed); and a particle with a physics body gives the body its placement, the velocity and an angular velocity (the
vector at +0x40 x the rotation speed at +0x7C). Whether other emittable types also spawn through 0xB328D0 was not
settled. After the spawn a mesh particle moves by the update (FX-PROC-8) or, with a body, by the physics.

[exe: BFBC2Game.exe 0xB328C0 (`fld` / `call 0xB32730` / `ret 4`); 0xB32730 (frame counter +0xA8 / +0xAC, time +0xB0 += dt, template +0xC8 Lifetime and +0xC4 LifetimeFrameCount clear flag bit 2 at 0xB32765 - 0xB327B0, the buffer freed 0xB32828 - 0xB32863, 0xB2B230 queues and sets bit 0x40 0xB3289F - 0xB328AB; EmitterTemplateData's field descriptors: Lifetime +0xC8, LifetimeFrameCount +0xC4); 0xB328D0 (UpdateAge +0x2D, SpawnCompositeMeshParts +0x23: the part count (+0xE4 - +0xE0) >> 2 at 0xB32998 - 0xB329C2; SpawnRate +0x21; SpawnSpeed / Position / Direction +0x24 - +0x26, SpawnRotation +0x2A, SpawnRotationSpeed +0x2B at 0xB32B23 - 0xB32B89; evaluations 0xB3313D, 0xB33249, 0xB3339F, 0xB33484, 0xB3353C (rotation speed x into +0x7C, times +0x40); the body 0xB33599 - 0xB33607: 0xAA7B80 (0.1 into body +0x188), vtable +0x48 and +0x9C, 0xAA7C10 / 0xAA7C60 (the body's +0xD0 vtable +0x40 / +0x44: linear and angular velocity))]

## Open questions

- **The effect instance cap** (FX-CLS-3, LIGHT-PT-7).
- **Appending images to a target's own atlas** (FX-ATL-4) instead of a merge: room in DICE's atlases is not measured
  (the merged Wake atlas is 82 % full).
- **The processor chain's semantics** (FX-PROC-5 - 11): every evaluator, the spawn and force processors, the vertex
  fill and the force vectors are read (round SPG2-rest6). Left: the particle shaders (how the quad is built from the
  corner index and the axes, the texture frame's lookup, the alpha levels' formula, the clip scale's use); which
  emittable types spawn through 0xB328D0; the turbulence draws' component order; `LocalForce`'s sign against an
  effect that uses it (a playtest: a DICE effect with a positive y should sink); what game code sets the emitter
  parameters; the caller's per-draw byte and vector in the vertex (+0x00 w, +0x08).
- **The 1,092 `BaseEmitterData` outside any chain** and the documents whose root starts there.
- **Cost** of camera-proximity grids and of many lights: a playtest.
