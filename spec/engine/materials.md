# Physics materials: the material grid, the material slots, penetration, impact effects, texture conventions

What decides how an object reacts when it is shot, blown up or rammed: the level's **material grid** (the materials
a level knows and, for each pair of them, a **relation**: damage multipliers, penetration, impact effect, sound,
decal, vehicle dust, terrain craters), the **material slots** an object's physics carries (part material, face
material; HAVOK-MAT-2), and how the game picks a slot for damage, penetration and effects. A last section covers the
texture side of a surface's look (grime, dirt, camouflage, material masks), which the grid does not touch.

The wrapper bytes that hold the slots are `formats/havok.md`'s (HAVOK-WRAP-1, HAVOK-RUN-1 - HAVOK-RUN-3,
HAVOK-MAT-1, HAVOK-MAT-2); this file builds on them and does not repeat them. What a destroyed part shows is
`engine/destruction.md`'s.

**Corpus.** The level bundles of Project OPFOR BFBC2's game copy (`game/**/levels/*/*.fbrb`), each partition taken
once from the first bundle that holds it, DICE's bundles first (a bundle is DICE's when its file date is before
2013, as the install leaves it; 444 of 501 level bundles). "DICE's grids" are the 74 `materials_win32` partitions
read from DICE's bundles; "the physics assets" are the 1,183 `<blueprint>_Havok.dbx` partitions of DICE's bundles
with their `.res`. Counted by `spec/probes/materials.py` (2026-11-22). The executable findings are OPFOR's static
RE of `BFBC2Game.exe` (`opfor:research/MATERIAL_SLOTS_RE.md`, 2026-11-09); nothing was run in game.

## 1. The material grid

### MAT-GRID-1: One grid partition per level

Status: **verified** · Variants: PC, BF1943

A level's materials are one partition `Levels/<LV>/Materials_Win32` (BF1943's levels: `materials_xenon`) holding one
`Entity.MaterialGridData` instance: `Name`, `Materials` (references to `Entity.MaterialData`), `MaterialRelations`
(references to `Entity.MaterialRelationData`) and `MaterialGridLayers`, empty in every grid. The level names it in
`LevelData.MaterialGrid` (LEVEL-DATA-1). A level bundle may also hold its other modes' grids (MP_001's bundle holds
MP_001SR's, MP_006's those of MP_006CQ and MP_006SDM).

[test: core/tests/test_spec_probes.py::spg_materials]
[data: spec/probes/materials.py on the game copy, 2026-11-22: 81 materials_win32 grid partitions (74 from DICE bundles), 51 - 92 materials and 953 - 3,160 relations each; 4 materials_xenon partitions (OPFOR's WW2 levels), 104 - 111 materials, 4,049 - 4,706 relations; MaterialGridLayers items over all grids 0]

### MAT-GRID-2: A grid holds only the level's materials, in its own order

Status: **verified**

Each grid lists the materials its level uses, not a shared catalogue: DICE's 74 grids name 121 distinct materials
between them, and 114 of the 121 sit at different indices of `Materials` in different grids. A material's grid index
(the low part of its runtime id, HAVOK-MAT-2) is therefore per level; across levels a material is the same only by
its name and `Hash` (MAT-GRID-3).

[test: core/tests/test_spec_probes.py::spg_materials]
[data: spec/probes/materials.py, 2026-11-22: 121 distinct MaterialData names in DICE's 74 grids; 114 of them at more than one index of MaterialGridData.Materials]
[exe: the runtime material id (grid index plus flag bits) is built at 0x10029E0]
[doc: opfor:research/MATERIAL_SLOTS_RE.md:17-21]

### MAT-GRID-3: MaterialData and its Hash

Status: **verified**

An `Entity.MaterialData` has `DebugColor` (4 floats), `Category` (`Construction`, `Projectiles`, `Explosions`,
`Vehicles`, `Terrain`, `Vegetation`, `Soldier`, `Default`), `Name`, five flags (`Penetrable`, `ClientDestructible`,
`SeeThrough`, `NoCollisionResponse`, `NoCollisionResponseCombined`) and `Hash`, a u32 that is always the 33-xor hash
of the lower-case name: `h = h * 33 ^ byte` from 5381, the function the physics wrapper uses for part names
(`bc2_formats::havok::part_hash`). The same name has the same hash in BFBC2 and BF1943.

[test: core/tests/test_spec_probes.py::spg_materials]
[data: spec/probes/materials.py, 2026-11-22: Hash equal to the 33-xor hash of the lower-case Name on 5,746 of 5,746 MaterialData instances of DICE's grids; categories Construction 1,771, Projectiles 1,135, Explosions 692, Vehicles 677, Terrain 573, Vegetation 395, Soldier 355, Default 148]
[code: rust/bc2-formats/src/havok.rs:587-591]
[doc: opfor:src/tools/ww2/material_grid.py:333-334]

### MAT-GRID-4: The flags belong to the name

Status: **verified**

No material's flags differ between DICE's grids. 23 of the 121 materials are `Penetrable`, 9 `SeeThrough`, 9
`ClientDestructible`, 3 `NoCollisionResponse` (glass_breakable, the two tree branch materials), 5
`NoCollisionResponseCombined` (the tank materials, tree_static_trunk, wood). Every `SeeThrough` material is also
`Penetrable` (chickenwire, glass_breakable, glass_bulletproof, metal_seethrough, middlegrowth_seethrough,
razorwire, water, water_pool, wood_seethrough). The other Penetrable ones: fabric, huey, metal_light, middlegrowth,
the four soldier body / head materials, transport_helo, the two tree branch materials, wood, wood_banger,
wood_fragile. Brick, concrete, metal and stone are not penetrable.

[test: core/tests/test_spec_probes.py::spg_materials]
[data: spec/probes/materials.py, 2026-11-22: 0 of 121 names with differing flags; Penetrable 23, SeeThrough 9 (all 9 also Penetrable), ClientDestructible 9, NoCollisionResponse 3, NoCollisionResponseCombined 5]

### MAT-GRID-5: Relations: two materials and their data

Status: **verified**

A `MaterialRelationData` names `Material1` and `Material2` (a `null` side stands for any material) and a `Data`
array of 1 - 4 items, each a relation data instance of the same partition:

| Class | Items in DICE's grids | Holds |
|---|---:|---|
| `MaterialRelationDamageData` | 162,950 | the damage fields (MAT-GRID-7) |
| `MaterialRelationEffectData` | 70,241 | `ImpactEffect` (set on 69,547) |
| `MaterialRelationSoundData` | 57,871 | `ImpactSound` (set on 30,228), `ScrapeSound` (25,515), `ScrapeLength` 1 - 3 |
| `MaterialRelationDecalData` | 19,730 | `Decal` (set on 17,338) |
| `MaterialRelationVehicleData` | 3,467 | `ChassiEffect`, `TrackEffect`, `WheelEffect`, `GroundEffect` |
| `MaterialRelationTerrainDestructionData` | 3,454 | crater `Width`, `Depth` (DSTR-TERR-3) |

Every data item has a `SourceMaterial` reference (set on 64,361 of 323,018 items).

[test: core/tests/test_spec_probes.py::spg_materials]
[data: spec/probes/materials.py, 2026-11-22: DICE's grids: relations with a null side 3,494; data items per relation 1 x 66,833, 2 x 49,068, 3 x 34,283, 4 x 13,800; the class counts above]

### MAT-GRID-6: Two damage items per relation, one per source material

Status: **verified**

A relation holds no, one or two `MaterialRelationDamageData` items; where it holds two, both name a
`SourceMaterial` (27,728 of 27,749), so one relation carries the damage in both directions of the pair, each item
for one side as the source.

[test: core/tests/test_spec_probes.py::spg_materials]
[data: spec/probes/materials.py, 2026-11-22: DICE's relations by damage items: 0 x 28,783, 1 x 107,452, 2 x 27,749; of the 27,749, every item with SourceMaterial set in 27,728]

### MAT-GRID-7: The damage fields and their ranges

Status: **verified**

`MaterialRelationDamageData` holds six floats and two booleans; DICE's values (all instances of DICE's grids):

| Field | Range | Most common |
|---|---|---|
| `DamageProtectionMultiplier` | 0 - 10 (43 values) | 1 x 93,256, 0 x 49,257, 0.5 x 9,806 |
| `DamageProtectionThreshold` | -100 - 290 (17 values) | 2 x 148,258, 0 x 8,986, 100 x 1,560 |
| `DamagePenetrationMultiplier` | 0 - 1.6 (11 values) | 1 x 158,562, 0 x 1,682, 0.8 x 1,320 |
| `CollisionDamageMultiplier` | 0 - 1,000 (24 values) | 1 x 70,432, 0 x 39,961, 3 x 10,932, 20 x 10,846 |
| `CollisionDamageThreshold` | 0 - 800 (20 values) | 30 x 109,192, 0 x 20,102 |
| `ExplosionCoverDamageModifier` | 0 - 1 (6 values) | 1 x 150,585, 0 x 5,736, 0.5 x 4,523 |
| `NeverPenetrate` | | true on 11,246 of 162,950 |
| `InflictsDemolitionDamage` | | true on 127,194 |

Values are read from the binary numbers, not the XML text (the XML misprints denormals, DBX's number rules).

[test: core/tests/test_spec_probes.py::spg_materials]
[data: spec/probes/materials.py, 2026-11-22: 162,950 MaterialRelationDamageData instances of DICE's 74 grids, the ranges above]

### MAT-GRID-8: Damage is multiplied by the relation and cut by its threshold

Status: **inferred**

Damage from a source material to a target material is the source's damage times `DamageProtectionMultiplier`, and
none when it falls below `DamageProtectionThreshold` (OPFOR's tables write a pair as "multiplier / threshold", C4
against Brick "1/290"); `CollisionDamageMultiplier` / `Threshold` play the same roles for collisions (vehicle rams);
`DamagePenetrationMultiplier` scales the damage a bullet keeps after passing through a penetrable material (wood
"bullets x0.8").

[inferred: from DICE's assignments read against the grid in OPFOR's physics-material review (every indestructible, metal and concrete relation x0; Bridge x0 against every vehicle ram) and the field names; no executable function was followed for the arithmetic]
[doc: opfor:research/PHYSICS_MATERIALS.md:75-79]

### MAT-GRID-9: Material properties DICE leaves at constants

Status: **verified**

The material property data (the `MaterialProperty*` classes among a relation's data) is nearly constant in DICE's
grids:
`MaterialPropertyPhysicsData` friction and restitution modifiers are all 1 (651), `MaterialPropertyImpulseData`'s
`ImpulseAbsorptionMultiplier` is 0.1 on all 939, `MaterialPropertySoundData.Softness` 0 on all 3,352,
`MaterialPropertyTerrainData.DestructionEffect` null on all 363 (`DirtTriggerFactor` 0.1 x 321, 0.001 x 42).
`MaterialPropertySoundData` carries the soldier sounds: `SoldierSettings` set on 3,298, each a
`MaterialSoldierSoundSettings` with `FootStepSound` (2,067 set) and `LandSound` (all 2,167); `ProneSound` is null on
all.

[test: core/tests/test_spec_probes.py::spg_materials]
[data: spec/probes/materials.py, 2026-11-22: DICE's grids, the counts above]

## 2. Material slots

### MAT-SLOT-1: The wrapper's material hashes are the names' 33-xor hashes

Status: **verified**

The u32 material hashes at the end of a physics wrapper (HAVOK-WRAP-1, HAVOK-MAT-1) are the 33-xor hashes
(MAT-GRID-3) of the HavokAsset partition's `MaterialMappings` names, in order. A material can therefore be appended
to a file's list with its hash computed from its name; no donor file is needed for the hash.

[test: core/tests/test_spec_probes.py::spg_materials]
[data: spec/probes/materials.py, 2026-11-22: 1,183 of 1,183 DICE physics assets: wrapper hashes equal to the hashes of MaterialMappings in order]
[doc: opfor:src/tools/ww2/bf1943_vehicles.py:360-374]

### MAT-SLOT-2: A part's byte and its PartInfoData material agree

Status: **implemented**

Every part's index in the part run (HAVOK-RUN-1) names, in `MaterialMappings`, the material its `PartInfoDatas`
entry (matched by the part name's hash) gives in `MaterialName`. Files whose parts all use one material carry the
4-byte run (d = 1): 904 of 1,183, 84 of them with two or more parts (the RE's census counted 69 such "one shared
byte" files of 991); the other 279 carry one byte per part. These are not a separate case: the shared byte names the
one material all their parts have. OPFOR's part edits write both together and refuse a file whose two slots
disagree.

[data: spec/probes/materials.py, 2026-11-22: 4,454 of 4,454 parts in 1,183 of 1,183 files; run forms d = 1 x 904 (84 with 2+ parts), one byte per part x 279]
[code: opfor:src/tools/ww2/physmat.py:62-109]
[doc: opfor:research/MATERIAL_SLOTS_RE.md:23-28]

### MAT-SLOT-3: DICE's part materials

Status: **verified**

DICE's 4,454 parts use 65 materials, every one in some DICE grid. The most used: brick 898, metal 425,
wood_furniture 350, glass_breakable 332, wood 190, wood_fragile 182, concrete 174, defaultmaterial 167,
tree_banger_branch 125, concrete_indestructible 122. 1,264 parts are on a Penetrable material, 520 on a SeeThrough
one, 427 on a ClientDestructible one. Brick is DICE's standard material for destructible building walls, whatever
their faces show (hangar walls: Brick parts, metal_light faces).

[test: core/tests/test_spec_probes.py::spg_materials]
[data: spec/probes/materials.py, 2026-11-22: PartInfoDatas of the 1,183 DICE physics assets: 4,454 parts, 65 distinct materials, none outside DICE's grids]
[doc: opfor:research/PHYSICS_MATERIALS.md:141-145]

### MAT-SLOT-4: Face lists

Status: **verified**

A physics asset's `FaceMaterials` (the face materials its mesh subparts use) holds 1 - 9 names (1 in 532 files, 2 in
337, 3 in 183); in 525 of 1,183 files it equals `MaterialMappings`, in the rest the part materials and face materials
differ (the list in the wrapper and the packfile's `HavokMaterials` are `MaterialMappings`, HAVOK-MAT-1).

[test: core/tests/test_spec_probes.py::spg_materials]
[data: spec/probes/materials.py, 2026-11-22: FaceMaterials items per file 1 - 9 over 1,183 files; equal to MaterialMappings in 525]

### MAT-SLOT-5: Damage reads the part material; an object without parts its default material

Status: **observed**

Bullet and explosion damage to a destructible part reads the part's material (per health state, DSTR-STATE-5);
damage to an entity without parts reads the entity's default material at `entity+0x30`. The entity data's
`Material` field (`Default` on DICE's `StaticModelEntityData` and `TerrainEntityData`) is presumably that default.

[exe: part damage 0x722990 (server), 0x8D6730 (client), 0x6122B0; explosions 0x6A19A0; whole-entity damage reads entity+0x30]
[doc: opfor:research/MATERIAL_SLOTS_RE.md:38-47]

### MAT-SLOT-6: A model-group member's slots are the group's

Status: **implemented**

A static model-group member has no physics of its own: its part materials are its type's bytes in the group's
wrapper (HAVOK-RUN-3), and its face materials are the group's subpart materials (COLL-MG-1), which are the hit
material of every member (HAVOK-MAT-2). Changing how a group member is hit means editing the group's own
`_HavokAsset.res` face list, not the member blueprint's file. OPFOR remaps members' faces in the group's packfile.

[exe: the face material read at 0xAAEEE0 / 0xAADE10 is the hit material of a group member]
[code: opfor:src/tools/ww2/physmat.py:112-140]
[doc: opfor:research/MATERIAL_SLOTS_RE.md:103]

## 3. Penetration

### MAT-PEN-1: A bullet passes a penetrable hit material unless the relation forbids it

Status: **observed**

A bullet penetrates when the hit material's runtime id carries the Penetrable flag (0x80000000) and the relation's
damage data does not set `NeverPenetrate`. SeeThrough (0x20000000) is a separate flag bit of the same id.

[exe: penetration test 0x6F6570; runtime id flags built at 0x10029E0]
[data: spec/probes/materials.py, 2026-11-22: NeverPenetrate true on 11,246 of 162,950 damage items; 23 Penetrable materials (MAT-GRID-4)]
[doc: opfor:research/MATERIAL_SLOTS_RE.md:43]

### MAT-PEN-2: SeeThrough lets the AI see through

Status: **inferred**

`SeeThrough` marks a material that AI line of sight passes (ladders, chain-link and wire fences, glass, water); DICE
moved open metal (ladders, harbour fences) from Metal to Metal_Seethrough without changing their damage, which only
makes sense if the flags are read.

[inferred: DICE's re-assignment R5 (Metal to Metal_Seethrough on indestructible open metal, the same x0 damage) and the flag's name; the AI's use of the flag was not traced in the executable]
[doc: opfor:research/PHYSICS_MATERIALS.md:58-62]

### MAT-PEN-3: On a plain placed object the hit material is probably the part

Status: **inferred**

For an object placed on its own (not a model-group member) the ray hit's material is, most likely, the part
material, so penetration, impact effects and sounds follow the part there as damage does; on group members it is the
face (MAT-SLOT-6). If it is the face instead, faces drive penetration and effects on every object.

[inferred: the hit path 0x8F47F0 -> 0xB059D0 -> 0xAB5430 leads to the part material but was not followed to its end]
[exe: 0x8F47F0, 0xB059D0, 0xAB5430]
[doc: opfor:research/MATERIAL_SLOTS_RE.md:57-62]

## 4. Impact effects and contacts

### MAT-FX-1: Impact effect, sound and decal follow the hit material

Status: **observed**

A bullet impact's effect, sound and decal are the effect, sound and decal data of the relation between the
projectile's material and the hit material (MAT-PEN-3 for which slot that is). A pair without such data plays
nothing (ChickenWire has no bullet effect or sound in DICE's grids).

[exe: impact 0x822D60; effect and sound relation lookup 0x6F0550; relation read 0x1005730]
[data: spec/probes/materials.py, 2026-11-22: DICE's grids: ImpactEffect set on 69,547 of 70,241 effect items, ImpactSound on 30,228 of 57,871 sound items, Decal on 17,338 of 19,730 decal items]
[doc: opfor:research/MATERIAL_SLOTS_RE.md:30-34]

### MAT-FX-2: Vehicle contacts and ground effects

Status: **observed**

A vehicle collision reads the contact material: the face material on mesh / MOPP shapes, the part material on convex
shapes; the vehicle relation (`MaterialRelationVehicleData`: chassis, track, wheel and ground effects) is looked up
separately. DICE sets `GroundEffect` on 2,119 of 3,467 vehicle items, `WheelEffect` 1,147, `ChassiEffect` 1,062,
`TrackEffect` 791.

[exe: contacts 0xAB0710 / 0x70F720; vehicle relation 0x6F0680]
[data: spec/probes/materials.py, 2026-11-22: 3,467 MaterialRelationVehicleData of DICE's grids, the counts above]

### MAT-FX-3: A material the grid lacks has no relations

Status: **inferred**

A part or face named with a material its level's grid does not hold gets no relation: no damage, effect or sound
against anything. A ported object must use materials the target level's grid holds, or the grid must gain them
(OPFOR's merged grid lacks Wood_Log and Glass_NoPenetrate, which DICE files it installs name).

[inferred: relations are looked up per material pair in the level's grid (0x1005730), and a grid holds only its level's materials (MAT-GRID-2)]
[doc: opfor:research/PHYSICS_MATERIALS.md:164-169]
[doc: opfor:research/PHYSICS_MATERIALS.md:283-285]

## 5. Texture side: grime, dirt, camouflage, material masks

The grid decides behaviour; a surface's look is its shader's textures and constants. The baked occlusion textures
(`_Oc`, `_OcRGB`, `_Occlusion`, `_AO`, `_SkyOc`) are RNDR-LIGHT-1's; the texture header is TEX-HDR-1's.

The counts of MAT-TEX-4 to MAT-TEX-11 are `spec/probes/grime.py`'s (2026-11-22, about 45 s): every DICE `Dx11ShaderDatabase`
(74, one per level), each entry (`<mesh>/<shader>`) once from the first database holding it unless a count says it
is per database, a texture record's sampler register named by the program's own pixel shader (its DXBC `RDEF` texture
bindings: `texture_DirtMask2`, `texture_Texture3` ...); the DICE DBX partitions that name a shader set, the 74 DICE
`materials_win32` grids, and the string table of `BFBC2Game.exe`. "Class" is the entry's or mesh's path: soldiers
`Characters/`, vehicles `Objects/Vehicles/`, weapons `Objects/Weapons/`, buildings `Objects/Buildings/`, objects
the rest of `Objects/`, terrain `Terrains/`. MAT-TEX-12 to MAT-TEX-19 are OPFOR's WW2 grime pass.

### MAT-TEX-1: DICE's texture suffixes

Status: **verified**

DICE names a texture by its role with a suffix: colour `_C` / `_D` / `_Df` / `_CM`, normal `_N` / `_Nm`, specular
`_S` / `_Spec`, gloss `_Gm`, occlusion `_Oc` / `_Occ` / `_Occlusion` / `_AO` (`_SkyOc` sky occlusion), packed masks
`_RGB` / `_RGBM` / `_RGBmask` / `_Mask` / `_M`, and shared tiling layers by prefix (`Dt_*` detail, `Msk_*` masks,
`Nm_*`). The suite reads a texture's role from these suffixes.

[data: spec/probes/materials.py, 2026-11-22: 2,852 distinct DICE ITexture names; _C 375, _D 253, _N 219, _RGB 127, _Nm 123, _Oc 118, _S 73, _Df 45, _OcRGB 43, _M 27, _Occ 27, _RGBM 24, _Occlusion 23, _RGBmask 22, _Mask 20, _AO 15, _SkyOc 14, _CM 14; no suffix 445; names containing dt_ 113, dirt 45, camo 20, rust 8, msk_ 7, grime 1]
[code: rust/bc2-formats/src/shaderdb/query.rs:211-226]
[test: rust/bc2-formats/src/shaderdb/query.rs::kinds]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:736-739]

### MAT-TEX-2: Grime and dirt are shader layers over the asset's maps

Status: **observed**

DICE's dirt and wear are drawn by the shader from textures of their own over the asset's colour map: dirt-mask
samplers (`texture_DirtMask`, `texture_DirtMask2`, `texture_DirtMask3`), dirt samplers (`texture_Dirt`,
`texture_Dirt2`), material-mask samplers (`texture_MaterialMask` ... `3`) and the per-instance dirt constants
`DirtColor` / `DirtFactor` (and `DirtColor2` / `DirtFactor2`; MAT-TEX-6). The named slots hold shared textures only
(MAT-TEX-5); most dirt and wear layers are bound through generic slots (`texture_Texture<N>`, `texture_Layer*_Detail*`):
the shared tiles `DirtMaskRGB`, `Dt_Dirt_001` / `_002`, `Msk_Generic`, `Msk_Scratch_Tear`, `Dt_Rust_001`, and
per-asset dirt maps (the vehicles' `<vehicle>_Dirt`, the soldiers' `DirtMap_01` / `DirtMap_01_C`). No database names
a grime constant. (Corrected 2026-11-22: the earlier text said dirt is never a painted map; the per-asset dirt maps
are painted, but as separate layers, not into the colour map.)

[data: spec/probes/materials.py, 2026-11-22: of 74 DICE Dx11 shader databases, texture_DirtMask in 69, texture_DirtMask2 54, texture_MaterialMask 48, texture_MaterialMask2 47, DirtColor and DirtFactor 36, DirtColor2 10, DirtFactor2 6, texture_OcclusionTexture 74]
[data: spec/probes/grime.py, 2026-11-22: (entry, slot) bindings over the distinct entries: DirtMaskRGB in texture_DirtMask* 84 and in texture_Texture<N> 310; Dt_Dirt_001 in texture_Layer2_Detail2 372, texture_Layer1_Detail2 171, texture_Texture<N> 136; DirtMap_01 in texture_Texture<N> 394, DirtMap_01_C 360; the per-vehicle dirt maps BMD3_Dirt, Cav_dirt, Cobra_dirt, M1A2_Dirt, M3Bradley_Dirt, T90_dirt, Truck_Dirt, Vodnik_Dirt only in texture_Texture<N> (or a register the pixel shader does not read)]
[doc: opfor:research/WW2_VISUAL_PARITY.md:10-24]

### MAT-TEX-3: OcRGB maps: occlusion in red, blend masks in green and blue

Status: **inferred**

A prop of DICE's "OcRGB" scheme has no colour map of its own: one small `_OcRGB` map holds the baked ambient
occlusion in red and two blend masks in green and blue that pick between tiled detail materials (`Dt_Rust_001`
where green, `Dt_Metal_Galvanized_001` where blue, a base material elsewhere), the detail read through the mesh's
extra texture-coordinate sets. The green channel is thus a material mask, not occlusion.

[inferred: OPFOR's channel study of three DICE OcRGB maps (detail_layers.py --masks) and the textures those props' shader entries list; the shaders' channel reads were not disassembled]
[doc: opfor:src/tools/ww2/detail_layers.py:12-15]
[doc: opfor:research/WW2_VISUAL_PARITY.md:26-29]

### MAT-TEX-4: Camouflage is a texture, not a parameter

Status: **verified**

Camouflage patterns are textures (`Camo_*`, `Dt_Camo_*`, `DesertCamo_01_D`, `Ghillie_Camo_C`, the vehicles'
`*_C_Desert` / `*_Snow_C` liveries) bound in generic slots (`texture_Texture<N>`) like any colour or detail layer:
no sampler, constant or engine parameter is named for camouflage. A camouflaged variant is a texture swap, made in
one of two ways: another shader set of the same mesh (vehicles, MAT-TEX-9), or another texture bound to the same
entry by the level's shader database (soldiers and weapons, MAT-TEX-10, MAT-TEX-11). No variant is a separate mesh
or blueprint.

[test: core/tests/test_spec_probes.py::grime]
[data: spec/probes/materials.py, 2026-11-22: 20 distinct DICE ITexture names containing camo]
[data: spec/probes/grime.py, 2026-11-22: every (entry, camo texture) binding over the 74 databases is in a texture_Texture<N> slot (texture_Texture6 86, texture_Texture2 41, texture_Texture3 37, texture_Texture8 25, texture_Texture 18, the rest 72); no pixel shader texture name and no parameter-block constant of the 74 databases contains camo]

### MAT-TEX-5: The dirt and material-mask slots hold shared textures

Status: **verified**

Where a shader names a dirt or material-mask slot, the texture in it is one of three shared tiles: every
`texture_DirtMask`, `texture_DirtMask2` and `texture_DirtMask3` holds `Textures/Vehicles/Dirt/DirtMaskRGB` (but two
helicopter-rotor entries, whose `texture_DirtMask` holds the rotor's own `Huey_Rotors_01_A`), every `texture_Dirt` /
`texture_Dirt2` holds `Dt_Dirt_002`, every `texture_MaterialMask` ... `3` holds `Msk_Plaster_Rust`. The dirt masks
are the vehicles', weapons' and soldiers' (`DirtMask2` mostly weapons'); the material masks the buildings' (rust and
plaster wear over the facade). No per-asset texture is ever bound in these slots: an asset's own dirt is a layer in a
generic slot (MAT-TEX-2).

[test: core/tests/test_spec_probes.py::grime]
[data: spec/probes/grime.py, 2026-11-22: of 9,747 distinct entries (terrain 3,592, objects 2,936, buildings 997, weapons 859, soldiers 627, vehicles 364, vegetation 314, other 58): texture_DirtMask 26 entries (vehicles 10, weapons 10, objects 6; DirtMaskRGB 24, Huey_Rotors_01_A 2; in 69 databases), texture_DirtMask2 51 (weapons 41, soldiers 8, objects 2; in 54), texture_DirtMask3 9 (weapons 8, soldiers 1), texture_Dirt 7 (objects 4, soldiers 3), texture_Dirt2 3 (soldiers), texture_MaterialMask 60 (buildings 59, objects 1), texture_MaterialMask2 55 (buildings 47, objects 7, terrain 1), texture_MaterialMask3 7 (objects)]

### MAT-TEX-6: DirtColor and DirtFactor are per-instance values the engine writes

Status: **verified**

The dirt constants are not a material's setting: in every parameter block that carries them `DirtFactor` (and
`DirtFactor2`) is 0 and `DirtColor` black (one entry grey 0.334), as `Destruction_factor` is 0 everywhere. The
executable lists `dirtColor` and `dirtFactor` at the end of its run of per-instance shader constant names, beside
`destruction_factor`, `PickupSpec`, `turretRotation`, `trackOffset`, `weaponHeat` ...: values the renderer writes per
drawn object over the block's placeholder. The entries that carry them are vehicles' and objects' (one terrain
entry). How an instance's dirt grows is MAT-TEX-7.

[test: core/tests/test_spec_probes.py::grime]
[data: spec/probes/grime.py, 2026-11-22: DirtColor and DirtFactor in 16 distinct entries (objects 8, vehicles 7, terrain 1), DirtColor2 4, DirtFactor2 2, Destruction_factor 23 (vehicles 11, objects 9, weapons 2, terrain 1); block values DirtFactor / DirtFactor2 0 in all, DirtColor (0, 0, 0) 15 and (0.334, 0.334, 0.334) 1, DirtColor2 (0, 0, 0) 4, Destruction_factor 0 in all]
[exe: BFBC2Game.exe string table, file offsets 0x101AF78 - 0x101B1A2: destruction_factor, PickupSpec, LightMap, edgeModelSize, edgeModelPos, edgeModelDir, statModPrimDraw, statModPrimBuil, weaponSpecialization, cameraRoll, cameraPitch, compassRotation, turretRotation, lockWarning, healthWarning, reloadPercentage, weaponHeat, isReloading, lightPower, trackOffset, dirtColor, dirtFactor (each once in the file); the run is looked up by 0x5BF470 and set per drawn instance (MAT-TEX-20)]
[inferred: the run's other names are values only the game knows per object (a turret's angle, a weapon's heat, a track's scroll), so the run is the per-instance constants the renderer fills; the zero block values are then placeholders]

### MAT-TEX-7: Terrain materials carry the dirt trigger

Status: **verified**

Every DICE grid gives its ground materials a `MaterialPropertyTerrainData` on the relation of that material with any
material (the relation's other side null): `DirtTriggerFactor` 0.1 (grass 0.001) and a `DirtTriggerColor`, a dull
brown-grey (red 0 - 0.275, green 0 - 0.269, blue 0 - 0.235; most often 0.255 / 0.228 / 0.148). The materials are
Gravel, Rock, Dirt, Grass, Tarmac, SeaBed, Sand, Mud, Snow and Field. The colour is a Vec3 whose fourth value is
padding: read the binary values (the XML prints it as a large integer). That a vehicle driving on such a material
takes on the colour at the factor's rate (`dirtColor` / `dirtFactor`, MAT-TEX-6) is the code's (round SPG2-rest2,
MAT-TEX-20).

[test: core/tests/test_spec_probes.py::grime]
[data: spec/probes/grime.py, 2026-11-22: 363 MaterialPropertyTerrainData in all 74 DICE materials_win32 grids, every one on a relation with one side null; per material Gravel 58, Rock 54, Dirt 44, Grass 42, Tarmac 40, SeaBed 40, Sand 37, Mud 24, Snow 15, Field 9; DirtTriggerFactor 0.1 x 321, 0.001 x 42 (Grass, every one); DirtTriggerColor 7 - 9 distinct values per channel]
[exe: BFBC2Game.exe names MaterialPropertyTerrainData's fields DirtTriggerFactor, DirtTriggerColor and DestructionEffect beside the other material property classes (MaterialPropertyHealthData, MaterialPropertySootData, MaterialPropertyAIData ...), file offsets 0x1071ACC - 0x1071AF4]
[inferred: the names (DirtTrigger*, dirtColor / dirtFactor) and the per-instance dirt constants on vehicle entries]

### MAT-TEX-8: A mesh's shader sets and who picks one

Status: **verified**

A mesh asset (`Render.RigidMeshAsset`, `SkinnedMeshAsset`, `CompositeMeshAsset`, `TreeMeshAsset`) lists its
`ShaderSets`: `Render.MeshShaderSetAsset` items, each a list of (surface name, shader) in `SurfaceShaders`; set 0 is
the default, a later set lists only the surfaces it changes (the M3A3's desert set 2 of the default's 6 surfaces) or
none. Most meshes have one set. What draws the mesh picks a set: by number on the spawners
(`VehicleSpawnEntityData.MeshShaderSetNumber`, `SoldierSpawnEntityData.MeshShaderSetNumber`) and as an override on
the soldier and its weapons (`SoldierEntityData.MeshShaderSetNumberOverride`,
`SoldierWeaponData.MeshShaderSetNumberOverride`, -1 = none), by reference on placed objects (`MeshShaderSet` of
`ReferenceObjectData`, `StaticModelEntityData`, `DynamicModelEntityData`, `VehicleEntityData` ...: the colour
variants of containers, crates and car wrecks, the level's own building variants).

[test: core/tests/test_spec_probes.py::grime]
[data: spec/probes/grime.py, 2026-11-22: mesh assets by ShaderSets count: objects 1 set 1,090, 2 - 11 sets 193; buildings 1 set 59, 2 - 16 sets 23, none 14; vehicles 1 set 86, 2 - 4 sets 14; soldiers 1 set 29, 2 - 5 sets 34; weapons 1 set 155, 2 - 3 sets 13, none 68; terrain 1 set 423, 2 - 5 sets 45; vegetation 1 set 115, 2 - 5 sets 4. VehicleSpawnEntityData.MeshShaderSetNumber 0 x 1,226, 1 x 99, 2 x 83; SoldierSpawnEntityData.MeshShaderSetNumber 0 x 1,910, 1 x 15, 2 x 20; SoldierEntityData.MeshShaderSetNumberOverride -1 x 71, 1 x 12, 0 x 1; SoldierWeaponData.MeshShaderSetNumberOverride -1 x 337, 1 x 22; MeshShaderSet set on ReferenceObjectData 24,177 of 141,724, StaticModelEntityData 6 of 1,407, DynamicModelEntityData 7 of 61, VehicleEntityData 5 of 98, null on every other class]
[exe: BFBC2Game.exe names MeshShaderSet, MeshShaderSetAsset, MeshShaderSetIndex, MeshShaderSetNumber, MeshShaderSetNumberOverride, ParachuteMeshShaderSet, ShaderSets and UseTeamShaderSet]

### MAT-TEX-9: Vehicle camouflage is shader set 1 (desert) or 2 (snow), picked per level

Status: **verified**

A vehicle with liveries keeps one mesh: its shader set 1 is the desert livery and set 2 the snow livery, each set's
shaders binding the livery textures (`M1A2_C_Desert`, `M3Bradley_Snow` + `M3Bradley_Snowmask`, `Humvee_C_Desert`
...). A vehicle without a desert livery repeats its default set as set 1 (the Russian vehicles), so set numbers mean
the same on every vehicle. The level's vehicle spawners choose: number 1 on the desert levels, 2 on the snow levels.
The Cobra's and Mi28's set 1 is a `_merc` set and the campaign's AH-60 has its own; the M1A2's and M48's set 3 is
empty.

[test: core/tests/test_spec_probes.py::grime]
[data: spec/probes/grime.py, 2026-11-22: 14 vehicle meshes with more than one set; set 1 desert on 6 (Cav, Humvee, M1A2, M3A3, M48, QuadBike), the same as set 0 on 5 (AA-BMD3, BMD3, T90, Truck, Vodnik), merc 2 (Cobra, Mi28), other 1 (AH60_Sp); set 2 snow on 9; set 3 empty on 2. Vehicle entries binding a _Desert / _Snow / snow-mask texture: 8 desert and 18 snow, each entry's shader name naming the same livery. MeshShaderSetNumber 1 on 13 levels (bc1_oasis cq / gr / sdm, mp_001, mp_001sdm, mp_005, mp_005gr, mp_006, mp_006cq, mp_006sdm, sp_006, sp_02, sp_04), 2 on 13 (mp_003, mp_007, mp_007gr, mp_007sdm, mp_008, mp_008sdm, mp_012cq, mp_012gr, mp_sp_002gr, mp_sp_002sdm, sp_004_b, sp_02, sp_04)]

### MAT-TEX-10: Soldier camouflage is bound per level in the level's shader database

Status: **verified**

A soldier's mesh and shader set are the same on every level; its camouflage is not. The same database entry (soldier
mesh / shader) binds another `Camo_*` texture in another level's database, in matched pairs: woodland
(`Camo_US1981Woodland` with `Camo_RUS_03`), desert (`Camo_Digital` with `Camo_RUS_04`), snow (`Camo_Snow_1` with
`Camo_RUS_gorod`). No DBX partition of DICE's names those textures at all, so the choice exists only in the compiled
databases. Vietnam's databases bind their own `US_Camo_02` / `US_Camo_detail`. The soldier meshes' shader set 1 is
the kit-upgrade appearance (`*_KitUpgrade_01` shaders, presumably what `MeshShaderSetNumberOverride` 1 picks), and
the Russian heads' and bodies' set 2 an `all_white` set; the campaign's spawners use sets 1 and 2 on four levels.

[test: core/tests/test_spec_probes.py::grime]
[data: spec/probes/grime.py, 2026-11-22: 58 soldier entries bind a camo texture, 38 of them another one in another database; per database the (US, RUS) pair its soldier entries bind most: woodland 22, snow 15, desert 14, woodland without RUS 2, RUS_03 only 1, none of the six 20 (Vietnam); DBX partitions referencing a Camo_* soldier texture: 0; soldier sets beyond 0: set 1 kitupgrade 20, empty 8, snow 3 (campaign headgear), other 3; set 2 all_white 4, empty 8; SoldierSpawnEntityData.MeshShaderSetNumber 1 on sp_003_b, sp_006, sp_007, 2 on sp_005]
[inferred: the level's camo choice is made when the content build compiles the level's database (a per-level texture substitution); the game only sees the result; the kit-upgrade reading is from the set's shader names and the override's values (-1, 0, 1)]

### MAT-TEX-11: Weapons' camouflage is a tiled detail layer, also bound per level

Status: **verified**

Weapons carry camouflage as a tiled detail texture in a generic slot (`Dt_Camo_Wood`, `Dt_Camo_Desert`,
`Dt_Camo_Snow`; Vietnam's weapons a fixed `DesertCamo_01_D`): the sniper rifles' shaders and some rifles',
sub-machine guns' and machine guns' (mostly their `KitUpgrade_01` shaders). As with the soldiers (MAT-TEX-10), the same weapon entry binds the tile of the
level: on every multiplayer level the tile follows the US soldiers' camo (desert with `Camo_Digital`, wood with
`Camo_US1981Woodland`, snow with `Camo_Snow_1`); four campaign levels keep the wood tile with snow or desert
soldiers. The quad bike instead swaps `Dt_Camo_Wood` / `_Desert` / `_Snow` through its three shader sets
(MAT-TEX-9), and the camouflage net is an OcRGB prop with `Dt_Camonet_RGB_01` as its detail (MAT-TEX-3). No weapon
spawner picks a camo set: weapon pickups never use the team's set.

[test: core/tests/test_spec_probes.py::grime]
[data: spec/probes/grime.py, 2026-11-22: 26 weapon entries bind a Dt_Camo_* tile, 17 of them another tile in another database; per database the tile its weapons bind most: Dt_Camo_Wood 28, Dt_Camo_Desert 13, Dt_Camo_Snow 12; against the US soldiers' camo of the same database: 49 agree, 4 differ (sp_004_b, sp_02, sp_04: wood with Camo_Snow_1; sp_006: wood with Camo_Digital); distinct entries over every database binding DesertCamo_01_D: weapons 68, vehicles 1; Dt_Camonet_RGB_01 objects 4, vehicles 1; Camonet_Oc_RGB objects 7; DynamicWeaponPickupEntityData / WeaponPickupEntityData.UseTeamShaderSet false on all 115]

### MAT-TEX-12: OPFOR weathers clean colour maps up to DICE's class median

Status: **implemented** · Variants: BF1943

OPFOR's WW2 port has no dirt layers on its own materials (MAT-TEX-2's layers come only with a DICE donor entry,
MAT-TEX-19), so its grime pass bakes grime into the colour maps of the custom assets that lack it. It measures a
colour map at the level whose longer side is at most 1,024 texels: fine contrast (the mean absolute Laplacian of
luminance over painted texels, luminance over 12) and local variation (the median standard deviation of 8 x 8
blocks). The target is the median of DICE's own textures of the class: 8 weapon and 7 vehicle colour maps, 5 OcRGB
props, up to 12 terrain tiles picked from the game copy and 14 `Dt_*` material tiles. A map needs grime when its
fine contrast is under 75 % of the target; where the shader layers DICE's grit itself the target is lowered (the
rifle grenades: 0.7).

[code: opfor:src/tools/ww2/grime.py:55-77]
[code: opfor:src/tools/ww2/grime.py:130-142]
[code: opfor:src/tools/ww2/grime.py:193-207]
[code: opfor:src/tools/ww2/grime.py:226-264]
[code: opfor:src/tools/ww2/grime.py:546]

### MAT-TEX-13: The weathering is DICE's own detail textures, at one strength found by bisection

Status: **implemented** · Variants: BF1943

The weathered map is the clean map with DICE's textures tiled across texture space at half their size: the grain of
`Textures/_GENERAL/Dt_Dirt_002` multiplied in (its bright lines kept faint), the sparse scratches of
`Textures/Metal/BrushedMetal` added on metal (metal from the specular map's red channel, else unsaturated colour),
the red channel of `Scratches_01_M` blurred into broad dusty-brown patches; creases darkened from the ambient
occlusion (an `_S` map's green channel) or, without one, from the normal map's curvature, which also brightens convex
metal edges. One strength scales it all: 14 bisection steps find the strength at which fine contrast meets the
target, at most 2.2; per item a lower cap, a weaker patch weight or areas kept near the original (the M1897 stock,
the Shin Gunto blade, the Type 30 guard).

[code: opfor:src/tools/ww2/grime.py:152-156]
[code: opfor:src/tools/ww2/grime.py:269-304]
[code: opfor:src/tools/ww2/grime.py:307-358]
[code: opfor:src/tools/ww2/grime.py:550-559]

### MAT-TEX-14: Weathering never darkens a map by more than 15 %

Status: **verified** · Variants: BF1943

The painted texels' mean brightness after weathering stays within 15 % of the clean map's: first on the float
result, then again on the encoded texture in linear luminance (sRGB decoded when the header's colour flag says so).
An encoding that breaks the budget is blended back towards the original (80 %, 60 %, 40 %, 20 % of the change);
if none fits, the original bytes are kept. The texture header and the alpha are kept.

[code: opfor:src/tools/ww2/grime.py:148]
[code: opfor:src/tools/ww2/grime.py:333-342]
[code: opfor:src/tools/ww2/grime.py:441-470]
[test: opfor:src/tools/tests/test_ww2_weathering_policy.py::test_encoded_weathering_respects_linear_luminance_budget_and_transparency]

### MAT-TEX-15: Only the listed custom assets are restyled; BF1943's artwork is restored

Status: **verified** · Variants: BF1943

The pass styles only four custom assets (the M1897 and Type 38 / Type 30 free models, OPFOR's rifle grenades, the
Shin Gunto repaint). Every other texture it surveys (BF1943's weapons, vehicles, props and Wake's map textures) is
left as its producer wrote it; where an earlier run had weathered one, the recorded weathered copy is replaced by the
tool's own output, and a file that is not a recorded result of the pass (a newer build or a user's edit) is never
overwritten.

[code: opfor:src/tools/ww2/grime.py:143-146]
[code: opfor:src/tools/ww2/grime.py:425-438]
[code: opfor:src/tools/ww2/grime.py:521-530]
[code: opfor:src/tools/ww2/grime.py:574-590]
[test: opfor:src/tools/tests/test_ww2_weathering_policy.py::test_weathering_restores_own_results_and_preserves_newer_edits]
[test: opfor:src/tools/tests/test_ww2_weathering_policy.py::test_weathering_preserves_packed_channels_without_reencoding]

### MAT-TEX-16: The pass records what it wrote, so it can run again

Status: **verified** · Variants: BF1943

Beside a weathered texture the pass keeps the tool's output as `<name>.res.orig` and a `<name>.res.grime.json`
record naming the weathered result's SHA-1 and every earlier result the file may still hold. The record is written
before the texture (both atomically), so a crash between them leaves a file the pass still knows for its own. A
texture whose bytes are no recorded result is the tool's new output and becomes the new original; the survey reads
the `.orig` only while the file is still a recorded weathered copy. Each run starts from the original: the pass is
idempotent.

[code: opfor:src/tools/ww2/grime.py:372-422]
[test: opfor:src/tools/tests/test_ww2_build.py::test_grime_crash_between_writes_keeps_the_original]
[test: opfor:src/tools/tests/test_ww2_grime_source.py::test_survey_source_is_the_orig_only_for_a_recorded_weathered_copy]
[test: opfor:src/tools/tests/test_ww2_grime_source.py::test_survey_source_without_an_orig_is_the_file]

### MAT-TEX-17: What the pass leaves alone, and the formats it keeps

Status: **verified** · Variants: BF1943

On the Wake level's converted textures the pass takes only DXT1 / DXT5 colour textures (the header's colour flag set,
not a normal map by name) and skips by path: sky, ambient occlusion, masks, gloss / specular / occlusion maps,
`RGBM`, decals, dirt maps (grime themselves), water and wakes, flat colour swatches, the intensity map, flags, road
masks, the carrier, terrain quads, shadow cards and razor wire; a skipped texture an earlier run weathered is
restored. The terrain's ground textures fall under the skip list. A Wake copy of an item's texture takes the item's
result byte for byte. A DXT5 map keeps its alpha and a DXT1 map its 1-bit transparency (decoded and re-encoded with
it; without, every transparent texel came back black).

[code: opfor:src/tools/ww2/grime.py:116-129]
[code: opfor:src/tools/ww2/grime.py:174-190]
[code: opfor:src/tools/ww2/grime.py:498-518]
[code: opfor:src/tools/ww2/grime.py:537-545]
[code: opfor:src/tools/ww2/grime.py:591-597]
[test: opfor:src/tools/tests/test_ww2_build.py::test_grime_keeps_dxt1_alpha]
[test: opfor:src/tools/tests/test_ww2_terrain_colour.py::test_ground_build_restores_source_alias_and_weathering_excludes_it]

### MAT-TEX-18: The pass runs after the shader clone pass, and its references are cached by their inputs

Status: **verified** · Variants: BF1943

The WW2 rebuild runs `grime.py --apply` (only with `--apply`) after `wake_shaders.py --clone`, which converts the
BF1943 textures the pass weathers. The DICE reference picks for the map classes are cached in `grime_refs.json` under
a hash of everything they depend on but the textures' bytes (the patterns, the candidates found in the game copy's
index, the material tiles, a version); any change, or a cache without the key, picks again. The before / after
sheets (`grime.py --sheet`, `grime_models.py`) are pictures for review: a 256-texel crop of each weathered map's
busiest painted area beside a DICE texture of its class at the same texel density, and the weathered weapons and
vehicles rendered whole from `.orig` (before) and the weathered file (after); nothing is installed from them.

[code: opfor:src/tools/ww2/rebuild_all.py:99-103]
[code: opfor:src/tools/ww2/grime.py:157-164]
[code: opfor:src/tools/ww2/grime_sheet.py:15-29]
[code: opfor:src/tools/ww2/grime_sheet.py:48-83]
[code: opfor:src/tools/ww2/grime_models.py:30-50]
[test: opfor:src/tools/tests/test_ww2_build.py::test_rebuild_step_order]
[test: opfor:src/tools/tests/test_ww2_build.py::test_from_selects_exactly]
[test: opfor:src/tools/tests/test_ww2_build.py::test_grime_ref_cache_is_invalidated]

### MAT-TEX-19: Ported materials borrow DICE's layers from a donor entry

Status: **implemented** · Variants: BF1943

A ported BF1943 material (colour map + normal + specular, no layers) can take a DICE entry of the layered scheme as
donor: the donor's colour, normal and specular samplers get the port's own textures, its shared layers (`Dt_*`,
`Nm_General*`, detail normal atlases, `Textures/_MASK/`, metal tiles, vehicle tiles, the shared dirt masks,
`Detailmetal_N`, `Scratches_*_M`) stay. A donor qualifies when its programs were compiled for the port's vertex
layouts, it sets every constant the port's shader sets, it has no unique texture of another role (a decal sheet, its
own tile map) and no normal, specular or sky reflection the port lacks; the best is the one with the preferred name,
a normal map and the most layers. OcRGB donors are left out: their masks need extra texture-coordinate sets the
ports do not have (MAT-TEX-3).

[code: opfor:src/tools/ww2/detail_layers.py:45-66]
[code: opfor:src/tools/ww2/detail_layers.py:96-135]
[doc: opfor:src/tools/ww2/detail_layers.py:1-25]
[doc: opfor:research/WW2_VISUAL_PARITY.md:37-53]

### MAT-TEX-20: Dirt builds up while a vehicle moves over a dirt-trigger material

Status: **observed** · Variants: PC

The client entity keeps a dirt factor (+0x1D0, 0 - 1) and a dirt colour (+0x180, three floats). Each update
(0x884C20, with the frame time) it measures its speed; under 5 m/s nothing happens. Otherwise it takes the material
under it and that material's `MaterialPropertyTerrainData` (MAT-TEX-7; none: nothing happens), and with a =
`DirtTriggerFactor` x dt:

- a clean entity (factor 0) takes `DirtTriggerColor` as its colour at once;
- factor = clamp(factor + a, 0, 1);
- colour = colour x (1 - a) + `DirtTriggerColor` x a.

So DICE's 0.1 dirties a vehicle fully in 10 s of driving at 5 m/s or more, grass (0.001) in about 17 minutes, and the
colour fades towards the last ground driven on. Only the client vehicle runs it (round SPG2-rest3):
`ClientVehicleEntity` (vtable 0x01436C38, "ClVehicle" in its slot 43) has 0x8C0210 in slot 125, which calls 0x893E50
-> 0x884C20 when its flags argument has bit 0 set; no other vtable holds 0x8C0210 and 0x893E50 has no other caller,
so soldiers, debris and server entities never get dirty. Nothing in that code lowers the factor: no washing in water or
decay was found (the factor's only other writer is the entity's set-up). The drawing reads the two through getters
(0x877520 the factor, 0x877530 the colour), copies them into the drawn instance (+0xEC or +0xDC the factor, +0xB0 the
colour) and sets the shader constants `dirtFactor` / `dirtColor` (MAT-TEX-6) through the handles the instance's table
looked up by name. A RenderDoc capture before and after driving over Mud confirms the values.

[exe: BFBC2Game.exe 0x884C20 (speed `sqrt(v.v)` against 5.0 at 0x14D9450; material 0x7BF160 -> 0x6C94E0 -> 0x6F0250, the terrain property at +0x10 with DirtTriggerColor +0x10 and DirtTriggerFactor +0x20 as MaterialPropertyTerrainData's field descriptors 0x1C35A88 / 0x1C35AB8 give; clamp with 0 and 1.0 at 0x14D94F0; the colour blend 0x884D28 - 0x884DB5), called from 0x893E9C; getters 0x877520 (+0x1D0) / 0x877530 (+0x180); copies 0x5EA24E - 0x5EA27B and 0x5EFB81 - 0x5EFBA9; constants set 0x5CB733 / 0x5CB7A7 (handles +0x6D0 / +0x6D4 of the table 0x5BF470 fills: dirtFactor, dirtColor, trackOffset ...; built by 0x5DD9D0)]
[exe: BFBC2Game.exe 0x893E50 (only caller 0x8C0282 in 0x8C0210, `test flags, 1`); 0x8C0210 is slot 125 of vtable 0x01436C38 (175 slots; slot 43 returns "ClVehicle"), its only pointer in the image (0x01436E2C)]

## 6. OPFOR's WW2 materials

### MAT-WW2-1: The merged grid: DICE's relation first, then BF1943's, then a stand-in

Status: **implemented** · Variants: BF1943

OPFOR's WW2 levels run a merged grid: Isla Inocentes' materials, every BF1943 material of the level and the BFBC2
materials its data names. Each pair's relation is DICE's when any BFBC2 grid holds the pair (BFBC2 weapons keep
their balance), else BF1943's, else a stand-in (the nearest BFBC2 material's relation with the BF1943 material in its
place: an airframe against a shotgun pellet takes Attack_Helo's).

[code: opfor:src/tools/ww2/material_grid.py:343-369]
[code: opfor:src/tools/ww2/material_grid.py:55-63]
[data: spec/probes/materials.py, 2026-11-22: 4 materials_xenon grids, 104 - 111 materials, 4,049 - 4,706 relations]
[doc: opfor:research/PHYSICS_MATERIALS.md:84]

### MAT-WW2-2: DICE's re-assignment of the shared objects

Status: **observed** · Variants: BF1943

Of the 88 physics files BF1943 and BFBC2 share, 48 differ (6 only in list order, 11 only in faces, 31 in part
materials). DICE's part changes reduce to role rules: jetty decks Brick -> Bridge (R1), small loose wooden props ->
Wood_Banger (R2), static wooden pieces Brick -> Wood (R3), thin wooden panels -> Wood_Fragile (R4), open metal
Metal -> Metal_Seethrough (R5), thin metal tanks -> Metal_Light (R6), razor wire part Metal_Light with RazorWire faces
(R7); R8 (airframes to Metal_Light) was forced by DICE's grid lacking airframe materials.

[data: OPFOR's physics-material census of 2026-11-09 (physmat scripts): 88 shared HavokPhysicsData, 48 differing, 31 part-material changes]
[doc: opfor:research/PHYSICS_MATERIALS.md:89-162]

### MAT-WW2-3: OPFOR's part-material decisions

Status: **implemented** · Variants: BF1943

OPFOR copies DICE's re-assignment by role, not by name: WallWooden's intact panel Wood -> Wood_Fragile; the two
antenna bangers defaultMaterial -> Metal; everything else keeps BF1943's material (the dragon-teeth chunks Brick, the
vehicle shed's panels Metal_Light, the log wall Brick). The Zero and Corsair take BF1943's airframe materials
(DICE's Metal_Light took about 3x the rifle damage), the bombers keep Bomber_Airframe. Each change is written to the
part byte and `PartInfoData.MaterialName` together (MAT-SLOT-2).

[code: opfor:src/tools/ww2/bf1943_objects.py:122-133]
[code: opfor:src/tools/ww2/bf1943_vehicles.py:352-392]
[doc: opfor:research/PHYSICS_MATERIALS.md:19-35]

### MAT-WW2-4: Wire members of the WW2 groups hit as RazorWire

Status: **implemented** · Variants: BF1943

On the WW2 levels' static model groups, a wire member's ChickenWire faces become RazorWire where the group's
`HavokMaterials` hold it (DICE's R7 on the face, the hit material of a group member): RazorWire has ChickenWire's
flags and a bullet effect and sound. Metal (R5's face) is not used: it is not penetrable.

[code: opfor:src/tools/ww2/wake_level.py:970-982]
[code: opfor:src/tools/ww2/ww2_level_build.py:259-293]

### MAT-WW2-5: The air raid's bombs against the WW2 soldiers

Status: **inferred** · Variants: BF1943

The WW2 soldiers are on DICE's Vietnam soldier materials (`SoldierBody_Vietnam`, `SoldierHead_Vietnam`), against
which the merged grid has no relation for BF1943's bomb explosions. For the air raid's bombs only, those materials
take BF1943's `SoldierBody` / `SoldierHead` relations; every other weapon keeps its relation against the soldiers.

[inferred: OPFOR's air-raid design on the unmerged branch ww2/air-raid (its AIR_RAID_SPEC and material_grid's SOLDIER_COUNTERPARTS); not in the main checkout or the game copy yet]

## Open questions

- **The hit material on plain placed objects** (MAT-PEN-3): the playtest probe of MATERIAL_SLOTS_RE section 4
  (shoot a hangar wall, Brick part with metal_light faces, logging the material at 0x722990 / 0x8D6730 and 0x822D60)
  settles it; a ladder shot (Metal_Seethrough part, metal faces) is the cleanest penetration check.
- **The damage arithmetic** (MAT-GRID-8): following 0x6E44A0 (the damage relation lookup) into the damage path would
  confirm multiplier and threshold and say what `DamagePenetrationMultiplier`, `ExplosionCoverDamageModifier` and
  `InflictsDemolitionDamage` do (the last is true on 78 % of DICE's damage items; demolition presumably means damage
  to destructible parts, DSTR-PART-3).
- **Which side `SourceMaterial` names** (MAT-GRID-6): the attacker's material is the likely reading; a relation whose
  two items differ, read against the damage path, settles it.
- **SeeThrough and the AI** (MAT-PEN-2): the AI's visibility ray test was not traced.
- **The entity default material** (MAT-SLOT-5): that `entity+0x30` is the DBX `Material` field is presumed; the
  entity data class's field table in the executable would show it.
- **OcRGB channels** (MAT-TEX-3): a disassembly of one OcRGB prop's pixel shader (which channel scales which
  sampler) would make the rule observed.
- **Dirt in game** (MAT-TEX-20 is read statically; client vehicles only, round SPG2-rest3): a RenderDoc capture of a vehicle before and after driving over Mud to confirm the
  constants. Which DirtMaskRGB channel a dirt shader reads is unread.
- **Who makes the per-level camo** (MAT-TEX-10, MAT-TEX-11): no shipped DBX names the soldier camo textures, so the
  level's choice was made by DICE's content build; a level added by a modder needs its database compiled (or merged)
  with the camo of its theme. Whether the game can switch camo at run time (it seems not) is open; the four campaign
  levels whose weapons keep the wood tile with snow or desert soldiers may be DICE's oversight.
- **Shader-set fall-back** (MAT-TEX-8): that a surface a later set does not list draws with set 0's shader is the
  reading of the M3A3's sets, not traced; the meaning of `MeshShaderSetIndex` (in the executable, in no DICE DBX
  field) is unknown.
- **The soldiers' set 1 and set 2** (MAT-TEX-10): that `MeshShaderSetNumberOverride` 1 is the rank's kit upgrade and
  what the Russian `all_white` set is for (a cutscene or a debug look) are unconfirmed.
- **OPFOR's grime against DICE's runtime dirt** (MAT-TEX-12): the pass bakes static grime into colour maps; DICE's
  vehicles get dirty at run time (MAT-TEX-7). Giving the WW2 vehicles DICE's dirt constants (a donor entry carrying
  `DirtColor` / `DirtFactor`, MAT-TEX-19) was not tried.
- **The grid index at run time** (MAT-GRID-2): whether the game resolves a part's material by hash against the level's
  grid when the level loads (the likely reading, since indices differ per level) or at each hit.
- **The air raid** (MAT-WW2-5) becomes implemented when `ww2/air-raid` is merged.
