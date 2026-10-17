# BC2 NavKit

Standalone tooling to create AI navigation data (`pathdata.hpd`) for Battlefield: Bad Company 2
levels – including multiplayer maps that never shipped with any – so the game's own AI can move on
them. Developed for Project OPFOR and published for other modders as `bfbc2-auto-navkit`, the automated
navigation-data generator of the BFBC2 suite. It measures levels through Project OPFOR's in-game sampler,
so it needs an OPFOR install, plus the suite's FbrbTool and DbxEdit.

**To use it, start with [QUICKSTART.md](QUICKSTART.md).** Double-click `navkitGUI.pyw` for the window, or
run `python navkit.py` from a command prompt.

## Why

BC2's AI uses Autodesk Kynapse. The game ships the Kynapse *runtime* (which loads `pathdata.hpd`)
under DICE's runtime licence, but the *generator* that DICE used to create that data is a separately
licensed developer tool: the copy that is still inside the game executable stops with a FLEXnet
licence error. NavKit replaces it with independent code.

## Rules (clean-room approach)

1. **Research** (`research\`): the Kynapse map builder in BFBC2Game.exe and the `.hpd` files are
   reverse engineered for interoperability. Results are written up as a *specification* – file
   formats, data structures, units, what each build stage produces. Licence checks are only located
   and documented as the boundary; they are not analysed in order to defeat them and nothing in this
   project bypasses them.
2. **Implementation** (`src\`): written from the specification with our own code and open-source
   libraries (e.g. recastnavigation, zlib licence). No Kynapse code is copied or translated
   line-by-line, no Kynapse binaries are shipped, and the tool never runs Kynapse code.
3. Game data (levels, collision meshes) always comes from the user's own installation.
4. Geometry is sampled through the **game engine's own physics queries** (DICE code, via Project
   OPFOR's `[dev] navsample` option) or read from the level's own data files – never through Kynapse.
   Observations of the original generator are limited to its boundary with DICE's code (the point
   queries it makes, the files it writes) and its output data.
5. The private folder `research\kynapse mapbuilder research\` is git-ignored and never published.

## Layout

```
QUICKSTART.md        how to build path data for a level, step by step
navkitGUI.pyw        the NavKit window (Tkinter): the steps below as buttons
navkit.py            driver: levels / prepare / sample / build / check / install / restore / status / all
research\            specification (KYNAPSE_SPEC.md) and images (img\)
tools\               NavKit tools (Python 3.13 + numpy / matplotlib)
  hpdtool.py         pathdata.hpd: info / sector / roundtrip / coverage / meshtrip
  kymesh.py          per-sector runtime mesh: parse_mesh / write_mesh (byte-exact)
  hpdview.py         render a sector to PNG (outlines, portals, graph)
  navsample.py       read in-game collision samples; compare with a pathdata.hpd
  hkpack.py          Havok 5.5 binary packfile reader (the level's *_Havok.res collision assets)
  levelobjects.py    object placements, pathfinding flags, forbidden-object triangles of a level
  navwalk.py         walkability rules + scoring against existing path data
  navbuild.py        the generator: samples + level objects -> pathdata.hpd
  ordercheck.py      scoring: AI order positions on the areas, pairwise route check, self-check
  navcompare.py      two files on a level's AI positions: coverage, main network, pairwise, size
  navdiff.py         two files layer by layer: sectors, walkable cells, areas, boundaries, route graph
  navparity.py       DICE-parity report over many levels, saved per named run
  navregress.py      regression test: rebuild levels with the current code, fail if a parity measure got worse
  navwater.py        water depth against path-data coverage (does the data leave out ground under water?)
  navboundary.py     area boundaries cell by cell against DICE's: matching / missing / extra edges, merged areas, portals; CSV + maps
  navedits.py        the walkability edits of a navigation side file as generator inputs (navkit.py apply)
  navlinks.py        a side file's links and route edges written into a path-data file's route graph
  navtraversal.py    a side file's traversal annotations (ladders, jumps ...) as <level>.traversal.json
src\, tests\         (reserved for the generator proper and its tests)
```

## Plan and status (2026-10-15)

1. **`.hpd` format – done.** Header, sector table, per-sector graph and runtime mesh fully decoded
   (spec §4); `hpdtool.py` parses every campaign file.
2. **Byte-exact rewrite – done.** `hpdtool.py roundtrip` / `meshtrip` re-emit every shipped file and
   all 7,944 sector meshes identically.
3. **Builder behaviour – done.** Spec §3: stages, the sampling grid, area cuts, graph rules,
   auto-seeding, the walkability rules (§3.7), structures and doors (§3.8), and the rules found on the
   other campaign levels (§3.9).
4. **Generator – runs on all 13 campaign levels (`tools\navbuild.py`).**
   - Full pipeline: walkable surfaces from two in-game sampling runs (normal and high capsule) plus the
     level's objects (`levelobjects.py`), 100 m sectors (level `SectorOffsets`), 3 × 3 tiles of 50
     samples, single-layer 8-connected areas (whole pieces of a tile joined by DICE's raw 0.6 m terrain step, no 20 m bisection; on a level with DICE path data the ground is bounded to its extent - a bound for comparison, not a rule found in the level data), outlines with 45° bridges, portals (B),
     border portals (C), neighbour links, route network (area, portal and shared border nodes; walkable
     edges up to 15 m; groups joined through string-pulled waypoints; no lone nodes), written as
     `pathdata.hpd`. Seeds: sector perimeter lines and soldier spawn points.
   - sp_01's file **loads in game** and the AI walks on it (play test 2026-10-15).
   - Accuracy against DICE's files (`ordercheck.py`: AI order / patrol positions on the areas, and the
     pairwise route check), 2026-10-15:

| Level | DICE-covered order positions also on NavKit's areas | Pairwise route check | Main route network (NavKit / DICE) |
|---|---|---|---|
| sp_01 | 618 / 633 (97.6 %) | 89.5 % | 639 / 607 |
| sp_02 | 241 / 260 (92.7 %) | 84.2 % | 313 / 362 |
| sp_03 | 343 / 353 (97.2 %) | 92.4 % | 374 / 232 |
| sp_04 | 500 / 526 (95.1 %) | 89.5 % | 582 / 593 |
| sp_003_b | 600 / 635 (94.5 %) | 87.3 % | 686 / 712 |
| sp_004_b | 169 / 181 (93.4 %) | 86.4 % | 185 / 192 |
| sp_005 | 304 / 309 (98.4 %) | 97.2 % | 308 / 312 |
| sp_005_b | 671 / 683 (98.2 %) | 94.8 % | 722 / 728 |
| sp_006 | 258 / 270 (95.6 %) | 92.7 % | 280 / 263 |
| sp_007 | 367 / 388 (94.6 %) | 88.6 % | 427 / 443 |
| sp_008 | 278 / 289 (96.2 %) | 83.4 % | 44 / 16 |
| sp_008_b | 110 / 113 (97.3 %) | 79.3 % | 107 / 58 |
| sp_009 | 68 / 72 (94.4 %) | 20.1 % | – (plane interior; measure unsuited) |
| **All 13** | **4,527 / 4,712 (96.1 %)** | | |

   - The pairwise check compares route-network layout; DICE's own network has gaps (spec §3.8), so it
     is a guide, not ground truth. Open leads: sp_008_b (sloped bridge foundation / fence passage DICE leaves
     out), sp_009 (plane aisles narrower than the capsule; DICE connects them), sp_03 / sp_008 (DICE network gaps).
   - Not generated yet: D lines, F cells, "up" border portals, diagonal border portals, detailed zones
     (pitch 0.2), vehicle data, ladder links (planned for MP bots, optional).
   - Next: play test of the current sp_01 build against DICE's file; then MP maps (self-check, no DICE
     data); later a standalone mode that reads Havok collision directly instead of sampling in game.
5. **Tooling – done (2026-10-15).** `navkit.py` runs the whole workflow from a level name (unpack, objects,
   automatic sampling box and sector offsets, both in-game sampling runs with `opfor.ini` restored
   afterwards, build, check, install / restore with the original kept); `navkitGUI.pyw` is a small window
   over it. Stock MP maps wait for MP support: the sampler needs the level's AI running.
6. **DICE parity – the goal from 2026-10-16: 100 % of DICE's content, not identical bytes.**
   - **Measured by content:**
     - `tools\navdiff.py` compares two files layer by layer: sectors, walkable cells, areas,
       boundaries, route graph. A file against itself scores 100 % on every layer.
     - `tools\navparity.py` runs it on every level, with islands (connectivity, over-connection) and the
       AI-position figures, saved per named run.
   - **Baseline A** (the builds behind the table above), means over the 13 levels:
     - 98.0 % of DICE's walkable ground reproduced, but +36.9 % extra ground;
     - 0 % of areas identical, and 0.6 % of route nodes in the same place;
     - connectivity 96.6 %, over-connection 26.4 %;
     - AI positions 95.8 %, pairwise 83.5 %.

     `python tools\navparity.py show "baseline A"` prints the per-level table.
   - **The earlier figures measured less:** the AI-position coverage in the table above counts only where
     the level's AI is ordered to go.
   - **Regression test (2026-10-16):** `tools\navregress.py` rebuilds levels with the current code (the
     inputs of `navkit.py build`) and fails if any parity measure got worse than the reference run
     (`regression`, made from the code of 2026-10-16 and compared with baseline A). Run it before and after
     every generator change; `--save regression` after an intended improvement.
     - **The reference against baseline A** (13 levels, 70 minutes): mostly within a few tenths of a point
       either way, from different build inputs (baseline A's files were built by hand, the reference uses
       `navkit.py build`'s: spawns as seeds, no exclusion files). Larger: over-connection on SP_008
       (10.5 -> 35.5 %) and SP_008_B (32.2 -> 69.0 %), connectivity on SP_009 (85.9 -> 83.8 %), AI pairwise on
       SP_008_B (79.3 -> 76.6 %) and SP_02 (84.2 -> 82.7 %); connectivity better on SP_02 (+0.7) and SP_008
       (+1.2). Which input makes the SP_008 difference is still to find (likely the exclusion files).
   - **Hand edits (2026-10-16): `navkit.py apply <level>`** builds the level with the edits of its
     navigation side file (`work\navkit\<level>\<level>.navtune.json`, made in the Navigation Studio):
     unwalkable regions remove the sampled surfaces inside them before seeding (so a cut really
     disconnects), walkable regions force them in past the slope and clearance rules (they still join the
     network by the step rule), seeds add a seed, unseeds drop the island at the point. Later edits win where
     regions overlap. Then links and route edges go into the route graph (`tools\navlinks.py`): each point
     joins its nearest route node, both nodes must be in one sector (border nodes count for both), links and
     added edges are two-way unless set one-way, costs follow DICE's rule. On a DICE base there is no rebuild:
     the links go into a copy of DICE's file, `pathdata_dice_edited.hpd`, for this PC only. The output is
     read back to check it. Traversals (ladders, jumps, vaults, drops) go to `<level>.traversal.json` next to
     it (`tools\navtraversal.py`) for OPFOR's bot layer, not into the path data: the game's AI would plan
     through a ladder it cannot climb. Each has its ends, the route node and island at each end, and whether
     it joins two islands. The whole log is kept as `<output>.report.txt`. `--install` then installs the
     result (`navkit.py install --from <file>` installs any built file), with the traversal file beside the
     installed path data; `restore` removes both and puts DICE's file back. A full rebuild (about 1-8
     minutes a level); without edits the build is unchanged.
   - **Edge costs (fixed 2026-10-16):** every DICE edge costs `floor(distance * 16)` from the positions as
     stored; NavKit computed it from unrounded positions, and 138 of SP_004_B's 60,142 costs came out one
     unit off. NavKit now uses the stored positions (0 off; parity measures unchanged).
   - **Ground under water (2026-10-16, `tools\navwater.py`):** DICE keeps walkable ground down to about
     1.5 m under water and almost none deeper (11 campaign levels: 85-90 % of NavKit's ground at 0-1.5 m is
     DICE's too, 1-8 % below that; DICE has 241 cells at 1.5-2 m, 154 at 2-3 m and none deeper; median
     per-level limit 1.45 m). NavKit keeps it all: 29 % of NavKit's extra ground is deeper than DICE goes on
     that level (SP_01 73 %, SP_02 52 %). DICE's multiplayer data does the same (Onslaught: ONS_MP_002 72 of
     18,878 floors under water deeper than 1.5 m, ONS_MP_004 1 of 19,639).
   - **The water rule (2026-10-16):** NavKit's builder leaves out ground more than 1.5 m under the level's
     water (`navbuild.MAX_WATER_DEPTH`; the water mesh from the terrain bundle; hand edits forcing ground
     walkable still win). Over the 13 levels, against baseline A: extra ground +36.9 % -> +29.1 % (SP_01
     54 -> 16 %, SP_008_B 127 -> 91 %, SP_007 17 -> 10 %), DICE ground reproduced 98.0 % (unchanged),
     connectivity 96.6 % (unchanged); over-connection lower than the earlier reference on most levels.
     Connectivity fell 0.16 points on SP_04 and SP_004_B (SP_04 is the level where DICE keeps ground deepest,
     2.64 m) - accepted; a per-level limit is possible later. The reference run `regression` now includes the
     rule (the one before it: `regression_before_water`).
   - **Next:**
     1. test each stage with DICE's own input (so it can reach 100 % on its own);
     2. the extra ground (first: ground more than 1.5 m under water) and seeding;
     3. detailed zones (DICE has 145 to 5,409 per level; NavKit none);
     4. area division.
   - **Accepted limits:** hand edits, and geometry that differs from the shipped game.

Credits: level data is read with the suite's FbrbTool and DbxEdit, based on **Frankelstner**'s format
documentation and **Heico**'s BFBC2 Toolkit. **Napisal**'s path data for multiplayer maps (BBC2 ModPack)
showed that BC2's AI can run on them. Kynapse is a trademark of Autodesk; Battlefield: Bad Company 2 is
© Electronic Arts / DICE.
