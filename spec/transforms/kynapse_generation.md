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

NavKit today: on a new map (no DICE path data) the build keeps the sample columns inside this rectangle, the spawns'
box grown by 24.5 m (`navregress.spawn_bounds`, `bc2_navbuild::inputs::spawn_bounds`, round NKG2). On a level with
DICE data it still clips to DICE's extent (`clip_to_reference`). Round NKG2 measured the rectangle against that extent
on the 15 regression levels: on 45 of the 52 campaign sides the two lie within 0.41 m of each other (the rule's
0.11-0.91 m band), but on the 7 sides where DICE's ground ends on its own (sp_03 +z 59.2 m, sp_04 -x 347.0 m, sp_005
+x 276.5 m, sp_006 -z 64.4 m, sp_008_b -x 140.9 / -z 12.5 / +z 239.5 m) the rectangle keeps ground DICE has none of:
there the end is the seeding's (KYG-SEED-1), not the rectangle's, and NavKit seeds objects and spawns more widely. So
on DICE's levels the extent stays the reference's until the seeding matches (XNAV-BLD-3), and the Onslaught levels
keep it by the exception above. `navparity:sectors_extra` 1.33 mean (sp_01 12), `extra_ground` 8.31 %.

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
Measures: `navparity:portals` 6.47 %, `areas_same` 5.18 %. The largest single structural gap. Round NKG2 did not
change it: the cut's trigger is not decoded (above: no feature, crossing count or hole separates cut from uncut
groups), so a cut NavKit made would be its own rule, not DICE's; and the off-grid portals join areas NavKit's softer
link rule (`LINK_RULE` slope) connects where the 0.6 m area rule splits them, so dropping them takes routes away
(`connectivity`) until the area rule and the link rule agree as DICE's do. Both wait for the raw-sample fit the Open
questions name.

Round NKS-deep B found the trigger in the executable (KYG-AREA-4): not a property of the ground but the area's size in
bytes while the mesh is built.

### KYG-AREA-4: An area too large in bytes is split at its cells' box midlines

Status: **observed**

After the AI mesh's tiles are built (`kymapbuilderobstacles.cpp`), the builder sums each local map's (a tile's areas')
binary sizes and tests every area against **MaxLocalMapBinarySize** (Kynapse's default 8,192 bytes; DICE does not set
it; at least 4,096). An area at or over it marks its tile and the four side-neighbour tiles for a rebuild, and the
area's cells are dealt into four groups by the middle of their cell box (x above (min + max) / 2 on the integer cell
indices, z likewise): each group becomes an area of its own, so the area is cut on its box's midlines, which for an
area that fills its tile are the tile's midlines (KYG-AREA-2's 20 m into a 40 m tile). The tiles are rebuilt and
tested again until no area reaches the limit. A cut through only one axis is a split whose cells lie in two of the
four groups; ground that meets across the midline in several stretches (KYG-AREA-2's crossings) is ground with long
outlines, the size the test counts. The size is the area's record as built, before the outlines are simplified, so
the stored records (uncut areas up to 14 KB, KYG-AREA-2) do not show the threshold; it is not fitted from the files,
and NavKit (whose records are its own) does not cut yet.

[exe: the size test 0x011AD550 (MaxLocalMapBinarySize +0x598: default 0x2000 at 0x01146ACB, the reader 0x0114A579 keeps values from 0x1000; the tiles +0x5D4 x +0x5D8, each map's area sizes summed; an area at or over the limit marks its tile and the four side neighbours in +0x6A4 and is listed in +0x698); the split 0x011B4BB0 (the cells' integer box, mid = (min + max) >> 1 at 0x011B4D16 / 0x011B4D18, four groups by x > mid and z > mid at 0x011B4D42-0x011B4D6A); the rebuild loop 0x011B2160 (the size test, then the split and the marked tiles built again)]
[data: round NKS-rest: stored records of cut groups 1-32 KB, uncut areas 0-14 KB; cuts on the tile midline 16,159 coarse B portals]

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

Round NKG2 built DICE's own list (`navkit.DICE_DETAILED`, `bc2_navbuild::inputs::DICE_DETAILED`; off). Only 462 of
DICE's 502 can be: the pitch-0.2 sample runs sampled NavKit's own choice (755 sectors), and 40 of DICE's (sp_04 10,
sp_006 9, sp_01 8, sp_02 3, sp_008_b 3, ons_mp_004 4, one each on sp_005, sp_007, sp_008) were never sampled at 0.2;
building them detailed from those runs left them with no ground (sp_02 `ground` 98.95 -> 96.17 %, `connectivity`
99.49 -> 74.5 %), so the switch builds DICE's sectors that were sampled. Even so, 8 of the means got worse on the 15
levels (`nodes_near` 47.63 -> 45.51 %, `connectivity` 98.79 -> 98.74, `runtime_connectivity` 98.41 -> 98.34,
`strong_share` 99.69 -> 99.66, `portals` 6.47 -> 6.35, `outline` 97.99 -> 97.88, `df_d_precision` 50.53 -> 49.92,
`costs`; better: `ground` 98.22 -> 98.38, `df_d_recall` 18.84 -> 22.34, `df_f_recall` 33.46 -> 39.96):
in the 293 sectors NavKit makes detailed and DICE builds at 0.8, NavKit's 0.8 m build reproduces DICE's 0.8 m ground
and graph less well than its own 0.2 m build does. So XNAV-BLD-3 keeps NavKit's own choice until the coarse build
closes that gap, and a pitch-0.2 run over DICE's list (in game, paused) is needed for the 40.

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

Round NKG2 tried moving every new node off the grid toward the centroid of its share of the ground (the cells whose
nearest node it is within 7.5 m, the move kept inside its own cell, so walkability is unchanged): `ai_pairwise` fell
92.73 -> 92.33 % (sp_04 97.3 -> 95.4, sp_009 71.8 -> 68.2), `nodes_same` 0.66 -> 0.58, `connectivity` -0.04, so it
was taken out. A move that is not DICE's placement only moves NavKit's nodes away from the cell centres DICE's 4 %
share. The coverage gap is measured instead (`kynapse_rules --only cover` on the builds, KYG-NODE-5): ground seeing a
node within 7.5 m, coarse 73.7 % (DICE's same 15 files 96.4 %), detailed 97.5 % (98.3 %). Exact positions stay
unrecoverable from the output so far (the exception the parity target allows, with the evidence above and under
Open questions).

Round NKS-deep B decoded the placement from the executable (KYG-NODE-7: propagation from the seeds in NbSectors
directions, DistPropagation apart; KYG-NODE-8: dense edges and edges collapsed into their middles), which explains
every trace above: positions on no grid are middles of middles, the step pairs are propagated nodes that never merged,
and the coverage follows from merging only while every neighbour stays within DistEdgeMax. NavKit builds it behind
`Switches::placement` (off until the user rules): on the 15 levels `nodes_near` 47.63 -> 56.21 %, `ai_pairwise`
92.67 -> 95.57 %, `connectivity` 98.79 -> 99.06 %, `over_connection` 15.31 -> 15.74 % (`rust/BENCHMARKS.md`
"NKS-deep B").

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

Round NKG2 (`graphfile::relocate_owner_edges`, `owner_edges` on): the band follows from the owner rule itself. Every
edge a sector's graph holds out of a node it does not own moves to the node's owner, the sector whose square holds
it; the owner gets a copy of the edge's far end, which becomes a border node (shared id) if it was an ordinary node,
and this repeats until nothing moves. So the nodes within an edge's reach of a line, on both sides, become shared:
the band, as wide as DistEdgeMax. On the 15 regression builds (probe, with the spanner): border ids 48,899 -> 125,145
(DICE's infantry files of the same levels 103,085); ids in 3 sectors 66 -> 3,967 (DICE 4,799), in 4 sectors 222 ->
2,272 (3,553); border ids with no owner copy 346 -> 0 (DICE 3,308); edges out of non-owner copies 111,968 -> 0 (DICE
7,355); nodes outside their sector's mesh rectangle 3,522 -> 53,577 (DICE 90,809). As the runtime walks the graph the
strongly connected share is 100 % on every level (`strong_share` 99.69 -> 100 %) and `runtime_connectivity` rises
98.41 -> 98.80 % (sp_003_b 90.01 -> 92.67 %); `connectivity`, `over_connection`, `nodes_near`, `ai_pairwise` and
the ground measures do not move. A second row of border nodes 0.8 m into the
lower sector (NKG1's try, again with the owner rule) is not needed and was worse (`connectivity` -0.02,
`over_connection` +0.23, `ai_pairwise` -0.12): left out. The band's node positions are NavKit's (KYG-NODE-1), so its
spacing along a line is not DICE's 1.6 m.

[test: rust/bc2-navbuild/src/graphfile.rs::tests::bounds_are_the_node_box_edges_move_to_owners_and_foreign_only_pairs_keep_no_node]

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
NKG1 left them: they matter once placement follows DICE's (NKG2), and DICE's tie rule is not decoded. Round NKG2 left
them too, as a named exception: placement does not follow DICE's (KYG-NODE-1), and the table above shows no rule on
the stored values reproduces DICE's order (the best, index order, 7,110 of 15,276 x ties); the inferred cause, a sort
on unrounded coordinates the file does not keep, makes the order unrecoverable from DICE's output.

### KYG-NODE-4: Step pairs: nodes one step of GraphAccuracy + 0.35 m apart along an axis

Status: **verified**

Some ordinary nodes come in pairs that share one coordinate exactly (the same f32) and lie exactly one step apart on
the other axis, the step being `GraphAccuracy` + 0.35 m: 3.35 m in coarse sectors, 0.85 m in detailed, 15.35 m in
vehicle files. The gap is exact to the millimetre and stands 80, 56 and 22 times above any other gap within 20 % of it.
Multiples of half the step appear too (5.025 and 6.7 m coarse, 23.025 and 46.05 m vehicle). The pairs are a minority
(7.6 % of coarse ordinary nodes lie whole half-steps from another on both axes), mostly beside small obstacles, and
their nodes carry few edges (KYG-EDGE-3:
the spanner's unexplained pairs gather on them). So part of the placement moves or splits nodes by half-steps along x
and z. Where the pair's centre comes from is not decoded. The step itself is the generator's `DistPropagation`
(KYG-PAR-1, round NKS-deep A).

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

### KYG-NODE-6: Node ids count from 101, sector by sector in table order

Status: **verified**

The generator's node counter starts at **101** in every file: the first id of the first sector with nodes is 101 in
35 of 35 files (ids 0-100 are never used). It numbers the sectors in table order (ascending sector id, x then z),
and each sector's new nodes take one range of numbers above the previous sector's: its ordinary nodes and the border
nodes it is the first sector in table order to hold, interleaved in one range (border ids are not numbered after the
ordinary ones: in 2,837 of 2,863 sectors holding both, some border id lies below the last ordinary id). About a fifth
of each range is unused: 468,913 of the 586,397 numbers in the sectors' ranges name a node (80 %; per file 72-96 %).
So the generator numbers a sector's nodes as it creates them, ordinary and border alike, and then drops about one in
five (KYG-NODE-3's gaps; KYG-EDGE-3's later step). The gap between one sector's last id and the next sector's first
is 0 before 1,136 of the 3,094 sectors that number new ids and mostly under 10: dropped nodes at a range's end.

For NavKit this fixes where a parity build's ids start and how they run (101, table order, one range per sector);
the dropped numbers mark where the later step removed nodes, which part B can compare against the step pairs
(KYG-NODE-4).

[test: rust/bc2-formats/tests/kynapse_corpus.rs::node_ids_count_from_101_in_table_order]
[data: first id 101 in 35 of 35 files; the new-id ranges of the 3,094 sectors that number new ids (the other 26 hold only border ids numbered in an earlier sector) rise in table order without overlap; 468,913 of 586,397 numbers used; a gap of 0 before 1,136 ranges, 1-9 before 1,835, 10 or more before 123 (the first sector's measured from 100)]

### KYG-NODE-7: Nodes propagate from the seeds in NbSectors directions, DistPropagation apart

Status: **verified**

Round NKS-deep B read the node generator in `BFBC2Game.exe` (Kynapse's `kymapbuilderrgg.cpp`, the map builder's
"random graph" stage). It starts with one node at every seed point (the same list the exploration seeds from,
KYG-SEED-1: the builder refuses to run with "No seed defined" when it is empty) and the seed graphs' nodes, then
propagates breadth first: each node taken from the front of the queue sends **NbSectors** probes, probe i in
direction (cos a, 0, sin a) with a = i x 2 pi / NbSectors, from +x towards +z. A probe walks the built mesh
DistPropagation + 2 x Pitch along its direction; where it stops at the mesh's edge, it is pulled back 2 x Pitch; its
end (DistPropagation from the node in open ground, nearer before an obstacle) is a candidate. The candidates are
sorted farthest first (a stable insertion sort of the 32 / 128 / 64) and each becomes a node and joins the queue
unless the end's ground is not found (its height is read from the explored samples of its area), an existing node
lies within **DistDensityMin** (3-D), or an existing node within **DistDensityMax** is in a straight walk of it either
way (KYG-PAR-1's 0.35 / 2.1775 m coarse). So NbSectors is the probe count (32 coarse, 128 in the detailed zones, 64
vehicle; KYG-PAR-2 had it "not decoded"), and a node kept from the propagation sits exactly DistPropagation from its
parent, on one of the NbSectors directions: KYG-NODE-4's step pairs, which are not only axis pairs (the axes are
directions 0, 8, 16, 24 of 32).

On DICE's 35 files, the ordinary node pairs of a sector exactly one step apart in plan (+- 2 mm) lie on the
NbSectors directions within 0.01 degree in 21,709 of 21,884 coarse pairs, 2,272 of 2,407 detailed and 341 of 359
vehicle pairs, about half of them on the odd directions (9,396 / 1,138 / 163), so the count is NbSectors and not half
of it; of the control pairs 1.02-1.10 steps apart, 243 of 15,521 coarse lie on a direction.

[exe: propagation 0x0115D520 (100 nodes a call; NbSectors +0x55C probes; the angle i x 6.2831855 (double 0x01412670) / NbSectors; sin / cos; the probe 0x01153000 (EntityRadius +0x4E0, EdgeRadius +0x56C, Pitch +0x520, DistPropagation +0x560): reach DistPropagation + 2 x Pitch, the end pulled back 2 x Pitch; NeighbourInfo sorted by its squared distance, farthest first (0x0115BDC0); the candidate test and insertion 0x01158BA0 (ground height 0x011545E0, DistDensityMin +0x568, DistDensityMax +0x564, the straight-walk test 0x013565E0 both ways, a 5 m node grid); the seeds 0x0115AC40 (seed points +0x420, count +0x428: "No seed defined" at 0x01143658 when 0; seed graphs +0x450); driver 0x0115D980]
[test: rust/bc2-formats/tests/kynapse_corpus.rs::step_pairs_lie_on_the_nbsectors_probe_directions]
[data: round NKS-deep B, `kynapse_place --only angles` over the 35 files: step pairs on the directions coarse 21,709 / detailed 2,272 / vehicle 341, off 175 / 135 / 18, on odd directions 9,396 / 1,138 / 163; control on 243 of 15,521 / 26 of 2,926 / 5 of 3,051]
[code: rust/bc2-navbuild/src/propagate.rs:1-40]

### KYG-NODE-8: Dense edges, then edges collapsed into their middles

Status: **observed**

After the propagation the builder joins every node to every node within **ConnectionDistance** across (DICE leaves
it unset: Kynapse then takes 1.2 x DistPropagation, 4.02 m coarse), at least DistDensityMin away in 3-D, in a straight
walk with EdgeRadius; and nodes closer than that across but stacked (the vertical links). Then it simplifies the graph
by **collapsing edges into their middles**: the edges in a list sorted by length, shortest first; an edge is collapsed
when neither end is locked (seed-graph nodes), neither end is a leaf (one neighbour), no two-edge node is joined to
exactly its two ends (unless in the last phase), the middle has ground (found by a walk from the first end), and every
other neighbour of the two ends walks straight to the middle within DistEdgeMax across (and MaxDeltaHeight in height).
The first end moves to the middle, the second goes, and the first takes the neighbours' edges. New edges are marked;
passes run over the marked edges until one collapses nothing; then (OverSimplify, on by default) the edges under 4 m
with a spatial lock (after a collapse no edge within 2 lock cells of 5 m collapses in that pass) and, where a
neighbour cannot reach the middle, its edge rerouted to the ends' other neighbours; then every edge without the
two-edge test; and after any collapse again from the marked edges, until a pass over every edge collapses nothing.
So nodes merge until a merge would leave a neighbour beyond DistEdgeMax or out of a straight walk: the coarse nodes'
7-8 m spacing in open ground (KYG-NODE-5) and their positions off every grid (KYG-NODE-1: middles of middles), with the
step pairs (KYG-NODE-7) the propagation's nodes that never merged.

[exe: dense edges 0x01154B00 (ConnectionDistance +0x588; set to DistPropagation x 1.2 (double 0x0142EE90) at 0x0114E2B1 when not positive; Kynapse's default -1 at 0x01146AEF); the simplification 0x0115B3C0 (its graph 0x011B81D0, edges sorted by `CSimplifyEdgeLess` 0x011B8030, a new edge dirty with its length 0x01152C00; state word +0x83C: 0 dirty edges, 1 OverSimplify's edges under (2 OneMeter)^2 = 4 with the cell lock +0x28 of the 5 m grid and rerouting, 2 every edge; leaves 0x011B64C0; the two-edge apex 0x011B6500; the collapse test 0x01158510 with 0x01157CA0 / 0x01158100: DistEdgeMax^2 +0x558, MaxDeltaHeight +0x578, the straight walk 0x01351F80 with EdgeRadius); the graph rebuilt 0x011B6740]
[data: round NKS-deep B, NavKit with this placement (`Switches::placement`) against DICE's files of the 15 regression levels: the structure counts and parity in `rust/BENCHMARKS.md` "NKS-deep B"]
[code: rust/bc2-navbuild/src/propagate.rs:1-40]

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
removal of numbered nodes) that the spanner does not revisit. That step is not decoded. Round NKS-deep B found
it in the executable: no later step is needed, the 1.2 rule itself is a removal pass over the simplified graph's edges
(KYG-EDGE-5), so the misses are pairs never adjacent there or edges whose route a later removal broke.

NavKit today (round NKG2): each sector's walkable pairs (every pair within DistEdgeMax whose straight line is
walkable, both pitches) go through the greedy 1.2-spanner, shortest first, ties by pair order
(`graph::spanner_prune`, `stage::SPANNER_STRETCH`; the Gabriel rule on detailed sectors is no longer used); the ladder
links and the group joins come after it, as before. The test checks the rule's two halves on random point sets: every
pair left out has a route of at most 1.2 times over the kept ones, and the kept pairs join what all pairs join. NavKit's
edge counts then follow its node counts (KYG-NODE-1): coarse edges per node fall to DICE's side, detailed ones rise
with NavKit's denser detailed nodes (counts in the round's table, `rust/BENCHMARKS.md` "NKG2").

[test: rust/bc2-navbuild/src/graph_route_tests.rs::spanner_keeps_routes_within_the_stretch]
[test: rust/bc2-navbuild/src/graph_route_tests.rs::spanner_drops_a_pair_with_a_route_within_1_2]

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

NavKit today (round NKG2): the same convention. Each edge lives on its source's owner copy (KYG-NODE-2's
relocation), so an edge into a non-owner copy has its way back in the far end's owner sector; by id the graph stays
symmetric (every pair is written both ways, each way in its source's owner).

### KYG-EDGE-5: The 1.2 rule removes edges in order from the simplified graph

Status: **observed**

KYG-EDGE-3's spanner is not a greedy addition over every pair in sight. The builder's edges are the simplified graph's
(KYG-NODE-8: the dense edges carried through the collapses), and the over-connection pass then takes them in order
(the graph's edge table) and removes an edge, with its reverse, when an A* search from its source to its target,
without it and inside a sphere of OverConnectionRatio x its length around the source, finds a route at most
OverConnectionRatio (1.2) times its length. Both halves of KYG-EDGE-3 follow: an edge left has no such route when it
was tested (later removals only lengthen routes, so 0.19 % keep one), and a pair in sight without an edge is either
never adjacent in the simplified graph (nodes in sight within DistEdgeMax are not all neighbours there) or a removed
edge whose route a later removal broke. That is KYG-EDGE-3's 37 % of coarse pairs "without an explanation": no node
numbering gap or later step is needed.

[exe: the over-connection pass 0x01155FA0 (only when OverConnectionRatio +0x58C > 1; each edge of the table +0x6E0 in order, not a seed graph's; removal 0x010DA6E0 of it and its reverse); the route test 0x01155150 (`CHeuristicEuclidianDistance`, `CMySphereConstraint` of radius ratio x length around the source, the A* 0x010EB1F0, the path's lengths summed against ratio x length); then nodes without an edge removed 0x011578B0 (ComponentSizeMin +0x57C, default 1)]
[data: round NKS-deep B: NavKit with the placement and this pass (`Switches::placement`) against DICE's files, `rust/BENCHMARKS.md` "NKS-deep B"]
[code: rust/bc2-navbuild/src/propagate.rs:1-40]

## 7. D lines and F cells

### KYG-DF-1: Infantry D / F from broken sides

Status: **observed**

In coarse areas a side between two cells of one area can be broken in two ways. **D sides follow the ground across
the side:** between two terrain cells, a side is a D side where the terrain's slope along the axis at the side's
midpoint exceeds 0.77 (about 37.6 degrees; round NKG3), whatever the step between the two cell centres; where either
cell is an object surface, where the centres' step exceeds the link step (0.6 m). **F cells follow the centres'
steps:** a cell stepped into and out of along one axis (both centre steps over 0.6 m) is an F cell of that axis. F
polygons are stored twice, -axis then +axis, ordered -x, +x, -z, +z (KYN-DF-2). D sides that are an F cell's side
along its axis are not D lines; the rest are, one per owning cell, owner on the right, collinear runs merged, corners
cut where a cell owns two perpendicular D sides (the 3-point "L" lines). In detailed areas only terrain-terrain sides
break, from about 0.17 m, and F is rare (KYM-DF-1).

The centres' step is the mean slope over a whole pitch; the side's slope is the terrain's where the side lies, and
DICE's D sides follow it. On the 15 regression levels' DICE infantry files, every in-area side of a normal-pitch area
whose two cells are terrain samples (`kynapse_df`: DICE's cells from the area boundaries, heights from NavKit's
pitch-0.8 sample run, the slope from the level's height field), against DICE's label (a D side either way, or a side
of an F cell along its axis):

| Rule | broken and DICE's | broken, DICE has none | DICE's missed | precision | recall | D found | F found |
|---|---:|---:|---:|---:|---:|---:|---:|
| centres' step > 0.6 m | 178,348 | 50,572 | 103,895 | 77.9 % | 63.2 % | 124,842 | 53,506 |
| side's slope > 0.77 | 199,918 | 18,025 | 82,325 | 91.7 % | 70.8 % | 147,795 | 52,123 |

The slope finds more D sides with a third of the false breaks; the step finds more F sides. Taking F cells from the
steps and D sides from the slope (sp_02 and sp_006, 2 levels): 29,608 found against 8,025 false (the slope alone
27,689 / 3,108, the step 28,892 / 12,076), F found 13,288 (the step 13,035, the slope 11,610). The slope threshold
sits between the height field's quantised slopes (1 m squares): sides with a slope of 0.760-0.765 are D / F in 17 %
(6,932 of 40,538), 0.765-0.770 in 34 %, 0.770-0.775 in 54 %, 0.775-0.780 in 91 % (52,329 of 57,222). Object sides
keep the step rule (2,331 false positives against 865 found: the object rule is open). In detailed areas neither rule
separates DICE's breaks (the slope at 0.915: precision 29 %, recall 43 %; the step at 0.175 m: 22 %, 43 %), so the
detailed rule stays open too.

What the slope rule leaves (8 % of its breaks not DICE's, 29 % of DICE's not found) is not in the samples or the height
field alone: within 0.01 of the threshold the label is mixed (a named exception: DICE's own measure of the slope is not
recoverable from its output past the height field's quantisation), and DICE's lines are cut at 45-degree half cells
that NavKit's areas do not have (61,026 of 491,869 normal D unit sides have a cell outside the area raster).

[data: coarse D lines 431,972 (98.1 % 2 points, 7,880 with 3, 268 with 4 or more), F records 40,908; detailed 81,158 and 3,472]
[data: round NKG3, `rust/bc2-navbuild/examples/kynapse_df.rs` on the 15 regression levels: the tables above]
[code: rust/bc2-navbuild/src/stage.rs:796-803] [code: rust/bc2-navbuild/src/detail.rs:136-261]
[test: rust/bc2-navbuild/src/stage_tests.rs::terrain_slope_across_a_side] [test: rust/bc2-navbuild/src/detail_tests.rs::a_side_test_decides_where_it_answers] [test: rust/bc2-navbuild/src/detail_tests.rs::split_rule_f_by_the_step_d_by_the_test]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:939-1008]

NavKit today: the split rule with corner cuts (`navdetail`, Rust since NK3); the detailed rule is implemented but off.
Round NKG3 implemented the split rule (`Switches::df_slope`, `navwalk.DF_SLOPE`: D sides between terrain cells
by the slope on the level's height field, F cells by the step; route edges stay off the step rule's D sides) and
measured it on the 15 regression levels (means): `df_d_precision` 50.53 -> 56.71 %, `df_d_recall` 18.84 -> 23.89 %
(normal pitch 32.80 -> 40.60 %), F unchanged, every other measure unchanged, D precision and recall up on 13 levels (sp_04's precision 49.54 -> 49.53 %,
sp_009 has no normal D) - but sp_02's `df_d_recall` fell 21.86 -> 21.53 % (43 of DICE's 12,932 sides,
lost at slopes 0.70-0.77 with centre steps of 0.60-0.65 m, the mixed band above). **On** by the user's ruling
(2026-11-22), which accepts sp_02's dip as a named exception (`navkit.md`, "Parity target": the band where DICE breaks
only 17-34 % of sides and its own slope measure is hidden by the height field's quantisation). Measured and left out: the slope rule for F too (F recall 33.46 ->
31.35 %), the route edges kept off the slope rule's D sides (ONS_MP_004 `connectivity` 97.90 -> 97.76 %,
`runtime_connectivity` 98.23 -> 98.07 %), D by slope or step (D precision down on 12 levels), the threshold at 0.755
(D precision 50.53 -> 46.09 %). Probe: 258,379 D lines (396 with 3+ points against DICE's 8,148 on all files) and
40,588 F records on the 15 levels; F order and twins hold.

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

## 9. The generator's parameters (round NKS-deep A)

DICE's path data stores few of the generator's settings itself: pitch, the tile size and the entity radius in each
mesh header (HPD-SET-1, KYN-TILE-1), DistEdgeMax on each sector's first node (HPD-NODE-1), and the area heights that
follow EntityHeight (KYM-AREA-3). The header and table words hold none (KYN-FILE-2, HPD-TAB-4). The rest are known
from two sources read in round NKS-deep A: the settings DICE ships (`AI/Pathfinding/Infantry`, `Vehicles`,
`InfantryData`, `VehicleData` in `sp_common` and `mp_common`, the same in both), and the Kynapse generator code in
`BFBC2Game.exe`, which turns them into the generator's own parameters. Each rule below names what the parameter
explains in DICE's data.

### KYG-PAR-1: DistPropagation is GraphAccuracy + 0.35 m, because DICE sets the radius after the accuracy

Status: **verified**

DICE's build chain (`FUN_00F194F0`) writes the map builder's configuration as name / value pairs in a fixed order:
StepMax, StepLength, HoleMax, GroundSlopeMax, GroundSlopeMin, MaxDeltaHeight, Pitch, VerticalPitch (only when
not negative), **GraphAccuracy**, NbSectors, EdgeRadius, OverConnectionRatio, ForbidCollidingEdge, DistEdgeMax,
UseOldPolylineSimplification (one generator kind only), CollisionModel, **EntityRadius**, EntityHeight,
BuildCompositeShapes. Kynapse's map builder (`graphgen\kymapbuilder.cpp`) reads them one at a time, in that order. Only
GraphAccuracy derives other parameters, from the EntityRadius it holds at that moment:

- DistPropagation = GraphAccuracy + EntityRadius;
- DistDensityMax = 0.65 x DistPropagation;
- DistDensityMin = EntityRadius when that is below DistDensityMax, else DistDensityMax / 2.

EntityRadius still has Kynapse's default, **0.35 m**, when GraphAccuracy is read: DICE's 0.4 / 2.5 arrives later and
is stored without recomputing them. So DICE's generator ran with:

| | GraphAccuracy | DistPropagation | DistDensityMax | DistDensityMin |
|---|---:|---:|---:|---:|
| coarse (infantry) | 3 | 3.35 | 2.1775 | 0.35 |
| detailed (zone settings) | 0.5 | 0.85 | 0.5525 | 0.35 |
| vehicle | 15 | 15.35 | 9.9775 | 0.35 |

DistPropagation is KYG-NODE-4's step, exactly, on all three kinds: the pairs of nodes one step apart along an axis
are one propagation step apart. It also shows as the nearest-neighbour distance: 7,505 coarse ordinary nodes have
their nearest ordinary node (same floor) exactly 3.35 m away (+- 2 mm), against 96 at 1.05 times that; detailed 2,455
at 0.85 m (199), vehicle 41 at 15.35 m (0). The median nearest-neighbour distance of coarse nodes is 3.35 m. The
density limits leave no such trace (coarse: 234 nodes at 2.1775 m against 306 at 1.05 times it; 4 at 0.35 m), and no
coarse node pair is closer than 0.02 m, so what the two limits do is not yet seen in the data. The detailed zones
(`MapBuilderLocalSettingsZone`, GraphAccuracy 0.5) follow the same derivation: their parse was not traced, but their
step is 0.5 + 0.35.

A NavKit that places nodes by propagation (part B) uses these values, not GraphAccuracy + the real radius
(3.4 / 0.9 / 17.5 m): the data contradicts the latter (no step at 3.4 m).

[test: rust/bc2-formats/tests/kynapse_corpus.rs::nearest_node_at_dist_propagation]
[test: rust/bc2-formats/tests/kynapse_corpus.rs::nodes_pair_one_step_apart_along_an_axis]
[exe: DICE's configuration writer 0x00F1A94D-0x00F1B028 (GraphAccuracy from settings +0x64 at 0x00F1AC33, EntityRadius from the type's +0x2C at 0x00F1AF72); the map builder's parameter reader 0x011483D0 (one name per call, `ret 4`): GraphAccuracy at 0x011494E7-0x01149553 (DistPropagation +0x560 = value + EntityRadius +0x4E0; DistDensityMax +0x564 = x 0.65 (double 0x014C2820); DistDensityMin +0x568 = EntityRadius or DistDensityMax x 0.5 (0x014D9488)); EntityRadius at 0x0114A742-0x0114A768 stores +0x4E0 only; defaults 0x01146822-0x01146B3D: EntityRadius 0.35 (double 0x0145A4C8), EntityHeight 1.75, DistPropagation 1.5, DistEdgeMax 15, NbSectors 16, OverConnectionRatio 1.2; the generator's info-file format 0x014C2EF8 (printed at 0x0115553D) lists the same fields]
[data: nearest ordinary node of the same floor exactly at DistPropagation (+- 2 mm): coarse 7,505 (at 1.05 x: 96), detailed 2,455 (199), vehicle 41 (0); coarse nearest-neighbour median 3.350 m; at DistDensityMax: coarse 234 (control 306), detailed 172 (187), vehicle 0]

### KYG-PAR-2: DICE's settings, as the generator receives them, and what each explains

Status: **observed**

The values DICE ships (sp_common = mp_common) under the generator's names, with their trace in the data:

| Parameter | Infantry | Vehicle | In the data |
|---|---:|---:|---|
| Pitch | 0.8 (zones 0.2) | 1.0 | mesh +0x24 (HPD-SET-1) |
| GraphAccuracy | 3 (zones 0.5) | 15 | the step via DistPropagation (KYG-PAR-1) |
| EntityRadius (the type's) | 0.4 | 2.5 | mesh +0x40 and every area (HPD-SET-1) |
| EntityHeight | 1.9 | 4 | area heights + 0.8 x, outline + 0.5 x (KYM-AREA-3) |
| DistEdgeMax (the type's) | 15 | 50 | first node +20 (HPD-NODE-1); edge reach (KYN-EDGE-1) |
| OverConnectionRatio | 1.2 | 1.2 | the greedy spanner's 1.2 (KYG-EDGE-3) |
| MaxDeltaHeight | 1000 | 1000 | no height limit on edges (KYN-EDGE-1: 7,186 edges longer than DistEdgeMax in 3-D) |
| StepMax / StepLength | 0.28 / 0.36 | 0.4 / 0.4 | the area and D / F break (KYG-PAR-3, inferred) |
| HoleMax | 0.28 | 0.75 | not yet seen in the data |
| GroundSlopeMin / Max | 40 / 40 | 25 / 40 | not yet seen (vehicle ground stops at 25-28 degrees: `navvehicle.SLOPE_MAX`) |
| EdgeRadius | 0.41 | 2.51 | not yet seen |
| NbSectors | 32 (zones 128) | 64 | not decoded |
| VerticalPitch | -1 (not written: Kynapse's default) | 2 | not yet seen |
| CollisionModel | swept sphere | cylinder | KYG-IN-1 |
| ForbidCollidingEdge, BuildCompositeShapes | false, true | false, true | not yet seen |

The parameters DICE does not write keep Kynapse's defaults: among them OverSimplify on, ExploreAll and
ExploreDiagonales on, ConnectionDistance, ComponentSizeMin and the steps run (FirstStep exploration to LastStep graph
optimisation). The reader also knows parameters DICE never sets (`PropagationSeedPoint`, `AllowSimplification`,
`RemoveOutVertices`, `ConnectSlightlyOutsideSeeds`, `JoinNeighbourObstacles`, `ExplorationHysteresisRatio`,
`DoNotSimplifyCornerVertices`, `GenerateHidingPoints`, `MaxDistanceToWall` and others); their names show the stages
(an exploration, an analysis, the graph's creation and its optimisation) but their effect was not traced.

[exe: parameter reader 0x011483D0: the names and the fields they store (EdgeRadius +0x56C, EntityHeight +0x4DC, Pitch +0x520, StepLength +0x4E4, StepMax +0x4E8, HoleMax +0x4EC, GroundSlopeMax +0x4F4, GroundSlopeMin +0x4F8, NbSectors +0x55C, DistPropagation +0x560, DistDensityMax +0x564, DistDensityMin +0x568, DistEdgeMax +0x554 and its square +0x558, MaxDeltaHeight +0x578, OverConnectionRatio +0x58C, OverSimplify +0x584, FirstStep +0x5A4, LastStep +0x5A8); DICE's writer 0x00F1A94D-0x00F1B028 reads them from `PathfindingSettings` +0x48 / +0x54 / +0x58 / +0x5C / +0x4C / +0x50 / +0x68 / +0x60 / +0x64 / +0x44 / +0x24 / +0x28 / +0x73 / +0x70 and the type's +0x20 / +0x2C]
[data: `AI/Pathfinding/Infantry`, `Vehicles`, `InfantryData`, `VehicleData` read from sp_common and mp_common `level-00.fbrb` (identical): the values in the table; MaxDeltaHeight 1000 and OverConnectionRatio 1.2 in both settings]

### KYG-PAR-3: The break slope is StepMax / StepLength

Status: **inferred**

StepMax / StepLength is 0.28 / 0.36 = 0.778 for infantry and 0.4 / 0.4 = 1.0 for vehicles. Three thresholds measured
on DICE's infantry data without a known source sit at that slope:

- KYG-AREA-1's area link step at pitch 0.8: linked steps up to 0.591 m, unlinked from 0.62 m; 0.778 x 0.8 = 0.622 m
  (closer than the 0.6 m used);
- KYG-DF-1's terrain slope across a side (round NKG3): the share of sides DICE breaks rises from 54 % at slopes
  0.770-0.775 to 91 % at 0.775-0.780, around 0.778;
- the detailed sectors' terrain break "from about 0.17 m" at pitch 0.2: 0.778 x 0.2 = 0.156 m.

So the generator's walkable step between two samples one pitch apart is most likely StepMax / StepLength x pitch (a
slope limit expressed as a step over a step length). For vehicles that is 1.0 m at pitch 1.0, which the vehicle D / F
fit (KYG-DF-2) can test once a vehicle-shaped sample run exists. Part B tests the rule on the raw samples.

Round NKS-deep B tested it as a step on the raw samples (`kynapse_df`, the 15 regression levels, every in-area side
of DICE's areas against DICE's D / F label): it does not take the place of the measured steps. At pitch 0.2,
0.156 m instead of 0.175 m finds a few more of DICE's breaks (terrain 14,616 against 14,010) for seventeen times the
false ones (824,630 against 48,791; objects 220,430 against 181,096); at pitch 0.8 on object sides, 0.622 m instead
of 0.6 m finds fewer (787 against 865) at no better precision (26.5 against 27.1 %); and KYG-AREA-1's own fit puts
the area link step below 0.62 m (DICE's unlinked steps start there, so 0.622 m would link some). The slope near
0.778 (KYG-DF-1) stays the one trace of StepMax / StepLength; NavKit keeps 0.6 m, 0.175 m and the 0.77 slope.

[data: round NKS-deep B, `rust/bc2-navbuild/examples/kynapse_df.rs` (the rule "step > StepMax / StepLength x pitch"): detailed terrain true / false 14,616 / 824,630 against the 0.175 m step's 14,010 / 48,791, detailed objects 3,398 / 220,430 against 2,451 / 181,096, normal-pitch objects found 787 / false 2,180 against 865 / 2,331]

[inferred: three independently measured thresholds (0.62 m link step at pitch 0.8, the 0.775-0.780 slope band, about 0.17 m at pitch 0.2) agree with 0.28 / 0.36 = 0.778 within their measurement steps; the code that applies StepMax / StepLength was not traced]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:629-652]

### KYG-DOC-1: What public Kynapse documentation adds

Status: **observed**

Round NKS-deep A searched the web for Kynapse's (Kynogon, later Autodesk) manuals, SDK pages, talks and articles on
its generator. No manual or SDK page of Kynapse itself is public; the pages opened (2026-11-22) give:

- https://www.gamedeveloper.com/game-platforms/tooling-around-pathfinding-with-kynogon-s-kynapse : Kynapse computes
  "a 3D topological description of the world called the pathdata out of the collision world", with an automatic
  generation and hierarchical data for large terrains, streamed at run time. Agrees with KYG-IN-1 (the generator
  samples the collision world) and the last stage "Building hierarchical database".
- https://www.gamedeveloper.com/game-platforms/product-kynogon-releases-kynapse-4-0 : version 4.0 handles "very large
  terrains using an automatic tile mechanism". Agrees with the sectors and tiles (KYG-SEC-1, KYN-TILE-1).
- https://en.wikipedia.org/wiki/Kynapse : bought by Autodesk in 2008, re-engineered as Autodesk Navigation in 2011;
  BFBC2 is not in its list of titles. No technical detail.
- https://help.autodesk.com/cloudhelp/ENU/Navigation-SDK-Help/api/class_kaim_1_1_nav_mesh_gen_parameters.html : the
  successor's generator takes an entity radius and height, a step limit "between neighboring raster pixels" and a
  slope limit. That generator is a rewrite (a raster NavMesh, not Kynapse's sampled sectors), so its parameters are
  not used for DICE's data; the step between neighbouring samples matches KYG-PAR-3's reading of StepMax only in kind.
- https://github.com/OpenDraconis/divinity2-lib/blob/main/engine/kynapse.md : a third-party reverse-engineering note
  on Divinity II's Kynapse files; its names agree with DICE's files except the +20 float (KYN-FILE-2).

So the executable's own generator code (KYG-PAR-1, KYG-PAR-2) is the better source; the public pages confirm the
overall design and contradict nothing but the one third-party claim KYN-FILE-2 reports.

[data: five pages opened; each fact checked against DICE's 35 files where it applies (KYN-FILE-2's +20 float contradicted: 2,605 of 3,120 sectors hold a longer edge)]

## The NavKit gap list, in priority order

Ordered by what the runtime does with each difference (path existence first, then which node a point gets, then
movement detail, then bytes):

1. **Detailed sectors** (KYG-DET-1): 710 against 502, 2.8x DICE's detailed areas. A wrong pitch changes ground, areas
   and nodes at once; DICE's bots stand in its detailed zones. **NKG2's target**: DICE's own detailed sectors on DICE
   levels (502). For new maps DICE's choice is not recoverable from level data (designer zones, KYG-DET-1); the best
   stand-in is `NeedsDetailedPathfinding` within 10 m (precise: 300 of 304 picks detailed) together with 28 or more
   placed objects with collision in the sector (91.8 % of DICE's sectors classified right), offered as the default
   and overridable per sector. **Round NKG2**: written (`DICE_DETAILED`) and measured, but off: 40 of DICE's 502 were
   never sampled at pitch 0.2, and with the other 462 eight means got worse (KYG-DET-1). Open: a pitch-0.2 sampling
   run over DICE's list (in game) and a coarse build as good as the detailed one in the 293 sectors only NavKit makes
   detailed; the new-map stand-in is not changed (the level's flag and stacked-floor choice stays).
2. **Node placement** (KYG-NODE-1, KYG-NODE-2): every NavKit node on a cell centre against DICE's 4 %; a sparse border
   row against DICE's band; corner sharing 288 against 9,426. The go-to's resolver takes the first node in lookup-list
   order within 15 m that passes a straight-line test (KYRT-NODE-1, KYRT-NODE-2), so placement decides routes
   (`nodes_near` 47.6 %). **NKG2's target**: DICE's border band, as wide as `SectorOverlap` = DistEdgeMax (KYRT-CFG-1),
   and interior nodes placed to DICE's coverage (KYG-NODE-5): every sample point of the ground sees a node within
   DistEdgeMax / 2 (DICE: 96.5 % coarse, 98.3 % detailed within 7.5 m; 95.2 % vehicle within 25 m), with no node on the
   cell-centre grid by construction. The exact positions are still open (KYG-NODE-1 lists what they are not, now the
   collision too; KYG-NODE-4's step pairs are their only decoded trace), so NKG2 measures the coverage, `nodes_near`
   and `ai_pairwise`, not `nodes_same`. **Round NKG2**: the band done (KYG-NODE-2: the owner rule's relocation; border
   ids 125,145 against DICE's 103,085, in three or four sectors 6,239 against 8,352). Interior placement not: nodes moved off
   the grid toward their share's centroid and coarse coverage nodes every 7.5 m were both measured worse (KYG-NODE-1;
   coverage nodes raised `nodes_near` 47.63 -> 50.14 % but `over_connection` rose on 4 levels and ONS_MP_004's
   `connectivity` fell 0.44): the exact placement stays a named exception. **Round NKS-deep B**: the placement decoded (KYG-NODE-7,
   KYG-NODE-8) and built behind `Switches::placement` (off until the user rules; the measures in KYG-NODE-1).
3. **Edges** (KYG-EDGE-1, KYG-EDGE-3, KYG-EDGE-4): the duplicates are gone (NKG1). 111,968 edges sit on non-owner
   border copies (DICE 8,039); the runtime uses them (KYRT-PATH-3), and with NavKit's single border row they are its
   only edges into the lower sector, so they move onto owner copies with the band (NKG2, KYG-NODE-2: round NKG1
   measured 100 -> 6.5 % strongly connected without them). DICE's one-way edges are its border convention: the graph
   is symmetric by id (KYG-EDGE-4), as NavKit's is. **NKG2's target**: KYG-EDGE-3's pair rule, a greedy 1.2-spanner over
   mutually reachable pairs within DistEdgeMax (which reproduces DICE's missing pairs at 63-81 % and keeps all but
   0.2 % of its edges); `edges_same` and `over_connection` measure it. **Round NKG2**: done, both: the spanner on every
   sector (KYG-EDGE-3) and edges on owner copies only (KYG-EDGE-4: 0 edges out of non-owner copies, DICE 7,355).
4. **Area division** (KYG-AREA-2): no midline cut and 5-10x the off-grid portals (`portals` 6.5 %, `areas_same`
   5.2 %). The cut's trigger is still open (size, tile, hole, crossing count and what lies between the crossings
   excluded); NKG2 can take the off-grid portals (KYG-AREA-1's step boundaries DICE does not link) first. **Round
   NKG2**: not changed (KYG-AREA-2: the trigger is undecoded, and the off-grid portals carry routes until NavKit's
   area and link rules agree); both wait for the raw-sample fit.
5. **The rectangle** (KYG-RECT-1): a borrowed bound on DICE levels, none on new maps. **NKG2's target**: the rule,
   verified on the 13 campaign levels: the box of the soldier and vehicle spawns grown by 25 m, sample points kept up
   to about 24.5 m beyond the outermost spawn (infantry; about 22.2 m on vehicle files), the same on new maps. It
   replaces `navbuild.CLIP_TO_REFERENCE` on campaign levels; the Onslaught levels keep DICE's extent (their files are
   not bounded by the prepared partitions' spawns). **Round NKG2**: the rule bounds every build without DICE path data
   (new maps, the editor's stages); on DICE's campaign levels it was measured in place of DICE's extent and kept off
   (KYG-RECT-1: the 7 sides where DICE's ground ends before the rectangle).
6. **D / F** (KYG-DF-1, KYG-DF-2): coarse recall 19 % / 33 %, nothing for vehicles or detailed areas. **NKG3's
   target** stays the coarse rule's recall. The vehicle rule cannot be fitted from the files on disk (no vehicle-shaped
   sample run exists, KYG-DF-2): NKG3 needs one in-game `[dev] navsample` run at the vehicle settings (pitch 1.0 m, a
   cylinder of radius 2.5 m, height 4 m) on sp_04 or sp_02 first, so it is a playtest item while in-game runs are
   paused. **Round NKG3**: the coarse rule decoded (KYG-DF-1: D sides between terrain cells follow the terrain's slope
   across the side, above 0.77; F cells the centres' steps), implemented behind `Switches::df_slope` and measured: D
   precision / recall 50.5 / 18.8 -> 56.7 / 23.9 %, F and every other measure unchanged, but sp_02's D recall -0.33,
   on by the user's ruling (2026-11-22), sp_02's dip a named exception. The detailed rule (no single measure separates DICE's breaks) and the vehicle rule
   (no vehicle-shaped sample run) stay open.
7. **C portal directions** (KYG-SEC-2, KYN-C-1): DICE's "diagonal" and "up" values are not geometric and are read in
   no test traced; not imitated. Diagonal-neighbour links: done (NKG1, KYG-LINK-1). DICE's C records are also cut
   differently (17 % cross a square line; its 45-degree corner cuts), which follows its tracing (NKG2).
8. **Table bounds** (KYN-SEC-2): done (NKG1): the node box in 2,489 of 2,494 sectors; the 5 others (no node, or a box
   an earlier sector has) keep the build's bounds.
9. **Lookup-list ties** (KYG-NODE-3): a stable sort; DICE's order is unknown. The runtime's resolver reads the lists
   in order (KYRT-NODE-2), so ties can change which node a point gets, but only between nodes with the same
   coordinate; with NavKit's own positions this matters only once placement matches DICE's. **Round NKG2**: a named
   exception (KYG-NODE-3: no rule on the stored values gives DICE's order).

## Open questions

Round NKS-rest fitted the rules below over all 35 files (`rust/bc2-formats/examples/kynapse_fit.rs`); round NKS-rest2
went on with `rust/bc2-formats/examples/kynapse_rules.rs` (DICE's files alone) and `rust/bc2-scene/examples/kynapse_geom.rs`
(DICE's files against the levels' collision, height field and entities). Answered: which pairs get edges (KYG-EDGE-3,
a 1.2-spanner), which are one-way (KYG-EDGE-4, the border convention), and the rectangle (KYG-RECT-1, the spawns' box
grown by 25 m, verified). Still open, with what the fits ruled out:

- The midline cut (KYG-AREA-2): its trigger decoded (KYG-AREA-4, an area's built record at MaxLocalMapBinarySize);
  open: the size of the unsimplified record, to be fitted against DICE's cut groups (part C's 0.2 m samples), before
  NavKit can cut.
- Node placement (KYG-NODE-1): decoded (KYG-NODE-7, KYG-NODE-8); open: DICE's exact seed list per sector and its mesh
  queries, which NavKit stands in for (border nodes and order positions; its sample grid).
- The remaining 37 % of coarse pairs without an edge (KYG-EDGE-3): answered by KYG-EDGE-5 (the removal pass over the
  simplified graph).
- The vehicle D / F rule (KYG-DF-2): needs a vehicle-shaped sample run in game (none on disk); a playtest item.
- Detailed sectors beyond `NeedsDetailedPathfinding` (KYG-DET-1): no level-data rule (best 92.2 %); designer zones that
  are not shipped. Closed as unrecoverable for DICE's levels (their own list is the record); new maps use the stand-in
  in the gap list.
- The rectangle on the Onslaught files (KYG-RECT-1's exception): which spawns DICE built them from.
- Round NKS-deep A (section 9): the density limits are the propagation's (KYG-NODE-7); StepMax / StepLength x pitch
  as a step does not reproduce DICE's breaks (KYG-PAR-3, round NKS-deep B).

The `formats/kynapse.md` section 6 table tracks the same questions.
