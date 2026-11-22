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

### KYG-RECT-1: Ground stops at a rectangle that is not in the shipped data

Status: **observed**

DICE's ground stops at straight lines 6-24 m inside the extent NavKit samples (the level's objects plus a margin).
These lines are not combat areas, water or forbidden volumes, and no shipped entity holds them. Along each side of
the ground's box, the last cell row covers only 9-64 % of the side, which is what a straight cut through uneven ground
leaves. The generator's input (`SectorsDefinitionFile`, `MapBuilderLocalSettingsZone`'s boxes) is not shipped. So for
a level with DICE data, DICE's own extent is the best record of the rectangle. Measured on the 35 files, the ground's
box lies inside the sector grid's outer lines by -1.5 to 100.5 m per side. So the rectangle is not the sector grid:
the grid is cut from the rectangle, not the other way round.

[data: per file, the outline box against the outer sector lines (probe): e.g. sp_004_b infantry 37.2 / 73.1 / 13.2 / 62.7 m (min x / max x / min z / max z), sp_005 13.2 / 96.4 / 45.2 / 5.2, ons_mp_004 4.4 / 4.4 / 1.2 / 28.1; a box reaches past the outer grid line on 9 files: 1.5 m on 8 vehicle files, 0.4 m on sp_01 infantry (the margins, KYM-HDR-2)]
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

NavKit today (probe): every C portal is an axis portal on that line (74,408 of 74,408). It writes no diagonal and no
"up" C portals. Measure: `navparity:borders` 94.69 %. Whether the runtime reads the direction is not known
(KYRT-IGN-1), so the missing kinds may not matter in game.

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
is on one axis only, along the longer side of the area's box. No tested feature triggers it on all levels: size,
height span, cell count and boundary length were tried. Over the corpus, B portals are:

| | on a tile line | on a midline | elsewhere |
|---|---:|---:|---:|
| coarse | 118,108 | 16,159 | 11,401 |
| detailed | 123,484 | 550 | 23,573 |
| vehicle | 22,748 | 1,450 | 69,095 |

"Elsewhere" is steps between areas inside a tile (KYG-AREA-1) and, in vehicle files, the many small areas whose
boundaries follow no grid.

[data: probe classification of 386,568 B portals; 325 of 326 cut groups cut along the longer side, 0 of 326 on both axes (sp_004_b, sp_005, sp_007)]
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

[data: detailed sectors per file from the probe (571 of 2,626 infantry sectors); NeedsDetailedPathfinding within 10 m: 300 of 304 picks detailed, 296 of 498 found]
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

[data: nodes on the cell-centre grid 22,375 and on the corner grid 784 of 596,069; ONS_MP_002 node distance from an outline point p50 / p90 detailed 1.7 / 4.3 m, coarse 2.9 / 6.8 m]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:165] [doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:891-905]

NavKit today (probe): every node on a cell centre (672,793 of 672,793). Measures: `navparity:nodes_same` 0.64 %,
`nodes_near` 47.63 %. The placement rule (which outline corners, what offset, how open ground is filled) is the main
open rule for the graph. KYG-NODE-2 is the border part.

### KYG-NODE-2: Border nodes form a band shared by two to four sectors

Status: **observed**

Border nodes lie in a band beside each 100 m line (median 2.2 m from it, p90 9.2 m, at most 15.7 m), 1.6 m apart
along it (p90 5.8 m). Each is one id in every sector that holds it. A node near a corner is shared by three or four
sectors. One copy, normally the one whose square holds the node, owns the edges (HPD-NODE-5).

[data: 113,874 border ids: 104,448 in 2 sectors, 5,570 in 3, 3,856 in 4; owners: 107,437 one (103,458 the holding sector), 3,623 none, 2,738 two, 76 three]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:897-901]

NavKit today (probe): border nodes in a row 0.8 m from the line, about 10 m apart (`BORDER_NODE_SPACING`). 48,611 ids
are in 2 sectors and only 288 in 3 or 4. It writes 112,296 edges from non-owner copies (DICE 8,039), which hold no
edges in DICE's convention. Measures: `navparity:connectivity` 98.79 %, `over_connection` 15.31 %.

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

NavKit today (probe): a stable sort by index (2,480 / 2,487 / 2,470 of 2,494 sectors). Content parity does not need
DICE's ties unless the runtime's range query depends on them (KYRT-NODE-1, open).

## 6. Route edges

### KYG-EDGE-1: Edges join nodes within DistEdgeMax across

Status: **verified**

An edge joins two nodes that can reach each other in a straight walk on the mesh. Between ordinary nodes it is at most
DistEdgeMax across (15 m infantry, 50 m vehicle). With a border node at an end it is at most twice that, the two
sectors' reaches added (KYN-EDGE-1). The graph is directed: 13 % of edges have no edge back (KYN-EDGE-3). DICE's edges
do not cross D lines on infantry files (1 of 24,514 on sp_004_b, 11 of 82,602 on sp_01). Vehicle edges cross them
(0.58 %). Lengths, infantry coarse: 343,271 under 5 m, 321,736 at 5-10, 350,668 at 10-15, 36,719 over 15 (3-D).

[test: rust/bc2-formats/tests/kynapse_corpus.rs::edge_reach_is_dist_edge_max_across]
[data: 219,500 of 1,694,066 edges one-way; border-to-border edges 425,545]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:982-985] [doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:1009-1016]

NavKit today (probe): within the reach (0 over), but undirected (0 one-way). It writes 6,916 duplicate (source,
target) pairs, which DICE never does. It has 90,754 border-to-border edges against DICE's 425,545 on more levels.
Measures: `navparity:edges_same` 0.02 %, `ai_pairwise` 92.67 %. Which pairs DICE joins (its `GraphAccuracy` 3 / 0.5 /
15 and the directed cases) is open.

### KYG-EDGE-2: The stored cost

Status: **verified**

The cost word is `0x80000000 | floor(L x 16)` with L the f32 3-D distance of the stored f32 positions (KYN-EDGE-2).

[test: bfbc2-auto-navkit/tools/hpdcheck.py::check]
[data: 1,694,066 of 1,694,066 edges with L computed in f32]

NavKit today (probe): 1,900,944 exact and 18 off by one unit (an f64 length rounded); `navparity:costs` 95.91 %
(over the edges whose ends match DICE's). The runtime computes step costs from positions anyway (KYRT-PATH-1).

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
(311 of 881 areas balanced). Vehicle edges cross D lines (0.58 %). The rule is open.

[data: vehicle D 8,597 lines (1,612 with 3+ points; 2,746 of 10,515 segments diagonal), F 2,162 records (897 diagonal); 881 areas with F, 311 balanced]
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

NavKit today (probe): 45,522 objects all resolved, 5,383 cross-pitch, 41,536 on the mirrored tile. It has no
diagonal-neighbour links, because it writes no diagonal C portals (KYG-SEC-2). Measure: `navparity:borders`.

## The NavKit gap list, in priority order

Ordered by what the runtime does with each difference (path existence first, then which node a point gets, then
movement detail, then bytes):

1. **Detailed sectors** (KYG-DET-1): 710 against 502, 2.8x DICE's detailed areas. A wrong pitch changes ground, areas
   and nodes at once; DICE's bots stand in its detailed zones.
2. **Node placement** (KYG-NODE-1, KYG-NODE-2): every NavKit node on a cell centre against DICE's 4 %; a sparse border
   row against DICE's band; corner sharing 288 against 9,426. The go-to's resolver takes the first node within 15 m
   that passes a straight-line test (KYRT-NODE-1), so placement decides routes (`nodes_near` 47.6 %).
3. **Edges** (KYG-EDGE-1): an undirected graph against DICE's 13 % one-way; 6,916 duplicate edges; 112,296 edges from
   non-owner border copies (DICE 8,039).
4. **Area division** (KYG-AREA-2): no midline cut and 5-10x the off-grid portals (`portals` 6.5 %, `areas_same`
   5.2 %).
5. **The rectangle** (KYG-RECT-1): a borrowed bound on DICE levels, none on new maps.
6. **D / F** (KYG-DF-1, KYG-DF-2): coarse recall 19 % / 33 %, nothing for vehicles or detailed areas.
7. **C portal kinds** (KYG-SEC-2) and **diagonal links** (KYG-LINK-1): no diagonal or "up" C portals, no
   diagonal-neighbour links.
8. **Table bounds** (KYN-SEC-2): the rectangle and height range instead of the node box. The loader's duplicate test
   reads these bounds; NavKit's are unique, so there is no failure, but they are not DICE's values.
9. **Lookup-list ties** (KYG-NODE-3): a stable sort; DICE's order is unknown.

## Open questions

- The midline cut's trigger (KYG-AREA-2); the node placement rule (KYG-NODE-1); which pairs get edges and which are
  one-way (KYG-EDGE-1); the vehicle D / F rule (KYG-DF-2); detailed sectors beyond the flag (KYG-DET-1); the
  rectangle (KYG-RECT-1). The `formats/kynapse.md` section 6 table says what would settle each.
