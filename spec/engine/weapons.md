# Weapons: the data of a soldier's weapon, and DICE's class conventions

How BFBC2 describes a hand-held weapon in data: the DBX classes of a weapon, its firing and its projectile, and how
they refer to each other; damage and falloff, muzzle speed and drop, magazines and pickups, sway and recoil, zoom,
auto-aim, grenades and launchers, attachments. Each balance field is given with the range DICE's stock weapons span,
class by class, and the conventions DICE followed when it brought Battlefield 1943's weapons into BFBC2, which
Project OPFOR BFBC2's WW2 ports follow by the user's ruling (2026-11-09). The DBX encoding is layer 1
(`formats/dbx.md`); which physics material a hit damages is HAVOK-MAT-2; vehicles' weapons are `engine/vehicles.md`.

**Corpus.** DICE's bundles in Project OPFOR BFBC2's game copy (dated before 2013, as the install leaves them: 1,021
bundles), measured by `spec/probes/weapons.py` on 2026-11-22. Each partition is taken once, from the first bundle
that holds it with Package before Dist (the copy the game reads, PREC-ORD-3): 1,130 partitions under
`objects/weapons/`, 6,111 copies. Values are read from the binary numbers (whole numbers for the count fields), not
the XML text, so the DBX XML denormal trap does not apply. **The weapons** are the 359 `SoldierWeaponData` instances:
321 players' weapons (276 BFBC2, 31 Vietnam, 14 in BF1943's folder `objects/weapons/handheld/beach/`) and 38 AI clones
(`objects/weapons/handheld/aiclones/`). Variants (a weapon's Kobra, scoped, slug or frag version) are weapons of their
own; where a count says **firings** it counts distinct firing partitions instead. Distances in metres, speeds in m/s,
angles in degrees, rates in rounds per minute. Nothing here was measured in game.

## 1. The classes of a weapon

### WPN-CLS-1: Weapon, firing, projectile, sway, aiming

Status: **verified**

A weapon is a partition whose `SoldierWeaponAsset` names a `SoldierWeaponData` (its primary object). The data names
its firing, `WeaponFiring` -> a `WeaponFiringData` (in a partition `<weapon>_firing`); that names `PrimaryFire` -> a
`FiringFunctionData` (shot, fire logic, ammo, overheat, recoil, effects and sounds) and `WeaponSway` -> a
`GunSwayData`; the function's `Shot.ProjectileData` names the projectile entity (`BulletEntityData`,
`GrenadeEntityData`, `MissileEntityData`, `ExplosionPackEntityData` and the gadgets' own classes), whose `Explosion`
names an `ExplosionEntityData`. The weapon's `AimingController` names a `SoldierAimingSimulationData`, whose
`ZoomLevels` list `ZoomLevelData` instances in order. A firing may be shared: a weapon's variants name the same one.

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: 359 SoldierWeaponData and 359 SoldierWeaponAsset, 280 SoldierAimingSimulationData, 574 ZoomLevelData, 180 WeaponFiringData and 180 FiringFunctionData, 117 BulletEntityData, 77 GunSwayData in 1,130 partitions; every weapon's WeaponFiring and PrimaryFire resolve (359 of 359), its projectile on 358]

### WPN-CLS-2: WeaponClass, and no pistol class

Status: **verified**

`SoldierWeaponData.WeaponClass` sorts the players' weapons: `wcUgl` 82 (the underbarrel launchers, WPN-GREN-1),
`wcAssault` 52, `wcSmg` 44, `wcShotgun` 42, `wcLmg` 34, `wcSniper` 32, `wcAt` 6, `wcHgr` 4, `wcKnife` 4, `wcC4` 3,
`wcMortarStrike` 3, `wcNone` 3 and two or fewer each of the gadgets (`wcATMine`, `wcAmmoBox`, `wcMedKit`,
`wcPowerTool`, `wcLaserDesignator`, `wcMotionSensor`, `wcTracerDart`). There is no pistol class: DICE's pistols are
`wcSmg` (9) or `wcAssault` (1) single-fire weapons with `AnimBaseSet` `wabsHg`. The kit a weapon belongs to is the
kit's choice, not the class's.

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: 321 players' weapons by WeaponClass as listed; AnimBaseSet wabsHg on 10 player firearms (wcSmg 9, wcAssault 1), all fltSingleFire]

### WPN-CLS-3: Body sets per class

Status: **verified**

`AnimBaseSet` picks the soldier's third-person body set: `wabsRif` on 197 of 227 player firearms, `wabsShg` on 20 (all
shotguns: 20 of the 42 shotgun weapons, the others `wabsRif`), `wabsHg` on 10 (the pistols); `wabsAT` on the rocket
launchers (6 of 6), `wabsHgr` on grenades, mines and explosives, `wabsHg` on the power tool (2 of 2).

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: AnimBaseSet over 227 player firearms wabsRif 197, wabsShg 20, wabsHg 10; wcAt wabsAT 6/6; wcHgr wabsHgr 4/4; wcC4 3/3 and wcATMine 2/2 wabsHgr; wcPowerTool wabsHg 2/2]

## 2. Where the balance is: the patch copies

### WPN-PATCH-1: DICE's balance patches are Package copies

Status: **verified**

DICE re-balanced the weapons after release by shipping changed copies of their partitions in the Package level
bundles (and `mp2_common`), not by editing `mp_common` in Dist. Where a partition has a Package copy the game reads it
(PREC-ORD-3), so the launch values in Dist are not DICE's final balance: the M24's launch firing does 100 -> 75 over
5-25 m, its patched copy 100 -> 50 over 10-36 m; the power tool's 5 a hit became 6.5. Every Package copy of a firing
is the same content in every bundle that holds it.

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: 351 weapon partitions have a Package and a Dist copy, 325 of them differ (XML form); 159 firing partitions: 130 have Package copies, each name 1 content over all its Package copies; 93 of the 159 have 2 contents over all copies; of 68 distinct player firings with both copies, the Package one changes the falloff start on assault 8 of 10, SMG 12 of 13, LMG 7 of 7, shotgun 11 of 14 (falloff end 13 of 14); end damage and both falloff distances on sniper 5 of 8; the damage on the power tool 1 of 1]

### WPN-PATCH-2: Readers take the Package copy first

Status: **implemented**

A tool that takes DICE's values from the game copy must read the Package copy before Dist's, or it gets launch
balance. The suite's stock index (`meshview.Index`, used by OPFOR's WW2 weapon ports for their donors) lists Package
before Dist and keeps the first bundle that holds a name, so the class overrides (WPN-WW2-5) copy the patched M24.

[code: bfbc2-mesh-viewer/meshview.py:54-72] [code: opfor:src/tools/ww2/bf1943_weapons.py:393-397]

## 3. Damage

### WPN-DMG-1: Damage lives on the projectile

Status: **verified**

A bullet's damage is `BulletEntityData.StartDamage` up to `DamageFalloffStartDistance` and `EndDamage` from
`DamageFalloffEndDistance` on; `Material` names its projectile material. The firing holds no damage. Explosive
projectiles add an `ExplosionEntityData` (`BlastDamage`, `BlastRadius`, `InnerBlastRadius`). Melee weapons and the
power tool fire bullets too: a slow, short-lived bullet with no drop (WPN-DMG-4).

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: StartDamage / EndDamage / falloff distances on every BulletEntityData a player weapon fires (293 weapons over 85 partitions); 72 of them name an Explosion]

### WPN-DMG-2: Falloff between the two distances is linear

Status: **inferred**

Damage falls from `StartDamage` to `EndDamage` between the two falloff distances, linearly.

[inferred: the fields name two end points and every stock profile is monotonic; the interpolation is not read from BFBC2Game.exe and was not measured in game]

### WPN-DMG-3: Damage profiles per class

Status: **verified**

Distinct player firings, start -> end damage over the falloff distances:

| Class / fire logic | Firings | Profiles (count) |
|---|---|---|
| assault, automatic | 14 | 12.5-25 -> 12.5-20; falloff 18-64 m (12), 18-42 m (2) |
| assault, single fire | 9 | 39 -> 28 over 8-64 or 12-64 m (4: the semi-automatic rifles), 14.3-33.4 -> 14.3-25 |
| SMG, automatic | 13 | 14-25 -> 11.2-15, all over 18-42 m |
| SMG, single fire (pistols) | 7 | 12.5-40 -> 9.1-33.4 over 6-12 to 42 m |
| LMG, automatic | 14 | 11.2-20 -> 10-16.7, all over 18-64 m |
| shotgun, pump / bolt | 15 | buckshot 14.3 or 15 -> 6 over 8-18 m (5); slug 100 -> 50 (5) or 85 -> 30 (5) over 10-36 m |
| shotgun, automatic and single | 6 | 14.3 -> 6 over 8-18 m; 29 -> 18, 39 -> 28 over 8-42 / 8-64 m |
| sniper, bolt action | 10 | 100 -> 50 over 10-36 m (6), 100 -> 55 over 18-36 m (2), 85 -> 30 over 10-36 m, 105 -> 70 over 20-56 m |
| sniper, single fire | 8 | 39 -> 28 over 8-64 / 12-64 m (6), 45 -> 45 over 8-75 m (2) |
| sniper, automatic | 2 | 25 -> 20 over 18-64 m |

Buckshot fires 12 pellets a shell (`Shot.NumberOfBulletsPerShell`; 6 on 5 weapons); every other player firearm 1.

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: the damage-profile section, 98 distinct player firearm firings; NumberOfBulletsPerShell 12 on 10 and 6 on 5 shotgun weapons, 1 on the other 212 player firearms]

### WPN-DMG-4: Melee and tools are slow bullets

Status: **verified**

The knives fire a `BulletEntityData` of 120 (125 on one) damage at 1.75-2.25 m/s that lives 1 s with gravity 0 and
no falloff, 52-60 a minute, magazines -1; two `wcNone` melee weapons fire 120 at 1.6 m/s. The power tool fires 6.5 a
hit at 900 a minute (97.5 a second) at 2 m/s, its overheat limiting how long it runs; magazines -1.

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: wcKnife 4 weapons: StartDamage 120 x3, 125 x1, speed 1.75 x2 / 1.8 / 2.25, TimeToLive 1, Gravity 0, RateOfFire 52 x3 / 60, NumberOfMagazines -1; wcNone StartDamage 120 x2 at 1.6; wcPowerTool 2: 6.5 at 900 rpm, speed 2, NumberOfMagazines -1; 8 player bullets with gravity 0, all of these]
[doc: bfo:docs/research/opfor/WW2_WEAPON_ANIM_AUDIT.md:45-47]

## 4. Shot and projectile

### WPN-PROJ-1: Muzzle speed is Shot.InitialSpeed

Status: **verified**

A firing's muzzle velocity is the vector `FiringFunctionData.Shot.InitialSpeed` (x, y, z, w), z forward. DICE's player
firearms fire at 600 (189 of 227); the exceptions are the underbarrel buckshot at 300 (24), shotguns at 200 (11) and
300 (2) and one at 400. The AI clones fire at 300 (25 of 25). Rockets fly at 25-100 (mode 75), hand grenades are
thrown at 20, rifle grenades at 40, 40 mm grenades at 50. `BulletEntityData` has an `InitialSpeed` of its own (2000 on
the M24) that the conversions leave alone (WPN-PROJ-4).

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: Shot.InitialSpeed z over 227 player firearms 600 x189, 300 x26, 200 x11, 400 x1; AI clones 300 x25; MissileEntityData 25 .. 100 mode 75; GrenadeEntityData 20 x4, 40 x1; wcUgl 50 x57]

### WPN-PROJ-2: Every player firearm drops at -9.81

Status: **verified**

`BulletEntityData.Gravity` is -9.81 on all 227 player firearms and all 25 AI clone firearms; 0 only on the melee
weapons' and the power tool's bullets (WPN-DMG-4) and the tracer dart. Hand grenades (`GrenadeEntityData.Gravity`) fall at -8, the rifle
grenade at -7, the underbarrel buckshot at -9.8, rockets at -9.81 to 0 (mode -1.5). Bullets live 2 s
(`TimeToLive`), rockets 30 s.

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: Gravity -9.81 x227 player firearms, -9.81 x25 AI clone firearms; BulletEntityData gravity 0 on 8 player weapons (4 knives, 2 wcNone, 2 power tools); GrenadeEntityData -8 x4, -7 x1; wcUgl buckshot -9.8 x24; MissileEntityData mode -1.5; TimeToLive 2 on every assault, SMG, LMG and sniper weapon, 30 on 5 of 6 wcAt]

### WPN-PROJ-3: Explosive projectiles

Status: **verified**

The 40 mm underbarrel grenade is a `BulletEntityData` (125 direct damage, 50 m/s) with an explosion over 5 m: 100
blast damage (inner 0.34 m) on 33 launchers, 0 on 24 (direct damage only). Frag shotgun shells and one sniper firing's frag round
carry a small explosion (10-20 over 1.1 m). Hand grenades explode for 110 over 8 m (inner 1 m), the rifle grenade 102
over 7 m; C4 and mines 290 over 5-8 m; rockets 80-100 over 2-7 m, the mortar strike 200 over 6 m.

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: player weapons whose projectile names an Explosion: wcUgl 50 m/s blast 100 radius 5 inner 0.34 x33, blast 0 radius 5 x24; frag shells blast 20 x10 / 10 x4 at radius 1.1 (one a wcSniper firing); GrenadeEntityData 110 radius 8 x4, 102 radius 7 x1; ExplosionPackEntityData 290 radius 8 x4 / 5 x1; wcAt MissileEntityData 80 .. 100 radius 2 .. 7; wcMortarStrike 200 radius 6]

### WPN-PROJ-4: Which of the two speeds flies the bullet

Status: **inferred**

The bullet flies at the firing's `Shot.InitialSpeed`, not at `BulletEntityData.InitialSpeed`.

[inferred: DICE changed Shot.InitialSpeed (to 600) on every BF1943 firearm it converted and left the bullet's own field; the M24's 2000 on the bullet would make every sniper hit near-instant, which the game's sniper bullet drop contradicts; not read from BFBC2Game.exe]
[doc: bfo:docs/research/opfor/WW2_WEAPON_ANIM_AUDIT.md:27-36]

## 5. Ammunition

### WPN-AMMO-1: Finite magazines and pickups per class

Status: **verified**

`FiringFunctionData.Ammo` holds `MagazineCapacity`, `NumberOfMagazines` (spares), `AmmoPickupMinAmount` /
`AmmoPickupMaxAmount` and the ammo box's rate. No player firearm has unlimited spares (-1) or a zero pickup. Class
values over distinct firings: assault 4 magazines (6 on the semi-automatic rifles), pickup 30 / 60 on the automatic
firings (5 - 30 / 8 - 60 on the single-fire ones); LMG 2-3 of 100 or 200, pickup 45 / 110; SMG 3-4, pickup 20-60 /
30-120 on the automatic firings (pistols 5 / 10-12); shotgun 4 or 6 of 4 or 6, pickup 8 / 16;
sniper 3-5 of 5, 10 or 20, pickup 3 / 8. -1 appears on the knives, the power tool, some gadgets and most AI clones.

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: 227 player firearms NumberOfMagazines 2 .. 9999, none -1, AmmoPickupMin and Max both 0 on none; per-class values from the damage-profile section; AI clone firearms NumberOfMagazines mode -1]

### WPN-AMMO-2: 9999 spares on two campaign weapons

Status: **verified** · Variants: PC, BF1943

Two of DICE's BF1943 weapons, the Type 5 rifle and the Nambu 100 SMG (campaign-only, fired by the campaign's
Japanese), carry 9999 spare magazines with pickups 8 / 8 and 30 / 30: unlimited in practice. Every multiplayer
firearm's spares are 2-9.

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: NumberOfMagazines 9999 on 2 player firearms, both in beach/ with copies only in sp_* bundles; every other player firearm 2 .. 9]
[code: opfor:src/tools/ww2/weapons.py:318-332]

## 6. Sway, recoil, steadiness

### WPN-SWAY-1: Two recoils: gun sway and camera

Status: **verified**

`GunSwayData` gives per stance (`Stand`, `Crouch`, `Prone`) and zoom (`NoZoom`, `Zoom`) a dispersion cone for
`BaseValue`, `Moving`, `Jumping` and vaulting (`MinAngle`, `MaxAngle`, `IncreasePerShot`, `DecreasePerSecond`), the
gun's `Recoil` (`RecoilAmplitudeMax`, increase per shot, angle, decrease factor) and `GunSwayLag`;
`DeviationScaleFactorNoZoom` and `SuppressionModifier` are global. `FireLogic.Recoil` is the camera kick: DICE zeroed
it (`MaxRecoilAngleX` 0) on every player firearm but three SMGs (0.5) and kept mostly its FOV kick (0-0.8).

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: MaxRecoilAngleX 0 on 224 of 227 player firearms, 0.5 on 3 SMG weapons; MaxRecoilFov 0.5 on snipers, 0.6 on most others, 0.8 on 4 LMG weapons]

### WPN-SWAY-2: Class sway values

Status: **verified**

Standing, unzoomed base cone (min / max): assault 1-2 / 3-5, SMG 1-1.5 / 3-5, LMG 2.5 / 7, shotgun 2 / 5 (0 / 0 on
two automatic and single-fire firings), sniper bolt action 7 / 7 (semi-automatic 1.5-2 / 4-5); zoomed min 0, except
0.2-0.8 on 9 SMG weapons and 0.3 on one assault weapon. `RecoilAmplitudeMax` is 99 on assault, SMG, LMG and
shotgun firings and 10 on the bolt-action snipers (both zoom states), whose `DeviationScaleFactorNoZoom` is 4 (1 on
the others).

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: damage-profile section: Stand.NoZoom base per class as listed; bolt-action snipers 10 of 10 firings 7 / 7, RecoilAmplitudeMax 10 / 10, DeviationScaleFactorNoZoom 4; 99 on every assault, SMG, LMG and shotgun firing]

### WPN-SWAY-3: Aim steadiness is a sniper field

Status: **verified**

`SoldierAimingSimulationData.<Stand|Crouch|Prone>Pose.AimSteadiness` is 0 on every class but the snipers: 1 standing
on 8 of 10 bolt-action firings' weapons and 4 of 8 semi-automatic ones (the M24: stand 1, crouch 0.75, prone 0).

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: AimSteadiness 0 on every stance of every assault, SMG, LMG, shotgun and gadget weapon; wcSniper stand 1 on 8 of 10 bolt-action and 4 of 8 single-fire firings]

## 7. Zoom

### WPN-ZOOM-1: Two zoom levels: hip and sights

Status: **verified**

Player firearms have two `ZoomLevelData` (the flamethrower, a `wcLmg` weapon, one): level 0 is the hip view (`FieldOfView` 55,
`LookSpeedMultiplier` 1), level 1 the sights: 40 for iron sights (look speed 0.2), 45-50 for some shotguns and
launchers, 17 for 4x scopes (0.15) and 8-40 on the snipers (mode 11.5, 0.1-0.2). Zoomed, the bolt-action snipers damp
recoil (`RecoilMultiplier` 0.2 on 6 of 10 firings, `RecoilFovMultiplier` -6 on 4); `SwayPitchMultiplier` is 0 on
every level but 18 sniper weapons' scope (0.002).

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: ZoomLevels 2 on every assault, SMG, shotgun and sniper weapon, 2 x33 / 1 x1 on LMG; zoom[0] FOV 55 on all; zoom[1] FOV assault 40 x35 / 17 x14, LMG 40 x24 / 17 x9, sniper 8 .. 40 mode 11.5; SwayPitchMultiplier 0.002 on 18 sniper weapons]

### WPN-ZOOM-2: Press to zoom; hold on AI clones and knives

Status: **verified**

`ZoomType` is `ztPressButtonToSwitchZoomLevel` on 226 of 227 player firearms (hold on one BFBC2 sniper rifle);
`ztHoldButtonToZoom` on every AI clone (25 of 25 firearms), every knife (4 of 4), the flamethrower and some grenades
and mines.

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: ZoomType over player firearms press 226, hold 1 (wcSniper, bfbc2); AI clone firearms hold 25; wcKnife hold 4; wcLmg hold 1 (the 50 m/s flamethrower, not a firearm); wcHgr hold 2 of 4; wcATMine hold 1 of 2]

### WPN-ZOOM-3: Full look speed while sprinting

Status: **verified**

`SprintLookSpeedMultiplier` is 1 on every zoom level of every player and AI firearm.

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: SprintLookSpeedMultiplier 1 on 454 zoom levels of 227 player firearms and 50 of 25 AI clone firearms]

### WPN-ZOOM-4: AlwaysAimHead depends on the class

Status: **verified**

`SoldierWeaponData.WeaponStates[].AlwaysAimHead` is true on 190 of 227 player firearms and false on 37: every sniper
(32 of 32), four SMGs (DICE's BF1943 Thompson twice and Nambu 100, and one Vietnam SMG) and one Vietnam assault
rifle; among the other weapons, 5 of the 6 rocket launchers.

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: AlwaysAimHead true x190 / false x37 over player firearms; wcSniper false 32 of 32 (20 firings); wcSmg false 4 of 44 (beach 3, vietnam 1); wcAssault false 1 of 52 (vietnam); wcAt false 5 of 6]

## 8. Auto-aim

### WPN-AIM-1: Auto-aim is off

Status: **verified**

`WeaponFiringData.UseAutoAiming` is false on 358 of the 359 firings the weapons name; the one true is a BFBC2 knife's.
No firearm, player or AI clone, auto-aims.

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: UseAutoAiming false x358, true x1 (wcKnife, bfbc2)]

## 9. Grenades and launchers

### WPN-GREN-1: Underbarrel launchers are weapons of their own

Status: **verified**

An underbarrel launcher is a `SoldierWeaponData` of class `wcUgl` that the rifle lists in `SpecializationWeapons`
(47 player weapons list one): the 40 mm grenade (57: one shot, 2 spares, pickup 1 / 1, 100 a minute, reload 2.45 s;
WPN-PROJ-3) and the buckshot "master key" (24: 12 pellets of 16.7 at 300 m/s, falloff 8-18 m). The launcher is single fire,
`wabsRif`, zoomed at 50; its unzoomed base cone 0 / 5 and `RecoilAmplitudeMax` 10.

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: wcUgl 82 weapons (bfbc2 80, vietnam 1, beach 1): BulletEntityData 81 (50 m/s x57, 300 m/s x24), GrenadeEntityData 1; MagazineCapacity 1 x82, NumberOfMagazines 2 x82; SpecializationWeapons items on 47 player weapons]

### WPN-GREN-2: Hand grenades and rifle grenades

Status: **verified** · Variants: PC, BF1943

Hand grenades are `GrenadeEntityData` thrown at 20 m/s with gravity -8 (`fltHoldAndRelease`, 85 a minute, one
carried, two on one), pickup 1 / 1. BF1943's rifle grenade on the Type 5 (`type5_rgl`, a `wcUgl` weapon) is a
`GrenadeEntityData` at 40 m/s with gravity -7, one round and two spares.

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: wcHgr 4 weapons: GrenadeEntityData, speed 20, gravity -8, fltHoldAndRelease, NumberOfMagazines 1 x3 / 2 x1; beach type5_rgl: GrenadeEntityData 40 m/s, gravity -7, mags 2, pickup 1 / 2]

## 10. Attachments

### WPN-ATT-1: Specializations, and the ones that swap the weapon

Status: **verified**

`SoldierWeaponData.Specializations` lists unlock items: each names a `persistence/specializations` entry and, where the
unlock changes the model, a `WeaponAsset` (the Kobra and scoped variants). 73 of 321 player weapons list 1-5; 48 swap
in other weapon assets (2 each on 46, 3 on 2): assault 12, shotgun 12, sniper 8, LMG 8, SMG 8.

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: specializations per player weapon 0 x248, 1 x23, 2 x2, 3 x33, 4 x13, 5 x2; weapon-asset swaps 0 x273, 2 x46, 3 x2]

## 11. BF1943's weapons in BFBC2

### WPN-WW2-1: DICE's BF1943 weapons, mostly the campaign's

Status: **verified** · Variants: PC, BF1943

BFBC2 ships BF1943 weapon partitions under BF1943's names and GUIDs (OPFOR's audit counts 109 over every folder). In
`objects/weapons/handheld/beach/` there are 76 partitions and 14 weapons: the M1911, the M1 Garand (two) and the
Thompson assault version only in multiplayer bundles, the Thompson in both, and nine campaign-only (the Type 5, its
rifle grenade, the Nambu pistol and Nambu 100, the Shin Gunto, the Type 97 and US hand grenades, the expack, the
Japanese M18).

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: 76 partitions under beach/; 14 SoldierWeaponData there: mp 4, mp+sp 1, sp 9]
[doc: bfo:docs/research/opfor/WW2_WEAPON_ANIM_AUDIT.md:14-25]

### WPN-WW2-2: How DICE converted BF1943's weapons

Status: **verified** · Variants: PC, BF1943

DICE changed every BF1943 firearm it brought over the same way: bullet `Gravity` 0 -> -9.81, `Shot.InitialSpeed`
400-460 -> 600, `NumberOfMagazines` -1 and pickup 0 / 0 -> finite, `UseAutoAiming` true -> false, `ZoomType` hold ->
press, `SprintLookSpeedMultiplier` 0.5 -> 1, `AlwaysAimHead` false -> the class's, `AnimBaseSet` `wabsHgr` /
`wabsShg` -> `wabsHg` / `wabsRif`, and the camera recoil retuned or zeroed. Its own BF1943 firearms hold the BFBC2 side
of every row. Gravity 0 and magazines -1 stay only on melee weapons, tools, gadgets and AI clones. The planes' and
tanks' guns DICE left as BF1943 had them (VEH-WPN-2).

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: the 8 firearms in beach/: Gravity -9.81 x8, InitialSpeed 600 x8, NumberOfMagazines 4 / 6 / 9999, no zero pickup, UseAutoAiming false x8, press x8, SprintLookSpeedMultiplier 1 on 16 of 16 zoom levels, AnimBaseSet wabsRif x6 / wabsHg x2]
[doc: bfo:docs/research/opfor/WW2_WEAPON_ANIM_AUDIT.md:25-38]

### WPN-WW2-3: The suite's port applies DICE's conventions

Status: **verified** · Variants: BF1943

`dbxport` applies the conversion of WPN-WW2-2 to a BF1943 weapon partition (one under `objects/weapons/`): firearm
bullets (faster than 100 m/s) at 600 m/s from 300-599 and drop -9.81 from 0; finite magazines and pickups from the
template where BF1943 had -1 and 0 / 0; auto-aim off; press to zoom unless the template holds; sprint look 1 from
0.5; `AlwaysAimHead` true only where the template's is; `wabsShg` -> `wabsRif` unless the weapon is a shotgun
(WPN-WW2-8), else the paired donor's body set;
the template's camera recoil; hand grenades thrown at 20 (from 24) with gravity -8 (from -9). BF1943's rate of fire,
magazine size, reload and zoom FOVs are kept. The Rust port does the same.

[code: bfbc2-bf1943-import/dbxport.py:647-665] [code: bfbc2-bf1943-import/dbxport.py:2111-2200]
[code: rust/bc2-convert/src/dbxport/port.rs:1474-1727]
[test: bfbc2-bf1943-import/tests/test_dbxport_rules.py::main] [test: bfbc2-bf1943-import/tests/test_dbxport_native.py::main]

### WPN-WW2-4: AlwaysAimHead true is not a universal convention

Status: **verified** · Variants: PC, BF1943

The ruling's "AlwaysAimHead true" holds for the classes whose DICE weapons have it. DICE's own snipers (32 of 32) and
its BF1943 SMGs (the Thompson twice, the Nambu 100) have false, so a ported bolt-action rifle keeps false, as the M24
does; the port sets true only where the template has true (WPN-WW2-3).

[test: core/tests/test_spec_probes.py::spg_weapons]
[data: spec/probes/weapons.py on the game copy, 2026-11-22: wcSniper AlwaysAimHead false 32 of 32; beach/ firearms true 5, false 3 (thompson, thompson_assault, nambu100)]
[code: bfbc2-bf1943-import/dbxport.py:2191-2200]

### WPN-WW2-5: Bolt-action rifles take the M24's class values

Status: **verified** · Variants: BF1943

The ported K98 and Springfield (and every Type 38 / K98 / Springfield variant, which share their firing) take the
M24's class fields whole: damage and falloff (BF1943's 100 -> 85 over 5-100 m becomes 100 -> 50 over 10-36 m), muzzle
speed, magazines and pickups, camera recoil, the gun sway's recoil per stance and zoom, `DeviationScaleFactorNoZoom`,
`SuppressionModifier`; on the weapon the poses' `AimSteadiness`, `AlwaysAimHead` and per zoom level the recoil,
recoil-FOV and sway-pitch multipliers, paired in `ZoomLevels` order. BF1943's rate of fire, bolt and reload times,
magazine size and zoom FOVs stay. Every class value of the build lies inside the range DICE's five bolt-action rifles
(M24, SV98, GOL, M40, M95) span.

[code: opfor:src/tools/ww2/bf1943_weapons.py:209-291]
[test: opfor:src/tools/tests/test_ww2_balance.py::test_class_overrides_take_the_donors_values]
[test: opfor:src/tools/tests/test_ww2_balance.py::test_class_overrides_refuse_unknown_shapes]
[test: opfor:src/tools/tests/test_ww2_balance.py::test_built_bolt_action_rifles_in_dices_class_ranges]
[doc: bfo:docs/research/opfor/WW2_WEAPON_ANIM_AUDIT.md:42-54]

### WPN-WW2-6: The wrench on the power tool's rates

Status: **verified** · Variants: BF1943

BF1943's wrench (120 a hit at 60 a minute, no overheat) keeps its swing rate and takes the power tool's per-second
damage and heat: damage and heat per hit are the power tool's times its rate over the wrench's (6.5 x 900 / 60 =
97.5 a hit), cooling, penalty time and threshold the power tool's. The donor is the patched power tool (WPN-PATCH-1).

[code: opfor:src/tools/ww2/bf1943_weapons.py:294-332]
[test: opfor:src/tools/tests/test_ww2_balance.py::test_wrench_on_the_power_tools_rates]
[test: opfor:src/tools/tests/test_ww2_balance.py::test_built_wrench_on_the_power_tool]

### WPN-WW2-7: Campaign weapons in multiplayer take finite copies

Status: **implemented** · Variants: BF1943

The WW2 kits' Type 5, Nambu 100 and Type 97 grenade fire copies of DICE's campaign firings (`<name>_Firing_WW2`) with
their US counterpart's spares and pickups (the Garand's, the Thompson's, the Mk 2's), so the finite-ammunition
convention holds in multiplayer although DICE's campaign data carries 9999 (WPN-AMMO-2).

[code: opfor:src/tools/ww2/weapons.py:318-332]

### WPN-WW2-8: wabsShg is DICE's shotgun body set

Status: **verified** · Variants: PC, BF1943

DICE uses `wabsShg` on 20 of its 42 shotgun weapons and on no other class: all of them pump-actions
(`fltSingleFireWithBoltAction`: the 870 MCS, the Neostead, the TOZ-194, the Vietnam 870), while its semi-automatic and
automatic shotguns (Saiga, USAS-12) and the pump-action SPAS-12 take `wabsRif`. BF1943 used `wabsShg` on one weapon,
its Shin-gunto knife (`wcKnife`), and has no shotgun. The port moves `wabsShg` to `wabsRif` only on a weapon whose
`WeaponClass` is not `wcShotgun`, so a ported shotgun keeps DICE's shotgun body set (or takes its paired donor's); the
knife's port is unchanged. The WW2 M1897 (a copy of DICE's 870 partitions, not ported) carries `wabsShg` as a
pump-action as DICE's do. Every WW2 sniper (Type 38, K98, Springfield, their bayonet and iron variants, the scoped
Garand and Type 5) has `AlwaysAimHead` false, as DICE's 32 do (WPN-WW2-4).

[data: spec/probes/weapons.py on the game copy, 2026-11-22: AnimBaseSet wabsShg on 20 player weapons, all wcShotgun; 1 AI clone]
[data: a read of the game copy, 2026-11-22: wcShotgun wabsShg x21 all fltSingleFireWithBoltAction (20 players, 1 AI clone); wabsRif x23 (fltAutomaticFire 9, fltSingleFire 6, fltSingleFireWithBoltAction 8: the SPAS-12); OPFOR's bundles: us_shg_m1897 x3 wcShotgun wabsShg fltSingleFireWithBoltAction, 11 WW2 wcSniper weapons AlwaysAimHead false 11 of 11]
[data: a read of BF1943's Xbox 360 bundles, 2026-11-22: SoldierWeaponData wabsShg x1 (jap_shingunto_knife, wcKnife); no wcShotgun]
[code: bfbc2-bf1943-import/dbxport.py:2180-2190] [code: rust/bc2-convert/src/dbxport/port.rs:1670-1697]
[test: bfbc2-bf1943-import/tests/test_dbxport_rules.py::main] [test: bfbc2-bf1943-import/tests/test_dbxport_native.py::main]

## Open questions

- **Falloff shape (WPN-DMG-2).** Linear is assumed. Settled by the damage function in `BFBC2Game.exe` (the reader of
  `DamageFalloffStartDistance`) or by an in-game measurement of a hit at the middle of a falloff range.
- **The two speeds (WPN-PROJ-4).** Which of `Shot.InitialSpeed` and `BulletEntityData.InitialSpeed` moves the bullet,
  and what the other does. Settled by the executable's projectile spawn, or by timing a shot at range in game.
- **The damage DICE's patches use online.** WPN-PATCH-1 reads DICE's last patch as installed; whether a server
  override (the online balance) ever changed these values further is unknown offline.
- **Magazines 9999 (WPN-AMMO-2).** Assumed to be unlimited in practice; whether the HUD or the game caps it is not
  checked.
- **AlwaysAimHead.** What the flag does (the third-person aim, the head target of the aim animation, or hit
  registration) is not read from the executable.
- **The AI clones.** Their 300 m/s, hold zoom and -1 magazines are measured; how the AI chooses a clone over the
  player weapon is in the AI data (`engine/ai.md`), not checked here.
- **BF1943's side of the conversion.** WPN-WW2-2's BF1943 column is OPFOR's audit of BF1943's partitions
  (`bfbc2 dbxparity --predict`); this file's probe measures only the BFBC2 side.
