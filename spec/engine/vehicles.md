# Vehicles: entity data, seats, health, weapons and spawners

How BFBC2 describes a vehicle in data: the entity and its component tree (chassis, engine, gearbox, wheels or rotors,
seats, parts, weapons), its physics configuration, its health zones and armour angles, the input each seat names, the
weapons it carries, and the level spawners that place and respawn it; then the vehicles DICE brought from
Battlefield 1943, the console-only Mark V, and what Project OPFOR BFBC2's WW2 ports had to change. A vehicle's weapon
uses the same firing classes as a soldier's (`engine/weapons.md`); which physics material a hit damages is
HAVOK-MAT-2; vehicle sounds are SND-VEH-1; where vehicle spawners may stand in an exported level is XVAL-PLACE-1.

**Corpus.** DICE's bundles in Project OPFOR BFBC2's game copy (dated before 2013, as the install leaves them: 1,021
bundles), measured by `spec/probes/vehicles.py` on 2026-11-22. Each partition under `objects/vehicles/` and
`objects/weapons/stationary/` is taken once, from the first bundle that holds it with Package before Dist
(PREC-ORD-3): 531 partitions, 1,893 copies, 97 `VehicleEntityData` (folders `land` 26, `air` 24, stationary weapons
17, `vietnam` 9, `sea` 8, `beach` 8, `static` 5); 44 of them have copies only in campaign (`sp_*`) bundles. Spawners
are counted in the stock level partitions (`Levels/<LV>.dbx`): 61 multiplayer and 13 campaign levels. Values are read
from the binary numbers. Masses in kg, times in seconds, distances in metres. Nothing here was measured in game.

## 1. The entity and its components

### VEH-ENT-1: A vehicle is an entity with one chassis component tree

Status: **verified**

A vehicle partition's `VehicleAsset` names a `VehicleEntityData` (its primary object). Its `Components` hold one
`VehicleComponentData`, the chassis, whose own `Components` tree holds everything else: `EngineComponentData`,
`PlayerEntryComponentData` / `SoldierEntryComponentData` (seats), `PartComponentData` (damageable parts),
`WeaponComponentData`, `WheelComponentData` / `TrackWheelComponentData` + `TrackComponentData`, `RotorComponentData`,
`WingComponentData`, `ChildComponentData` (turrets and moving parts), `CameraComponentData`,
`VehicleExitPointComponentData`, `SoldierGripComponentData` and effect and radar components. The entity itself holds
the health zones, the material, the warning sounds, the explosion and the HUD data.

[test: core/tests/test_spec_probes.py::spg_vehicles]
[data: spec/probes/vehicles.py on the game copy, 2026-11-22: 97 VehicleEntityData, each with a VehicleComponentData; 80 VehicleAsset in objects/vehicles/; component classes over every vehicle's tree as listed per kind in the probe's output]

### VEH-ENT-2: HUD type is DICE's own classification

Status: **verified**

`VehicleEntityData.HudData.VehicleType` (enum `VehicleHudType_*`) classes every vehicle: Car 22, Helicopter 18,
Stationary 15, Tank 11, Dummy 10, Boat 9, StationaryUAV 4, Strategic 4, UAV 3, Count 1. It agrees with the
components (wheels for cars, track wheels for tanks, rotors for helicopters, no drive for the stationary classes)
except that 4 boats carry a `RotorComponentData` for their propeller, and Dummy covers vehicles not driven by players
(the campaign planes, set pieces). The executable names no other type (Airplane is in the enum but no stock vehicle
uses it).

[test: core/tests/test_spec_probes.py::spg_vehicles]
[data: spec/probes/vehicles.py on the game copy, 2026-11-22: HUD type / build from the components: Car/wheeled 22, Helicopter/helicopter 18, Stationary/stationary 15, Tank/tracked 11, Dummy/stationary 7, Dummy/helicopter 3, Boat/boat 5, Boat/helicopter 4, StationaryUAV 4, Strategic 4, UAV/helicopter 3, Count 1]
[doc: opfor:src/tools/ww2/bf1943_vehicles.py:237-241]

## 2. Physics: chassis, engine, gearbox, wheels

### VEH-PHYS-1: Chassis configuration and engine classes

Status: **verified**

The chassis names `VehicleConfig` -> a `Physics.VehicleConfigData` (`BodyMass`, centre of mass, `AeroDynamicPhysics`,
`FloatPhysics`, `Stabilizer`, `MotionDamping`, the drag and down-force fields) and `GearboxConfig` -> a
`Physics.GearboxConfigData` (`ForwardGearRatios` / `ForwardGearSpeeds` and their reverse twins, gearbox type and
change times). Each `EngineComponentData.Config` names the engine class: `CombustionEngineConfigData` (torque over rpm
curve) on every car (22) and tank (11), `PropellerEngineConfigData` on helicopters (38 over 18, main and tail rotor)
and planes, `JetEngineConfigData` on every boat (9 of 9, a water jet).

[test: core/tests/test_spec_probes.py::spg_vehicles]
[data: spec/probes/vehicles.py on the game copy, 2026-11-22: engine configs Car CombustionEngineConfigData 22, Tank 11, Helicopter PropellerEngineConfigData 38, Dummy Propeller 11, UAV Propeller 6, Boat JetEngineConfigData 9; GearboxConfigData on every driven vehicle]

### VEH-PHYS-2: Mass, gears and wheels per class

Status: **verified**

Body mass: cars 1000-2500 (1500 on 15 of 22), tanks 4000-9000 (5000 on 5, 8000 on 4), helicopters 5000 (17 of 18),
boats 750-4500. Forward gears: cars 3-5, tanks 3-4, helicopters 5 (all 18), boats 1-3. Wheels: cars 4 (16), 6 (5) or
3 (1); tanks 14-26 track wheels with their tracks; helicopters 3 (17 of 18, the landing gear).

[test: core/tests/test_spec_probes.py::spg_vehicles]
[data: spec/probes/vehicles.py on the game copy, 2026-11-22: body mass Car 1000 x2 / 1500 x15 / 2500 x5, Tank 4000 .. 9000, Helicopter 5000 x17 / 4000 x1, Boat 750 x4 / 1000 x3 / 2500 / 4500; forward gears Car 3 x6 / 4 x11 / 5 x5, Tank 3 x6 / 4 x5, Helicopter 5 x18, Boat 1 x3 / 2 x2 / 3 x4; wheels Car 4 x16 / 6 x5 / 3 x1, Tank 14 .. 26, Helicopter 3 x17 / 2 x1]

## 3. Health and armour

### VEH-HP-1: Four health zones that share damage

Status: **verified**

A vehicle's health is four zones on the entity (`FrontHealthZone`, `RearHealthZone`, `LeftHealthZone`,
`RightHealthZone`), each a `MaxHealth` with an effect and its damage-angle fields; `HealthZonesShareDamage` is true on
all 97, so in effect one pool. DICE's values: tanks 1250 (8 of 11), helicopters 500-1600 (500 on 9), cars 200-1250
(mode 500), boats 100-1000, stationary guns 125 (15 of 15), strategic guns 200 or 500, UAVs 50-100.
`LowHealthTreshold` (the damaged-warning level) is a fraction of it: about 0.27 on tanks, 0.3 on cars and
helicopters, 0.4 on stationary guns.

[test: core/tests/test_spec_probes.py::spg_vehicles]
[data: spec/probes/vehicles.py on the game copy, 2026-11-22: HealthZonesShareDamage true x97; FrontHealthZone MaxHealth Tank 1250 x8 / 500 / 1000 / 1200, Helicopter 500 x9 .. 1600, Car 200 .. 1250 mode 500, Boat 100 .. 1000, Stationary 125 x15, Strategic 200 x3 / 500, UAV 50 x2 / 100; LowHealthTreshold / MaxHealth Tank mode 0.27, Car and Helicopter mode 0.3, Stationary 0.4 x15]

### VEH-HP-2: Armour angles only on tanks

Status: **verified**

`UseDamageAngleCalculation` is true on 10 of the 11 tanks and false on every other vehicle. On them the front zone has
`MinDamageAngle` 1 (8) and `DamageAngleMultiplier` 1 (0.75 on 2); the rear and sides have `MinDamageAngle` 0.43-0.6
and multiplier 2 (rear 3 on one).

[test: core/tests/test_spec_probes.py::spg_vehicles]
[data: spec/probes/vehicles.py on the game copy, 2026-11-22: UseDamageAngleCalculation true x10 / false x1 on Tank, false on all 86 others; Tank front MinDamageAngle 1 x8, multiplier 1 x9 / 0.75 x2; rear multiplier 2 x8 / 3 x1 / 1 x2; side multiplier 2 x9, MinDamageAngle 0.43 .. 0.6]

### VEH-HP-3: Side and rear hits on a tank do more damage

Status: **inferred**

With the angle calculation on, a hit's damage is scaled by the zone's `DamageAngleMultiplier` when its incidence is
beyond `MinDamageAngle`: a tank takes up to twice the damage from the side and rear as from the front.

[inferred: the field names and DICE's values (front 1 or 0.75, rear and sides 2) match BFBC2's known armour behaviour; the angle measure (cosine or radians) and the formula are not read from BFBC2Game.exe]

### VEH-HP-4: Health states of the chassis and the parts

Status: **verified**

The chassis and each `PartComponentData` list `HealthStates` (`HealthStateData`: `Health`, the mesh and effects of
that state, loose parts, a spawned banger). The chassis's states end in 100000 on 93 of 97 vehicles: 0 then 100000 on
78, 100 then 100000 on 10, 10000 then 100000 on 4 boats, 100000 alone on 1. Parts per vehicle: cars 2-42 (mode 36), helicopters 8-31 (mode 24), tanks 0-35 (mode 17); their first
state is 0 on most (334 of 364 helicopter parts).

[test: core/tests/test_spec_probes.py::spg_vehicles]
[data: spec/probes/vehicles.py on the game copy, 2026-11-22: chassis HealthStates ending in 100000 on 93 of 97 (0/100000 on Car 22, Tank 11, Stationary 15, StationaryUAV 4, Strategic 4, Helicopter 10, Boat 5, Dummy 7); PartComponentData per vehicle as listed; Helicopter parts' first state 0 x334 of 364]

### VEH-HP-5: A health of 100000 is indestructible to Destroy

Status: **observed**

The server's static-model Destroy event skips a part whose final health state is 100000 or more: its predicate reads
the last `HealthStates` entry and compares its `Health` with the float 100000. BF1943's carrier cradle uses exactly
100000; OPFOR sets it to 99999 so the scripted release can destroy it.

[exe: BFBC2Game.exe ServerStaticModelEntity event handler 0x714C90 (Destroy hash 0xEFA23F67) calls 0x70F400; predicate 0x7A7EF0 compares the final HealthStates entry with 100000.0 at 0x1423798; Destroy skips such parts at 0x70F528]
[doc: bfo:docs/research/opfor/WAKE_FOLLOWUP_FIXES_2026-11-11.md:35-37]

## 4. Seats and input

### VEH-SEAT-1: Seats are entry components in EntryOrderNumber order

Status: **verified**

A seat is a `PlayerEntryComponentData` (a driver or gunner: the soldier is hidden or posed, controls the vehicle or a
weapon) or a `SoldierEntryComponentData` (a passenger who keeps his own weapon, with aiming constraints).
`EntryOrderNumber` orders them, 0 the driver: tanks [0,1,2,3] (7 of 11), helicopters [0,1] (12) or [0,1,2] (6), cars
[test: core/tests/test_spec_probes.py::spg_vehicles]
[0,1,3,4] (8). Stationary guns have one seat numbered 1 and no driver. `MaxPlayersInVehicle` is -1 on 94 vehicles
(1 on the 3 UAVs): the seats are the limit.

[data: spec/probes/vehicles.py on the game copy, 2026-11-22: player entries per vehicle Car 1 .. 5 (4 x11), Tank 2 x3 / 4 x8, Helicopter 2 x12 / 3 x6, Boat 1 .. 4; soldier entries on Car 16 of 22, Helicopter 14 of 18, Boat 4 of 9, Tank 0; EntryOrderNumber sequences as listed; Stationary [1] x15; MaxPlayersInVehicle -1 x94, 1 x3]

### VEH-SEAT-2: The input each seat names, by role

Status: **verified**

Each player entry names an `InputConceptDefinition` and an `InputMapping` (partitions under `input/`). DICE's
multiplayer vehicles use: drivers of cars, tanks and boats `BFBC2VehicleInputConcepts` + `BFBC2VehicleInputMapping`;
helicopter pilots `BFBC2AirInputConcepts` + `BFBC2AirInputMapping` (17 of 17 in `air`); gunners
`BFBC2GunnerInputMapping` (tanks 16, helicopters 20, boats 12, stationary guns 13 of 13); passengers' player entries
`BFBC2SoldierInputMapping` (cars 22). Campaign vehicles differ: the Chi-Ha on `VeniceTankInputConcepts` /
`VeniceSoldierInputConcepts` with a mapping under `objects/vehicles/input`, and the campaign planes and AI vehicles
with no input at all (null).

[test: core/tests/test_spec_probes.py::spg_vehicles]
[data: spec/probes/vehicles.py on the game copy, 2026-11-22: seat input folders per HUD type and folder as listed in the seats section; e.g. Helicopter (air): driver BFBC2AirInputMapping x17, other BFBC2GunnerInputMapping x20; Stationary (stationary weapons): BFBC2GunnerInputMapping x13; Tank (beach): venice concepts x2; Dummy (beach): null x2]

### VEH-SEAT-3: Ported gunner seats take the gunner mapping

Status: **implemented** · Variants: BF1943

BF1943's entries name action maps (`defaultgroundseavehicleinput` drivers, `soldierinvehicle` gunners,
`defaultaeroplanevehicleinput` pilots); BFBC2's name an `InputMapping`, which the port filled from the template's first
entry, a driver's, so every ported gunner seat drove the vehicle (the LCVP's gunners, playtest 2026-11-08). OPFOR's
`gunner_input` sets each seat by its BF1943 action map: gunners on `BFBC2GunnerInputMapping`, pilots on the air
concepts and mapping; the Sherman's seats (donor: the campaign Chi-Ha) on the multiplayer tanks' pair (VEH-SEAT-2).

[code: opfor:src/tools/ww2/bf1943_vehicles.py:212-236] [code: opfor:src/tools/ww2/bf1943_vehicles.py:323-349]

## 5. Weapons and materials

### VEH-WPN-1: Vehicle weapons are weapon components with firings

Status: **verified**

A vehicle's gun is a `WeaponComponentData` whose `WeaponFiring` names a `WeaponFiringData` (in the vehicle's
`*_firing` partitions), then the same function, projectile and explosion chain as a soldier's (WPN-CLS-1). Weapons per
vehicle: tanks 3-13 (mode 10: a weapon component per firing mode and position), helicopters 2-8 (mode 3), cars 0-3,
boats 0-3, stationary guns 1. Projectiles: bullets and missiles (and the UAVs' `VehicleProjectileEntityData`).

[test: core/tests/test_spec_probes.py::spg_vehicles]
[data: spec/probes/vehicles.py on the game copy, 2026-11-22: WeaponComponentData per vehicle Tank 3 .. 13 mode 10, Helicopter 2 .. 8 mode 3, Car 0 x4 / 1 x7 / 2 x10 / 3 x1, Boat 0 .. 3, Stationary 1 x15; projectiles Tank BulletEntityData 109 / MissileEntityData 3, Helicopter 39 / 26 / TracerDart 2, StationaryUAV VehicleProjectileEntityData 3]

### VEH-WPN-2: DICE kept the vehicles' guns' ballistics

Status: **implemented** · Variants: BF1943

The weapon conventions DICE applied to BF1943's hand-held weapons (WPN-WW2-2) do not apply to vehicle guns: DICE kept
the planes' bullets as BF1943 had them, 480 m/s with no drop (the Bofors' 600 m/s, no drop), while it turned auto-aim
off on every vehicle gun and retuned some damage and rates (the Zero's gun 35 -> 30 a hit, 400 -> 500 a minute; its
Corsair keeps 35 and 400). The Chi-Ha's cannon fires 875 at 120 m/s with gravity -12, its machine guns 150-600 m/s
at -9.81. The port applies the weapon conventions only to partitions under `objects/weapons/`.

[code: bfbc2-bf1943-import/dbxport.py:662-664]
[data: spec/probes/vehicles.py on the game copy, 2026-11-22: DICE's beach/ vehicles' guns: Zero 4 bullets 480 m/s gravity 0 damage 30 rate 500, Corsair 4 bullets 480 m/s gravity 0 damage 35 rate 400, bombs gravity -20; Bofors 600 m/s gravity 0; Chi-Ha cannon 875 at 120 m/s gravity -12; machine guns 150-600 m/s at -9.81; UseAutoAiming false on all 20]
[doc: bfo:docs/research/opfor/WW2_FIELD_PARITY_AUDIT.md:121-123]

### VEH-MAT-1: The entity's material per class

Status: **verified**

`VehicleEntityData.Material` names a material by class: tanks `Tank`, `Medium_Tank`, `Tank_Vietnam`; cars
`Armoured_Jeep`, `Light_Jeep`, `Light_Jeep_Vietnam`, `Civilian_Vehicle`; helicopters `Transport_Helo` (13 of 18),
`Attack_Helo`, `HUEY`; boats `Light_Boat`, `Light_Rubber_Boat` (`Civilian_Vehicle` on one); stationary guns `Stationary_Metal` (the AA gun
`Stationary_Metal_AA`). DICE's BF1943 planes: the Zero `Attack_Helo`, the Corsair `default`.

[test: core/tests/test_spec_probes.py::spg_vehicles]
[data: spec/probes/vehicles.py on the game copy, 2026-11-22: Material per HUD type as listed; beach zero_fighter Attack_Helo, us_corsair default]

### VEH-MAT-2: Airframes go back on BF1943's airframe material

Status: **implemented** · Variants: BF1943

DICE's re-export of the Zero's and Corsair's physics puts every part (20 and 22) on `Metal_Light`, which against
OPFOR's merged material grid takes about three times the rifle damage of BF1943's airframe. OPFOR writes DICE's
physics with each part on BF1943's material of the part of that name (`Aircraft_Airframe`,
`Aircraft_Airframe_corsair`, the propellers' invisible material, the canopy's bulletproof glass), appending the
materials DICE's list lacks; the bombers keep their own `Bomber_Airframe` physics.

[code: opfor:src/tools/ww2/bf1943_vehicles.py:352-395]
[doc: bfo:docs/research/opfor/PHYSICS_MATERIALS.md:36] [doc: bfo:docs/research/opfor/PHYSICS_MATERIALS.md:128-129]
[doc: bfo:docs/research/opfor/WW2_FIELD_PARITY_AUDIT.md:30]

## 6. Spawners

### VEH-SPAWN-1: VehicleSpawnEntityData

Status: **verified**

A level places a vehicle with a `VehicleSpawnEntityData` in its level partition: `Team`, `Vehicle` (one blueprint) or
`Vehicles` (items, for team-dependent choices), `SpawnDelay` and `InitializedSpawnDelay`, `MaxVehicles` and
`MaxVehiclesPerMap`, `AutoSpawn`, `SpawnVehicleFromBeginning`, `Immortal`, `RespawnRange`, `SpawnAreaRadius`,
`TimeUntilAbandoned` and `TimeUntilAbandonedIsDestroyed`, `KeepAliveRadius`, `WreckDuration`, `EnterRestriction`.
The stock multiplayer levels hold 1,044 (0-65 a level), the campaign levels 367; 68 multiplayer spawners use the
`Vehicles` list (1 item on 45, 2 on 23).

[test: core/tests/test_spec_probes.py::spg_vehicles]
[data: spec/probes/vehicles.py on the game copy, 2026-11-22: 61 multiplayer level partitions, 1,044 VehicleSpawnEntityData; 13 campaign, 367; Vehicles items 0 x976 / 1 x45 / 2 x23 (mp), 0 x367 (sp)]

### VEH-SPAWN-2: DICE's multiplayer spawner values

Status: **verified**

On DICE's multiplayer spawners: `MaxVehicles` 1 (1,043 of 1,044), `TimeUntilAbandoned` 10 s (1,027), the abandoned
vehicle destroyed after 2-20 s (mode 10), `RespawnRange` 15 (1,026), `SpawnAreaRadius` 2.5 (971), `SpawnDelay` 0-600
s (mode 0); `AutoSpawn` false on 314 (spawned by events: Rush stages, flags), `Immortal` true on 240, `EnterRestriction`
`eerOnlyPrimaryEntry` on 653. Spawned vehicles by folder: stationary weapons 290, Vietnam 287, land 249, static 112,
air 101, sea 27.

[test: core/tests/test_spec_probes.py::spg_vehicles]
[data: spec/probes/vehicles.py on the game copy, 2026-11-22: mp spawner field ranges as listed in the spawners section]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:139]

## 7. BF1943's and the console's vehicles

### VEH-WW2-1: DICE's BF1943 vehicles are campaign vehicles

Status: **verified** · Variants: PC, BF1943

DICE ships 8 vehicles in `objects/vehicles/beach/`, all with copies only in campaign bundles: the Zero and the Corsair
(HUD type Dummy, one seat with no input: never flown by a player), the Kurogane (a playable and an AI version), the
Type 97 Chi-Ha (on the campaign's Venice inputs), the Bofors AA gun (Strategic) and two .30 cal positions. A
multiplayer WW2 vehicle needs multiplayer seat input (VEH-SEAT-3) and, for the planes, a pilot's.

[test: core/tests/test_spec_probes.py::spg_vehicles]
[data: spec/probes/vehicles.py on the game copy, 2026-11-22: 8 VehicleEntityData under beach/, every copy in sp_* bundles; HUD types Dummy 2, Car 2, Tank 1, Strategic 1, Stationary 2; the planes' seats name no InputMapping]

### VEH-WW2-2: The landing craft release needs an input event BFBC2 lacks

Status: **verified** · Variants: BF1943

BF1943 drops each carrier landing craft from its cradle when the driver opens the throttle: a `VehicleStateTrigger`
(entry input action `EIAThrottle`) raises `OnOccupantInputTriggered`. BFBC2's `VehicleStateTriggerEntityData` has no
input check (its levels use it for health only), so the boats hung in their cradles. OPFOR moves each such connection
onto the trigger's vehicle spawner, keeping the event name; OPFOR's DLL raises it from the spawner when the driver
opens the throttle. Without the DLL the boats stay in their cradles.

[code: opfor:src/tools/ww2/wake_level.py:819-850]
[test: opfor:src/tools/tests/test_ww2_boat_release.py::test_boat_release_comes_from_the_spawner_on_bf1943s_event]

### VEH-PART-1: Composite parts are stored about their own origins

Status: **implemented** · Variants: PC, BF1943

A vehicle's composite mesh stores each moving part (turret, barrel, wheels, a plane's propeller) about its own origin;
the part's matrix (the mesh set's transform of that composite part) places it, and a vertex's blend index is the
composite part's number itself, not a slot of a bone palette (DICE's M1A2 body carries parts 0-59). Drawn with
identity matrices the Sherman's turret and wheels sat inside the hull; with part 0 for every vertex the Zero's
propeller drew at the plane's origin.

[code: opfor:src/tools/ww2/wake_preview.py:407-420]
[doc: bfo:docs/research/opfor/WW2_VISUAL_PARITY.md:172-178]

### VEH-CON-1: The Mark V boat is console-only

Status: **verified** · Variants: PC, Xbox 360

PC BFBC2 has no Mark V vehicle: none of the 47 entries named for it is under `objects/vehicles/`. PC has the Mark V
as a distant ship (`objects/objects/distant_ships/markv`, in `mp_common` and Atacama's, `mp_005`), as mesh
particles in the two `sp_008` campaign bundles and in Atacama's victory cut-scene (`mp_005`). The drivable Mark
V (Valparaiso Onslaught) exists only in the Xbox 360 data; OPFOR builds it from the console files.

[test: core/tests/test_spec_probes.py::spg_vehicles]
[data: spec/probes/vehicles.py on the game copy, 2026-11-22: 47 entry names containing markv in DICE's bundles: animations/cutscenes/rome 32, objects/objects/distant_ships 8 (2 level folders), objects/mesh_particles/specific 3 (2 level folders), sound 2, textures 2; under objects/vehicles/ 0; VehicleEntityData 0]
[doc: bfo:docs/research/opfor/BFBC2_CONSOLE_RESEARCH.md:171-172]

## Open questions

- **Armour formula (VEH-HP-3).** How `MinDamageAngle` and `DamageAngleMultiplier` combine, and in what unit the angle
  is. Settled by the vehicle damage function in `BFBC2Game.exe`, or by measured hits on a tank's front and side.
- **Shared zones.** With `HealthZonesShareDamage` true everywhere, whether the four `MaxHealth` values are summed,
  averaged or the hit zone's alone is the pool; DICE sets all four equal on nearly every vehicle, which hides it.
- **The chassis's 100000 state (VEH-HP-4, VEH-HP-5).** Whether the vehicle's own destruction reads the chassis's last
  state the way the static-model Destroy event does (vehicles die by zone health instead), or whether the 100000 only
  makes the chassis part indestructible to explosions.
- **Part health.** Most parts' first state has health 0; what a part's health counts against (the zone pool or its
  own) is not read from the executable.
- **Dummy and Count.** What the engine does differently for HUD types `Dummy` and `Count` (an enum's size entry used
  as a value on one campaign vehicle).
- **AutoSpawn false.** Which events start the 314 multiplayer spawners that do not spawn by themselves (Rush stage
  enables, capture events) is measured by the map editor's L1 study for Rush only.
- **Ported seats in game.** VEH-SEAT-3's mappings and VEH-WW2-2's release are checked by data and a test, not by an
  in-game play of every seat.
