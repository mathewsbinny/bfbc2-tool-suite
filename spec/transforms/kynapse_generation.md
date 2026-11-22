# DICE's Kynapse generator: the behaviour its data shows

The rules DICE's path-data generator (Kynapse's `CMapBuilder` build chain inside BFBC2's tools) followed, written as
numbered rules from what its output shows. These rules are NavKit's target: OPFOR's goal is a full content match with
DICE's path data (the same ground, areas, connections and routes; not the same bytes). Each rule gives its status, the
evidence measured on DICE's 35 files (`formats/kynapse.md`'s corpus), and **NavKit today**: where NavKit
(`rust/bc2-navbuild`, `bfbc2-auto-navkit`) deviates, with the parity measure that shows it.

The generator itself is licence-locked and never runs (AI-KY-2). Its stage names, settings and engine callbacks were
seen in the executable, and its first two stages ran once on sp_009 / sp_01 before the licence check stopped it
(KYNAPSE_SPEC.md section 3). Everything after stage 2 is known only from the output. So most rules here are
**observed** (measured on the files) or **inferred**. A rule is **verified** when a corpus test checks it.

**NavKit's measures.** `navparity` compares a NavKit build with DICE's file of the same level. Its columns are named
here as `navparity:<column>`. The values quoted are the regression run of 2026-11-20
(`work\navkit\parity\regression_last.json`, 15 levels, means): `sectors_extra` 1.33, `ground` 98.22 %,
`extra_ground` 8.31 %, `areas_same` 5.18 %, `outline` 97.99 %, `portals` 6.47 %, `borders` 94.69 %, `nodes_same`
0.64 %, `edges_same` 0.02 %, `costs` 95.91 %, `nodes_near` 47.63 %, `connectivity` 98.79 %, `over_connection`
15.31 %, `ai_coverage` 99.16 %, `ai_pairwise` 92.67 %, D precision / recall 50.5 / 18.8 %, F 34.0 / 33.5 %. Round
NKS's probe adds structural counts on the same builds (`formats/kynapse.md` section 9; written `probe:` below).

## 1. Inputs and stages

### KYG-IN-1: The generator samples the running engine's collision

Status: **observed**

The generator takes no source assets. It queries the loaded level's Havok world through five callbacks: a point
query, a ray cast, a capsule (infantry, `PCMSweptSphere`) or cylinder / box query, and the level's exclusion
predicates (`ForbidPathfinding`, `ExcludeFromPathfinding`, water below `WaterLevelAdjustment`, `ForbidHeight`). It
reads per-type settings (`PathfindingSettings`: pitch, step, hole, slope, entity radius and height, `GraphAccuracy`,
`NbSectors`, `DetailedZoneSettings`) and seeds (perimeter auto-seeds and spawns, KYG-SEED-1). Its stages are "Dividing
sectors", "Generating seeds", "Generating maps", "Linking meshes", "Linking graphs", "Building hierarchical database".

[exe: build driver 0x00F299A0 (no caller), step classes created by 0x00F194F0, map-builder callbacks installed by 0x00F13570 (+0x868 point 0x00EFE7F0, +0x870 ray 0x00EFE840, +0x874 capsule 0x00EFE920 or box 0x00EFE980)]
[doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:294-332] [doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:140-166]

NavKit today: samples the same queries in game (`[dev] navsample`) or offline from the level's Havok and height field
(`formats/navsample.md`); `navparity:ground` 98.22 %.

### KYG-SEED-1: Seeds: sector perimeters on terrain, and soldier spawns

Status: **observed**

Stage 2 probes only each sector's perimeter lines, 1.6 m (two samples) inside its edges, every 0.8 m, sweeping a 0.4 m
sphere down in 0.4 m steps. Ground reached from those seeds is kept. The seeded surfaces are terrain: objects have
`AllowPathdataSeeds` false by default. Upper floors with soldier spawns are kept as islands of their own, so spawns
seed too. Vehicle files seed from vehicle spawns and terrain splines (`VehicleSeeds`, `BuildRoads`). Ground
unreachable from any seed is not in the data.

[data: the run on sp_01 (1,044,900 point queries on the perimeter lines); DICE's isolated upper-floor islands of 5-8 nodes each hold `SoldierSpawnEntityData` positions; vehicle sectors with ground holding a spline control point or vehicle spawn: sp_04 63 of 65, sp_01 30 of 33, sp_006 75 of 77]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:205-222] [doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:362-366]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:108-118]

NavKit today: seeds terrain perimeters, soldier spawns, and objects too. The object-top islands this makes are dropped
afterwards by the shadow-island rule (NK2, `graphfile::shadow_islands`). Measures: `navparity:islands_candidate`
against `islands_reference` (402 / 424 mean), `over_connection` 15.31 %.

## 2. The generator's rectangle and the sectors

### KYG-RECT-1: Ground stops at the spawns' box grown by 25 m

Status: **verified**

DICE's ground stops at straight lines 6-24 m inside the extent NavKit samples (the level's objects plus a margin).
Round NKS-rest2 found the lines in the level data: they are the box of the level's **soldier spawns (alternate spawns
included) and vehicle spawns, grown by 25 m** on every side. On the 13 campaign levels' infantry files, 45 of the 52
sides of the ground's box lie 0.11-0.91 m inside that box, a band one pitch (0.8 m) wide: the sample points kept are
those up to about 24.5 m beyond the outermost spawn (the box less the entity radius and 0.1 m), and the outline runs
half a pitch beyond the last one. The 7 other sides end 13-347 m before it, where the ground itself ends. No side
passes it. Vehicle files are cut by the same box: the 7 of 48 vehicle sides that reach it lie 2.33-3.09 m inside it
(radius 2.5 m, pitch 1.0 m), none passes it, and the others end on their own (vehicle ground is seeded from vehicle
spawns and roads, KYG-SEED-1). Along each side the last cell row covers only 9-64 % of the side, which is what a
straight cut through uneven ground leaves. The box is not the combat area, the height field, the placed objects or the
AI orders: each of these leaves the ground outside its box on some sides or ends far inside it (below). The ground's
box lies inside the sector grid's outer lines by -1.5 to 100.5 m per side, so the grid is cut from the box, not the
other way round.

Exceptions: the five Onslaught files (ONS_MP_002 / 004 / 005 / 005GR / 008) are not bounded by the spawns of the
partitions NavKit prepares for those levels: ONS_MP_002's infantry ground reaches 242 m past them in +x and ONS_MP_008's
39 m in -z. ONS_MP_005 has no prepared partition.

[data: per file, the outline box against the outer sector lines (probe): e.g. sp_004_b infantry 37.2 / 73.1 / 13.2 / 62.7 m (min x / max x / min z / max z), sp_005 13.2 / 96.4 / 45.2 / 5.2, ons_mp_004 4.4 / 4.4 / 1.2 / 28.1; a box reaches past the outer grid line on 9 files: 1.5 m on 8 vehicle files, 0.4 m on sp_01 infantry (the margins, KYM-HDR-2)]
[data: round NKS-rest (`kynapse_fit`): the vehicle file's ground box lies within 2.5 m of the infantry file's on 13 of 68 sides (17 levels) and 2.6-1,094 m inside it on the other 55, so the vehicle ground (seeded from vehicle spawns) shows no shared level-wide rectangle; on sp_04 all four sides lie 2.3-10.8 m apart]
[test: rust/bc2-scene/tests/kynapse_rect.rs::ground_stops_25_m_beyond_the_spawns]
[data: round NKS-rest2 (`bc2-scene` example `kynapse_geom`, `rect` fit, 17 prepared levels): the infantry ground's sides against the spawns' box + 25 m, inset per side: sp_007 0.11 / 0.49 / 0.39 / 0.83 m, sp_02 0.20 / 0.50 / 0.41 / 0.19, sp_01 0.48 / 0.33 / 0.53 / 0.76, sp_003_b 0.63 / 0.80 / 0.79 / 0.22; the 45 sides at the box 0.110-0.908 m; vehicle sides at the box sp_003_b 2.33, sp_005_b 2.54, sp_007 2.33, sp_008 2.60 / 2.37, sp_008_b 3.09, sp_04 3.07]
[data: round NKS-rest2, other boxes against the 68 infantry sides (17 levels): the ground lies more than 2 m outside the combat areas' box on 27 sides and outside the placed objects' box on 19; the height field's box ends more than 25 m beyond the ground on 66 sides and inside it on 2; with soldier spawns alone (+ 25 m) the ground passes the box (by more than 0.2 m) on 36 of the 132 sides of the 33 files, with vehicle spawns alone on 34, with both on 2 (Onslaught)]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:784-816]

NavKit today: on a level with DICE data it clips to DICE's extent (`navbuild.CLIP_TO_REFERENCE`), which is a bound
borrowed from the answer, not a rule. On a new map it keeps its sampling box. `navparity:sectors_extra` 1.33 mean
(sp_01 12), `extra_ground` 8.31 %.

### KYG-SEC-1: Sectors: a 100 m grid numbered from the file's offsets

Status: **verified**

The generator cuts the rectangle's ground into 100 m squares and numbers them `x << 8 | z`. The numbering starts at 1
on each axis for infantry data (HPD-USE-2's rule). Vehicle data is numbered on its own offsets, which differ from the
infantry offsets on 6 of 17 levels (KYN-SEC-3). Each sector's mesh rectangle is its square widened as KYM-HDR-2 says.
Its tiles are 40 / 20 / 50 m from the rectangle's minimum (KYN-TILE-1). Every sector written has at least one route
node: the empty sectors the generator's message mentions ("No seed found, write empty pathdata") are not in the
shipped files.

[test: rust/bc2-formats/tests/kynapse_corpus.rs::one_sector_offset_per_file] [test: rust/bc2-formats/tests/kynapse_corpus.rs::table_bounds_are_the_route_nodes_box]
[data: 3,120 of 3,120 sectors on one offset pair per file and holding a node]

NavKit today: the same grid, rectangle and tiles (probe: 0 vertices outside their tile, KYN-TILE-2). It keeps the
1.1-pitch margin at the grid's maximum where DICE drops it (`navbuild.EDGE_SECTOR_TILES` covers the minimum only).
Measure: `navparity:borders` 94.69 %.

### KYG-SEC-2: Sectors overlap by their margins, and C portals close them

Status: **observed**

A sector's rectangle reaches one finest pitch before its square and 1.1 pitch past it (KYM-HDR-2). So ground near a
sector line is in both sectors, and the generator traces each sector's ground separately. Where ground leaves the
rectangle, the area's boundary is a C portal on the first cell-corner line outside it, pointing out of the sector
(KYN-C-1). Where an outline meets the rectangle's side, a C portal points "up". Diagonal C portals sit where ground
leaves across a corner. Areas are joined across the line by link objects (KYG-LINK-1).

[data: axis C portals on the first corner line outside the rectangle: 97,720 of 101,418; C directions axis / diagonal / up 101,418 / 89,315 / 70,126]

NavKit today (probe, round NKG1): every C portal is an axis portal on that line, its run cut at the sector square's
lines so the pieces beyond link to the diagonal neighbour (KYG-LINK-1). It writes no "diagonal" or "up" directions:
those are not kinds of geometry (KYN-C-1: they change along one straight run, and the runtime reads the direction in
no test traced, KYRT-IGN-1), and what the vector holds is not decoded. C records whose middle lies in a diagonal
neighbour's square: coarse 7,787, detailed 3,927 (DICE's infantry files of the same 15 levels 22,344 / 5,051; before
NKG1 941 / 2,058). Measure: `navparity:borders` 94.69 %.

## 3. Areas

### KYG-AREA-1: Walkable cells link at a raw 0.6 m step

Status: **observed**

Within a tile, two straight-neighbour cells join one area when the raw height step between their centres is at most
0.6 m (pitch 0.8). A diagonal neighbour joins when a route through either side cell has both straight steps at most
0.6 m. Steeper cells (36.9 degrees at 0.8 m) start areas of their own. Reachability (seeding, KYG-SEED-1) uses the
softer StepMax rule relative to the local ground plane. Areas are single-layer: a surface whose column the area
already holds starts a new area (KYM-AREA-4).

[data: regrouping DICE's own cells by this rule reproduces 63-79 % of its areas exactly over 8 levels (sp_004_b 72.3 %), against 33-63 % for joining every neighbour; linked steps max 0.591 m, unlinked from 0.62 m]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:629-652]

NavKit today: implements it (`navbuild.AREA_LINK`). Measure: `navparity:areas_same` 5.18 % (exact areas need
KYG-AREA-2 as well), `outline` 97.99 %.

### KYG-AREA-2: Tiles cut areas; a conditional midline cut on the longer side

Status: **observed**

Areas never cross a tile line, so every tile line through ground is a B portal. Inside a tile, DICE sometimes cuts
ground again on the tile's midline (20 m into a 40 m tile; 10 m in a detailed tile; 25 m in a vehicle tile). The cut
is on one axis only. On the three levels first measured it ran across the longer side of the area's box, but over all
35 files only in 67 % of coarse cuts (round NKS-rest). No tested feature triggers it on all levels: size, height
span, cell count, record size, boundary length, the tile and holes were tried. Round NKS-rest2 followed the lead that
cut ground meets across the midline in two stretches. It holds as a tendency, not a rule: of the areas that cross a
midline, a cut is likelier the more often the ground crosses it (coarse: 0.6 % of single crossings are cut, 4 % of
double, 21 % of five or more), and 86 % of coarse cut groups cross two or more times against 30 % of uncut areas. What
lies between the stretches is the same for cut and uncut ground (99 % no ground at all, not a raised or sunken area).
"An area may not hold a hole across a midline" (a merge of half-tiles that refuses to close a loop) is excluded too:
coarse areas hold 3,221 holes that cross a midline. Over the corpus, B portals are:

| | on a tile line | on a midline | elsewhere |
|---|---:|---:|---:|
| coarse | 118,108 | 16,159 | 11,401 |
| detailed | 123,484 | 550 | 23,573 |
| vehicle | 22,748 | 1,450 | 69,095 |

"Elsewhere" is steps between areas inside a tile (KYG-AREA-1) and, in vehicle files, the many small areas whose
boundaries follow no grid.

[data: probe classification of 386,568 B portals; 325 of 326 cut groups cut along the longer side, 0 of 326 on both axes (sp_004_b, sp_005, sp_007)]
[data: round NKS-rest over all 35 files (`kynapse_fit`, groups of areas joined across a tile midline by B portals): cut groups coarse 1,064, detailed 106, vehicle 400; cut across the longer side of the joined box in 711 / 51 / 237 only; the joined ground meets across the midline in two or more separate stretches (it surrounds something that crosses the midline) in 928 / 22 / 6; areas that straddle a midline uncut 22,695 / 20,381 / 1,633]
[data: features that do not separate cut from uncut (round NKS-rest): the cells' share of the tile (uncut areas from 0 to 100 %, 954 coarse at a full tile; cuts from 1 %), the record size (cut 1-32 KB, uncut 0-14 KB, overlapping), the outline points, the tile (574 uncut straddlers in the 787 coarse tiles with a cut), and a hole crossing the midline (2,635 such holes in uncut coarse areas)]
[data: round NKS-rest2 (`kynapse_rules`, `midline` and `holes` fits): crossings of the midline (stretches where the ground lies on both sides, sampled every half pitch) for coarse cut groups 1 / 2 / 3 / 4 / 5+ = 148 / 241 / 228 / 164 / 283 and for uncut areas 24,108 / 5,409 / 2,656 / 1,231 / 1,050; the gaps between stretches: cut 2,743 without ground and 29 holding another area's ground, uncut 19,192 and 159; holes of single areas (sample points no area holds, enclosed): coarse 99,997 within a quarter tile, 1,492 across the x midline, 1,582 across z, 147 across both (98 % without ground); detailed 36,821 / 2,406 / 2,498 / 675; joined cut groups: 641 of 1,064 coarse hold a hole across a midline, 416 holes in quarters only, 7 none]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:517-531]

NavKit today (probe): coarse 75,436 / 1,748 / 64,370 and detailed 155,194 / 2,404 / 237,972. It has no midline cut,
and five to ten times DICE's off-grid portals: staircase boundaries between areas, which DICE mostly does not link.
Measures: `navparity:portals` 6.47 %, `areas_same` 5.18 %. The largest single structural gap.

### KYG-AREA-3: Area heights

Status: **observed**

An area's height pair (header and tile item, KYN-AREA-1) is its lowest ground and its highest ground plus 0.8 x
EntityHeight (infantry 1.52, vehicle 3.2 m). Outline vertices lie at the ground plus EntityHeight / 2 (0.95 / 2.0 m).
The runtime selects areas by the item's pair (KYRT-AREA-1), so a range that reaches a floor above captures that floor's
points.

[data: sp_004_b normal-pitch areas median +0.000 / +1.520 against the outline heights minus 0.95; vehicle files lowest ground .. highest + 3.2 m]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:690-696]

NavKit today: implements it (`navbuild.AREA_Y_MIN` / `AREA_Y_MAX`); the item pair equals the header's on every
NavKit area (probe: 214,318 of 214,318).

## 4. Detailed sectors

### KYG-DET-1: Which sectors are detailed

Status: **observed**

DICE built some infantry sectors at pitch 0.2 with 20 m tiles, the whole sector or none of it (KYN-SEC-4). Detailed
sectors per infantry file: sp_009 4 of 9, sp_004_b 19 of 40, sp_008 19 of 41, sp_008_b 23 of 27, sp_01 41 of 126,
sp_02 38 of 120, sp_03 50 of 304, sp_003_b 28 of 306, sp_04 55 of 211, sp_005 42 of 269, sp_005_b 41 of 270, sp_006 51
of 380, sp_007 47 of 288, ONS_MP_002 23 of 56, ONS_MP_004 21 of 27, ONS_MP_005 / 005GR 27 of 65, ONS_MP_008 15 of 22.
The blueprint flag `NeedsDetailedPathfinding` on an object within 10 m picks 304 sectors over the 13 campaign levels.
300 of those are detailed, but that finds only 296 of DICE's 498, because the flag's value is read for only 7-49 % of
the placements (most blueprints are in shared packages). Sampled features (object ground, stacked floors, stairs,
spawns) correlate 90-97 % but no single threshold holds on every level.

Round NKS-rest2 measured every infantry sector of the 17 prepared levels against the level data (spawns, AI orders,
objectives, capture points, ladders, placed objects with collision, collision triangles, and DICE's own ground on
objects and in stacked floors). No feature or pair of features gives the detailed sectors: the best single one, 28 or
more placed objects with collision in the sector's square, classifies 91.8 % of the 2,561 sectors (the detailed ones
hold 150 such objects on average, the coarse 4.5); the best pair (an OR or an AND of two thresholds, the 8 neighbours
included) 92.2 %. Detailed sectors are where the buildings are, but the choice is the designer's: with the blocks not
rectangles either, the generator's input (`DetailedZoneSettings` / `MapBuilderLocalSettingsZone` boxes, not shipped)
is the likely source, so DICE's own list is the record for DICE's levels.

[data: detailed sectors per file from the probe (571 of 2,626 infantry sectors); NeedsDetailedPathfinding within 10 m: 300 of 304 picks detailed, 296 of 498 found]
[data: round NKS-rest: the 571 detailed sectors form 42 blocks of side-adjacent sectors (1-6 per file); only 14 blocks (30 sectors) are rectangles of the file's sectors, the others hold coarse sectors inside their box; so the detailed ground is not one designer box per block]
[data: round NKS-rest2 (`bc2-scene` example `kynapse_geom`, `detailed` fit; 544 detailed of 2,561 sectors, ONS_MP_005 not prepared): best single thresholds, share right: placed objects with collision >= 28 91.8 %, collision triangles >= 16,300 90.1 %, DICE ground on objects >= 1 sample 87.6 %, AI spawns >= 1 87.1 %, AI orders >= 1 86.5 %, stacked ground 84.8 %, vehicle spawns 84.5 %, objectives 81.5 %, ladders 81.3 %, capture points 79.7 %; best two-feature rule 92.2 % (objects in the 3 x 3 block >= 66 and in the sector >= 12); per level the best "ground on objects" threshold ranges from 1 to 2,757 m2]
[inferred: no level-data rule reaches 93 % and the blocks are not rectangles, so the detailed sectors come from generator input that is not shipped (designer zones); a sector is detailed as a whole (KYN-SEC-4)]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:532-542] [doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:596-600]

NavKit today (probe): 710 detailed sectors where DICE's infantry files of the same 15 levels have 502, and 129,219
detailed areas (DICE: 51,461 over all 18 files). Measure: `navparity:detailed_candidate` / `detailed_reference`
8,605 / 3,099 areas mean (2.8x).

## 5. Route nodes

### KYG-NODE-1: Node placement is off the sampling grid

Status: **observed**

DICE's route nodes are not at cell centres or cell corners: 96 % lie on neither grid (both coordinates within 5 mm). In
the samples measured, about 75 % of infantry nodes (60 % vehicle) lie within about 1 m of an outline corner, offset
inwards by roughly the entity radius, and the rest are in open ground. Nodes are at the ground plus EntityHeight / 2
(vehicle median 1.98-1.99 m). Near AI order positions DICE has a node within 0.2 m. Areas without a node are normal
(ONS_MP_002: 2,098 of 3,279, most under 4 m2).

Round NKS-rest2 compared the nodes with the levels' collision and found that the placement does not come from it
either. Coarse nodes stand on open terrain: 95.5 % have no collision triangle within 3 m in the soldier's height band
(the ground + 0.3 to + 1.9 m), as many as points beside them (control points 1.7 m / -1.1 m away). Detailed nodes keep
a slight preference for 0.4-0.6 m from collision (6.4 % against 3.5 % of control points), but "an obstacle corner or
edge offset by the radius" (0.35-0.45 m from collision) holds for 2.2 % of detailed nodes against 1.9 % of control
points, and 0.08 % of coarse nodes. What the nodes do show is a coverage property (KYG-NODE-5: the ground sees a node
within DistEdgeMax / 2) and a construction signature (KYG-NODE-4: pairs exactly one step, GraphAccuracy + 0.35 m,
apart along an axis). They are not a converged centroidal layout either: a node's share of the ground (the points
whose nearest seen node it is) has its centroid a median 0.65 m (coarse), 0.55 m (detailed), 1.5 m (vehicle) away.

[data: nodes on the cell-centre grid 22,375 and on the corner grid 784 of 596,069; ONS_MP_002 node distance from an outline point p50 / p90 detailed 1.7 / 4.3 m, coarse 2.9 / 6.8 m]
[data: round NKS-rest over all 35 files (`kynapse_fit`), placements that do not hold: a lattice (the fraction of x on periods 0.05, 0.1, 0.2, 0.25, 0.4, 0.5 and 1 m is flat within 15 % but for the cell-centre spike); outline corners offset by the radius along both sides (607 of 197,599 coarse ordinary nodes; 66,494 lie within 2 m of an outline corner); the medial axis (the nodes' nearest-two-obstacles balance has the distribution of control points 0.6 r / 0.9 r away); one node at each area's cell centroid (coarse: 90,367 of 118,937 areas hold no node, 17,936 five or more)]
[data: vehicle files: 7,324 of 18,590 areas hold one node and 9,939 none; a lone node lies within 0.7 m of its area's cell centroid in 6,557 of 7,324, spread evenly over 0-0.7 m (anywhere in the central 1 m cell)]
[data: round NKS-rest2 (`bc2-scene` example `kynapse_geom`, `clearance` fit, 17 prepared levels): ordinary nodes against the collision clipped to the height band, nearest distance: coarse 186,858 of 195,699 with none within 3 m (control 186,840), 0.35-0.45 m 162 (control 195); detailed 61,312 of 143,040 none within 3 m (control 61,553), 0.35-0.45 m 3,134 (control 2,714), 0.40-0.60 m 9,145 (control 4,980); vehicle 3,778 of 9,383 none (control 3,814)]
[data: round NKS-rest2 (`kynapse_rules`, `cover` fit): the centroid of a node's share of the ground against the node, for ordinary nodes 10 m or more inside their square: median 0.65 m coarse (144,583 nodes), 0.55 m detailed (100,368), 1.5 m vehicle]
[inferred: positions on no grid, no lattice, no outline feature and no collision feature point to the generator placing nodes from its own sample cloud by a procedure that leaves the KYG-NODE-4 / KYG-NODE-5 traces (axis steps of GraphAccuracy + 0.35 m; coverage within DistEdgeMax / 2), not from the stored outline or the Havok shapes]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:165] [doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:891-905]

NavKit today (probe): every node on a cell centre (672,793 of 672,793). Measures: `navparity:nodes_same` 0.64 %,
`nodes_near` 47.63 %. The placement rule (which outline corners, what offset, how open ground is filled) is the main
open rule for the graph. KYG-NODE-2 is the border part.

### KYG-NODE-2: Border nodes form a band shared by two to four sectors

Status: **observed**

Border nodes lie in a band beside each 100 m line (median 2.2 m from it, p90 9.2 m, at most 15.7 m), 1.6 m apart
along it (p90 5.8 m). Each is one id in every sector that holds it. A node near a corner is shared by three or four
sectors. One copy, normally the one whose square holds the node, owns the edges (HPD-NODE-5).

Every border id has edges in at least two of its sectors: a border node joins sectors.

[test: rust/bc2-formats/tests/kynapse_corpus.rs::border_ids_have_edges_in_two_sectors]
[data: 113,874 border ids: 104,448 in 2 sectors, 5,570 in 3, 3,856 in 4; owners: 107,437 one (103,458 the holding sector), 3,623 none, 2,738 two, 76 three; 0 border ids with edges in fewer than two sectors in 35 of 35 files]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:897-901]

NavKit today (probe): border nodes in a row 0.8 m from the line, inside the higher sector, about 10 m apart
(`BORDER_NODE_SPACING`). 48,611 ids are in 2 sectors and only 288 in 3 or 4; 2,704 of 48,899 have edges in one sector
only. All are owned by the higher sector, so 111,968 edges sit on non-owner copies (DICE 8,039). The runtime walks
every copy's edges (KYRT-PATH-3), and these are NavKit's only edges from a border node into the lower sector: with
them left out (round NKG1's `navbuild.OWNER_EDGES`, off) the strongly connected share of the graph as the runtime walks
it falls from 100 to 6.5 % on sp_004_b (10.6 % sp_008_b, 22.3 % sp_009). So the edges move onto owner copies only with
DICE's band, owned nodes on both sides of each line (NKG2). A second row 0.8 m into the lower sector, tried in NKG1,
gave 92.7 % with owner-only edges but changed the sector graphs (new 2-3 node islands where its copy in the higher
sector found no pair; a coarse / detailed seam on sp_007 split as the detailed graph re-pruned around the carried
row): `connectivity` fell up to 0.5 points on 5 levels, so it waits for NKG2's band. Measures:
`navparity:connectivity` 98.79 %, `over_connection` 15.31 %, `runtime_connectivity` 98.41 % and `strong_share`
99.69 % (DICE 97.30 %).

### KYG-NODE-3: Node ids and the lookup lists

Status: **observed**

The generator numbers nodes from one counter over the whole file, border and ordinary alike (KYN-NODE-1). The
numbering has gaps (599,172 numbers for 355,039 ordinary ids), so nodes were numbered before some were dropped. It then
writes each sector's nodes sorted by unsigned id (HPD-NODE-2). The three lookup lists (HPD-NODE-6) are sorted by the
f32 coordinate in every sector. Among equal coordinates no simple rule reproduces DICE's order:

| Tie order tried | x ties matched | y | z |
|---|---:|---:|---:|
| ties in index order | 7,110 of 15,276 | 1,079 of 4,092 | 4,862 of 13,313 |
| reverse index order | 5,152 | 1,102 | 4,864 |
| by the other two coordinates | 5,752 | 1,244 | 4,860 |
| whole list equal to a stable sort by index (sectors) | 1,225 of 3,120 | 2,328 | 1,089 |
| whole list equal to MSVC `qsort` on 0..n (sectors) | 1,040 of 3,120 | 2,328 | 1,046 |

[data: lists sorted by their coordinate in 3,120 of 3,120 sectors for x, y and z]
[inferred: equal f32 coordinates whose order follows no rule on the stored values points to a sort on the generator's unrounded coordinates before they were stored as f32]

The runtime reads the lists in their order (KYRT-NODE-2): the node resolver tests candidates in the order of the
list whose axis has the fewest of them, and takes the first that passes. So the tie order can change which node a
point resolves to, when two candidates share a coordinate on that axis and both pass.

NavKit today (probe): a stable sort by index (2,480 / 2,487 / 2,470 of 2,494 sectors). The runtime reads the lists in
order (KYRT-NODE-2), so the ties can decide which node a point gets, but only between nodes with the same coordinate;
with NavKit's own node positions (on cell centres, KYG-NODE-1) the ties are among NavKit's nodes, not DICE's, so round
NKG1 left them: they matter once placement follows DICE's (NKG2), and DICE's tie rule is not decoded.

### KYG-NODE-4: Step pairs: nodes one step of GraphAccuracy + 0.35 m apart along an axis

Status: **verified**

Some ordinary nodes come in pairs that share one coordinate exactly (the same f32) and lie exactly one step apart on
the other axis, the step being `GraphAccuracy` + 0.35 m: 3.35 m in coarse sectors, 0.85 m in detailed, 15.35 m in
vehicle files. The gap is exact to the millimetre and stands 80, 56 and 22 times above any other gap within 20 % of it.
Multiples of half the step appear too (5.025 and 6.7 m coarse, 23.025 and 46.05 m vehicle). The pairs are a minority
(7.6 % of coarse ordinary nodes lie whole half-steps from another on both axes), mostly beside small obstacles, and
their nodes carry few edges (KYG-EDGE-3:
the spanner's unexplained pairs gather on them). So part of the placement moves or splits nodes by half-steps along x
and z. Where the pair's centre comes from is not decoded.

[test: rust/bc2-formats/tests/kynapse_corpus.rs::nodes_pair_one_step_apart_along_an_axis]
[data: round NKS-rest2 (`kynapse_rules`, `spacing` fit): pairs sharing x or z exactly, by gap: coarse 6,578 at 3.350 m (+- 2 mm; the next gap within 20 % 82), detailed 226 at 0.850 m (next 4), vehicle 44 at 15.350 m (next 2); coarse 753 at 6.700 m and 759 at 5.025 m; ordinary nodes joined to another by x and z offsets that are both whole half-steps: coarse 14,956 of 197,599 (12,166 in pairs, 2,313 in threes), detailed 4,176 of 147,130, vehicle 98 of 10,310]
[data: round NKS-rest2, the axis gaps of the cell grid (multiples of half the pitch: 0.8, 1.2, 1.6, 2.4, 3.2, 4.0 m coarse) come from the 4 % of nodes on cell centres (KYG-NODE-1)]

### KYG-NODE-5: The ground sees a node within DistEdgeMax / 2

Status: **observed**

Almost every point of the ground has a node of its sector in sight (no outline between) within half of DistEdgeMax:
7.5 m on infantry files, 25 m on vehicle files. Plotted sectors show what this gives: in open ground a roughly
triangular layout with nodes 7-8 m apart, in clutter nodes in the pockets and at the junctions of passages. The share
falls steeply just past the half reach, so the coverage is a target of the placement, not a by-product. The rest (2-5 %)
is ground that sees no node within 30 m (1.8 % of coarse points), on small areas without a node (KYG-NODE-1) or behind
an outline at another height that the 2-D sight test counts.

[data: round NKS-rest2 (`kynapse_rules`, `cover` fit): every sample point of DICE's areas (0.8 m apart; 1 m on vehicle files), the nearest node of the sector at the area's height it sees: coarse 96.53 % within 7.5 m (88.6 % within 6 m, 96.97 % within 8 m) of 19,096,684 points, 1.83 % none within 30 m; detailed 98.31 % within 7.5 m (92.0 % within 6 m, 98.79 % within 8 m) of 5,735,964; vehicle 95.2 % within 25 m (81.0 % within 15 m) of 1,415,580, 2.7 % none within 30 m]
[data: the same within the sectors' own squares (margins left out): coarse 96.94 % within 8 m, detailed 98.80 %, vehicle 95.5 % within 25 m: the tail is not the margins]

## 6. Route edges

### KYG-EDGE-1: Edges join nodes within DistEdgeMax across

Status: **verified**

An edge joins two nodes that can reach each other in a straight walk on the mesh. Between ordinary nodes it is at most
DistEdgeMax across (15 m infantry, 50 m vehicle). With a border node at an end it is at most twice that, the two
sectors' reaches added (KYN-EDGE-1). Within a sector 13 % of edges have no edge back (KYN-EDGE-3), but by node id
the graph is symmetric to 99 % (KYG-EDGE-4). Which pairs are joined is KYG-EDGE-3. DICE's edges
do not cross D lines on infantry files (1 of 24,514 on sp_004_b, 11 of 82,602 on sp_01). Vehicle edges cross them
(0.58 %). Lengths, infantry coarse: 343,271 under 5 m, 321,736 at 5-10, 350,668 at 10-15, 36,719 over 15 (3-D).

[test: rust/bc2-formats/tests/kynapse_corpus.rs::edge_reach_is_dist_edge_max_across]
[data: 219,500 of 1,694,066 edges one-way; border-to-border edges 425,545]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:982-985] [doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:1009-1016]

NavKit today (probe, round NKG1): within the reach (0 over); one edge per (source, target) since NKG1 (KYN-EDGE-4;
6,916 duplicates before, 1,894,046 edges now on the 15 regression builds); symmetric in every sector (0 one-way). By
node id DICE's graph is symmetric too (KYG-EDGE-4); NavKit keeps a border node's edges on its non-owner copies until
the band gives every line owners on both sides (KYG-NODE-2). Border-to-border edges 90,754 against DICE's 425,545 on
more levels. Which pairs are joined is KYG-EDGE-3's spanner (NKG2). Measures: `navparity:edges_same` 0.02 %,
`ai_pairwise` 92.67 %.

### KYG-EDGE-2: The stored cost

Status: **verified**

The cost word is `0x80000000 | floor(L x 16)` with L the f32 3-D distance of the stored f32 positions (KYN-EDGE-2).

[test: rust/bc2-formats/tests/kynapse_corpus.rs::edge_costs_are_the_f32_length_in_sixteenths]
[data: 1,694,066 of 1,694,066 edges with L computed in f32]

NavKit today (probe): 1,900,944 exact and 18 off by one unit (an f64 length rounded); `navparity:costs` 95.91 %
(over the edges whose ends match DICE's). The runtime computes step costs from positions anyway (KYRT-PATH-1).

### KYG-EDGE-3: Edges are added unless the graph already has a route at most 1.2 times as long

Status: **verified**

Between ordinary nodes of a sector, the generator leaves out an edge when the graph already joins the two nodes by a
route at most **1.2 times** their straight 3-D distance; otherwise two mutually reachable nodes within DistEdgeMax
get an edge. This is a greedy 1.2-spanner. In DICE's final graphs:

- an edge almost never has another route of at most 1.2 times its length (infantry 0.19 %, vehicle 3.6 %; a greedy
  spanner keeps a few, because routes found later can be shorter than the one checked when the edge was added);
- of the node pairs that see each other (no outline or D line between them) and have no edge, 63 % (coarse), 68 %
  (detailed) and 81 % (vehicle) have such a route. The cut-off is sharp: coarse edges with a route within 1.20 times
  their length number 186, within 1.22 times 1,206.

The rest of the pairs without an edge are not explained yet: coarse 52,527 (37 %), whose line stays on the mesh and
keeps a clearance from the outline that does not differ from the edges'; vehicle 3,844, of which 3,283 leave the
mesh. A `GraphAccuracy` (3 / 0.5 / 15) does not give the cut-off: the excess of the route over the straight line,
measured in metres or in units of `GraphAccuracy`, has no sharp edge. Round NKS-rest2 tried the two leads:

- **The reach test on the sample cells** (every pitch cell the line passes holds ground at its height, so the
  outline's simplified corners cannot hide a blocked cell): 95.7 % of the coarse misses pass it, as 96.8 % of the
  edges do, so it does not explain them. On vehicle files it does: 85 % of the misses leave the mesh's cells (10 % of
  the edges).
- **The order of the pairs**: no order explains a miss on its own. A greedy spanner only adds edges, so a route of at
  most 1.2 times that existed when a pair was skipped is still there at the end. A miss therefore needs edges or
  nodes that were taken away after the pass, and the data point there: the file's node numbering has gaps (KYG-NODE-3:
  599,172 numbers for 355,039 ordinary ids), and the misses gather on the nodes a later step touched. 32 % of coarse
  misses have an end on a step pair (KYG-NODE-4) against 9 % of edges, and in 69 % of misses neither end has more than
  4 edges (39 % of edges).

So the infantry rule is the spanner over the nodes of the main pass, with a later step (the step pairs, and the
removal of numbered nodes) that the spanner does not revisit. That step is not decoded.

[test: rust/bc2-formats/tests/kynapse_corpus.rs::edges_are_not_redundant_at_1_2]
[data: edges between ordinary nodes with another route in their sector at most 1.2 times as long: infantry 946 of 487,308 node pairs, vehicle 427 of 11,814]
[data: round NKS-rest probe (`kynapse_fit`), ordinary pairs within DistEdgeMax across and 3 m in height: coarse 253,010 with an edge (637 cross the outline or a D line) and 651,044 without (142,493 of them in sight); with a route at most 1.2 times as long: coarse edges 186 of 252,373, pairs without 89,966 of 142,493; detailed 742 of 189,995 and 134,546 of 198,161; vehicle 279 of 8,087 and 16,874 of 20,718]
[data: vehicle edges cross the stored outline in 3,676 of 11,891 ordinary pairs, so the vehicle generator's reach test is not the stored outline]
[data: round NKS-rest2 (`kynapse_rules`, `pairs` fit; pairs in sight of the outline and D lines): coarse edges 252,373 and misses (no edge, no route of at most 1.2 x) 52,338; the cell test passed by 244,196 edges and 50,067 misses; an end on a step pair: edges 22,808, misses 16,807; both ends with at most 4 edges: edges 99,269, misses 35,933; detailed misses 50,691 (step-pair end 1,168, cell test passed 46,565); vehicle misses 3,842 (cells off the mesh on 4 or more: 3,271; edges 822 of 8,086)]
[inferred: a greedy spanner's skipped pairs keep their short route, so misses need a later step that removed nodes or edges or added nodes outside the pass; the numbering gaps and the misses' concentration on step-pair and low-degree nodes point to it]

### KYG-EDGE-4: The graph is symmetric by node id; a sector's one-way edges end at non-owner copies

Status: **verified**

Taken over the whole file, with border copies joined by their masked id, 99.1 % of infantry edges and 95.1 % of
vehicle edges have an edge back. KYN-EDGE-3's 13 % one-way edges are mostly an effect of where border edges are
stored: an edge into a border node ends at that node's copy in the source's sector, which is a non-owner copy and
holds no edges (HPD-NODE-5), so the edge back lives in the owner's sector. 96.9 % of infantry one-way edges in their
sector end at a non-owner copy (vehicle 70.4 %). The few edges one-way by id climb and descend in equal numbers, so
they are not a drop rule.

[test: rust/bc2-formats/tests/kynapse_corpus.rs::edges_have_a_reverse_by_node_id]
[data: one-way by id: infantry 14,692 of 1,646,482 edges (border-border 5,533, ordinary-border 7,113, ordinary-ordinary 2,046; 5,921 down, 2,867 level, 5,904 up by more than 0.3 m), vehicle 2,343 of 47,584; one-way in their sector: infantry 212,664 (206,058 into a non-owner copy), vehicle 6,836 (4,815)]

## 7. D lines and F cells

### KYG-DF-1: Infantry D / F from broken sides

Status: **observed**

In coarse areas, a side between two cells of one area whose step exceeds the link step (0.6 m) is broken. A cell with
both sides of one axis broken is an F cell. F polygons are stored twice, -axis then +axis, ordered -x, +x, -z, +z
(KYN-DF-2). Other broken sides are D lines, one per owning cell, owner on the right, collinear runs merged, corners cut
where a cell owns two perpendicular D sides (the 3-point "L" lines). In detailed areas only terrain-terrain sides break,
from about 0.17 m, and F is rare (KYM-DF-1).

[data: coarse D lines 431,972 (98.1 % 2 points, 7,880 with 3, 268 with 4 or more), F records 40,908; detailed 81,158 and 3,472]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:939-1008]

NavKit today: the coarse rule with corner cuts (`navdetail`; NK3 ports it to Rust); the detailed rule is implemented but
off. Probe: 258,379 D lines (396 with 3+ points against DICE's 8,148 on all files) and 40,588 F records on the 15
levels; F order and twins hold. Measures: `navparity:df_d_precision` / `df_d_recall` 50.5 / 18.8 %,
`df_f_precision` / `df_f_recall` 34.0 / 33.5 %.

### KYG-DF-2: Vehicle D / F follow another rule

Status: **observed**

Vehicle D lines are not twinned facing pairs (163 pairs of 7,949 lines measured), 19 % have 3-8 points and 26 % of
their segments are diagonal. F records include the four diagonals (897 of 2,162) and are not stored per direction
(311 of 881 areas balanced). Vehicle edges cross D lines (0.58 %). The rule is open, and cannot be fitted from what is
on disk: it needs the ground as the vehicle query sees it. Round NKS-rest2 surveyed OPFOR's sample files: every one was
run with the soldier's capsule (radius 0.4 m, or 0.2-0.3 m in experiments), none with the vehicle's shape. What the
fit needs is one in-game `[dev] navsample` run per level with vehicle data, at the vehicle settings (pitch 1.0 m,
radius 2.5 m, height 4 m, the vehicle StepMax), over the vehicle file's extent; sp_04 (all four sides of its vehicle
ground near the infantry's, KYG-RECT-1) and sp_02 are the first candidates. The fit then compares the sampled steps
with DICE's D lines and F records cell by cell, as KYG-DF-1 was fitted.

[data: vehicle D 8,597 lines (1,612 with 3+ points; 2,746 of 10,515 segments diagonal), F 2,162 records (897 diagonal); 881 areas with F, 311 balanced]
[data: round NKS-rest2, OPFOR's sample files (`work\navsample`, `work\navsample_offline`, `work\navkit`): pitch 0.8 or 0.2 with capsule radius 0.4 in all but 4 (radius 0.2, 0.25, 0.3); no vehicle-shaped run]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:1009-1016]

NavKit today: no D / F in vehicle files (`navvehicle.DETAIL` off). The runtime uses D / F only in straight-move tests
(KYRT-MOVE-1), so vehicles may cut corners where DICE's would not.

## 8. Borders and links across sectors

### KYG-LINK-1: Link objects join each C portal run to the neighbour's area

Status: **verified**

For each neighbour sector (all eight, diagonals included), the generator writes one link entry. Each entry holds one
link object per neighbour area reached, naming that area by the neighbour's tile and item index, and listing this
sector's C records that lead into it (KYN-LINK-1). Links join coarse and detailed sectors as well (9,396 of 55,008
objects).

[test: rust/bc2-formats/tests/kynapse_corpus.rs::link_objects_name_a_tile_item_of_an_adjacent_sector]
[data: 16,450 link entries (6,263 to diagonal neighbours), 55,008 objects all resolved]

A C record lying in a diagonal neighbour's square links to that neighbour: of the C records a diagonal-neighbour link
object lists, 91 % have an end in that neighbour's square (coarse 13,479 of 14,827).

NavKit today (probe, round NKG1): each C run is cut where it crosses the sector square's line, and a piece beyond it
links to the diagonal neighbour whose square holds it (to the axis neighbour when the diagonal one has no area there).
On the 15 regression builds: 14,904 link entries, 5,922 to diagonal neighbours (before NKG1 8,994 and 0; DICE's
infantry files of the same levels 14,562 and 5,682); 51,995 objects, all resolved, 6,614 cross-pitch (DICE 49,600 and
8,875). Measure: `navparity:borders` 94.69 %.

## The NavKit gap list, in priority order

Ordered by what the runtime does with each difference (path existence first, then which node a point gets, then
movement detail, then bytes):

1. **Detailed sectors** (KYG-DET-1): 710 against 502, 2.8x DICE's detailed areas. A wrong pitch changes ground, areas
   and nodes at once; DICE's bots stand in its detailed zones. **NKG2's target**: DICE's own detailed sectors on DICE
   levels (502). For new maps DICE's choice is not recoverable from level data (designer zones, KYG-DET-1); the best
   stand-in is `NeedsDetailedPathfinding` within 10 m (precise: 300 of 304 picks detailed) together with 28 or more
   placed objects with collision in the sector (91.8 % of DICE's sectors classified right), offered as the default
   and overridable per sector.
2. **Node placement** (KYG-NODE-1, KYG-NODE-2): every NavKit node on a cell centre against DICE's 4 %; a sparse border
   row against DICE's band; corner sharing 288 against 9,426. The go-to's resolver takes the first node in lookup-list
   order within 15 m that passes a straight-line test (KYRT-NODE-1, KYRT-NODE-2), so placement decides routes
   (`nodes_near` 47.6 %). **NKG2's target**: DICE's border band, as wide as `SectorOverlap` = DistEdgeMax (KYRT-CFG-1),
   and interior nodes placed to DICE's coverage (KYG-NODE-5): every sample point of the ground sees a node within
   DistEdgeMax / 2 (DICE: 96.5 % coarse, 98.3 % detailed within 7.5 m; 95.2 % vehicle within 25 m), with no node on the
   cell-centre grid by construction. The exact positions are still open (KYG-NODE-1 lists what they are not, now the
   collision too; KYG-NODE-4's step pairs are their only decoded trace), so NKG2 measures the coverage, `nodes_near`
   and `ai_pairwise`, not `nodes_same`.
3. **Edges** (KYG-EDGE-1, KYG-EDGE-3, KYG-EDGE-4): the duplicates are gone (NKG1). 111,968 edges sit on non-owner
   border copies (DICE 8,039); the runtime uses them (KYRT-PATH-3), and with NavKit's single border row they are its
   only edges into the lower sector, so they move onto owner copies with the band (NKG2, KYG-NODE-2: round NKG1
   measured 100 -> 6.5 % strongly connected without them). DICE's one-way edges are its border convention: the graph
   is symmetric by id (KYG-EDGE-4), as NavKit's is. **NKG2's target**: KYG-EDGE-3's pair rule, a greedy 1.2-spanner over
   mutually reachable pairs within DistEdgeMax (which reproduces DICE's missing pairs at 63-81 % and keeps all but
   0.2 % of its edges); `edges_same` and `over_connection` measure it.
4. **Area division** (KYG-AREA-2): no midline cut and 5-10x the off-grid portals (`portals` 6.5 %, `areas_same`
   5.2 %). The cut's trigger is still open (size, tile, hole, crossing count and what lies between the crossings
   excluded); NKG2 can take the off-grid portals (KYG-AREA-1's step boundaries DICE does not link) first.
5. **The rectangle** (KYG-RECT-1): a borrowed bound on DICE levels, none on new maps. **NKG2's target**: the rule,
   verified on the 13 campaign levels: the box of the soldier and vehicle spawns grown by 25 m, sample points kept up
   to about 24.5 m beyond the outermost spawn (infantry; about 22.2 m on vehicle files), the same on new maps. It
   replaces `navbuild.CLIP_TO_REFERENCE` on campaign levels; the Onslaught levels keep DICE's extent (their files are
   not bounded by the prepared partitions' spawns).
6. **D / F** (KYG-DF-1, KYG-DF-2): coarse recall 19 % / 33 %, nothing for vehicles or detailed areas. **NKG3's
   target** stays the coarse rule's recall. The vehicle rule cannot be fitted from the files on disk (no vehicle-shaped
   sample run exists, KYG-DF-2): NKG3 needs one in-game `[dev] navsample` run at the vehicle settings (pitch 1.0 m, a
   cylinder of radius 2.5 m, height 4 m) on sp_04 or sp_02 first, so it is a playtest item while in-game runs are
   paused.
7. **C portal directions** (KYG-SEC-2, KYN-C-1): DICE's "diagonal" and "up" values are not geometric and are read in
   no test traced; not imitated. Diagonal-neighbour links: done (NKG1, KYG-LINK-1). DICE's C records are also cut
   differently (17 % cross a square line; its 45-degree corner cuts), which follows its tracing (NKG2).
8. **Table bounds** (KYN-SEC-2): done (NKG1): the node box in 2,489 of 2,494 sectors; the 5 others (no node, or a box
   an earlier sector has) keep the build's bounds.
9. **Lookup-list ties** (KYG-NODE-3): a stable sort; DICE's order is unknown. The runtime's resolver reads the lists
   in order (KYRT-NODE-2), so ties can change which node a point gets, but only between nodes with the same
   coordinate; with NavKit's own positions this matters only once placement matches DICE's.

## Open questions

Round NKS-rest fitted the rules below over all 35 files (`rust/bc2-formats/examples/kynapse_fit.rs`); round NKS-rest2
went on with `rust/bc2-formats/examples/kynapse_rules.rs` (DICE's files alone) and `rust/bc2-scene/examples/kynapse_geom.rs`
(DICE's files against the levels' collision, height field and entities). Answered: which pairs get edges (KYG-EDGE-3,
a 1.2-spanner), which are one-way (KYG-EDGE-4, the border convention), and the rectangle (KYG-RECT-1, the spawns' box
grown by 25 m, verified). Still open, with what the fits ruled out:

- The midline cut's trigger (KYG-AREA-2): not the cell count, record size, outline length, tile, a hole crossing the
  midline (single areas hold 3,221 such holes), the number of crossings (a tendency only) or what lies between them.
  Next: the cut groups against the raw samples (OPFOR's `work\navsample` at pitch 0.8 / 0.2), e.g. the order in which
  a tile's cells are grown.
- Node placement (KYG-NODE-1): not a lattice, outline corners offset by the radius, the medial axis, area centroids,
  the Havok collision's corners or edges offset by the radius, or a converged centroidal layout. Known: the coverage
  (KYG-NODE-5) and the half-step moves along the axes (KYG-NODE-4). Next: where a step pair's centre comes from (its
  two nodes' common start), against the raw samples.
- The remaining 37 % of coarse pairs without an edge (KYG-EDGE-3): not the reach test on the sample cells, and no pair
  order alone; they need a step after the spanner (inferred from the numbering gaps and the misses on step-pair and
  low-degree nodes). Next: the same step as KYG-NODE-4's.
- The vehicle D / F rule (KYG-DF-2): needs a vehicle-shaped sample run in game (none on disk); a playtest item.
- Detailed sectors beyond `NeedsDetailedPathfinding` (KYG-DET-1): no level-data rule (best 92.2 %); designer zones that
  are not shipped. Closed as unrecoverable for DICE's levels (their own list is the record); new maps use the stand-in
  in the gap list.
- The rectangle on the Onslaught files (KYG-RECT-1's exception): which spawns DICE built them from.

The `formats/kynapse.md` section 6 table tracks the same questions.
