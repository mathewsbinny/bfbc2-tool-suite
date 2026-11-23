# Animation and characters

What BFBC2's soldiers are made of and how they move: the one soldier skeleton, the compiled animation trees and what
they bake, the weapon zoom rules learnt from the WW2 weapons, soldier kits as DBX classes, the skinning kinds the
renderer needs, the ragdoll. Written by round SPG2 (2026-11-22) for the queued translation of soldiers and weapons
and any new kit or weapon; the formats are [formats/granny.md](../formats/granny.md) and
[formats/aimanim.md](../formats/aimanim.md), the resource catalogue RES-TYPE-1, the weapons' data
[weapons.md](weapons.md). Counts are of DICE's bundles (the 1,021 `.fbrb` under `game\Package` and
`game\Dist\win32` dated before 2013, the first copy of each name), read with the suite's DBX and bundle readers on
2026-11-22 (round SPG2's research scripts); the in-game findings are OPFOR's RenderDoc playtests of 2026-11-08 to
2026-11-13 on the WW2 weapons.

## 1. Resources and the skeleton

### ANIM-RES-1: The animation resources DICE ships

Status: **observed** · Variants: PC

| Type | Entries | Distinct names | Where |
|---|---:|---:|---|
| `AnimTreeInfo` (compiled trees) | 2,594 | 1,926 | `animations/cutscenes` 1,221, `objects/weapons` 251, `characters/animtree` 72, `objects/vehicles` 62, the rest per campaign level |
| `GrannyAnimation` | 11,381 | 4,792 | |
| `DeltaAnimation` | 1,272 | 295 | |
| `AimAnimation` | 815 | 266 | `animations/weapons` 258, `characters/animations` 8 |
| `GrannyModel` | 126 | 73 | |
| `RagdollResource` | 2 | 1 | `characters/ragdolls/ragdoll01` |

RES-TYPE-1's larger numbers count the whole game copy, OPFOR's bundles included.

[data: DICE's 1,021 bundles, 2026-11-22: entry types from the bundle headers; distinct = first copy of each lower-case name]

### ANIM-SKEL-1: One soldier skeleton, ske01, of 147 bones

Status: **verified** · Variants: PC, BF1943

Every soldier animation binds to `characters/skeletons/ske01` (a `GrannyModel`): 147 bones, bone 0 `grannyRootBone`,
19 weapon bones `jntWpn_*` (`jntWpn_6` and `jntWpn_7` children of `jntWpn_1`; every weapon bone's bind pose the weapon
root), 28 facial bones `FJ_*`, `LeftHand` / `RightHand`. Its partition's `SkeletonAsset` has `AccumulateMotion` true,
`MinBoneIndex` 18, `MaxMainBoneIndex` 146. The only other skeleton under `characters/` is `skevehicle` (4 bones,
`AccumulateMotion` false). BF1943's ske01 has the same 147 bones and parents in another order: an animation's
per-bone table is renumbered by name.

[test: bfbc2-anim-tools/tests/test_granny.py::main]
[code: bfbc2-anim-tools/tests/test_granny.py:147-157]
[data: DICE's bundles, 2026-11-22: ske01 147 bones (root grannyRootBone, 19 jntWpn_*, 28 FJ_*), skevehicle 4; 2 SkeletonAsset in characters/skeletons with the values listed]
[code: bfbc2-bf1943-import/bf1943import.py:21-23]

### ANIM-SKEL-2: BF1943's animations port by bone name, with no retargeting

Status: **implemented** · Variants: BF1943

A BF1943 soldier animation is ported by renumbering its per-bone table onto BFBC2's ske01 by bone name. The two
skeletons differ (BFBC2 binds the forearm-roll and hand bones 70 degrees differently, its Eyes bone sits 3.16 cm off)
and DICE did not compensate: DICE's own copies of shared animations match the converted ones at 0.000 degrees on the
common tracks where DICE did not re-author them (60 of 533 compared; 473 re-authored, 71 with other durations; the 33
shared aim animations structurally identical).

[code: bfbc2-anim-tools/granny.py:688-693]
[doc: opfor:research/WW2_WEAPON_ANIM_AUDIT.md:67-73]

### ANIM-FRAME-1: Game data is right-handed: a soldier faces +z, his right is -x

Status: **implemented** · Variants: PC, BF1943

Soldiers, weapons and vehicles are modelled facing +z with y up, the soldier's right at -x. A view that takes "up x
forward" as screen right is mirrored (every OPFOR preview before 2026-11-08 was).

[code: opfor:src/tools/ww2/weapon_preview.py:311]
[code: opfor:src/tools/ww2/wake_preview.py:226-227]
[code: opfor:src/tools/ww2/pictures.py:9]

## 2. Compiled animation trees

### ANIM-TREE-1: A compiled tree names no animation; animations bind by place

Status: **implemented** · Variants: PC, BF1943

An animation tree ships as a partition (`AnimTreeData`, `ResultAnimTreeData`, `ParameterGroupData`) plus its compiled
`AnimTreeInfo` resource; the resource holds no animation GUID or name: the partition lists the tree's animations
(`AnimationAsset` / `DeltaAnimationAsset` partitions, each naming its Granny resource), in place. A tree can be cloned
with each listed animation replaced in the same place and the compiled resource copied; a delta animation may only
replace a delta animation. BFBC2 ships no tree source, so BF1943's tree partitions are not ported: OPFOR clones DICE's.

[code: opfor:src/tools/ww2/animswap.py:4-9]
[code: opfor:src/tools/ww2/animswap.py:48-79]
[data: DICE's animations/weapons partitions, 2026-11-22: AnimationAsset 1,922, AimAnimationAsset 258, DeltaAnimationAsset 206]
[doc: opfor:research/WW2_WEAPON_ANIM_AUDIT.md:75-78]

### ANIM-TREE-2: The compiled tree bakes each hand's IK target

Status: **implemented** · Variants: PC

DICE's tree compiler bakes the tree's `HandsIkPose` into the `AnimTreeInfo` resource: per hand, the hand's transform
relative to the gun bone `jntWpn_1` as 12 floats (its x, y and z axes as rows, each row's fourth word 0xCDCDCDCD), the
position (3 floats), and the hand index (0 left, 1 right) as a u32 16 bytes after the position. Copying a donor's
compiled tree keeps the donor's hand targets even when the `HandsIkPose` partition is swapped: the ported K98 and
Springfield held their grips 3.3 / 3.5 cm off until the records were rewritten. A record is found by the stock pose's
values (position within 2e-4, rotation within 2e-3) and must match exactly once.

[code: opfor:src/tools/ww2/animswap.py:82-124]
[data: OPFOR's RenderDoc playtest 2026-11-08: the ported rifles' hands off the grips with the donor's baked targets, on them after the rewrite]

### ANIM-TREE-3: Same-base weapon trees keep their slot durations at fixed offsets

Status: **inferred** · Variants: PC

Two first-person weapon trees built on the same base tree have the same compiled size (the M24's and the M1's: 36,388
bytes) and their slot durations as f32 seconds at 0x5100 - 0x5810: a word there that differs between the two is a
duration unless it differs by the trees' pointer shift (their words at +4) or belongs to a kept zoom animation.
OPFOR's scoped trees are the M24's resource with the rifle's own durations put back and the hands re-baked
(ANIM-TREE-2). The offsets come from pairwise differences, not a decoded layout. Round SPG2-rest4 placed them in the
compiled tree's layout: 0x5100 - 0x5810 lies in block B (0x4FB4 - 0x5824 in all 10 DICE trees of those sizes), the
flat data block whose one pointer is its first word (ATREE-BLK-2), so the durations sit in plain data and their
rewrite moves no pointer; which words of block B are durations is still matched by differences, not by the reader.

[inferred: pairwise differences of same-size compiled trees; DICE's first-person weapon trees are 36,388 (2), 36,836 (2), 36,468, 36,292 and 36,500 bytes]
[data: spec/probes/resources.py's block sizes, round SPG2-rest4 2026-11-23: block A 0x4FA0 and block B 0x870 bytes in all 10 DICE trees of those five sizes, so B runs 0x4FB4 - 0x5824 in the file]
[code: opfor:src/tools/ww2/weapons.py:148-181]

## 3. Zoom

### ANIM-ZOOM-1: Hand IK in a scope's zoom is switched off in the compiled tree

Status: **observed** · Variants: PC

Whether hand IK stays on in the zoomed pose is decided by the compiled tree, not the data: the M1's keeps it on, the
M24's does not. A scope on a tree with IK on pulls the arms 0.5 - 0.9 m forward onto the camera axis, into the scope
view; `ZoomRenderFov` is not the cause (changing it was reverted).

[data: OPFOR's RenderDoc playtest 2026-11-08: post-VS arm positions in the scoped Garand / Type 5 zoom; fixed by the M24-based tree, confirmed by the user]
[code: opfor:src/tools/ww2/weapons.py:139-153]

### ANIM-ZOOM-2: A FOV specialisation the weapon lacks draws no world

Status: **observed** · Variants: PC

A zoom level with `UseFovSpecialisation` true takes its field of view from the weapon's scope-power specialisation; on
a weapon whose specialisations do not provide it the game draws no world when zoomed (grey) and keeps not drawing it
after the zoom ends. OPFOR sets it false on the scoped Garand and Type 5.

[data: OPFOR's RenderDoc playtest 2026-11-08: the grey zoom]
[code: opfor:src/tools/ww2/weapons.py:303-306]

### ANIM-ZOOM-3: A sniper zoom on a rifle's gun sway needs the sniper's multipliers zeroed

Status: **observed** · Variants: PC

The scoped Garand / Type 5 need the sniper's `FirstPersonCamera`, `WeaponClass`, `AlwaysAimHead` and `ZoomType` on
the zoom, and `RecoilMultiplier` 1, `RecoilFovMultiplier` 1, `SwayPitch` / `SwayYaw` multipliers 0: the sniper class's
multipliers applied to a rifle's own gun sway make the zoomed projection NaN (worldViewProj, the shadow transforms
and the focal length NaN; the screen grey for good). WPN-ZOOM's rules do not cover this.

[data: OPFOR's RenderDoc playtests 2026-11-08 and 2026-11-13: NaN matrices in the zoomed draws]
[code: opfor:src/tools/ww2/weapons.py:265-302]
[doc: opfor:research/WW2_WEAPON_ANIM_AUDIT.md:21]

### ANIM-UNDEP-1: DICE added 25 tracks to every third-person undeploy

Status: **verified** · Variants: PC, BF1943

BF1943's third-person undeploys (`StandUnDeploy`, `CrouchUnDeploy`) have 63 tracks; DICE's PC versions of all 22 shared
ones have 88: 19 weapon-part bones and 6 thumbs added. DICE holds the weapon parts at the same weapon's same-stance
deploy's last frame (exact on 15 of 22); the thumbs are animated. The importer adds the 25 tracks the same way (weapon
parts from the deploy's last frame, or from the hands' IK pose when the deploy parks a part 0.5 m or more away; curve
formats from DICE's Garand undeploy; thumbs from a donor).

[test: bfbc2-bf1943-import/tests/test_bf1943import.py::undeploys]
[code: bfbc2-bf1943-import/bf1943import.py:636-711]
[doc: opfor:research/WW2_WEAPON_ANIM_AUDIT.md:80-84]

## 4. Soldiers and kits

### ANIM-RIG-1: The soldier rig's named bones in DBX

Status: **observed** · Variants: PC

`Objects/Soldiers/Common` holds 6 `SharedSoldierAnimationTreeData`, all with `WeaponBoneName` `jntWpn_1` and
`HipBoneName` `Hips` (`CameraBoneName` `cameraJoint` on 4, empty on 2), with `BaseSetAnimTrees`, `DeathAnimTree`,
`Skeleton`, `BoneCollision`, `RagdollSettings`, `UpperBodyRotation`, `CameraFovBoneName`. ske01's partition holds one
`SoldierIKData` (`WeaponBoneName` `jntWpn_1`, `WeaponBoneIndex` 10, two hands of `HandBoneName` / `HandIkProperty` /
`HandInfluenceMap` / `BoneIndices`, the legs, the IK LODs).

[data: DICE's bundles, 2026-11-22: SharedSoldierAnimationTreeData 6, SoldierIKData 1 (characters/skeletons/ske01), BonePriority 9, InfluenceMapEffectData 3, SoldierProceduralLeaningData 1 in characters/skeletons]

### ANIM-KIT-1: A kit is a SoldierEntityData

Status: **observed** · Variants: PC

`objects/soldiers/` holds 84 `SoldierEntityData` (73 fields, among them `Weapons`, `WeaponUnlocks`, `AnimTree1p` /
`3p`, `Meshes1p` / `3p`, `HidableMeshParts`, `DeathPickup`, `RagdollSettings`, `MaxHealth`, `Specializations`,
`MeshShaderSetNumberOverride`, `Licenses`), each in a `SoldierAsset` with one `PlayerEntryComponentData`,
`FPSCameraData` and `FakeHingeData`; by folder sp_ru 19, sp_mec 14, sp 13, beach 10, ru 8, us 8, nam_nva 4, nam_us 4,
sp_us 4. The us / ru kits are 4 classes and 4 SPECACT kits each (licences `ka` / `ke` / `km` / `kr`); multiplayer
kits have 9 `Weapons` slots, 45 / 47 / 117 `WeaponUnlocks`, `MaxHealth` 100 (campaign 50 - 125). The slots: 0 primary,
1 pistol, 2 gadget 1, 5 gadget 2, 7 knife, 8 grenade; a `WeaponUnlocks` entry's `WeaponSlot` is that index, a rifle's
launcher (2) and bayonet (7) following it with `IsBundledWithSelectable` (whether the flag hands them over is untested).
Beside them: 168 `SoldierAnimationTreeData`, 86 `IndexedAnimationData`, 25 `SoldierSpawnTemplateData`.

[data: DICE's bundles, 2026-11-22: the class counts, folders and field values listed]
[code: opfor:src/tools/ww2/kits.py:27-34]
[doc: opfor:research/WW2_KITS.md:85-92]

### ANIM-KIT-2: A kit's body: one mesh, kit joints and a static pose

Status: **implemented** · Variants: PC, BF1943

The Vietnam and campaign kits draw the body as one mesh posed by a kit-joints tree and a static pose
(`SoldierAnimationTreeData.KitJointsAnimTree`, `StaticPose`, `StaticJoints`: the eye joints, `StaticPoseInfluenceMap`).
DICE's campaign and Vietnam kits name `Characters/InfluenceMaps/EyePose`, which ships nowhere (`Eyes` does: 6 eyelid /
eyeball bones). BF1943 weights helmets, chin straps and buckles to face bones (Jaw, `FJ_LipLower*`, `FJ_Tongue`) at
weight 1 in its kit-joints map, where BFBC2's gives those bones 0 (facial animation moves them): a port bakes those
vertices to the kit pose and moves their weight to Head.

[code: opfor:src/tools/ww2/kits.py:6-8]
[code: opfor:src/tools/ww2/soldiers.py:16-26]
[doc: opfor:research/WAKE_VEHICLE_SEATED_POSE_2026-11-12.md:14-20]

### ANIM-KIT-3: DICE's death pickups draw the weapon, not a bag

Status: **observed** · Variants: PC, BF1943

All 24 `DynamicWeaponPickupEntityData` in `objects/soldiers/` have `UseWeaponMesh` true, `Mesh` null,
`UnspawnOnAmmoPickup` true, `HasAutomaticAmmoPickup` false: DICE never shipped BF1943's kit bags. BF1943's death pickup
draws a bag per side, one shader set per class body, the class symbol chosen by the material's constants (OPFOR ports
them).

[data: DICE's bundles, 2026-11-22: 24 of 24 as listed]
[code: opfor:src/tools/ww2/kitbags.py:6-11]

### ANIM-SKIN-1: A part is drawn only with programs for its skinning kind

Status: **verified** · Variants: PC

A `MeshData` part's skinning kind (its `fa` byte: 0 rigid, 1 one bone a vertex, 4 four bones, 8 the material-masking
skin) chooses which programs the engine asks the part's material for (+8 on destructible parts); a part whose material
has none of that kind is not drawn (soldiers' far LODs, helmets, hair, eyes and the third-person wrench, 2026-11-12). A
kind 1 part whose vertices each take one bone at full weight draws the same as kind 4.

[test: opfor:src/tools/tests/test_ww2_skinning.py::test_set_part_fa_changes_only_the_named_parts]
[test: opfor:src/tools/tests/test_ww2_skinning.py::test_single_bone]
[code: opfor:src/tools/ww2/skinning.py:1-28]

### ANIM-RAG-1: One ragdoll

Status: **observed** · Variants: PC

DICE ships one `RagdollResource`, `characters/ragdolls/ragdoll01` (2 copies; its `RagdollAsset` has `Name`,
`MemoryReportGroup`, `SourceFile`, `UseServerRagdoll`) and a pose animation `characters/animations/3p/ragdollpose_body`;
soldiers name it through `SoldierEntityData.RagdollSettings` and `SharedSoldierAnimationTreeData.RagdollSettings`. The
resource's layout is unread (copied as is).

[data: DICE's bundles, 2026-11-22: 2 RagdollResource entries, 1 RagdollAsset (Dist levels/mp_common/level-00.fbrb)]

## Open questions

- The `AnimTreeInfo` layout: the slot table beyond durations, where the hand-IK on / off flag lives, the pointer shift.
- Whether a missing track falls back to the bind pose in game.
- The per-bone time-interval table (differs in 31 of 533 files).
- Which field selects a kit's camouflage per team or level (`MeshShaderSetNumberOverride`, the team's shader set).
- What the engine does with the dangling `EyePose` influence map.
- Vehicle seat alignment: BF1943's seat offsets against DICE's (the Kurogane's), unresolved.
- AimAnimation at run time beyond the format (the K98 / Springfield use BF1943's own aim poses).
