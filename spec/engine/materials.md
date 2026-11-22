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

Status: **observed** · Variants: PC, BF1943

A level's materials are one partition `Levels/<LV>/Materials_Win32` (BF1943's levels: `materials_xenon`) holding one
`Entity.MaterialGridData` instance: `Name`, `Materials` (references to `Entity.MaterialData`), `MaterialRelations`
(references to `Entity.MaterialRelationData`) and `MaterialGridLayers`, empty in every grid. The level names it in
`LevelData.MaterialGrid` (LEVEL-DATA-1). A level bundle may also hold its other modes' grids (MP_001's bundle holds
MP_001SR's, MP_006's those of MP_006CQ and MP_006SDM).

[data: spec/probes/materials.py on the game copy, 2026-11-22: 81 materials_win32 grid partitions (74 from DICE bundles), 51 - 92 materials and 953 - 3,160 relations each; 4 materials_xenon partitions (OPFOR's WW2 levels), 104 - 111 materials, 4,049 - 4,706 relations; MaterialGridLayers items over all grids 0]

### MAT-GRID-2: A grid holds only the level's materials, in its own order

Status: **observed**

Each grid lists the materials its level uses, not a shared catalogue: DICE's 74 grids name 121 distinct materials
between them, and 114 of the 121 sit at different indices of `Materials` in different grids. A material's grid index
(the low part of its runtime id, HAVOK-MAT-2) is therefore per level; across levels a material is the same only by
its name and `Hash` (MAT-GRID-3).

[data: spec/probes/materials.py, 2026-11-22: 121 distinct MaterialData names in DICE's 74 grids; 114 of them at more than one index of MaterialGridData.Materials]
[exe: the runtime material id (grid index plus flag bits) is built at 0x10029E0]
[doc: opfor:research/MATERIAL_SLOTS_RE.md:17-21]

### MAT-GRID-3: MaterialData and its Hash

Status: **observed**

An `Entity.MaterialData` has `DebugColor` (4 floats), `Category` (`Construction`, `Projectiles`, `Explosions`,
`Vehicles`, `Terrain`, `Vegetation`, `Soldier`, `Default`), `Name`, five flags (`Penetrable`, `ClientDestructible`,
`SeeThrough`, `NoCollisionResponse`, `NoCollisionResponseCombined`) and `Hash`, a u32 that is always the 33-xor hash
of the lower-case name: `h = h * 33 ^ byte` from 5381, the function the physics wrapper uses for part names
(`bc2_formats::havok::part_hash`). The same name has the same hash in BFBC2 and BF1943.

[data: spec/probes/materials.py, 2026-11-22: Hash equal to the 33-xor hash of the lower-case Name on 5,746 of 5,746 MaterialData instances of DICE's grids; categories Construction 1,771, Projectiles 1,135, Explosions 692, Vehicles 677, Terrain 573, Vegetation 395, Soldier 355, Default 148]
[code: rust/bc2-formats/src/havok.rs:587-591]
[doc: opfor:src/tools/ww2/material_grid.py:333-334]

### MAT-GRID-4: The flags belong to the name

Status: **observed**

No material's flags differ between DICE's grids. 23 of the 121 materials are `Penetrable`, 9 `SeeThrough`, 9
`ClientDestructible`, 3 `NoCollisionResponse` (glass_breakable, the two tree branch materials), 5
`NoCollisionResponseCombined` (the tank materials, tree_static_trunk, wood). Every `SeeThrough` material is also
`Penetrable` (chickenwire, glass_breakable, glass_bulletproof, metal_seethrough, middlegrowth_seethrough,
razorwire, water, water_pool, wood_seethrough). The other Penetrable ones: fabric, huey, metal_light, middlegrowth,
the four soldier body / head materials, transport_helo, the two tree branch materials, wood, wood_banger,
wood_fragile. Brick, concrete, metal and stone are not penetrable.

[data: spec/probes/materials.py, 2026-11-22: 0 of 121 names with differing flags; Penetrable 23, SeeThrough 9 (all 9 also Penetrable), ClientDestructible 9, NoCollisionResponse 3, NoCollisionResponseCombined 5]

### MAT-GRID-5: Relations: two materials and their data

Status: **observed**

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

[data: spec/probes/materials.py, 2026-11-22: DICE's grids: relations with a null side 3,494; data items per relation 1 x 66,833, 2 x 49,068, 3 x 34,283, 4 x 13,800; the class counts above]

### MAT-GRID-6: Two damage items per relation, one per source material

Status: **observed**

A relation holds no, one or two `MaterialRelationDamageData` items; where it holds two, both name a
`SourceMaterial` (27,728 of 27,749), so one relation carries the damage in both directions of the pair, each item
for one side as the source.

[data: spec/probes/materials.py, 2026-11-22: DICE's relations by damage items: 0 x 28,783, 1 x 107,452, 2 x 27,749; of the 27,749, every item with SourceMaterial set in 27,728]

### MAT-GRID-7: The damage fields and their ranges

Status: **observed**

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

Status: **observed**

The material property data (the `MaterialProperty*` classes among a relation's data) is nearly constant in DICE's
grids:
`MaterialPropertyPhysicsData` friction and restitution modifiers are all 1 (651), `MaterialPropertyImpulseData`'s
`ImpulseAbsorptionMultiplier` is 0.1 on all 939, `MaterialPropertySoundData.Softness` 0 on all 3,352,
`MaterialPropertyTerrainData.DestructionEffect` null on all 363 (`DirtTriggerFactor` 0.1 x 321, 0.001 x 42).
`MaterialPropertySoundData` carries the soldier sounds: `SoldierSettings` set on 3,298, each a
`MaterialSoldierSoundSettings` with `FootStepSound` (2,067 set) and `LandSound` (all 2,167); `ProneSound` is null on
all.

[data: spec/probes/materials.py, 2026-11-22: DICE's grids, the counts above]

## 2. Material slots

### MAT-SLOT-1: The wrapper's material hashes are the names' 33-xor hashes

Status: **observed**

The u32 material hashes at the end of a physics wrapper (HAVOK-WRAP-1, HAVOK-MAT-1) are the 33-xor hashes
(MAT-GRID-3) of the HavokAsset partition's `MaterialMappings` names, in order. A material can therefore be appended
to a file's list with its hash computed from its name; no donor file is needed for the hash.

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

Status: **observed**

DICE's 4,454 parts use 65 materials, every one in some DICE grid. The most used: brick 898, metal 425,
wood_furniture 350, glass_breakable 332, wood 190, wood_fragile 182, concrete 174, defaultmaterial 167,
tree_banger_branch 125, concrete_indestructible 122. 1,264 parts are on a Penetrable material, 520 on a SeeThrough
one, 427 on a ClientDestructible one. Brick is DICE's standard material for destructible building walls, whatever
their faces show (hangar walls: Brick parts, metal_light faces).

[data: spec/probes/materials.py, 2026-11-22: PartInfoDatas of the 1,183 DICE physics assets: 4,454 parts, 65 distinct materials, none outside DICE's grids]
[doc: opfor:research/PHYSICS_MATERIALS.md:141-145]

### MAT-SLOT-4: Face lists

Status: **observed**

A physics asset's `FaceMaterials` (the face materials its mesh subparts use) holds 1 - 9 names (1 in 532 files, 2 in
337, 3 in 183); in 525 of 1,183 files it equals `MaterialMappings`, in the rest the part materials and face materials
differ (the list in the wrapper and the packfile's `HavokMaterials` are `MaterialMappings`, HAVOK-MAT-1).

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

### MAT-TEX-2: Grime and dirt are shader layers, not painted maps

Status: **observed**

DICE's dirt and wear come from shader layers over the asset's own maps: dirt-mask samplers (`texture_DirtMask`,
`texture_DirtMask2`), material-mask samplers (`texture_MaterialMask`, `texture_MaterialMask2`) and the dirt
constants `DirtColor` / `DirtFactor` (and `DirtColor2` / `DirtFactor2`), with shared tiled textures (`DirtMaskRGB`,
`Dt_Dirt_00N`, `Msk_Generic`, `Msk_Scratch_Tear`, `Dt_Rust_001`). No database names a grime constant.

[data: spec/probes/materials.py, 2026-11-22: of 74 DICE Dx11 shader databases, texture_DirtMask in 69, texture_DirtMask2 54, texture_MaterialMask 48, texture_MaterialMask2 47, DirtColor and DirtFactor 36, DirtColor2 10, DirtFactor2 6, texture_OcclusionTexture 74]
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

Status: **inferred**

Camouflage patterns are textures (`Camo_*`, `Dt_Camo_*`, ghillie colour maps) bound like other colour or detail
layers, not a shader parameter: a camouflaged variant is a texture swap.

[inferred: 20 distinct DICE texture names contain camo, and a name scan of MP_005's Dx11 database (exploration for this file, not a probe output) found camo only in texture names, among constants and samplers none]
[data: spec/probes/materials.py, 2026-11-22: 20 distinct DICE ITexture names containing camo]

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
  sampler) would make the rule observed; the values of `DirtFactor` per material were not measured.
- **The grid index at run time** (MAT-GRID-2): whether the game resolves a part's material by hash against the level's
  grid when the level loads (the likely reading, since indices differ per level) or at each hit.
- **The air raid** (MAT-WW2-5) becomes implemented when `ww2/air-raid` is merged.
