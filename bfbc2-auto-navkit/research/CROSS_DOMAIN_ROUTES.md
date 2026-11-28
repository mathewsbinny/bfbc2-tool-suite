# Boats and routes across domains: research and design (2026-10-26)

Companion to `KYNAPSE_SPEC.md` §2.1 and battlefield-opfor's `bfo:docs/plans/opfor/STOCK_MP_AI_PLAN.md` §11 (the WW2
follow-up: `bfo:docs/plans/opfor/WW2_BOT_ROUTES_PLANNING_2026-11-16.md`). The research below was done offline: level
and common data, DICE's path data and string tables of the executable. Nothing was run in the game. Tools (both
prototypes, no other tool calls them): `tools\navwater_mesh.py` (boat meshes), `tools\navdomains.py` (domain graph and
planner; its Dijkstra in the Rust core since round E4e, `bc2_navbuild::domains`, the Python as fallback). Tests:
`tests\test_navwater_mesh.py` and `tests\test_navdomains.py`. Status (suite 0.37.0): the findings and design are as of 2026-10-26; later rounds only moved the tools' inner loops
to Rust (E4e, NK5-rest) with the same answers.

## 1. Findings: water and boats in BC2's AI

- **The engine knows three navigation types, and none is for water.** `AIVehicleBehaviourData.Navigation` has
  exactly three values: `NTNone`, `NTGround` and `NTHelicopter`. These are the only `NT*` enum strings in
  BFBC2Game.exe. There are four movement plans (`MoveToGround`, `MoveToInfantry`, `MoveToInfantrySpecial`,
  `IdleAir` / `IdleGround`) and four control kinds (`CarMovement`, `TankMovement`, `Helicopter`, `Soldier` /
  `Turret`). No boat, water or swim entry exists in the AI enums. Kynapse path data stays ground only:
  `ForbidWater` is true in both `PathfindingSettings`, with `WaterLevelAdjustment` -3.5 for infantry and -1.6 for
  vehicles.
- **The patrol boat's driver never asks for a route.** `mp_common` and `sp_common` `AI/Behaviours` both contain a
  vehicle profile named `PBL`:
  - Controls `CarMovement`, MoveTo plan `MoveToGround`.
  - **`Navigation` `NTNone`, `PathfindingType` null.**
  - Goals: Idle, MoveTo, Change, Interlude, MoveToRestrictedArea, Avoid (no Attack).

  The PBL's driver entry (`Pbl.xml`, `Entry Driver` AIData) points to it, with `MobilityData` MaxSpeed 17. The
  other profiles for comparison:

  | Profile | Navigation | PathfindingType |
  |---|---|---|
  | Car, Tank, UnarmedTank | `NTGround` | `VehicleData` |
  | Infantry, Squad, Sniper, Support | `NTGround` | `InfantryData` |
  | Helicopter, SerdarHind, UnarmedHelicopter | `NTHelicopter` | null |
  | PBL, MarkV, Fixed, UnarmedFixed, OpenVehicle | `NTNone` | null |

  So a Kynapse water layer would be ignored by the stock boat driver. With NTNone the driver can only steer
  straight at a MoveTo target. That fits Napisal's Isla Inocentes boats having no orders.
- **The jet ski's driver uses the `Car` profile** (`PWC_test.xml` AIData 4ea863e0 → `Car`: `NTGround` on
  `VehicleData`, MaxSpeed 20). An AI jet-ski driver would ask the land vehicle file for routes, and that file has
  no water. No level seats AI on a jet ski.
- **DICE's campaign never pathfinds a boat.** Sea-vehicle spawns per level:
  - SP_01's Zodiacs: the player plus cutscene drivers.
  - SP_03: `SP03_AB2_BoatIntro`, an AI driver (`SSAiPlayer`, tank-driver template) and two AI gunners, moved by a
    `CutsceneVehicleEntityData`. Its `ChangeOrderEntityData` only sets readiness for the gunners. The player's
    PBL is player-driven.
  - SP_003_B: a cutscene rowing boat.
  - SP_008 / SP_008_B: no sea-vehicle spawn in the level partition.
- **Napisal's Isla Inocentes (ONS_MP_004).** Two Team2 `Spawn_Pbl` at (45, 77, -319) and (-8, 76, -2), each
  crewed by `Stranded Ship_SS03` `SSAiPlayer` spawners (driver seat 0, gunner seat 1). No order entity, waypoint
  or patrol references them. Nine more sea spawns (PBLs and PWCs, mostly at the US base dock) are for players.
  The level's vehicle file stays on land (spec §2.1).
- **Other water data:** no waypoint or patrol paths on water. `AnimatedWaypointData` / `WaypointData` are soldier
  routes. The water itself is in `Terrains/<t>/<t>.Water.Mesh.res` (`core/bfbc2core/water.py`).

**Consequence.** OPFOR's bots must drive boats themselves (option B of plan §11): steer by input along OPFOR's
own boat graph. The game cannot use a Kynapse water layer (option A) for the PBL driver unless the data is changed.
That would mean switching `PBL` to `NTGround` with a third `PathfindingType`, plus a level `PathfindingTypes`
entry and a `<OutputName>\pathdata.hpd`. Whether the engine loads a third type, and how `MoveToGround` drives a
boat, can only be tested in game; this is on the ON HOLD list. For a jet ski, water areas added to the vehicle
file would be used as they are (the Car profile), with the snapping risk plan §11 names.

## 2. Chosen approach: a boat mesh as a side file (b), holding its own waypoint graph (c)

`navwater_mesh.py <level> [--profile pbl|pwc]` writes `work\navwater\<level>\<level>.<profile>.watermesh.json`:

1. **Grid.** Cells of 2 m over the water surface (`water.py`; the highest piece where pieces overlap), inside a
   box. The default box is OPFOR's in-game sampling box; `--box` gives another.
2. **Blocked.** A cell is blocked if anything lies in the hull band, from `draft` under the surface to `height`
   above it:
   - the terrain (the highest of five height-field points per cell);
   - any surface OPFOR's in-game samples recorded (soldiers excepted);
   - any collision triangle of a placed object or group instance (`objects.json` / `group_instances.json` and the
     blueprints' `_Havok.res`, sampled every pitch / 3).

   Piers, ship hulls, wrecks, buoys and bridge piers block. A bridge deck above the band does not, and walls are
   caught because their triangles cross the band.
3. **Free.** Navigable cells with no blocked or dry cell within the hull radius: where the boat's centre may be.
4. **Regions** of free cells (8-connected; a diagonal needs a free side cell).
5. **Route graph.** One node per region piece in each 8 x 8 block, at the cell with the most clearance. Edges go
   to nodes up to two blocks away that are in sight over free cells (cost in metres). Where a region's graph would
   split, repair paths are added (grid search, string-pulled).
6. **Shore links.** Each link joins a free cell within boarding reach (radius + 2 m) to infantry ground at the
   water line, from a path-data file (DICE's by default):
   - **beach:** terrain ground from 1.5 m under the surface (DICE's wading limit, `navwater.py`) to 0.5 m above
     it;
   - **dock:** object ground 0.3-3 m above the surface.

   There is one link per 20 m tile of shore, per land island and boat region. Each link records its land
   island, its route node and the file's SHA-1.
7. **Spawns.** Sea-vehicle `VehicleSpawnEntityData` are attached to the mesh. A spawn off the free water (at a
   dock) gets a launch cell up to 15 m out.

Profiles: `pbl` (radius 3.5 m, draft 1.0, height 3.0, 17 m/s) and `pwc` (1.5 / 0.5 / 1.5, 20 m/s). The speeds are
the AI's MaxSpeed values.

### Validation (offline)

ONS_MP_004, PBL, default box (x -220..120, z -420..500):
- water: 24,457 cells (97,828 m²);
- navigable 87.7 %, free 79.8 %;
- 10 regions, the largest holding 99.9 % of the free water;
- 439 nodes, 3,443 edges, 4 repair paths.

Structure checks:
- 0 nodes off free cells, 0 edges blocked or crossing regions, 0 split regions;
- in 2,000 sampled free cells, 100 % see a node;
- the side file reads back identical.

Shore links:
- 111 links (109 beach, 2 dock);
- 100 % land on the infantry file's cells;
- 90.1 % land on islands of 25+ nodes, 85.6 % have a route node within 10 m.

Spawns:
- all 11 sea-vehicle spawns are attached, in one region;
- the two AI `Spawn_Pbl` are on free water;
- the dock spawns launch 4-12.7 m out (PBL) and 0-2 m (PWC);
- every spawn reaches 97 of the 111 links over the graph.

Wider box (x to 700): 55,325 water cells, 86.2 % free, structure OK. PWC: 93.0 % free, 143 links. Other levels,
all with the structure OK and links 100 % on infantry cells:

| Level | Free water | Regions (largest) | Shore links |
|---|---|---|---|
| ONS_MP_002 | 90.2 % | 8 (98.4 %) | 138 |
| SP_01 | 77.5 % | 56 (73.3 %) | 95 |
| SP_02 (river) | 47.4 % | 4 | 52 |
| SP_008 (pond) | 85.6 % | 1 | 2 |

SP_02's low figure needs a look: about 4,700 collision cells, from vegetation or rocks in the river (§6).

Sections 3 to 8 (the domains, the transitions between them, the planner, what the game and OPFOR's bot layer each
do, how vehicle path data plugs in, and what is implemented or only designed) moved to battlefield-opfor's knowledge
base in round KB2 (2026-11-28): `bfo:docs/research/navigation/CROSS_DOMAIN_ROUTES.md`. Their headings stay here.

## 3. Domains

`bfo:docs/research/navigation/CROSS_DOMAIN_ROUTES.md`, "3. Domains".

## 4. Transitions

`bfo:docs/research/navigation/CROSS_DOMAIN_ROUTES.md`, "4. Transitions".

## 5. Planner

`bfo:docs/research/navigation/CROSS_DOMAIN_ROUTES.md`, "5. Planner".

## 6. What the game does itself, and what OPFOR's bot layer does

`bfo:docs/research/navigation/CROSS_DOMAIN_ROUTES.md`, "6. What the game does itself, and what OPFOR's bot layer does".

## 7. How vehicle path data plugs in

`bfo:docs/research/navigation/CROSS_DOMAIN_ROUTES.md`, "7. How vehicle path data plugs in".

## 8. Implemented, and designed only

`bfo:docs/research/navigation/CROSS_DOMAIN_ROUTES.md`, "8. Implemented, and designed only".
