# L1: how DICE laid out gameplay on the stock multiplayer maps

Round L1, 2026-11-20. Read-only research for the map editor's V1 warnings and A1 auto-layout. Nothing in `game\`,
OPFOR's `work\` or either repository was changed; no level was prepared and the game was not launched.

Files (all in this folder):

- `L1_placement_report.md`: this report.
- `l1_measurements.json`: 8,771 records, one per level, mode, metric and subject. Each record carries its value, unit
  and source entity GUIDs. Each level also has a `layout` list: the positions of its flags, bases and M-COMs, as
  numbers.
- `l1_distributions.md`: the full tables (min, p10, median, p90, max) per mode and era group, with the outliers named.
- `l1_rules_check.md` / `l1_rules.json`: every proposed rule run against the stock data.
- `l1_a1_params.json`: the typical layout per mode, for A1.
- The scripts. "Re-running" at the end lists them.

## 1. Summary: the tendencies DICE followed

Distances are horizontal (XZ) in metres. The DICE group is BFBC2 + BFBC1 + Vietnam (the BFBC2-engine content DICE
shipped for PC). Counts such as "16/16" are how many stock levels (or stages) obey the tendency.

**Rush (16 levels, 59 stages)**

- **Every stage has exactly 2 M-COMs** (59/59). Squad Rush has exactly 1 per stage, over 2 stages (14/14).
- **The 2 M-COMs of a stage are 12-91 m apart** (median 40 m, middle half 34-56 m). They are within 8.5 m of each
  other in height.
- **Consecutive stages are 93-316 m apart** (median 174 m, centroid to centroid; 43/43 steps). Squad Rush stages are
  45-169 m apart (median 119 m). So "stage 3 is 40 m from stage 2" is well outside anything DICE shipped.
- **Rush has 3-5 stages** (BFBC2 3-5, BFBC1 4-5, Vietnam always 3).
- **Defenders spawn close to the objectives and behind them.**
  - Nearest defender spawn to an M-COM: 6-61 m (median 34 m).
  - The defenders' spawn centroid is behind the objectives as the attackers see them (57/59 stages; median 39 m
    behind).
- **Attackers spawn far away and in front.**
  - Nearest attacker spawn to an M-COM: 72-459 m (median 160 m; stage 1 median 198 m).
  - All 87 Rush and Squad Rush stages have the attackers' spawns 64-490 m in front of the objectives.
  - The attackers' nearest spawn is 1.8-21 times as far from the objectives as the defenders' (median 5x).
- **The attackers' first base is outside the defenders' combat area** in 30/30 Rush and Squad Rush levels, by 2-141 m
  (median 91 m). This is the only spawn protection DICE built into Rush. From stage 2 on, the attackers' forward
  base usually lies inside the defenders' area.
- **The attackers' forward base is near the previous stage's defender base**: 27-321 m away, median 75 m.
- **The combat areas grow stage by stage.** Each keeps 99-100% of the previous stage's area (median 99.9%; 41/43). The
  exceptions are Cold War (`mp_sp_002gr` stage 3 keeps 38%) and `nam_mp_006r` stage 3 (78% for the defenders).
- **Every M-COM is inside both sides' combat areas**, at least 5.7 m from the edge (p10 18 m).

**Conquest (16 levels)**

- **3-4 flags** (12 levels have 3, 4 have 4). The WW2 ports always have 5.
- **No two flags are closer than 58 m.** The nearest-neighbour distance is 58-450 m (median 121 m).
- **An HQ is never within 85 m of a flag**: the nearest flag is 85-384 m away (median 151 m). HQs are 285-1358 m
  apart (median 424 m).
- **Every flag lies between the two HQs.** Along the HQ-to-HQ axis the flags sit at 0.1-0.9 (52/52, median 0.5). They
  are within 0.4 HQ-distances of that line.
- **The bases are fair.** The two HQs' distances to their nearest flag differ by at most 54 m (median 14 m).
- **Capture zones** (outlines, spheres and the CaptureRadius field) are 7.5-37 m in equivalent radius (median 19 m).
  Capture takes 20 s (51/52; one 25 s). WW2: 10 s.
- **Spawns per flag**: usually one spawner per team (36/52) or one neutral spawner (16/52), with 2-21 spawn points.
  The spawn points lie 8-87 m from the flag (the median per flag is 23-113 m, typically 39-53 m).
- **Spawns per HQ**: 4-13 points (median 9), typically within 25 m (all within 95 m).
- **Tickets**: 200 (24 of 32 teams; 250 on 6, 350 on 2), with `TicketLossPerMin` 10 on every level. WW2: 130, with
  `TicketLossPerMin` 4.
- **Combat area**: 44,000-479,000 m2 (median 99,000). Flags are at least 11 m inside it (median 67 m). Every HQ is
  inside its own team's area, and every spawn point is at least 1.3 m inside its team's area.
- **Spawn protection by area.** Where each team has its own combat area (Vietnam Conquest, WW2), each HQ lies outside
  the enemy's area: 29-88 m outside on Vietnam, 3-35 m on WW2 (16/16). BFBC2 and BFBC1 Conquest use one shared area
  and have no such protection.
- **Layout shape** varies. Six of the 16 levels have their flags in a line, 3 in a band, 4 in a triangle and 3 spread
  out. The flag letters usually run in order across the map (|rho| >= 0.8 on 10/14), from either side.

**Squad Deathmatch (15 levels)**

- 4 squads (15/15), one vehicle (15/15, an IFV or tank shared by all), and a 50-kill target (15/15).
- 14-34 distinct spawn points, 13-38 m apart (median nearest neighbour), spread over 159-289 m.
- Combat area 15,000-84,000 m2 (median 40,000). Every spawn point is at least 2 m inside it.

**All modes**

- Every soldier spawner checks for enemies within `SpawnEnemyRadius` = 20 m (409/410; one 10 m on `mp_005gr`).
- **High ground.** DICE did not put objectives on hill tops.
  - Flag prominence (the ground at the flag minus the median ground 30-100 m around) is -6.5 to +7.4 m, median 0
    m. Only 4 of 52 flags are 5 m or more above their surroundings.
  - Rush M-COMs: -8.8 to +9.6 m, median 0 m (15/118 at 5 m or more).
  - The WW2 maps differ: 8 of 15 flags are 5 m or more above their surroundings (median +6.7 m).

## 2. Coverage

| Group | Conquest | Rush | Squad Rush | Squad Deathmatch | Other |
|---|---|---|---|---|---|
| BFBC2 (`mp_*`, `mp_sp_*`) | 9: mp_001, mp_003, mp_005, mp_006cq, mp_007, mp_008cq, mp_009cq, mp_012cq, mp_sp_005cq | 9: mp_002, mp_004, mp_005gr, mp_006, mp_007gr, mp_008, mp_009gr, mp_012gr, mp_sp_002gr | 7: mp_001sr, mp_002sr, mp_003sr, mp_005sr, mp_009sr, mp_012sr, mp_sp_002sr | 8: mp_001sdm, mp_004sdm, mp_006sdm, mp_007sdm, mp_008sdm, mp_009sdm, mp_sp_002sdm, mp_sp_005sdm | |
| BFBC1 (`bc1_*`) | 2 | 2 | 2 | 2 | |
| Vietnam (`nam_*`) | 5 | 5 | 5 | 5 | |
| WW2 (OPFOR's BF1943 ports) | 3: wake_island_s, guadal_canal, iwo_jima_s | - | - | - | coral_sea (Air Superiority): not measured, outside L1's modes |
| Onslaught (OPFOR's console ports) | | | | | 5: ons_mp_002, ons_mp_004, ons_mp_005, ons_mp_005gr, ons_mp_008 (flags, steps, zones, areas only) |

- **Levels measured:** 70, every level in `game\maps.ini` plus the Onslaught levels. Every one had its terrain height
  field.
- **Left out:**
  - `opfor_ons_mp_004`, OPFOR's own 28 KB patch bundle.
  - The campaign levels.
  - The common bundles.
- **Nothing is missing.** Every stock multiplayer level has its partition in the game copy.
- **Stock data.**
  - All BFBC2, BFBC1 and Vietnam level bundles in `game\` carry their 2010 / 2012 install dates, so they are DICE's
    files, unmodified.
  - The Onslaught and WW2 levels are OPFOR conversions of DICE data (Xbox 360 and BF1943), so they are kept as
    separate groups.
  - Where a level has both a Package and a Dist bundle (`mp_005`), the Package copy is used, as the game does.

## 3. Method

### 3.1 Reading the data

- `l1_read.py` reads each level's partition (`levels/<lv>.dbx` in `level-00.fbrb`, Package before Dist) from the
  bundle in memory. It uses the suite's `bfbc2core.fbrb.read_uncached` and parses with `bfbc2core.dbx.parse`, the
  Rust core.
  - No level was unpacked or prepared. `work\levels` was not used, because 50 of the 70 levels are not prepared
    there.
  - `bfbc2core.level.load` would have prepared them into `work\`.
- The terrain height field is read the same way, from the `terrain-00.fbrb` that holds
  `terrains/<name>/<name>.heightfield-000.res`. The name comes from the level's `TerrainEntityData`, as in
  `level.terrain_name`. It is parsed with `bfbc2core.terrain.parse`.
- **Positions** are each instance's `Transform` translation, floats 12-14. This is the row layout of
  `bfbc2core.level.Entity.matrix`.
- **Numbers** are read from their raw bits, never through `to_xml`, because of the DBX denormal trap. These fields are
  read as whole numbers: `TimeToGetControl`, ticket fields, `MaxKillCount` and the other fields DICE stores as
  integers.
- **Events** are `LevelData.EventConnections` (`Source`, `Target`, `SourceEvent.Name`, `TargetEvent.Name`). These are
  the edges `LevelWriter::connection_arrays` reads.

### 3.2 Entity semantics, and how each was checked

| Thing | Data | Checked against |
|---|---|---|
| HQ (base) | `CapturePointEntityData` with `UnableToChangeTeam` true and `InitialOwnerTeam` Team1 / Team2 | `mpconvert.rs` `capture_points` (base flag); `EDITOR_ARCHITECTURE.md` 3.12 |
| Conquest / Onslaught flag | `CapturePointEntityData` with `UnableToChangeTeam` false (a `CapturePoint` flag-pole blueprint, a `MapMarker`) | `modelayer.rs` `signature` (flags); flag counts match the maps (Panama Canal 4: A-D) |
| Rush spawn base | `CapturePointEntityData` (team-owned, never captured) enabled and disabled per stage | the event simulation below; names match (e.g. `mp_002`: US Base -> Fishing Port -> Lighthouse -> Village -> Hilltop) |
| Capture zone | `CapturePointEntityData.Geometry`: a `SphereGeometryEntityData` (`Radius`), an `AabbGeometryEntityData` (`Extents`), or an outline of `AreaGeometryEntityData` points linked by `Next`; else `CaptureRadius` | spec LEVEL-ENT-2 (outlines); data |
| Soldier spawns | `CapturePointEntityData.Spawners` -> `SoldierSpawnEntityData` -> `AlternativeSpawnPoints` (`AlternateSpawnEntityData`) | `EDITOR_ARCHITECTURE.md` (entity types); `modelayer.rs` `cluster` |
| Vehicle spawns | `VehicleSpawnEntityData`: its `Vehicles` items (vehicle blueprint + team), else `Vehicle`; classed by blueprint path (`objects/vehicles/<domain>/<kind>`) | data (list in `l1_measure.py` `VEH`) |
| M-COM | a `ReferenceObjectData` whose events reach a `GameInteractionEntityData` (arm / disarm) | `modelayer.rs` `mcoms` |
| Rush stage | M-COMs grouped by the set of `MultipleTriggerEntityData` their `onDestroyed` triggers | `modelayer.rs` `stages`; gives E3d's tested counts (`mp_005gr` 4x2, `mp_005sr` 2x1, `nam_mp_002r` 3x2) |
| Attackers | the team of the `LifeCounterEntityData` with `Attacker` true | data (Team1 on every stock Rush level) |
| Combat area | `CombatAreaTriggerEntityData.Geometry`: one or more outlines whose union is the playable area; `Team` + `UseTeamRestriction` = the team it applies to | spec LEVEL-ENT-2; data: every HQ is inside its own team's area and every Vietnam / WW2 HQ outside the enemy's |
| Squad Deathmatch squads | 4 `CapturePointEntityData` Team1-Team4 (bases) with one spawner each; `KillCounterEntityData` | `modelayer.rs` (4 kill counters); data |

**Rush stage order and per-stage state** (`l1_measure.py` `simulate`):

- **Start state.** It begins from every instance's `Enabled` field.
- **Stage completion.** For the stage being played, it follows the M-COMs' `onDestroyed` events through the event glue
  (`MultipleTrigger`, `DelayTrigger`, `EventSplitter`, `TeamFilter`, `EventGate`). Along the way:
  - it applies `Enable` / `Disable`, and fires the target's `OnEnabled` / `OnDisabled`;
  - it never fires another stage's counters.
- **Stage order.** The next stage is the one whose counters are now enabled.
- **Results.** All 30 Rush and Squad Rush levels resolved without ambiguity (`order_uncertain` false on every one).
  The bases' names progress as in the game.
- **Independent check.** On every one of the 30 levels, the stage count equals DICE's own count: the
  defenders' `LifeCounterEntityData.BaseCount`.
- **Simplifications.** Trigger counts are not modelled, and gates are assumed open. Both are harmless here because a
  stage ends only when all its M-COMs are destroyed.

### 3.3 Metrics

- **Distances** are horizontal; height differences are reported separately as `dy`.
- **Combat-area areas** are unions of the outlines, rasterised at 2 m or coarser to at most 3 M cells.
- **Margins** are signed distances to the union's edge (+ inside).
- **Rush per stage:**
  - spawn points are those of the side's bases active in that stage;
  - "attack direction" runs from the previous stage's M-COM centroid to this stage's (stage 1: from the attackers'
    spawn centroid);
  - "spawn depth" projects a side's spawn centroid onto that direction.
- **Prominence** is the ground under an objective minus the median ground on rings 30, 50, 75 and 100 m around it.
  The ground-percentile metric (the objective's ground among all the combat area's ground) is reported too. It is
  skewed by sea floor on island maps, so the rules use prominence.

### 3.4 Uncertain readings (marked as such)

1. **AABB `Extents`: half or full size?**
   - The box's transform is at the flag in every case (flags within a few metres of it), so the transform is the
     box's centre.
   - Read as half-sizes, BFBC2's AABB capture zones come out at 26-65 m equivalent radius, larger than its outlines
     (12-33 m) and spheres (26-37 m).
   - Read as full sizes, they come out at 13-33 m.
   - On WW2 the half-size reading agrees with the CaptureRadius-only flags (18-20 m).
   - Not settled. L1-CQ-008 leaves AABB zones out until it is (an executable check of the AABB containment test
     would settle it).
2. **`SurroundingGeometry`** is a larger outer outline on many combat areas. On `mp_005` it is 410,000 m2 against
   356,000; on WW2 it is about 7.3 million m2. Its meaning is not proven (minimap frame, or deserter zone?). It is not
   used in any rule. It is recorded as `outer_m2`.
3. **A spawner's own transform.**
   - With alternatives present, the spawner's own transform is treated as an anchor, not a spawn place. Spawn points
     are its `AlternativeSpawnPoints`; the spawner itself counts only when it has none (1 of 441).
   - Evidence: on Squad Deathmatch the four squads' spawners stand stacked 2.5 m apart, and on `nam_mp_007sdm`
     14-20 m outside the combat area, while their alternatives spread over the map.
   - On capture points, spawners are 5-72 m from the flag, like alternatives. The reading is plausible, not proven.
4. **Rush vehicles per stage** count the vehicle spawners in the active bases' `Spawners`.
   - Spawners in no base are listed separately (`standalone_enabled`) as an upper bound.
   - That list includes event-started and ambient vehicles (Vietnam Squad Rush shows 7 "defender" Hueys that are
     scenery), so it is not used.
   - Conquest vehicles are counted both ways: in the capture point's list, and as the "pool" of spawners within
     150 m whose nearest base or flag is that one. On some maps (`mp_006cq`, `mp_008cq`, Vietnam, WW2) HQ vehicles are
     standalone spawners near the HQ.
5. **No dedicated spawn-protection entity was found.**
   - `DamageAreaTriggerEntityData` instances are building-collapse kill zones (triggered by objects' `onDestroyed`,
     10,000 damage per second).
   - Spawn protection is done by per-team combat areas and the spawners' `SpawnEnemyRadius`.

## 4. Findings in detail

The full tables are in `l1_distributions.md`. The DICE group is below; WW2 and Onslaught are kept separate.

### 4.1 Conquest

| Metric (DICE, 16 levels) | min (where) | p10 | median | p90 | max (where) |
|---|---|---|---|---|---|
| flags | 3 | 3 | 3 | 4 | 4 (mp_001, mp_008cq, bc1_harvest_day_cq, nam_mp_007cq) |
| flag pair distance, m | 58 (mp_006cq) | 93 | 150 | 283 | 794 (mp_sp_005cq) |
| flag nearest neighbour, m | 58 (mp_006cq) | 78 | 121 | 203 | 450 (mp_sp_005cq) |
| flag pair height difference, m | 0 | 0.5 | 4.0 | 11 | 24 (mp_sp_005cq) |
| HQ to nearest flag, m | 85 (nam_mp_002cq) | 104 | 151 | 326 | 384 (bc1_harvest_day_cq) |
| HQ to farthest flag, m | 193 (mp_009cq) | 242 | 328 | 664 | 1072 (mp_sp_005cq) |
| HQ to HQ, m | 285 (mp_009cq) | 346 | 424 | 929 | 1358 (mp_sp_005cq) |
| HQ nearest-flag asymmetry, m | 1.7 | 4.3 | 14 | 42 | 54 (mp_012cq) |
| flag position on the HQ axis (0 = Team1 HQ) | 0.1 | 0.2 | 0.5 | 0.8 | 0.9 |
| flag lateral offset / HQ distance | 0.0 | 0.0 | 0.1 | 0.3 | 0.4 (mp_003) |
| capture zone, outline / sphere / radius, m | 7.5 (nam_mp_003cq) | | 19 | | 37 (mp_005) |
| flag spawn points | 2 (nam_mp_002cq B) | | 12 | | 21 |
| flag spawn distance, median per flag, m | 23 | | 48 | | 113 (mp_sp_005cq) |
| HQ spawn points | 4 | | 9 | | 13 |
| HQ spawn distance, median per HQ, m | 9 | | 25 | | 51 |
| HQ vehicle pool (spawners within 150 m) | 1 | | 4 | | 8 |
| tickets per team | 200 | 200 | 200 | 250 | 350 (mp_006cq) |
| combat area, m2 | 44k (mp_003) | 50k | 99k | 365k | 479k (mp_sp_005cq) |
| flag margin inside the area, m | 11 (nam_mp_002cq) | 30 | 67 | 155 | 238 |
| HQ margin inside its own area, m | 1.2 (nam_mp_002cq) | 5 | 21 | 52 | 65 |

**Capture zones by kind** (DICE):

| Kind | Flags | Size |
|---|---|---|
| outlines | 30 | 7.5-33 m equivalent radius |
| spheres | 2 | 26-37 m radius |
| `CaptureRadius` only | 4 | 12-20 m |
| AABBs | 16 | see 3.4.1 |

Outlines are used by BFBC1 and Vietnam throughout.

**Vehicles.**

- HQ pools are symmetric: the two HQs have the same count on 10 of 13 levels with pools, and differ by 2-3
  otherwise.
- The classes differ by faction: for example, `nam_mp_006cq` gives the US a Huey where the VC get a third tank.
- HQ pools hold tanks, emplacements, quads, IFVs, cars and helicopters, in that order of frequency.
- Flags hold mostly emplacements, IFVs, tanks and cars.
- Respawn delays (medians):

  | Class | Median delay |
  |---|---|
  | quad | 30 s |
  | car | 40 s |
  | tank | 40 s |
  | IFV | 45 s |
  | emplacement | 45 s |
  | UAV | 120 s |
  | heli | 0 s on 103 of 141 spawners (what 0 means is not proven; 35-105 s on the others) |

**Layout shape.**

- The flags make a line on 6 levels, a band on 3, a triangle on 4 and a spread on 3.
- DICE's Index order does not follow the axis (rho -1 to 1, median 0).
- The flag letters do follow the axis: |rho| >= 0.8 on 10/14 levels, in either direction.

### 4.2 Rush and Squad Rush

| Metric (DICE) | Rush: min (where) | median | max (where) | Squad Rush: range (median) |
|---|---|---|---|---|
| stages | 3 | 3 (BFBC2 4) | 5 (mp_002, mp_006, mp_009gr, bc1_oasis_gr) | 2 (2) |
| M-COMs per stage | 2 | 2 | 2 | 1 |
| M-COM pair distance, m | 11.7 (mp_007gr s1) | 40 | 91 (nam_mp_006r s1) | - |
| M-COM pair angle to the attack direction | 1 deg | 57 deg | 90 deg | - (no strong tendency) |
| stage step, m | 93 (mp_sp_002gr s3) | 174 | 316 (bc1_oasis_gr s2) | 45-169 (119) |
| attacker nearest spawn to objective, m | 72 (mp_007gr s1) | 160 (stage 1: 198) | 459 (mp_006 s1) | 55-165 (111-118) |
| defender nearest spawn to objective, m | 6 (mp_008 s3) | 34 | 61 (nam_mp_005r s2) | 23-72 (45) |
| spawn ratio attacker / defender | 1.8 | 4.9 | 21.5 (mp_004 s2) | 1.0-4.4 |
| defender spawn depth behind objectives, m | -16 (nam_mp_002r s3) | 39 | 74 | -27 (mp_012sr s1) to 64 (42) |
| attacker spawn depth, m (negative = in front) | -490 | -187 | -97 | -203 to -64 (-120) |
| attacker base to previous defender base, m | 27 (mp_008 s3) | 75 | 321 (bc1_oasis_gr s5) | 27-114 (85) |
| stage-1 attacker spawns vs defenders' area, m | -141 | -91 | -1.7 (mp_007gr) | -82 to -24 (-43) |
| attacker area per stage, m2 | 10k (nam_mp_002r s1) | 104k | 359k (bc1_oasis_gr s5) | 9k-56k |
| defender area per stage, m2 | 8k | 95k | 359k | 7k-56k |
| area kept from the previous stage, % | 38 (mp_sp_002gr s3) | 99.9 | 100 | 99-100 |
| M-COM margin inside the attackers' / defenders' area, m | 5.7 (mp_009gr s3) | 41 / 50 | 96 / 126 | 11-77 |
| attacker lives | 75 | 75 (BFBC2 100) | 100 | 20 |
| attacker / defender vehicles per stage (base-attached) | 0-11 / 0-9 | 3 / 2 | | 0 / 0-1 |

- **Stage 1 versus later stages.** Stage-1 combat areas are the smallest: attackers 10k-128k m2 (median 53k),
  defenders 8k-98k (median 32k). Each later stage adds 8k-110k m2 (median 29k).
- **Areas per side.** The defenders' area is the same size as the attackers' from stage 2 on (often the same
  outline). At stage 1 it is smaller, so that the attackers' HQ lies outside it.
- **Vehicle classes**, over all stages:
  - Attackers: boats, UAVs, quads, tanks, IFVs, cars, helicopters.
  - Defenders: mostly emplacements (111 of 137), some tanks.

### 4.3 Squad Deathmatch

| Metric (DICE, 15 levels) | min (where) | median | max (where) |
|---|---|---|---|
| squads | 4 | 4 | 4 |
| distinct spawn points | 14 (bc1_harvest_day_sdm) | 19 | 34 |
| spawn nearest neighbour (median), m | 12.8 | 21 | 37.9 (bc1_harvest_day_sdm) |
| spawn spread (largest distance), m | 159 (mp_sp_005sdm) | 239 | 289 (nam_mp_006sdm) |
| combat area, m2 | 15k (mp_004sdm) | 40k | 84k (nam_mp_006sdm) |
| spawn margin (median), m | 7.6 | 19 | 41 |
| vehicles | 1 | 1 | 1 |

The 4 squads share one pool of spawn points: each squad's spawner has the same alternatives. On 13/15 levels the
squads' capture points stand at one spot.

### 4.4 Onslaught (OPFOR's console ports; descriptive only)

- 3-5 flags, captured in Index order.
- Consecutive flags are 138-336 m apart (median 240 m). The HQ is 128-437 m from the first flag.
- Capture zones are 14-107 m. ONS_MP_004's AABB-union zones are the large ones; `CaptureRadius` is 24 m where it is
  the only zone.
- 3-8 combat areas per level, 25k-289k m2.
- These are Napisal / OPFOR conversions of DICE's Xbox data, so no V1 rule is proposed for them.

### 4.5 The WW2 group (OPFOR's BF1943 Conquest ports)

They follow BF1943's design, not BFBC2's, and need their own thresholds:

- **Flags and bases.**
  - 5 flags (3/3).
  - Flags are 103-205 m from their nearest neighbour.
  - HQs are 202-633 m from the nearest flag and 794-812 m from each other (carriers and island bases).
- **Capturing.** Capture takes 10 s. Zones are 12-20 m (CaptureRadius and spheres).
- **Spawns.** Each flag has one neutral spawner with 4-5 points, 10-53 m out.
- **Tickets.** 130, losing 4 per minute.
- **Areas.** Combat areas are per team (a shared outline plus the team's own base piece), 1.17-1.70 million m2. Each
  HQ is 3-35 m outside the enemy's area.
- **Hill tops.** Flags favour them: median prominence +6.7 m, and 8 of 15 flags at 5 m or more.
- **Asymmetry.** The HQ nearest-flag asymmetry reaches 94 m (`wake_island_s`), beyond DICE's BFBC2 maximum.

## 5. V1 rule proposal

**Conventions.**

- Severity: **warning** = outside every stock value with a margin (DICE never did this); **info** = outside DICE's
  usual band (a few stock levels do it; they are named).
- The thresholds are checked against the stock data in `l1_rules_check.md`. No rule raises a warning on any stock
  level. The info flags on stock levels are listed under "Stock flags".
- Scope "DICE" = BFBC2, BFBC1 and Vietnam levels of that mode. WW2 rules apply to WW2-era projects.
- `{s}` is the subject's name, `{v}` the value, `{st}` the stage.

| ID | Scope | What | Warning when | Info when | Message | Evidence | Stock flags |
|---|---|---|---|---|---|---|---|
| L1-CQ-001 | Conquest, DICE | flag count | 0 flags | not 3-4 | "Conquest has {v} flags; DICE's Conquest maps have 3-4." | 16/16 have 3-4 | none |
| L1-CQ-001w | Conquest, WW2 | flag count | 0 | not 5 | "WW2 Conquest has {v} flags; the BF1943 maps have 5." | 3/3 have 5 | none |
| L1-CQ-002 | Conquest, DICE | nearest other flag | < 50 m | < 70 or > 300 m | "Flag {s} is {v} m from the nearest other flag; DICE keeps flags 58-450 m apart (median 121 m)." | 52 flags, 58-450 m | info: mp_006cq A/B/C (58-68), mp_008cq C/D (64), mp_sp_005cq A-C (up to 450) |
| L1-CQ-002w | Conquest, WW2 | nearest other flag | < 80 m | not 100-250 m | "... the WW2 maps keep 103-205 m." | 15 flags | none |
| L1-CQ-003 | Conquest, DICE | HQ to nearest flag | < 80 m | not 100-400 m | "HQ {s} is {v} m from its nearest flag; on DICE's maps an HQ is 85-384 m from the nearest flag." | 32 HQs | info: nam_mp_002cq (85, 100), mp_007 US_Guardpost (97) |
| L1-CQ-003w | Conquest, WW2 | HQ to nearest flag | < 150 m | not 200-650 m | "... on the WW2 maps 202-633 m." | 6 HQs | none |
| L1-CQ-004 | Conquest, DICE | HQ to HQ | < 250 m | not 280-1000 m | "The HQs are {v} m apart; DICE's are 285-1358 m apart (median 424 m)." | 16 levels | info: mp_sp_005cq (1358) |
| L1-CQ-005 | Conquest, DICE + WW2 | HQ nearest-flag asymmetry | - | > 60 m | "One HQ is {v} m closer to its nearest flag than the other; DICE keeps this within 54 m." | 16/16 DICE <= 54 m | info: wake_island_s (94), guadal_canal (65) |
| L1-CQ-006 | Conquest, all | flag position on the HQ axis | outside 0-1 (behind an HQ) | outside 0.1-0.9 | "Flag {s} is not between the HQs ({v} of the way from Team 1's HQ); DICE's flags lie at 0.1-0.9." | 67/67 at 0.1-0.9 | none |
| L1-CQ-007 | Conquest, DICE | flag's sideways offset / HQ distance | - | > 0.5 | "Flag {s} is {v} HQ-distances off the line between the HQs; DICE's are within 0.4." | 52/52 <= 0.4 (WW2 up to 1.1) | none |
| L1-CQ-008 | Conquest, all | capture zone equivalent radius (outline, sphere, CaptureRadius) | < 5 m | not 7-40 m | "Flag {s} has a capture zone of {v} m; DICE's are 7.5-37 m." | 39 zones (AABB zones left out, 3.4.1) | none |
| L1-CQ-009 | Conquest, DICE / WW2 | time to capture | - | not 20-25 s (WW2: not 10 s) | "Flag {s} takes {v} s to capture; DICE's take 20 s (BF1943's 10 s)." | 51/52 at 20 s, 1 at 25; WW2 15/15 at 10 | none |
| L1-CQ-010 | Conquest, all | flag spawn points | 0 | < 2 | "Flag {s} has {v} spawn points; DICE's flags have 2-21 (median 12)." | 67 flags | none |
| L1-CQ-011 | Conquest, all | farthest spawn point from its flag | > 200 m | > 140 m | "A spawn point of flag {s} is {v} m from the flag; DICE's are within 137 m." | 67 flags, 15-137 m | none |
| L1-CQ-012 | Conquest, all | HQ spawn points | 0 | < 4 | "HQ {s} has {v} spawn points; DICE's HQs have 4-13 (median 9)." | 38 HQs | none |
| L1-CQ-013 | Conquest, all | flag inside the combat area | outside (< 0 m) | < 10 m from the edge | "Flag {s} is {v} m inside the combat area edge (negative: outside); DICE's are 11 m or more inside." | 67/67 >= 10.9 m | none |
| L1-CQ-014 | Conquest, all | HQ inside its own team's area | outside | < 1 m | "HQ {s} is outside its team's combat area." | 38/38 inside | none |
| L1-CQ-015 | Conquest with per-team areas | HQ against the enemy's area | inside it | < 3 m outside | "HQ {s} is inside the enemy's combat area by {v} m; where each team has its own area, DICE keeps every HQ outside the enemy's (by 3-88 m)." | 16/16 outside | none |
| L1-CQ-016 | Conquest, DICE | tickets | - | not 200-350 | "Conquest starts with {v} tickets; DICE's maps use 200-350." | 32 teams | none |
| L1-CQ-017 | Conquest, DICE | combat area | - | not 40,000-500,000 m2 | "The combat area is {v} m2; DICE's Conquest areas are 44,000-479,000 m2." | 16 levels | none |
| L1-RU-001 | Rush, DICE | stages | < 2 | not 3-5 | "Rush has {v} stages; DICE's Rush maps have 3-5." | 16/16 | none |
| L1-RU-002 | Rush / Squad Rush, DICE | M-COMs per stage | not 2 (Squad Rush: not 1) | - | "Stage {st} has {v} M-COMs; every DICE Rush stage has 2 (Squad Rush 1)." | 59/59 and 28/28 | none |
| L1-RU-003 | Rush, DICE | M-COM pair distance | < 10 or > 120 m | not 25-80 m | "The stage {st} M-COMs are {v} m apart; DICE's pairs are 12-91 m apart (median 40 m)." | 59 pairs | info: mp_007gr s1 (12), mp_006 s5 (16), mp_sp_002gr s3 (21), nam_mp_006r s1 (91) |
| **L1-RU-004** | Rush, DICE | stage step (the user's example) | < 75 m | not 100-320 m | "Stage {st} is {v} m from stage {st-1}; DICE's Rush stages are 93-316 m apart (median 174 m)." | 43 steps | info: mp_sp_002gr s3 (93) |
| L1-RU-004s | Squad Rush, DICE | stage step | < 40 m | not 60-180 m | "Squad Rush stage {st} is {v} m from stage {st-1}; DICE's are 45-169 m apart (median 119 m)." | 14 steps | info: mp_sp_002sr s2 (45) |
| L1-RU-005 | Rush (Squad Rush) | defenders' nearest spawn to an M-COM | > 100 m | not 5-65 m (SR: 20-75) | "The defenders' nearest spawn is {v} m from a stage {st} M-COM; DICE's are 6-61 m (median 34 m)." | 59 + 28 stages | none |
| L1-RU-006 | Rush (Squad Rush) | attackers' nearest spawn to an M-COM | < 40 m | < 60 m or > 460 m (SR: not 50-170) | "The attackers' nearest spawn is {v} m from a stage {st} M-COM; DICE's are 72-459 m (median 160 m)." | 59 + 28 | none |
| L1-RU-007 | Rush (Squad Rush) | attackers' nearest / defenders' nearest | < 1.0 (SR 0.75) | < 1.5 (SR 1.0) | "At stage {st} the attackers spawn only {v} times as far from the M-COMs as the defenders; DICE: 1.8-21x (median 5x)." | 59 Rush stages >= 1.8; Squad Rush 1.0-4.4 | info: none (SR at 1.0 sits on the line) |
| L1-RU-008 | Rush + Squad Rush | defenders' spawns behind the objectives | < -30 m | < 0 m | "At stage {st} the defenders spawn {v} m behind the M-COMs (negative: in front, towards the attackers)." | 85/87 behind | info: nam_mp_002r s3 (-16), mp_012sr s1 (-27) |
| L1-RU-009 | Rush + Squad Rush | attackers' spawns in front of the objectives | >= 0 (at or behind) | > -60 m | "At stage {st} the attackers spawn only {v} m in front of the M-COMs; DICE's spawn 64-490 m in front." | 87/87 | none |
| L1-RU-010 | Rush + Squad Rush | stage-1 attacker spawns against the defenders' area | any inside | < 1 m outside | "A stage-1 attacker spawn point is {v} m inside the defenders' combat area; DICE keeps the attackers' first base outside it (by 2-141 m)." | 30/30 outside | none |
| L1-RU-011 / 012 | Rush + Squad Rush | M-COM inside the attackers' / defenders' area | outside | < 10 m from the edge | "A stage {st} M-COM is {v} m inside the attackers' (defenders') combat area edge; DICE's are 6 m or more inside." | 146/146 inside | info: mp_009gr s3 (5.7), mp_sp_002gr s2-s3 (8-10) |
| L1-RU-013 | Rush + Squad Rush | area kept from the previous stage | - | < 75% | "The attackers' combat area at stage {st} keeps only {v}% of the stage before; DICE grows it (median 99.9%)." | 56/57 >= 99% | info: mp_sp_002gr s3 (38) |
| L1-RU-014 | Rush, DICE | M-COM pair height difference | - | > 10 m | "The stage {st} M-COMs differ by {v} m in height; DICE's pairs are within 8.5 m." | 59/59 | none |
| L1-RU-016 | Rush + Squad Rush | defenders' `LifeCounterEntityData.BaseCount` against the stage count | not equal | - | "The defenders' life counter counts {v} bases but the layer has {n} stages; on every DICE Rush level they are equal." | 30/30 equal (checked by hand from `rush.lives` / `sr.lives` `base_count`; not in `l1_rules.py`) | none |
| L1-RU-015 | Rush, DICE | attacker lives | - | not 75-100 (Squad Rush: not 20) | "The attackers have {v} lives; DICE's Rush uses 75-100 (Squad Rush 20)." | 16/16, 14/14 | none |
| L1-SD-001 | Squad Deathmatch, DICE | squads | < 2 | not 4 | "Squad Deathmatch has {v} squads; DICE's has 4." | 15/15 | none |
| L1-SD-002 | Squad Deathmatch, DICE | distinct spawn points | < 8 | not 12-40 | "Squad Deathmatch has {v} spawn points; DICE's maps have 14-34." | 15 | none |
| L1-SD-003 | Squad Deathmatch, DICE | spawn spacing (median nearest neighbour) | - | not 10-40 m | "Spawn points are {v} m apart; DICE's: 13-38 m." | 15 | none |
| L1-SD-004 | Squad Deathmatch, DICE | combat area | - | not 14,000-90,000 m2 | "The Squad Deathmatch area is {v} m2; DICE's are 15,000-84,000 m2." | 15 | none |
| L1-SD-005 | Squad Deathmatch, DICE | spawn point inside the area | > 5 m outside | outside | "A spawn point is {v} m outside the combat area; DICE's are all 2 m or more inside." | 15/15 | none |
| L1-SD-006 | Squad Deathmatch, DICE | vehicles | - | not 1 | "Squad Deathmatch has {v} vehicles; DICE's maps have one." | 15/15 | none |
| L1-AL-001 | all modes | spawner `SpawnEnemyRadius` | - | not 10-20 m | "A spawner checks for enemies within {v} m; DICE uses 20 m." | 409/410 at 20 m | none |
| L1-AL-002 | Conquest (any mode with team areas) | spawn point inside its team's area | outside | < 1 m | "A spawn point of {s} is outside its team's combat area." | 1,146/1,146 inside (>= 1.3 m) | none |
| L1-HT-001 | Conquest, DICE | flag prominence | - | above or below its surroundings by > 10 m | "Flag {s} stands {v} m above the ground around it; DICE's flags are within 7.5 m." | 52/52 | none (WW2: up to 21 m; do not apply) |
| L1-HT-002 | Conquest, DICE | flag pair height difference | - | > 25 m | "Two flags differ by {v} m in height; DICE's differ by up to 24 m." | 60 pairs | none |

**Notes for the implementer.**

- **Structural checks are already elsewhere.** The editor's existing `mapexport check` and `modelayer::missing`
  already refuse an incomplete layer: no spawn, no flag, missing stages. The L1 rules are about placement and stay
  advisory.
- **Rush rules need the stage simulation.** Every Rush rule needs each stage's active bases and combat areas. The
  editor can get them the way `simulate` does: the stage counters' `Enable` / `Disable` closure. Alternatively,
  E4's gameplay tools can store them explicitly.
- **Spec form.** Each rule fits the spec's format as an **observed** rule with a `[data: ...]` line. For example:

  ```text
  L1-RU-004: Rush stages are 93-316 m apart
  Status: observed
  [data: 16 DICE Rush levels, 43 stage steps, min 93 m mp_sp_002gr s3, median 174, max 316 bc1_oasis_gr s2]
  ```

  The spec README's layer 3 is "engine subsystems", so gameplay / mode layout could be a `engine/gameplay.md` or a
  new modes topic. That placement is the user's call.

## 6. A1 input: the typical layout per mode

Values are median (middle half), from `l1_a1_params.json`. A generator should sample within the middle half and stay
inside the warning bands of section 5.

**Conquest (DICE)**

- **Bases.** 2 HQs, 424 m apart (378-655).
- **Flags.**
  - 3 flags; 4 when HQ-HQ > ~600 m.
  - Each flag sits on the HQ axis at t = 0.5 (0.4-0.7) for the middle flag, with lateral offset 0.1 (0.1-0.2) x the
    HQ distance.
  - Flags are 121 m apart (104-153) to their nearest neighbour.
  - The nearest flag is 151 m (125-236) from each HQ, with the two HQs within ~15 m of each other on that distance.
- **Capture.** Zone radius 19 m (15-22) on flat open ground; the outline follows the cover. Capture time 20 s.
- **Flag spawns.** One spawner per team (or one neutral spawner) with 12 points (10-14), 39-53 m from the flag,
  nearest 21-36 m, farthest 51-73 m.
- **HQ spawns.** 9 points (7-11), 19-34 m out (farthest 37-61 m).
- **Vehicles.** HQ pool 4 (3-7): tank, IFV, quads, an emplacement, on large maps a helicopter. Flags 2 (2-4), the
  same for both owners' variants.
- **Tickets.** 200, losing 10 per minute.
- **Combat area.** 99,000 m2 (81,000-140,000) around everything. Flags 41-97 m inside, HQs 13-42 m inside, every
  spawn inside.
- **Lettering.** Letter the flags A, B, C ... along the HQ axis.
- **Height.** Prominence about 0: flags are not on hill tops.

**Conquest (WW2 / BF1943)**

- **Bases.** HQ-HQ 805 m. Per-team combat areas, with each base 3-35 m outside the enemy's.
- **Flags.**
  - 5 flags, 152 m (117-179) apart.
  - The nearest flag is 360 m (288-496) from each HQ.
  - Lateral offset up to 1.1.
  - Flags on hill tops (prominence +7 m; 0-10).
- **Capture.** Zones 18 m (15-19); capture in 10 s.
- **Spawns.** One neutral spawner per flag with 4-5 points, 24-30 m out. HQ: 6 points.
- **Vehicles.** HQ pool 8-11: boats, planes, emplacements.
- **Tickets.** 130, losing 4 per minute.

**Rush (DICE)**

- **Stages.** 3-5; 4 on BFBC2. Count about total attack length / 174 m.
- **Stage steps.** 174 m (134-215) apart, each step continuing the attack direction.
- **M-COM pairs.** 2 per stage, 40 m apart (34-56), within 8.5 m of each other in height. Pair angle to the attack
  direction 57 deg (34-75): side by side more often than one behind the other.
- **Defenders.**
  - Base nearest spawn 34 m (26-41) from an M-COM, spawn centroid 39 m (33-48) behind the objectives.
  - Vehicles mostly emplacements.
  - The defenders' life counter: 1,500 (Squad Rush 1,000), with `BaseCount` = the number of stages.
- **Attackers, stage 1.**
  - HQ 198 m (150-274) to the nearest M-COM, spawn centroid 187 m (152-249) in front.
  - The HQ lies outside the defenders' area by 91 m (69-119).
- **Attackers, later stages.**
  - The forward base is 75 m (57-123) from the previous stage's defender base.
  - Its nearest spawn is 148 m (122-187) from the new M-COMs.
- **Attacker vehicles.** 3 (1.5-5) per stage: boats, UAV, quads, a tank, an IFV, a car. Lives 75 (BFBC2 75-100).
- **Combat areas.**
  - Stage 1: attackers 53,000 m2 (31-95k), defenders 32,000 (20-53k).
  - Each later stage adds 29,000 m2 (20-39k) and keeps the previous area. From stage 2 on, both sides share the same
    outline.
  - M-COMs 27-56 m inside the attackers' area, 34-74 m inside the defenders'.

**Squad Rush (DICE)**

- 2 stages of 1 M-COM each, 119 m (107-142) apart.
- Defenders' nearest spawn 45 m (37-53), 42 m behind. Attackers' nearest spawn 111-118 m, 120 m in front.
- Stage-1 attacker spawns 43 m outside the defenders' area.
- Areas 18,000 / 13,000 m2 at stage 1, plus 16,000 at stage 2.
- No vehicles. Attacker lives 20.

**Squad Deathmatch (DICE)**

- 4 squads on one shared pool of 19 spawn points (18-24), 21 m (16-28) apart, spread 239 m.
- Combat area 40,000 m2 (25-50k). Spawns 14-30 m inside the edge.
- One shared IFV / tank. 50 kills to win.

## 7. Open questions

1. **AABB `Extents`, half or full size** (3.4.1). Check the engine's AABB containment test in `BFBC2Game.exe`. Until
   then, L1-CQ-008 leaves AABB zones out.
2. **`SurroundingGeometry`'s role** (minimap frame, deserter strike zone?). It would add an "outer margin" rule if
   proven.
3. **Whether a spawner's own transform is a spawn place** when it has alternatives (3.4.3). A play test of a Squad
   Deathmatch level would settle it.
4. **Rush vehicles from standalone spawners started by events.** They need the simulation extended to `Spawn` /
   `DisableSpawnPointFunctionality`, and ambient vehicles told apart.
5. **Onslaught.** Its stages could be simulated the same way (`OnCaptured` of flag k as the transition) if ONS rules
   are wanted later.

## 8. Re-running

Run with the suite's interpreter (`bin\python.txt`: `C:\Python313\python.exe`) from this folder, in this order. Each
step reads the game copy only.

1. `l1_measure.py [out.json] [levels...]`: the measurements (about 35 s for all 70 levels; under 1 GB).
2. `l1_analyze.py`: `l1_distributions.md`.
3. `l1_rules.py`: `l1_rules_check.md` and `l1_rules.json`.
4. `l1_a1.py`: `l1_a1_params.json`.

The modules they use:

- `l1_read.py`: bundle and partition reader.
- `l1_level.py`: instances, fields, events, outlines.
- `l1_geom.py`: polygons, raster areas, margins, height field.
- `l1_levels.py`: level list from `maps.ini`.

The checks behind section 3 are kept as scripts:

| Script | What it checks |
|---|---|
| `l1_scan.py` | event names, vehicle kinds and capture-zone kinds over every level |
| `l1_explore.py` | a Rush level's stage event graph |
| `l1_cps.py` | capture points, spawners and areas |
| `l1_dmg.py` | damage and area triggers |
| `l1_try_ca.py` | combat-area outlines |
| `l1_try_hf.py` | height-field lookup |
| `l1_anchor.py` | spawner versus alternatives |
| `l1_aabb.py` | AABB frames |
| `l1_sdmcheck.py` | Squad Deathmatch spawns outside the area |
| `l1_rushcheck.py` | stages, bases and areas per Rush level |
| `l1_dump.py`, `l1_show.py` | printing helpers |
