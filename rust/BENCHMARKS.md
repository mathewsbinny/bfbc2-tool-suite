# Rust core: baseline timings

Milestone E0 (`bfbc2-map-editor/EDITOR_ARCHITECTURE.md` 3.8, "measured, not assumed"): the DBX reader in Python
(`core/bfbc2core/dbx.py`) and in Rust (`rust/bc2-formats`), on the same files on the same machine.

**Machine:** AMD Ryzen 7 8845HS (8 cores, 16 threads), 15.3 GB RAM, Windows 11, Python 3.13.15, Rust 1.100.0
(release, thin LTO). Measured 2026-11-16 while other work ran on the PC (a Project OPFOR BFBC2 tool), so single runs
vary by up to about 30 %; every figure is the best of three runs unless marked.

**How:** the files are read into memory first, so disk time is not counted.
- `python rust\bench_dbx.py [--distinct] [--stream] <folders>`: `bfbc2core.dbx` with each backend. *1 by 1* calls
  `dbx.parse` / `dbx.to_xml` once per file; *many* passes all files to `dbx.parse_many` / `dbx.to_xml_many` (the
  cycle collector paused; with the Rust core the files are read on all cores, then the Python trees are built in
  order). Every result is kept, as a level load keeps its trees, unless `--stream`.
- `cargo run --release -p bc2-formats --example dbx_bench -- [--distinct] [--stream] <folders>`: the Rust library
  alone (Rust trees, no Python objects), on one thread and on every core.


Earlier sections are in `rust/BENCHMARKS_ARCHIVE_1.md` (moved there on 2026-11-23 when this file passed the
repository's 1,000,000-byte limit; their line numbers are unchanged there). Its sections:

- A stock level: MP_005
- A WW2 level: Guadalcanal
- The whole corpus
- What the numbers say
- Leaner DBX trees
- The level loader on `parse_many`
- FBRB bundles
- The level loader in Rust
- The alias table, NavKit's collision readers, a WW2 level
- The writers in Rust
- Shader databases and cull trees
- Not ported yet
- The converter pipeline and NavKit
- NavKit's walkability rules, areas and cell picking
- Retired Python
- Retired C# tools
- E3a: the XAS audio encoder
- The rest of NavKit's generator
- E3d: HLSL compiles on all cores, dbxport's ranking from an index
- E3b: DbxEdit's commands in process
- E3e: the bundle cache
- E3f: the Xbox 360 shader translator in Rust
- E3g: the BF1943 model, physics and effect ports
- E3i: Granny skeletons and animations
- E3h: the rest of NavKit
- E3m: the sweep
- E3j: the stock-MP AI layer
- E3l: the render harness and the differential test
- E3n: the menu tools' APT code
- E3r: the rest of the BF1943 importer
- E4d: dbxport's partition port in Rust
- E3o: navdata, kymesh, navpath, havok.verify, texqa
- E3k: FbrbTool's commands in process
- E4a: the stock-MP AI layer's logic in Rust
- E4c: aptgraft's model, aptcheck and aptrefs in Rust
- E4b: the shader database logic
- E4f: the map and bot test harness and the crash-reader tools
- E4e: NavKit's tools around the build
- F0: the file formats that had no Rust
- G1: the map editor's shell and read-only map view
- F2b: the BF1943 importer's remaining Python, the menu tools' PE reader, xmlinst and navtune
- G2: the user's decisions on G1
- E2: editing an existing map, first part
- E2b: context menus, edit modes, sink by mouse, the asset browser
- F2a: the console converter's rest, the model tools, Granny's writing side
- R5: game-accurate rendering, phase 1
- R5c: game-accurate rendering, phase 3a - interactive and editable
- G3: the levels that live only in game\Package
- G4: the start flow - Open project, Edit existing level, Create level
- E3d
- O1: the orchestration and command-line glue
- F3: the rest of the Python format readers and writers
- S1: streamed packing, process-wide BF1943 restart points, the shader coverage index
- D1: the disk budget
- O2: the menu, crash-reader and NavKit / converter command lines
- O3: caches, stock-MP, aptgraft, audio, the BF1943 parity tools, meshview, culltree
- O4: NavKit's driver and tools, the editor's prepare, the test harness, texqa
- CT1: culltree add's memory and speed
- O5: the shader tools, the console readers, the BF1943 importer, stockmp's pipeline, NavKit's leftovers
- F4: F3's leftovers
- SD1: the shader database reader, end to end
- RT1: the full test tier, then the first Python twins retired
- RT2: RT1's leftovers - references recorded then retired, argparse at the console's width
- GR1: the Map Inspector folded into the editor, then retired
- SV1: the simplified view draws render meshes
- GR2: the Navigation Studio folded into the editor's AI workspace, then retired
- QW1: the audit's quick wins
- EX1: every name of an export, its preview, the Diff panel; LevelDiff folded in and retired
- GW1: the game-accurate view opens the WW2 levels
- VW1: the editor's view toggles
- XF1: the export fixes before E4
- R5d: the game-accurate view's visual gaps
- RT3: RT2's leftovers - the hpd / Wave readers and five command lines retired, abbreviations, one rule
- VW2: the mode view layer, weather in the views and in exports
- DF2: install and revert safety - one install ledger
- DF1: the editor's state and gating
- TS1: faster test tiers
- QW2: the audits' leftovers
- HG1: the hangs and the slow tests of the Rust test binaries
- R5e: the WW2 open sea drawn black
- GR3: the mesh viewer folded into the asset browser's preview
- OP1: the map editor's core before E5
- AV1: O2's stand-in exe out of Defender's reach
- GR4: DbxEditGUI folded into the map editor as the DBX inspector, then retired
- SG1: static group members editable
- OR1: OPFOR in Rust, stage 1 - the WW2 install's game-copy side
- NK1: NavKit's apply in Rust, first stage
- V1: the export's compatibility and validation layer
- NK2: NavKit's build, graphs to file
- PR1a: the post-Rust improvements' small fixes that stop drift
- AIS: the bot AI specification
- NKS-deep A: the Kynapse generator's parameters and public documentation

## NKS-rest2: the Kynapse generator against the levels' own data (2026-11-22, branch `nksr2`)

A specification round: no tool's behaviour changed. KYG-RECT-1 answered and verified, two new rules (KYG-NODE-4
verified, KYG-NODE-5 observed), KYG-NODE-1, KYG-AREA-2, KYG-DET-1, KYG-EDGE-3 and KYG-DF-2 narrowed; the gap list's
NKG2 / NKG3 targets, the open questions and `formats/kynapse.md`'s OQ6 / OQ8 updated.

**Method.**
- **DICE's files alone.** `rust/bc2-formats/examples/kynapse_rules.rs` (read-only, counts only), one fit per
  `--only`: `cover` 95 s in release (every sample point of the 35 files, 26 million, against the nodes it sees),
  `spacing` 2.1 s, `midline` 2.6 s, `holes` 18 s, `pairs` 12 s.
- **Against the levels.** `rust/bc2-scene/examples/kynapse_geom.rs` loads each of the 17 prepared levels with
  collision and height field through `bc2_scene::scene::load` (0.7-1.6 s a level; 0.5-8.4 million collision
  triangles) beside DICE's files: `clearance` (nodes against the collision clipped to the soldier's height band, and
  control points 1.7 m / -1.1 m beside each), `detailed` (each infantry sector's level data, `--rows` for the numbers),
  `rect` (the ground's box against each entity category's box). ONS_MP_005 is not prepared and is skipped. Release
  times for the 17 levels: `clearance` 104 s, `detailed` 25 s, `rect` 15 s.
- **Looked at.** Single sectors were plotted from a scratch dump (deleted): they showed the coverage layout and the
  axis pairs that the fits then measured.
- **Tests.** `kynapse_corpus.rs::nodes_pair_one_step_apart_along_an_axis` (1.3 s in the dev profile) and
  `bc2-scene/tests/kynapse_rect.rs::ground_stops_25_m_beyond_the_spawns` (1.1 s; entities only), both game tier.
- No NavKit build, no game run; no vehicle-shaped sample run exists (165 sample files surveyed).

**The fits (DICE's 35 files; 17 prepared levels for the geometry):**

| Rule | Fit | Coverage | Status |
|---|---|---|---|
| the rectangle (KYG-RECT-1) | the box of soldier + vehicle spawns grown by 25 m; samples kept to about 24.5 m beyond the outermost spawn (vehicle 22.2 m) | infantry: 45 of 52 campaign sides 0.11-0.91 m inside it, the other 7 end 13-347 m before, none past; vehicle: 7 of 48 sides 2.33-3.09 m inside, none past. Exceptions: the 5 Onslaught files (ONS_MP_002 242 m past the prepared partition's spawns) | verified |
| step pairs (KYG-NODE-4) | ordinary nodes sharing x or z exactly, one step GraphAccuracy + 0.35 m apart (3.35 / 0.85 / 15.35 m) | 6,578 coarse pairs at 3.350 m +- 2 mm, 80 x any other gap within 20 %; detailed 226 (56 x), vehicle 44 (22 x); 7.6 % of coarse nodes on such half-step offsets | verified |
| coverage (KYG-NODE-5) | every point of the ground sees a node within DistEdgeMax / 2 | 96.5 % coarse and 98.3 % detailed within 7.5 m, 95.2 % vehicle within 25 m; steep fall just past it; 1.8 % of coarse points see none within 30 m | observed |
| node placement (KYG-NODE-1) | not the collision | coarse nodes: 95.5 % without collision within 3 m, as control points; "collision corner + radius" (0.35-0.45 m): 2.2 % of detailed nodes against 1.9 % of control points; a node's share's centroid a median 0.65 m away (not a converged centroidal layout) | observed |
| the midline cut (KYG-AREA-2) | open | a cut rises with the crossings (single 0.6 %, five or more 21 %); between crossings 99 % no ground for cut and uncut alike; coarse areas hold 3,221 holes across a midline, so "no loop across a midline" is excluded | observed |
| detailed sectors (KYG-DET-1) | no level-data rule | best single feature (>= 28 placed objects with collision) 91.8 % of 2,561 sectors, best pair 92.2 %; designer zones, not shipped | observed |
| the spanner's misses (KYG-EDGE-3) | a step after the pass (inferred) | a cell-level reach test is passed by 95.7 % of coarse misses and 96.8 % of edges; no pair order explains a miss; misses on step-pair nodes 32 % (edges 9 %), both ends of degree <= 4 69 % (edges 39 %); vehicle misses leave the mesh's cells (85 %) | verified (edges), inferred (misses) |
| vehicle D / F (KYG-DF-2) | not fittable from disk | needs one in-game `[dev] navsample` run at the vehicle settings (pitch 1.0, radius 2.5, height 4) on sp_04 or sp_02 | open |

**What NKG2 can implement now.** The rectangle as a rule (the spawns' box + 25 m, the same on new maps, replacing
`CLIP_TO_REFERENCE` on campaign levels; Onslaught keeps DICE's extent); interior node placement to DICE's coverage
(every point sees a node within DistEdgeMax / 2) measured by the coverage, `nodes_near` and `ai_pairwise`; DICE's
detailed sectors on DICE levels and a stand-in for new maps (`NeedsDetailedPathfinding` within 10 m, or 28 or more
placed objects with collision in the sector; 91.8 %). NKG3: the coarse D / F recall; the vehicle rule waits for the
vehicle sample run (a playtest item).

**Left (NKS-rest3 in the ROADMAP):** the midline trigger, exact node positions (where a step pair's centre comes from),
the step after the spanner, the rectangle on the Onslaught files, the vehicle D / F rule (in game), the runtime's LPF
and node +0x14.

## NKS-rest: the Kynapse generator's rules fitted, and the runtime RE left (2026-11-22, branch `nksr`)

A specification round: no tool's behaviour changed. New rules KYG-EDGE-3 / KYG-EDGE-4 (verified, two game-tier tests
in `kynapse_corpus.rs`), KYRT-NODE-2 / KYRT-PATH-3 (observed), KYRT-DYN-2 (inferred); KYRT-CFG-1, KYRT-PATH-1,
KYRT-NODE-1, KYG-AREA-2, KYG-NODE-1 / 3, KYG-DET-1, KYG-RECT-1, KYN-EDGE-3, HPD-NODE-5 / 6 and the open-question
tables updated.

**Method.**
- **Data.** `rust/bc2-formats/examples/kynapse_fit.rs` reads DICE's 35 files read-only and prints counts and
  histograms only: 19 s in release (the pair fits dominate: every ordinary pair within DistEdgeMax, a 2-D sight test
  against the outline and D lines, the graph's route between them, an on-mesh test of the line).
- **The executable.** Read statically with capstone and pefile (scratch scripts, deleted): the RTTI's
  complete-object locators gave the `Kaim::` vtables, and the reflected field table of `PathfindingType` gave
  `SectorOverlap`'s field. No Ghidra project was copied; the game was not run.
- **Tests.** `edges_have_a_reverse_by_node_id` and `edges_are_not_redundant_at_1_2`: 1.4 s together in the dev
  profile (the whole `kynapse_corpus` binary 2.2 s).

**The generator (DICE's 35 files):**

| Rule | Fit | Coverage | Status |
|---|---|---|---|
| which pairs get edges (KYG-EDGE-3) | a greedy 1.2-spanner: no edge where the graph already has a route at most 1.2 x as long | edges kept: all but 946 of 487,308 infantry and 427 of 11,814 vehicle ordinary pairs; pairs in sight without an edge explained: coarse 63 %, detailed 68 %, vehicle 81 % (+ 3,283 vehicle lines that leave the mesh); the cut-off is sharp (coarse edges with a route within 1.20 x: 186, within 1.22 x: 1,206) | verified (the edge half), observed (the missing pairs) |
| one-way edges (KYG-EDGE-4) | the border convention: by node id the graph is symmetric | 99.1 % infantry, 95.1 % vehicle edges have a reverse by id; 96.9 % of infantry one-way edges in their sector end at a non-owner copy | verified |
| the midline cut (KYG-AREA-2) | open | 1,064 / 106 / 400 cut groups; "across the longer side" holds for 67 % of coarse cuts (not 325 of 326); not the cells, record size, outline points, tile or a hole crossing the midline; 87 % of coarse cuts join ground that meets across the midline in two stretches | observed |
| node placement (KYG-NODE-1) | open | not a lattice (0.05-1 m), not outline corners + radius (0.3 %), not the medial axis, not area centroids; vehicle lone nodes within 0.7 m of their area's cell centroid (90 %) | observed |
| detailed sectors (KYG-DET-1) | open | 42 blocks, only 14 rectangles (30 sectors) | observed |
| the rectangle (KYG-RECT-1) | open | the vehicle ground shares a side with the infantry ground's box on 13 of 68 sides | observed |
| vehicle D / F (KYG-DF-2) | not fitted | needs a vehicle sample run | open |

**The runtime (`BFBC2Game.exe`, all observed):**

| Finding | Addresses | Rule |
|---|---|---|
| `SectorOverlap` is `PathfindingType.DistEdgeMax` (+0x20): 15 m infantry, 50 m vehicle; `SectorSize` its `SubgraphBoxSize` (+0x40) | set-up 0x00F20C0E-0x00F20CD9; field table 0x01C39DB8-0x01C39EBF | KYRT-CFG-1 |
| the A*'s cost object `Kaim::IConstraint` (default `CConstraintShortestPath`: f32 3-D distance of the node positions; the AI's `CSphereConstraint`), heuristic `Kaim::IHeuristic` (none or `CHeuristicEuclidianDistance`: 3-D distance to the goal); 100 expansions per call, then state saved | 0x01059480, 0x01059970, 0x01059210, 0x01079320, 0x010803D1, 0x00F0F122-0x00F0F192, 0x0107A5CE | KYRT-PATH-1 |
| the owner bit is the byte at node +0x22: edges into a node with no loaded owner copy dropped; edges on every copy gathered; non-owner copies never a lookup candidate; random picks skip them | 0x0105C8C0 (from 0x0105CB5B, 0x0105CD36, 0x0105D43D), 0x0105E786, 0x0105C303 | KYRT-PATH-3 |
| the node lookup is a box query that binary-searches the three lookup lists, walks the one with the fewest candidates in its order, and the resolver takes the first candidate that passes (no distance sort): the lists' tie order is read | 0x0105E270, 0x0105ED80, 0x0105EF5A / 0x0105F278 | KYRT-NODE-2, KYRT-NODE-1 |
| LPF and gaps are an obstacle layer and dynamic avoidance beside the path data (names only) | 0x00F1F584, 0x014A9360-0x014A9A48 | KYRT-DYN-2 (inferred) |

**What changes for NavKit.** NKG1: edges on owner copies only (the runtime uses non-owner copies' edges, so NavKit's
112,296 add connections), and no directed graph is needed: DICE's is symmetric by id. NKG2: the edge pairs as a
1.2-spanner; the border band as wide as `SectorOverlap`; DICE's detailed sectors on DICE levels. The lookup lists'
ties matter in game only between nodes at the same coordinate, so they wait for node placement. NKG3: unchanged (the
vehicle D / F rule needs a vehicle sample run).

**Left (NKS-rest2 in the ROADMAP):** the midline trigger, node placement, the 37 % of coarse pairs the spanner does
not explain, the vehicle D / F rule, detailed sectors beyond the flag, the rectangle; the LPF's methods and the load
write of node +0x14.

## NKS: the Kynapse specification (2026-11-22, branch `nks`)

A specification round: no tool's behaviour changed. Three spec files (`spec/formats/kynapse.md`,
`spec/engine/kynapse_runtime.md`, `spec/transforms/kynapse_generation.md`), 52 rules: 14 verified, 38 observed.

**Method.**
- **Data.** `rust/bc2-formats/examples/kynapse_probe.rs` reads every DICE path-data file of the game copy: 35 files,
  3,120 sectors, 188,988 areas, 596,069 nodes, 1,694,066 edges. It takes 2.4 s (release), and it does the same for
  NavKit's builds named on its command line (the 15 regression builds: 2,494 sectors, about 6 s).
- **Corpus tests.** `rust/bc2-formats/tests/kynapse_corpus.rs` holds 9 game-tier tests: 2.6 s in the dev profile and
  6.2 s in release with two threads.
- **The executable.** It was read statically with a small capstone disassembler (scratch only, no file of the suite).
  Key claims were spot-checked against the disassembly: the tile lookup, the point-to-sector lookup, the header checks,
  the sector-kind count, the cost word's static bit and the computed step cost.

**Findings that change what NavKit should do:**

| Finding | Rule | NavKit today (probe) |
|---|---|---|
| resolution = 1 / tile size (40 / 20 / 50 m), tile = trunc((p - rect min) x resolution) | KYN-TILE-1, KYRT-TILE-1 | holds |
| the table's bounds are the route-node box; the loader merges sectors with equal bounds | KYN-SEC-2, KYRT-LOAD-2 | writes the rectangle and height range |
| the table's "reserved" word is the sector kind (non-zero shifts the block by a byte) | KYN-SEC-1 | 0, as DICE |
| node ids are unique over the whole file, ordinary and border from one counter | KYN-NODE-1 | holds |
| ordinary edges reach at most DistEdgeMax across; border edges twice that | KYN-EDGE-1 | holds |
| 13 % of DICE's edges are one-way | KYN-EDGE-3 | 0 % one-way; 6,916 duplicate edges |
| DICE's nodes are off the sampling grid (96 %) | KYG-NODE-1 | 100 % on cell centres |
| border ids shared by 3-4 sectors: 9,426 | KYG-NODE-2 | 288 |
| C portals: axis, diagonal and "up"; links to diagonal neighbours | KYN-C-1, KYG-LINK-1 | axis only |
| link objects name the neighbour's own tile and item; all 55,008 resolve | KYN-LINK-1 | all resolve |
| the search computes step costs from node positions; the cost word's bit 31 marks a static edge | KYRT-PATH-1, KYN-EDGE-2 | bit 31 set |
| the runtime picks a point's area by the tile item's heights, not the area header's | KYRT-AREA-1 | the two equal, as DICE |
| A records are read (with B, by a nearest-segment search) | KYRT-IGN-1 | none written, as DICE |
| a tile's trailing word gives the last area's size for the C links | KYRT-LINK-1 | DICE's layout (KYM-ORD-1) |

The NavKit gap list, in priority order: detailed sectors, node placement, edges, area division, the rectangle, D / F,
C portal kinds and diagonal links, table bounds, lookup-list ties. The detail is in `kynapse_generation.md`. What is
left is queued as NKS-rest in the ROADMAP.

## NK3: NavKit's build, D lines and F cells (2026-11-22, branch `nk3`)

NK1's plan, stage 2 of the build: `navdetail`, an area's broken sides, D lines and F cells (KYM-DF-1). NK1 and NK2
found that the Python's set iteration order reaches this output. The round had two steps. First, prove the port
against the Python. Then decide, per the DICE-fidelity rule, what the order-decided output should be.

### What moved

| Python (retired) | Rust | where it runs |
|---|---|---|
| `navdetail.broken_sides`, `f_cell_sets`, `d_sides`, `corner_cuts`, `d_lines` (unit sides, chains, corner cuts), `_components`, `_quarters`, `_loops`, `_simplify`, `f_cells`, `_f_point` | `bc2_navbuild::detail` (`detail_features`, `features`, `f_records`) | `bc2_native.navkit_detail_features` / `navkit_detail_f_cells` through `navdetail.features` / `detail_features` / `f_cells` |
| `navbuild.detail_pass`'s second pass over the broken sides for the D sides (`f_cell_sets` + `d_sides` again) | the D sides in the same answer (`Features.d_sides`) | `navbuild.detail_pass` |

`navdetail.py` keeps its interface: `eligible(i, j, di, dj)` and `breakable(a, b)` are still the caller's callables.
The core asks them only about sides that step more than the link step, in the reference's order. The cells' dict and
the half cells' dict are read in their order. `CUT_CORNERS` and `CHAIN_TURNS` are read per call. What stays Python
until NK5: `detail_pass` itself (the cells' dict from `level.y`, `eligible`'s portal keys, the outline sides moved to
D, `level.blocked`). Half of the detail step's remaining time is that dict.

### What Python's set order did

Every call of every regression build was recorded and replayed (`rust/parity_nk3_detail.py`: `capture`, then
`order`). The recording holds the area's cells in their dict order, the half cells, and what `eligible` and
`breakable` answered. That is 27,193 calls with 16.6 M cells. The Python left two choices to its sets' iteration
order. CPython hashes int tuples without randomisation, so the order is the same on every run, but it is arbitrary.
The other set uses do not reach the output: every iteration feeds a sort, a membership test or a sum of at most two
values, and a sum of two values does not depend on their order.

1. **A loop of an F group's boundary that starts at a pinch** (`_loops`: two edges leave the start vertex, and
   `pop(0)` takes whichever the quarters' set inserted first). This happens only for a group's second loop (a hole,
   or a second piece). Found on **0** of the 27,193 calls.
2. **The nearest F cell's tie** (`_f_point`'s last fallback, `min(fset, key=distance)`). This applies to a vertex
   with no area cell on the record's side and no F cell touching it: the centre of an empty cell where 45-degree
   bridges meet. Three F cells around it are equally near. Found at **17** vertices on 10 levels, always with the
   tied cells at different heights.

Neither order changes when the cells' dict is reversed (the `reversed` probe: same answers on all calls), so
reproducing the Python's choices would mean emulating CPython's tuple hash and set probing for these sets.

**DICE's data** (`parity_nk3_detail.py dice`: the 18 DICE infantry files of the game copy, read only):

- 44,380 of 44,380 F records are closed and counter-clockwise, with no repeated vertex.
- No two records of one direction in an area share a vertex.
- 195,813 of 195,813 normal-pitch F vertices are cell corners; none is at a cell centre.

So DICE never produces either shape: no holes, no pinches, no vertex at an empty cell's centre. Its data cannot
pick a rule.

**The decision** (the coordinator agreed, 2026-11-22): spec rule **KYM-DF-2** (`spec/formats/hpd.md`). It is
deterministic, and it is derived from DICE's own convention that an F record's corner heights come from the cells on
its side.

1. A loop starting at a pinch leaves along the edge whose end vertex is least (x, then z). At a pinch on the way it
   takes the sharpest right turn, as before.
2. Such a vertex takes the **mean height of the nearest F cells that lie furthest toward the record's side**. The
   Python took whichever nearest cell its set gave first. It could take a cell on the record's opposite side.

The rule decides only cases DICE never produces.

### Parity

- **The port** (`parity_nk3_detail.py compare`: the old checkout's `navdetail.py` and this checkout's Rust, each in
  its own process, on every recorded call). The Rust gives the old Python's answers bit for bit (floats as bits,
  -0.0 apart from 0.0) on **27,176 of 27,193 calls**. The other **17** are exactly the calls whose answers the order
  decided. On those, the Rust gives the old Python's own answer once the Python's two choices are replaced by the
  spec's rule (`loops_spec`, `f_point_spec`). That holds on **27,193 of 27,193** calls: 15 of 15 levels as
  expected. The Python also gives its recorded answer on all 27,193 calls replayed (the replay is faithful).
- **The whole builds** (`capture` with each checkout, the build clock fixed, files kept):
  - 8 levels are **byte-identical** to the old build: ons_mp_004, sp_004_b, sp_008_b, sp_009 (the **4 golden
    builds, as recorded**), sp_01, sp_007, sp_02 and sp_03.
  - On sp_007, sp_02 and sp_03 the one changed vertex lies in a normal-pitch sector that the level's detailed zone
    replaces (`merge_detailed`: sectors 0x0911, 0x0703, 0x0C06 at pitch 0.2), so it is not written.
  - 7 levels differ, and **every differing byte is explained** (`parity_nk3_detail.py filediff`). The header,
    every sector's fields, nodes and edges, and every mesh field are equal. Only F record point heights differ: 13
    vertices, 30 bytes in all (the same `float32`s, 1-4 bytes each).
  - sp_006's fifth vertex also lies in a replaced sector (0x1108).
- **The 17 values** (heights with the 0.95 m offset; for written vertices, the files' `float32`s; area / record /
  point index in the sector's mesh):

| level | sector | area / record / point | record | vertex (x, z) | tied F cells | Python took | old y | new y | change |
|---|---|---|---|---|---|---|---:|---:|---:|
| ons_mp_002 | 0x0502 | 23 / 7 / 10 | +z | (292.8, -474.4) | (115, 32), (116, 31), (117, 32) | (115, 32) | 65.8034 | 65.9464 | +0.1430 |
| sp_003_b | 0x0C06 | 40 / 1 / 11 | -x | (327.2, -432.0) | (34, 84), (34, 86), (35, 85) | (34, 84) | 187.2975 | 187.1257 | -0.1718 |
| sp_005 | 0x0A05 | 14 / 10 / 4 | +x | (43.2, -341.6) | (53, 73), (54, 72), (54, 74) | (54, 74) | 199.0039 | 198.9950 | -0.0089 |
| sp_005 | 0x0A0C | 104 / 11 / 6 | +z | (69.6, 391.2) | (86, 114), (87, 113), (88, 114) | (87, 113) | 273.3969 | 272.7541 | -0.6428 |
| sp_005 | 0x0B0B | 70 / 0 / 10 | -x | (172.0, 257.6) | (90, 71), (90, 73), (91, 72) | (91, 72) | 189.1686 | 189.8559 | +0.6873 |
| sp_005_b | 0x020F | 72 / 0 / 3 | -x | (-431.2, 682.4) | (86, 102), (86, 104), (87, 103) | (86, 102) | 323.3802 | 323.1239 | -0.2563 |
| sp_005_b | 0x0D04 | 69 / 2 / 7 | +z | (644.0, -442.4) | (54, 72), (55, 71), (56, 72) | (56, 72) | 217.5252 | 217.4724 | -0.0528 |
| sp_006 | 0x0312 | 6 / 9 / 10 | +x | (-689.6, 848.0) | (12, 60), (13, 59), (13, 61) | (13, 59) | 234.9392 | 235.0250 | +0.0858 |
| sp_006 | 0x0313 | 181 / 0 / 3 | -x | (-616.8, 984.0) | (104, 104), (104, 106), (105, 105) | (104, 106) | 223.4036 | 223.3761 | -0.0275 |
| sp_006 | 0x0E13 | 5 / 1 / 9 | -x | (448.8, 921.6) | (61, 26), (61, 28), (62, 27) | (62, 27) | 124.6782 | 124.0620 | -0.6162 |
| sp_006 | 0x1108 (detailed zone) | not written | -x | (755.2, -185.6) | (69, 17), (69, 19), (70, 18) | (69, 17) | 107.2985 | 106.8850 | -0.4135 |
| sp_006 | 0x120A | 21 / 7 / 4 | +x | (850.4, 64.8) | (62, 81), (63, 80), (63, 82) | (63, 80) | 163.8825 | 164.0486 | +0.1661 |
| sp_007 | 0x0911 (detailed zone) | not written | -z | (-36.0, 891.2) | (79, 114), (80, 115), (81, 114) | (79, 114) | 257.3235 | 257.3434 | +0.0199 |
| sp_008 | 0x0302 | 15 / 5 / 8 | +z | (-759.2, 196.8) | (50, 121), (51, 120), (52, 121) | (51, 120) | 148.2292 | 148.8641 | +0.6349 |
| sp_02 | 0x0703 (detailed zone) | not written | -x | (23.2, -264.0) | (29, 44), (29, 46), (30, 45) | (29, 46) | 206.4756 | 206.4417 | -0.0339 |
| sp_03 | 0x0C06 (detailed zone) | not written | -x | (327.2, -432.0) | (34, 84), (34, 86), (35, 85) | (34, 84) | 187.8239 | 187.6591 | -0.1648 |
| sp_04 | 0x0106 | 86 / 7 / 3 | -z | (-211.2, -312.0) | (110, 110), (111, 111), (112, 110) | (110, 110) | 344.1512 | 344.1973 | +0.0461 |

The tied cells are local (i, j) on the sector's sample grid; the vertex is the centre of the empty cell they
surround. Example: on ons_mp_002, the +z record's vertex has no cell on its +z side, (116, 33). Of the three tied
cells, (115, 32) and (117, 32) lie furthest toward +z. The new height is their mean; the Python took (115, 32) alone.

- **XNAV-BLD-3** (builds may only get closer to DICE's). OPFOR's `work\navkit\regress` files were left as they
  are. On the 10 affected levels, one level at a time, the old and the new build each went through navregress's
  checks: hpdcheck's structure, navparity's measures against DICE's file, and navregress's `compare` with the saved
  reference run "regression" at its 0.05 tolerance (scratch `xnav.py`, which writes nothing into OPFOR).
  - On every level all navparity measures are **identical** between the old and the new build, D / F included: an
    F vertex that moves at most 0.69 m stays within navdf's 1 m height match. No row value differs at all.
  - **No measure is worse** than the reference on any of the 10 levels, old or new, and hpdcheck finds no structure
    problem.
  - The new builds' measures:

| level | ground | areas | nodes | connectivity | AI coverage | D precision / recall | F precision / recall |
|---|---:|---:|---:|---:|---:|---:|---:|
| ons_mp_002 | 98.38 | 1.16 | 0.13 | 100.00 | 100.0 | 49.73 / 10.35 | 25.53 / 19.23 |
| sp_003_b | 97.41 | 0.84 | 1.47 | 93.68 | 96.5 | 50.10 / 31.25 | 31.77 / 46.94 |
| sp_005 | 98.12 | 1.94 | 1.01 | 99.42 | 99.7 | 53.88 / 28.49 | 31.50 / 31.15 |
| sp_005_b | 98.02 | 0.87 | 1.04 | 99.36 | 99.6 | 54.86 / 25.97 | 29.99 / 29.92 |
| sp_006 | 99.18 | 5.77 | 0.33 | 99.46 | 98.1 | 45.56 / 28.89 | 31.93 / 48.97 |
| sp_007 | 97.61 | 0.90 | 0.78 | 98.43 | 99.2 | 53.29 / 20.88 | 24.33 / 24.24 |
| sp_008 | 97.63 | 1.20 | 0.54 | 98.63 | 99.3 | 57.95 / 13.63 | 40.99 / 18.03 |
| sp_02 | 98.95 | 3.11 | 0.41 | 99.49 | 98.5 | 40.43 / 21.86 | 26.67 / 37.45 |
| sp_03 | 97.53 | 1.18 | 1.23 | 98.46 | 99.4 | 50.28 / 28.48 | 27.41 / 38.66 |
| sp_04 | 98.56 | 3.07 | 0.52 | 99.61 | 98.9 | 49.54 / 27.96 | 35.62 / 34.78 |

  A navregress run would rewrite those 7 levels' `work\navkit\regress` files with the new bytes and report
  them as "ok" with no measure change. It compares measures, not bytes, so it flags nothing here.
- **NK1's apply parity**: 33 of 33 (`rust/parity_nk1_apply.py`, the old checkout against this one: the 18 DICE
  files and the 15 regression builds). The apply's edit stage is untouched.
- **`test_detail_features.py`**: all pass, with the random areas also checking the F cell sets the core returns.
  **`test_navbuild_native.py`'s quick tier** (E2a / E3c / E3h recorded answers, whole stages with D and F): all
  pass. Rust unit tests: `detail_tests.rs`, 8.

### Time and memory

Replaying the recorded calls (each checkout in its own process; peak = the process's peak working set above its
working set once the calls were loaded):

| | old Python | Rust |
|---|---:|---:|
| all 15 levels, 27,193 calls | 11.8 s | **3.3 s** (3.6x; most of it the dicts and lists crossing into and out of the core, and the `eligible` callbacks) |
| sp_006 (2,945 calls) | 1.86 s, 61 MB | **0.42 s, 64 MB** |
| sp_005 (3,240 calls) | 1.66 s, 54 MB | **0.40 s, 56 MB** |

Inside the whole builds (each checkout's own build, alone, the build clock fixed; other agents' work beside them):

| | old | new |
|---|---:|---:|
| `detail_features`, 15 levels | 17.6 s | **3.0 s** (5.9x) |
| `detail_pass` (with its cells' dicts and outline edits), 15 levels | 38.1 s | **17.1 s** |
| whole builds, 15 levels | 547 s | 356 s (the machine's load moved this more than the stage did) |

The builds' peaks (2.7-9.4 GB) come from the stages before this one. The detail step's own memory is the replay's
peak above: tens of MB either way.

### Retired

`navdetail.broken_sides`, `f_cell_sets`, `d_sides`, `_unit`, `corner_cuts`, `_cut`, `d_lines`, `_components`,
`_quarters`, `_loops`, `_turn`, `_simplify`, `_f_point`, `SIDES` and `F_DIRECTION` are Python twins inside a living
tool (RETIRED.md rule 4: recorded here and in CHANGELOG, not as RETIRED.md rows). The last commit with them is
`8433e3a`. No command line went, and nothing outside NavKit imported them (Project OPFOR has no caller of navdetail),
so `VERSION` stays (RELEASING.md).

### What is left: NK4-NK5

- NK4 (walkability end to end) and NK5 (the stage, `merge_detailed`, the driver) stand as NK1 and NK2 planned them.
- NK5 now also takes `detail_pass`'s glue: the cells' dict from `level.y`, `eligible` from the portal keys, the
  outline sides moved to D and `level.blocked`. With the store's areas in the core, the detail pass of a whole stage
  can run in parallel in Rust, with no dict built and no callback. That is half of the 17 s above.
- KYM-DF-2 also notes a fidelity gap left for later (PY1 or a NavKit parity round). NavKit's F groups can hold
  holes, pinches and vertices at empty cells' centres, and DICE's never do: its 44,380 records are all simple loops
  through cell corners. So DICE groups F cells differently, maybe 4-connected, or without the 45-degree bridges.
  Measuring that on DICE's areas is a content question, not a port's.

## A1: the auto layout of a mode's gameplay, the site and path rules (2026-11-21, branch `a1`)

Round A1 (ROADMAP "A1", after V1 and L1): an optional auto layout of a mode's gameplay in DICE's style, offered in
the editor as a proposal; V1's leftovers on the ground (spawns over water or in rocks, a vehicle spawner's room and
domain, objects outside the combat area) and spawns with no path, as rules of V1's registry. The export's path-data
step (part 2 of the round) did not fit: its plan is below and in the ROADMAP as A2.

### The site (`bc2_levelwrite::site`)

The base level's ground as the rules and the auto layout read it: the height field (`Heightfield::height`, the game's
diagonal split), the water mesh on 4 m cells (the highest surface over each cell's centre), every static object's
collision in world space indexed on 8 m cells (the scene's `world_triangles`: placed objects and static group
members; triangles wider than 160 m in plan are ground sheets, left out) and the level's art (static objects within
48 m, a destructible one counting four times, as a share of the busiest places). A place is **buried** in an object
when a vertical line through it crosses that object's triangles an odd number of times above and below it (crossings
at one height counted once: a line through an edge); vegetation never buries. The editor makes the site once per
project from its base scene (`validation::site_of`; the height field and water from the game copy's bundles,
`export::ground_of`, when the views have no terrain unpack): 0.2-1.9 s on the stock levels (1.0-5.4 million collision
triangles), release.

### The site and path rules, calibrated on DICE's levels

Each threshold lies past what DICE's own objects do. `examples/a1_calibrate.rs` runs the rules on every object of 27
levels the editor opens with collision - the 21 stock multiplayer levels of `game\Dist` (mp_001 to mp_012sr) and six
of `game\Package` (`mp_005gr`, Vietnam's `nam_mp_002cq` and `nam_mp_005r`, OPFOR's WW2 ports `wake_island_s`,
`guadal_canal`, `iwo_jima_s`; `coral_sea`, Air Superiority, left out: the export does not offer the mode, and its
carrier spawns stand outside the ground its terrain holds): 1,475 spawn points, 707 vehicle spawners, 165 capture
points - with what each object measures:

| measured on DICE's objects | value | the rule's bound |
|---|---|---|
| spawn points over water deeper than 1.2 m (surface to the ground or the pier under it) | 0 | XVAL-PLACE-2: deeper than 1.2 m |
| spawn points buried in collision (depth up to the surface above) | 13, 0.01-4.24 m (boulders, `nam_mp_005r`'s rock); before the interior rule (a point standing on the object's own surface up to 0.6 m below it: a hangar, a container, a barrier's top) 47 more, up to a hangar's 5.4 m on `iwo_jima_s`; one 8.3 m in a bush's box | buried deeper than 4.5 m; interiors and vegetation left out |
| spawn points under the height field | deepest 4.59 m (under a cliff on `mp_008`) | deeper than 6 m |
| vehicle room, the tightest spawner per class (static collision 0.5-3 m above it, every 0.5 m on 16 bearings) | tank 2.5 m (`iwo_jima_s`), car 1 m, heli 4 m (`nam_mp_005r`), boat 1 m, IFV and quad 0.5 m | tank 2, heli 3.5, car, boat, IFV, quad 0.5 m |
| boats: water under them | 44 of 48 over water, 0.4-28 m deep; `nam_mp_005r`'s 4 river boats on rivers the water mesh does not hold (its surface 86-104 m below them) | a boat on dry land, judged where the water mesh lies within 10 m below the ground |
| land and sea vehicle spawners outside every combat area | up to 142 m (`mp_006cq`'s tanks, `mp_007`'s IFVs); helicopters and the ambient aircraft farther | more than 150 m; aircraft not held |
| spawn points outside every combat area | one, less than 1 m (`mp_008`) | more than 1 m |
| capture points outside every combat area | Rush, Squad Rush and Squad Deathmatch bases and squads (8) | not held (L1-CQ-013 / 014 hold flags and HQs to their team's area) |
| **the site rules on the 27 levels** | **0 findings** (before the calibration: 202 on the 21 Dist levels) | |

The rules also found what the target's first guess left on two of V1's export canaries (`mapexport validate` on
their projects, and with collision `a1_calibrate --project`): Arica at dawn's Conquest layer has 18 spawn points in
the harbour (water 1.4-56.2 m deep), Wake Island with Iwo Jima's state 22 in the sea (1.3-30.8 m) - the first guess fits
the
donor layer's box into the base's combat area without looking at the ground; the four Atacama canaries have none.
The auto layout is what moves such a layer onto land.

The rules report only what the edits moved or made (what the base has at the same place is DICE's), live in the
editor and, without collision (the ground and water from the game copy), in `mapexport validate`:

| rule | what | severity |
|---|---|---|
| XVAL-PLACE-2 | a spawn point over water, buried in a static object's collision (naming the object), under the ground or outside the terrain | warning |
| XVAL-PLACE-3 | a vehicle spawner without its vehicle's room, or out of its domain (a land vehicle over deep water, a boat on dry land, any buried, a land or sea vehicle outside the terrain) | warning |
| XVAL-PLACE-4 | an M-COM, a spawn point or a land or sea vehicle spawner outside every combat area | warning |
| XVAL-PLACE-5 | a spawn point, flag or M-COM with no route node within 15 m in plan and 4 m in height, or whose nearest node is on an island cut off from the main network (GR2's islands) | warning |

XVAL-PLACE-5 reads the route graph of the path data the AI workspace has read (NavKit's build, else DICE's file:
`validation::routes_of`, `site::Routes` on 15 m buckets); the Problems panel checks it from then on. The registry:
**112 rules, 51 errors, 59 warnings, 2 notes** (V1's 108 and these four). New spec rules: GAME-SPAWN-3, GAME-VEH-1
(`engine/gameplay.md`, observed on the calibration), AI-NAV-1 (`engine/ai.md`), XVAL-PLACE-2 to 5 and XLAY-AUTO-1
(`transforms/validation.md`).

### The auto layout (`bc2_levelwrite::autolayout`)

The project's mode layer is DICE's own layout (the target's donor: its flags, bases, M-COMs, spawns, combat areas and
event logic, E3d's `modelayer`). The auto layout keeps its shape and fits it to the level (module doc): the whole layer
turned (24 bearings), scaled (0.9-1.1 of the donor's own distances, the target's first-guess scale undone) and placed
on a 24 m grid of centres where the level has art; scored on the ground (flags, bases, M-COMs and spawn points on open
land not in water, collision or past 35 degrees; the combat outlines over the terrain; the combat areas' land and art;
the art at the objectives); the best 48 refined, the best 8 fitted spot by spot (a flag, base or M-COM with its
cluster up to 40 m, a spawn point alone up to 16 m, heights following the ground) and measured with V1's
`placement::measure` against L1's rules, `l1_a1_params.json` and the site rules. The proposal is a list of moves; the
editor commits it as one move operation when it is accepted. Deterministic: SplitMix64 for the seed, no hash or thread
order reaches it (the editor test proposes twice, once per theme: equal).

**The proof** (`examples/a1_layout.rs`, release): 16 stock levels of every mode (BFBC2 and Vietnam), each a project
whose target takes its own gameplay out: the target's donor is each other stock level of the era and mode in turn
(never the level's own map), the auto layout proposes on the level's site (its own gameplay layer left out of it),
the proposal is pushed as one move and validated (`validate::project` and the site rules with collision); per level
the donor is chosen without DICE's own layout (fewest warnings, then statistics outside DICE's ranges, then notes,
then the ground score). Then compared with DICE's own layout of the level: each of DICE's anchors matched to the
proposal's (Conquest flags by the least total distance, HQs by team, Rush stages in order, Squad Deathmatch the spawns'
centre), the combat areas' intersection over union, the share of the proposal's gameplay points inside DICE's area.

| level | mode | donor chosen | errors / warnings / notes | statistics inside DICE's ranges | turn, scale | objects moved (clusters, points fitted) | DICE's anchors matched: n (mean m) | combat area IoU | proposal inside DICE's area | s |
|---|---|---|---|---|---|---|---|---|---|---|
| MP_001 | CQ | MP_005 | 0 / 0 / 0 | 30 of 30 | 355 deg, 0.86 | 263 (0, 0) | 5 (299) | 0.36 | 76 % | 2.2 |
| MP_003 | CQ | MP_001 | 0 / 0 / 0 | 36 of 36 | 205 deg, 0.85 | 270 (2, 0) | 5 (187) | 0.39 | 41 % | 1.6 |
| MP_005 | CQ | MP_009CQ | 0 / 0 / 0 | 30 of 30 | 350 deg, 1.00 | 200 (2, 0) | 5 (282) | 0.23 | 100 % | 1.9 |
| MP_006CQ | CQ | MP_001 | 0 / 0 / 0 | 36 of 36 | 25 deg, 0.94 | 270 (2, 0) | 5 (242) | 0.45 | 70 % | 1.6 |
| MP_009CQ | CQ | MP_001 | 0 / 0 / 0 | 36 of 36 | 0 deg, 0.85 | 270 (2, 0) | 5 (128) | 0.29 | 51 % | 1.7 |
| NAM_MP_002CQ | CQ | NAM_MP_005CQ | 0 / 0 / 0 | 30 of 30 | 139 deg, 0.89 | 253 (3, 3) | 5 (116) | 0.41 | 61 % | 2.8 |
| MP_005GR | Rush | MP_008 | 0 / 0 / 0 | 31 of 31 | 124 deg, 0.97 | 882 (3, 0) | 3 (280) | 0.25 | 100 % | 6.8 |
| MP_009GR | Rush | MP_004 | 0 / 0 / 0 | 31 of 31 | 30 deg, 0.85 | 572 (5, 0) | 3 (241) | 0.33 | 87 % | 10.4 |
| MP_012GR | Rush | MP_008 | 0 / 0 / 0 | 31 of 31 | 83 deg, 0.91 | 882 (2, 0) | 3 (176) | 0.36 | 88 % | 5.2 |
| NAM_MP_005R | Rush | NAM_MP_003R | 0 / 0 / 1 | 31 of 31 | 348 deg, 0.90 | 391 (5, 0) | 3 (240) | 0.48 | 94 % | 4.3 |
| MP_002SR | SR | MP_001SR | 0 / 0 / 0 | 15 of 15 | 349 deg, 1.00 | 182 (0, 0) | 2 (78) | 0.58 | 100 % | 2.5 |
| MP_005SR | SR | MP_003SR | 0 / 0 / 0 | 15 of 15 | 335 deg, 0.96 | 232 (1, 0) | 2 (173) | 0.17 | 35 % | 2.4 |
| MP_012SR | SR | MP_001SR | 0 / 0 / 0 | 15 of 15 | 166 deg, 1.00 | 182 (1, 0) | 2 (142) | 0.49 | 100 % | 1.9 |
| MP_004SDM | SDM | MP_SP_005SDM | 0 / 0 / 1 | 4 of 5 | 135 deg, 1.00 | 214 (1, 0) | 1 (9) | 0.52 | 57 % | 1.7 |
| MP_007SDM | SDM | MP_004SDM | 0 / 0 / 1 | 4 of 5 | 115 deg, 1.00 | 152 (0, 0) | 1 (65) | 0.36 | 83 % | 1.6 |
| MP_009SDM | SDM | MP_004SDM | 0 / 0 / 1 | 4 of 5 | 0 deg, 1.00 | 152 (0, 0) | 1 (415) | 0.00 | 0 % | 2.1 |

**16 of 16 levels: 0 errors and 0 warnings; 379 of 382 statistics inside DICE's ranges** (the 3 outside: Squad
Deathmatch's vehicle count, below); 4 notes - those three and, on `NAM_MP_005R`, a stage's two M-COMs 63 m apart in
height on its hills (L1-RU-014; `l1_a1_params.json` has no statistic for it). Against DICE's own layout of each level
the proposals differ, as a layout from another map's DICE layout would: DICE's anchors are matched 192 m away on average
(median 182 m, from 9 to 415 m), the combat areas overlap with an intersection over union of 0.35 on average (median
0.36, from 0.00 to 0.58), and 71 % of the proposals' flags, bases, M-COMs and spawn points fall inside DICE's combat
area (from 0 to 100 %). The closest: the Squad Rush levels `MP_002SR` and `MP_012SR` (IoU 0.58 and 0.49, 100 % inside),
`MP_004SDM` (9 m), `NAM_MP_005R` (IoU 0.48, 94 % inside); the farthest: `MP_009SDM` (its art drew the layout to the town
415 m from DICE's squads). The seconds are a donor's whole run (the project made, the target chosen, the proposal, its
validation), release; the proposal alone is 0.8-0.9 s in the editor on `MP_003`. Seed 7 (before the calibration's last
widening) gave other proposals with the same verdicts on the 11 levels it finished in the round's time (0 errors, 0
warnings, every statistic inside DICE's ranges, one note).

Squad Deathmatch's one value outside DICE's range is its vehicle count: the mode layer brings no vehicle spawner (E3d
leaves the donor's vehicles to E4), so the layout has 0 where DICE's have 1 (L1-SD-006, a note); the auto layout only
moves what the layer has.

**The editor** (`rust/bc2-editor/tests/auto_layout_panel.rs`, game tier): Laguna Alta (`MP_003`) with Panama Canal's Conquest
layer; the Gameplay workspace's Auto layout panel proposes on its worker (0.7-0.9 s, debug build), the views draw the
proposal (rings and pins at the new flags, bases and M-COMs, the lines from where they stand, the spawn points, the
new combat outlines) while the project is unchanged (its revision and log), Accept commits one move operation (the
History names it with the proposal's words), the Problems panel lists no error and no warning, and an undo puts every
object back. Screenshots (Dark, Light; `BC2_EDITOR_SHOTS`): `a1_proposal_dark.png`, `a1_proposal_light.png`,
`a1_validation_dark.png`, `a1_validation_light.png`.

**Synthetic** (`rust/bc2-levelwrite/tests/auto_layout.rs`, no game data): a 2,048 m terrain with a lake, a town of
400 buildings and a rock; the site rules raised by a layout broken once per case (a spawn point in the lake, one in the
rock, a tank over the lake, one inside a building, one 1.5 m from a wall, a spawn point 100 m outside the area) and
silent on the clean layout and on DICE's own objects where the base has them; the auto layout moves a Conquest layout
standing half in the lake onto the town (no bad spot, no warning, every statistic inside DICE's ranges, 0.1 s); the
path rule on a route graph with an island around one flag names exactly that flag's cluster, and every object when no
node is near.

**With path data** (`--routes`, the AI workspace's case): the Onslaught level `ONS_MP_004` (its installed path data:
8,843 route nodes) as a project with a BFBC2 Conquest target. DICE's own spawns, flags and M-COMs on it: 0 off the main
network (XVAL-PLACE-5 silent). Without the routes the proposals left 0 to 47 objects off the main network per donor (7
for the donor then chosen);
with them (`autolayout::propose_with`: every spawn and objective also on the main network in the search, the fit and
the check) 7 of 9 donors' proposals have none off it, and the chosen one (`MP_005`'s layer) has **0 objects off the
main network, 0 errors, 0 warnings, 0 notes, 30 of 30 statistics inside DICE's ranges**, its anchors 75 m from DICE's
on average (combat area IoU 0.34).

### Part 2, the export's path-data step: the plan (A2)

What the round did of it: the path rule (XVAL-PLACE-5) on the routes the AI workspace reads, and the site rules that
keep spawns and objectives where NavKit's walkability finds ground. What it did not: an export that builds the level's
path data and its AI layer for the changed layout. Why: NavKit's build for a level without DICE's path data needs the
level sampled - in the game (OPFOR's `[dev] navsample`: no game launch in these rounds) or by
`navsample_offline.py` (Python and NumPy, not part of `navkit build`; the regression builds peak far past this
round's memory budget) - and `navkit build` itself is Python around the Rust core until NK2-NK5; and the stock-MP AI
layer (`bfbc2.exe stockmp`) is made for a stock level by its name, not for a staged one. The plan, in order:

1. **Inputs.** The export's stage (its level bundle with the edited partition and group files) and the base level's
   prepared folder (`work\levels\<base>`, `navkit prepare`).
2. **Sampling without the game.** `navsample_offline` with the stage's partition and group files in place of the
   base's (a partition override), in Rust once NK4 ports `navsample.load` and the walkability glue; the objective
   floors and seeds from the project's layout, not the base's.
3. **Build and edits.** `navkit build` on those samples (in process once NK5 lands; `bfbc2.exe navkit build` before
   that), then the project's navigation edits (`navkit apply`, Rust since NK1 on a DICE base).
4. **The bot module.** `stockmp generate / build` on the staged partition under the export's level id (a
   `--partition` / `--level` input): HasAI, PathfindingTypes, the SectorOffsets of the new path data, AI spawners at the
   project's spawns, orders at its flags and M-COMs, Rush's stage wiring; the level bundle repacked by the stage writer.
5. **Staging and checks.** `Output\win32\ai\levels\<id>\infantry\pathdata.hpd` (and `vehicle\`) recorded in the stage
   (`StageRecord`), `mapexport check` lines for them (present, parse, sorted sector nodes: AI-CRASH-4, SectorOffsets
   matching), XVAL-PLACE-5 as a stage line on the staged path data, the install ledger covering the AI files.
6. **The AI workspace** reads a stage's path data (the stage's folder beside `work\navkit`), islands and spawns as
   today.
7. **Proof.** The six VW2 canaries and SG1's member-edit export (Wake Island, MP_005): path data built and staged,
   every check line passing, every spawn and objective on the main network, the AI layer's orders covering each
   objective; in-game behaviour unproven until a playtest.

## SPG: the specification's gap pass (2026-11-22, branch `spg`)

A spec-writing round: no tool behaviour changed, no Rust built. `spec/COVERAGE.md` maps 36 BFBC2 systems to their
tools, the queued rounds and the rules; the missing specs were written in the order the next rounds need them
(VW2c / VW2e night conversion, E5 occlusion paint, VW3 weather editor first). Nothing written to `game\`; the game
never launched.

### Coverage, before and after

| | rules | files | covered | thin | none |
|---|---:|---:|---:|---:|---:|
| before | 769 | 46 | 15 | 13 | 8 |
| after | 933 | 54 | 24 | 12 | 0 |

New files and rules (verified / implemented / observed / inferred): `engine/lighting.md` 26 (1 / 2 / 19 / 4),
`weather.md` 12 (2 / 0 / 9 / 1), `effects.md` 11 (3 / 1 / 5 / 2), `weapons.md` 34 (3 / 2 / 27 / 2), `vehicles.md` 22
(1 / 4 / 16 / 1), `materials.md` 30 (1 / 5 / 17 / 7), `destruction.md` 22 (3 / 1 / 16 / 2), `persistence.md` 7
(0 / 2 / 5 / 0).

### The probes (`spec/probes/`, read-only, on the game copy)

| probe | reads | time |
|---|---|---:|
| `lighting.py` | 501 level bundles (444 DICE's): level partitions, 4,285 effect / emitter partitions, every ITexture name and IrradianceVolume, the weather partitions | about 6 min |
| `weapons.py` | 1,021 DICE bundles, 1,130 weapon partitions | 5 s |
| `vehicles.py` | 531 vehicle partitions, 74 stock level partitions | 30 s |
| `materials.py` | 74 material grids, 1,183 physics assets | 30 s |
| `destruction.py` | 1,262 blueprints, 74 level partitions | 20 s |

The worktree has no `bc2_native` build; the probes ran with `PYTHONPATH` naming the main checkout's
`core\bfbc2core` (its module's source hash matches).

### Findings that change planned work

- **`PointLightEntityData` exists** (68 on the Vietnam levels: radius 4 - 20 m, fade 10 - 50 m, shadow-map flags):
  research WX0's "no light entities" and the editor's overlay comment are wrong; the Lights overlay shows only
  `ActAsPointLight` emitters (35 of 2,882 templates). VW2e-2 should add them.
- **One point light per draw** and an **effect instance cap of 30 against DICE's 53 night lamps**: both open (RE).
- **DICE's patched balance is the Package copy**: the M24 is 100 -> 75 over 5 - 25 m in Dist, 100 -> 50 over 10 -
  36 m in Package; 325 of 351 weapon partitions with both copies differ. Any reader that takes Dist first gets launch
  values (OPFOR's donors read Package first: WPN-PATCH-2).
- **AlwaysAimHead true** in the WW2 balance ruling holds per class only: DICE has false on all 32 snipers (WPN-WW2-4).
  `dbxport` maps every `wabsShg` to `wabsRif`, though DICE uses `wabsShg` on 20 of 42 shotguns (WPN-WW2-8).
- **The material hash is the name's 33-xor hash** (5,746 of 5,746 materials, 1,183 of 1,183 files): a new material
  needs no donor file. Material indices differ per level grid (114 of 121 materials): a port must use materials the
  target grid holds.
- **Destroyed meshes are parts of the same physics file** (3,423 of 3,431 `MeshName`s), and 1,837 of 2,583
  static-group member types are destructible.
- **`ApplyFog` is false in all of DICE's states**; the weather editor's ranges are WTH-STATE-3.

### Left

`spec/COVERAGE.md` section 2 (queued as SPG2): terrain writing, vegetation, the shader translation, the game
module's patch points, animation and characters, decals, the rest of persistence, BF1943 as a source, Onslaught,
the effects' processor chain, streaming.

## QW3: small cleanups (2026-11-22, branch `qw3`)

Three fixes and a sweep; nothing written to `game\`, the game never launched.

### The shotgun body set (dbxport)

DICE's data (a read of the game copy, Package before Dist): `wcShotgun` takes `wabsShg` on 21 weapons (20 players',
1 AI clone), every one `fltSingleFireWithBoltAction` (870 MCS, Neostead, TOZ-194, the Vietnam 870), and `wabsRif` on
23 (Saiga and USAS-12, semi-automatic and automatic, and the pump-action SPAS-12). No other class uses `wabsShg`.
BF1943's 367 weapon partitions hold one `wabsShg`, the Shin-gunto knife (`wcKnife`), and no shotgun. The conventions
pass now moves `wabsShg` to `wabsRif` only off a shotgun, in `dbxport.py` and `bc2_convert::dbxport`; the knife is
ported as before, so the WW2 rebuild's outputs do not change and the oracle stays as committed (no rebuild run).
OPFOR's installed WW2 weapons: the M1897 (a copy of DICE's 870, not ported) `wcShotgun` / `wabsShg` /
pump-action; all 11 WW2 `wcSniper` weapons `AlwaysAimHead` false, as DICE's 32.

### hkpack's unsized arrays

`bc2_native.HkPackfile`'s value conversion put `Value::Unknown` into a list although the reader already holds it in
one: `[[('?', n)]]` against the Python's `[('?', n)]`, since round F4 (F4's 120 built packfiles never held such an
array with data). O2's live leg (`test_cli_o2.py --live --game`) passes again: 361 s, every group; F4's hkpack area
checks O2's sample packfile both ways (2 of 2, 18 s).

### Fixed temporary names in tests

`test_argfmt.py` (trace), `test_aptgraft.py` (movie), `test_prepare_o4.py` (stand-in), `cli_o5_stockmp.py`
(stand-in): a folder of the run's own (`tempfile.mkdtemp`), removed after; `--root` / `BFBC2_O5_STANDIN` still name
one. `test_f4_native.py`'s mesh-viewer index stays a shared, signed cache and is copied in through
`atomicio.copy_file`. Two `test_argfmt.py` runs at once: 943 of 943 each, no folder left behind. The quick
tier (`run_tests.py quick -j 2`): 133 of 133 in 1,023 s, O2 among them. The Rust tests all name their folders with the process id already. Left alone: OPFOR's
`wake_ambience.py` writes fixed `%TEMP%` names, but it is a rebuild step (one rebuild at a time), not a test.

## PYI: the Python inventory (2026-11-22, branch `pyi`)

`PYTHON_STATUS.md` / `python_status.json` at the root, generated by `python rust\python_status.py` from its table: all
358 `.py` files of the suite at the QW3 merge (`49f79c3`, plus the generator), 104,486 non-blank lines. Read only: no
tool code changed, nothing built, the game never launched. `bfbc2-auto-navkit`'s rows are the state on this base;
NK4 / NK5 (in progress beside this round) change them.

| status | files | lines |
|---|---:|---:|
| live (only Python does the job) | 22 | 6,105 |
| partly (part ported; the note names the rest) | 21 | 18,479 |
| twin (Rust twin exists; kept for parity, fallback or callers) | 55 | 19,395 |
| ported (a thin caller of Rust) | 20 | 2,503 |
| retired-pending (retired tools' folders) | 3 | 310 |
| stays (benchmarks, parity and dev scripts) | 50 | 7,975 |
| test (tests and helpers) | 187 | 49,719 |

The port work left (live, partly and twin, by folder, non-test): NavKit 10,704 lines (live 1,872, partly 5,879), the
shader tools 6,414 (live 1,430: `pcbuild`, `xdiff`, `xenos_exec`), the BF1943 importer 5,865, core 5,012, the menu
tools 4,407 (`aptgraft` 2,941 partly), console-convert 3,102, the harness 2,525, stock-MP 2,403, map-editor's L1
scripts 1,282, anim-tools 1,214, culltree 1,042, model-tools 1,022, console-readers 835, mesh-viewer 603. ROADMAP's
"Python left" line counts are of 2026-11-21 and folder-wide; the inventory's are per file and current.

**Found on the way** (no change made; for the rounds that own them):

- `bfbc2.exe` embeds 23 of the Python files with `include_str!`: the docstring is the usage it prints (`aptgraft`,
  `shaderdb`, `navkit.py`, `hpdtool` ...) or the file's hash keys a cache (`culltreelib.py`, the row cache). Editing
  one changes the binary's output; retiring one needs its text moved first (as RT3 did with `*_usage.txt`). The
  inventory marks them "embedded".
- "Every command line `bfbc2.exe` answers is Rust" holds for the argument handling; the main job of `navkit build /
  all / apply (NavKit base) / sample`, `navparity run`, `navregress`, `hpdview`, `sample_all`, `harness run`, `texqa`'s
  sheets, `renderqa`'s draws, `xdiff`, `bf1943import import / check`, `dbxparity` and `portparity` is still handed to
  the Python.
- Python only tests reach: the stock-MP library (`stockmplib`, `pipeline`, `validate`, `author`, `config`,
  `levelmodel`; its command line went in RT3; `dbxbin` is also levelwrite's reference), `xiso/xisolib.py`,
  `aptrefslib.py`, `navdomains.py` with `navwater_mesh.py` (prototypes), `navboundary.py`, `navsample_offline.py`.
- Nothing calls `tools/hpdview.py` but `bfbc2.exe hpdview`'s hand-off, nor 12 of L1's exploration and one-off check
  scripts (their output is not kept); `l1_show.py` and `l1_try_hf.py` start with a UTF-8 byte-order mark.
- The editor's test hang guard runs `rust\hang_stacks.py` (a Rust test needs Python); `gen_argspecs.py` needs the
  Python tools' argparse parsers alive.
- `rust\build_native.py --help` reads no options: it builds and installs (here it reinstalled main's up-to-date module
  and `bfbc2.exe`, the same sources).
- Out of the brief's `*.py` scope: `BFBC2 Tool Suite.pyw` (the hub) and `bfbc2-auto-navkit\navkitGUI.pyw`.

Proof: two runs (one with `PYTHONHASHSEED=7`) wrote the same bytes; `tests/test_repo.py` passes, fails naming the file
with a stray `core/stray_pyi_check.py`, and fails naming both with a `.py` renamed (the row's file gone, the new one
unclassified); ruff and mypy clean on the generator.
## SPG2: the rest of the specification's gap pass (2026-11-22, branch `spg2`)

A spec-writing round: no tool behaviour changed. Items 1 - 11 of `spec/COVERAGE.md` section 2 (as SPG left it)
written, and the probes' first game-tier check. Nothing written to `game\`; the game never launched. The worktree
built its own `bc2_native` (`python rust\build_native.py`, the shared build folder); the probes and the check ran
against it.

### Coverage, before and after

| | rules | files | covered | thin | none |
|---|---:|---:|---:|---:|---:|
| before (SPG + AIS) | 1,051 | 59 | 24 | 12 | 0 |
| after | 1,169 | 68 | 34 | 2 | 0 |

New rules (verified / implemented / observed / inferred): `transforms/terrain_edit.md` 15 (7 / 2 / 0 / 6),
`engine/vegetation.md` 8 (3 / 1 / 4 / 0), `transforms/shaders.md` 14 (3 / 9 / 2 / 0), `engine/module.md` 15
(0 / 14 / 1 / 0), `engine/animation.md` 17 (3 / 5 / 8 / 1), `engine/decals.md` 6 (1 / 1 / 3 / 1), `engine/onslaught.md`
14 (0 / 7 / 7 / 0), `engine/streaming.md` 7 (0 / 1 / 6 / 0), `formats/bf1943.md` 7 (0 / 1 / 6 / 0),
`engine/persistence.md` +12 (3 / 1 / 8 / 0), `engine/effects.md` +3 (2 / 0 / 1 / 0); DSTR-TERR-4 observed -> verified.

### The probes and the check (read-only, on the game copy's 444 DICE level bundles)

| | reads | time |
|---|---|---:|
| `spec/probes/terrain.py` | 30 terrains: height fields, visual terrains, mask atlases, terrain partitions; 74 level partitions' vegetation and splines | 23 s |
| `spec/probes/effects_chain.py` | 50,141 per-level emitter partitions: chains, classes, evaluators | 35 s |
| `core/tests/test_spec_probes.py` (`run_tests.py game`, `spec-probes:game`) | both, asserting the stable counts of 14 rules | 59 s |

### Findings that change planned work

- **E5 / E7 (terrain)**: DICE's mask-tile flags run 2 - 64 (not only 4 and 8: TERR-VT-7 corrected), one tile is shared
  by several blocks on 8 terrains, and 9,590 rule-only cells keep a leftover flag (TEDIT-VT-1): a painter must handle
  all of these. The height range is per level (500 m on 16 of 30, up to 1,700 m): RNDR-TERR-5's "every stock range is
  500 m" was wrong; the engine writes the scale, so a sculpt may widen the range without touching shaders. The terrain
  partition is DICE's editor source (1,186 decal references, none in any bundle): an edit writes the resources.
  E7 still cannot write a main block (its tail is undecoded).
- **E6 (vegetation)**: forests and middlegrowth are ordinary model groups (SG1's member edits apply); undergrowth has
  no placement data at all - it is the patch shaders' mesh entries, one undergrowth mask shader exactly when meshes.
  **The suite's visual-terrain reader misreads NAM_MP_003** (its undergrowth entries carry a second string the reader
  takes as one byte): a reader fix is queued in SPG2-rest.
- **Pacific War**: BF1943's award table, ranks and stings are specified (PERS-WW2-1 - 4); PERS-SCORE-1 (RE of
  2026-11-15) says bots' deaths are scored in the round with the scoring patch, contradicting AI-NEED-5: a kill-probe
  playtest decides before the award engine's scoring work.
- **Onslaught**: the game copy's ONS_MP_002 is OPFOR's console port, not Napisal's (WX1's attribution corrected); its
  first stage dropped DICE's 53 night lamps, the moon, the lighthouse swap and the night ambience - a faithful re-port
  must convert those resources instead (ONS-PORT-3). Matchmaking CONQUEST is deliberate (stats), not ONSLAUGHT.
- **The game module** uses 221 addresses (not about 170) on build 795745 with no version check beyond 21 signatures
  and per-site bytes; two kinds of slot write are unchecked (MOD-HOOK-3).

### Left

`spec/COVERAGE.md` section 2 (queued as SPG2-rest).

## NK4: NavKit's walkability end to end (2026-11-22, branch `nk4`)

NK1's plan, stage 3 of the build: everything `navbuild.stage` did from the sample file to the kept surfaces. The
build's largest memory peaks came from this stage (PR1b's "NavKit regression's 15 GB"): the pitch-0.2 runs of the
detailed pass (100-650 MB files, up to 30 M surfaces) went through NumPy's masks, gradients and loops.

### What moved

| Python (retired) | Rust | where it runs |
|---|---|---|
| `navbuild.stage`'s walkability (`navsample.load`, `coarse`, `navedits.edits_for`, `deep_water`, `outside_bounds`, the forced flags folded together, the high and narrow runs, `walkable`, the ladders' kept ends) | `bc2_navbuild::walk::stage` | `bc2_native.navkit_walk_stage`, called once per stage by `navbuild.stage` |
| `navwalk.walkable`'s NumPy glue: the masks (clear, characters, slope, terrain), the gradients and their `np.where`s, the steep-terrain and lip rules' wiring, the stairs and landings, `raised_bars`, the narrow capsule, the forbidden and dynamic objects, the headroom band, `DYNAMIC_FLOOR`, the edge seeds (`round(x % SECTOR, 1)`), the seed picks, the floods, the ladders' re-floods, the unseeds, the roof islands with their anchors | `walk::walkable` | `navwalk.walkable` (a thin call: `navkit_walkable`) |
| `navwalk.near_points`, `ladder_ends`, `objective_floor`, `coarse` | `walk::near_points`, `ladder_ends`, `objective_floor`, `Samples::coarse` | `navwalk.near_points` / `ladder_ends` / `coarse` (thin calls); `objective_floor` inside `walkable` |
| `navedits.region_mask` (NumPy and matplotlib's `Path.contains_points`), `edits_for`'s masks | `walk::region_mask_at`, `edit_masks`, `points_in_polygon` | `navedits.region_mask` / `edits_for` (thin calls); `navedits.regions_of` parses a side file's operations |
| `navbuild.water_tops`, `deep_water`, `outside_bounds` | `walk::water_tops`, `deep_water`, `outside_bounds` | the same names (thin calls; `navvehicle` and `navwater_mesh` call `water_tops`) |
| `navsample.load` of a version 3 file: the whole file in memory, then the arrays | `walk::Samples::load_v3`: the counts from the trailer, then the records a block at a time | `navsample.load` (`navkit_navsample_stream`); versions 1 / 2 and damaged files as before |

The rules `walkable` calls were ported before (E2a, E3c, E3h: `rules::Grid`, `Level::flood`, `graph::roof_drop`);
NK4 ports the glue between them, so a stage hands the core the file names and the inputs once and gets back the
run, the kept flags, the gradients, the stair flags, the floods' `NavLevel`, the statistics and the counts the stage
logs. `rules::Grid` now shares its arrays (`Arc`) with the floods' level (`Level::on_grid`): the heights, column
starts and surface columns are held once.

What stays Python until NK5: `navbuild.Level` (its `y`, `col_of`, `terrain` from the run), the rest of `stage`, and
the module settings, read at each call (`navwalk._settings`: the tests switch them).

### Python's order and floats

Nothing a set or dict iterates reaches the answer: `anchor_k` (the roof islands' anchors), the sector-line columns
`per_i` / `per_j` and the ladders' `done` are only tested for membership (the roof rule marks an island anchored; the
order of the anchors does not matter, `graph::roof_drop`); the hand edits' counts are the side file's operations in
order. No NumPy reduction over many floats: the sums count flags (exact), `np.median` of the objective floor's
heights takes the mean of at most two, `np.unique` sorts. So no order rule was needed.

The floats are the Python's, bit for bit (the arrays below):

- **NumPy 2's comparisons (NEP 50).** A Python float compared with a `float32` array or scalar is cast to `float32`
  first: the normals' slope tests (`normal[:, 1] >= cos(radians(slope)) - 0.01`, the 55-degree steep limit, the
  lips' 15-degree flat test) and the ladder ends' height bands (`y0 <= s.y[k] <= y1` on the run's `float32`
  heights); likewise `on_objects`' bounding-box filter on `float32` triangles. Measured: for a sample file's normals
  (k / 127 as `float32`) the `float32` limit and the `float64` one decide the same for all 255 values at all three
  limits, so the cast decides nothing there. The ladder bands are NavKit's own rule (DICE's path data has no ladder
  links; parity treats them as outside), so they keep the run's precision as a spec rule (XNAV-BLD-4) rather than a
  DICE question.
- **Dtypes.** The gradients are `float32` from the normals (`-n.x / max(n.y, 0.05)` in `float32`) until a rule
  replaces some (`np.where` with `float64` slopes: `float64`); a build always runs the stair rule, so its gradients
  are `float64` (as before).
- **Python's arithmetic.** `round()` half to even for column indices, `%` with the divisor's sign and `round(x, 1)`
  (the correctly rounded decimal) for the sector lines, `math.hypot` (CPython's), `min` / `max` taking the first of
  equals, tuples ordered `(not kept, distance)` for the ladder ends; the trigonometric constants are computed in
  Python (the C library's `cos` / `tan`, as before) and passed in.
- **matplotlib's crossing test.** A polygon region's points on its outline are in or out as matplotlib 3.11 decides
  (`_path.h`'s `point_in_path_impl`: half-open edges, the closing edge back to the first vertex). The port follows it
  and is checked against matplotlib: 400 random polygons, half with vertices on a 0.5 m lattice so sample points fall
  on edges and vertices, some self-crossing, on a 0.25 m point lattice: 0 differ (`test_navedits.py`). Polygons with
  fewer than 3 vertices contain nothing, as matplotlib's.

One behaviour is not the Python's: a version 1 run (no hit words) has `hit` None. `navsample.load` gave an empty
`(0, 3)` array, on which `walkable` and `navbuild.Level` failed (a broadcast error), so no build could use such a run
before; now it builds with every surface counted as terrain, as a run without hit words always was in `coarse`.

### Parity

- **Every regression level, against the old Python** (`rust/parity_nk4_walk.py capture`: each level's whole build
  through `navregress.build_level`, the build clock fixed, once with a checkout of `30f19b0` - the Python before NK4
  - and once with this branch, one level at a time, the two interleaved; `compare`): **15 of 15 identical**: per
  stage (the normal pass and the detailed pass, 30 stages) the run as `navbuild.Level` gets it (`start`, `y`,
  `normal`, `flags`, `hit`, pitch, grid) and `keep`, `gx`, `gz`, `stair` with their dtypes, as SHA-1s; `STEEP_STATS`;
  every log line (the seconds blanked); and the path data's SHA-1. The 4 golden builds (sp_009, sp_004_b, sp_008_b,
  ons_mp_004) as recorded.
- **XNAV-BLD-3**: the files are byte-identical to the Python's, so every navparity measure is unchanged by
  construction; no regression reference changes.
- **NK1's apply parity**: 33 of 33 (`rust/parity_nk1_apply.py`; the apply's edit stage is untouched).
- **The quick tier's recorded answers** (`test_navbuild_native.py`: E2a's 331 answers - `walkable` with every rule
  on random runs, in `float32` and `float64`, among them -, E3c's 456, E3h's 39 whole stages): all as recorded.
  `test_navedits`, `test_water_rule`, `test_reference_clip`, `test_headroom`, `test_lips`, `test_roof_prune`,
  `test_objective_floor`, `test_characters`, `test_dynamic_props`, `test_apply`: pass. Rust: `walk_tests.rs`, 7.

### Time and memory

Each level's whole build, old and new interleaved (one process each, one at a time; other agents' builds beside
them). Walkability: from the stage's start to `navbuild.Level` (old: `navsample.load` ... `walkable`; new: the one
call). Memory is the process's private bytes (committed memory, sampled every 10 ms; the working set understates a
peak when Windows trims pages under load); "above its start" is the stage's peak less what the process held when the
stage began (the normal pass's results).

| level | surfaces 0.8 / 0.2 | walkability 0.8, old -> new (s) | walkability 0.2, old -> new (s) | walkability 0.2 peak above its start, old -> new (MB) | build peak, old -> new (MB) |
|---|---:|---:|---:|---:|---:|
| ons_mp_002 | 0.8 M / 9.6 M | 0.83 -> **0.45** | 6.77 -> **2.66** | 3,559 -> **1,659** | 4,419 -> **3,920** |
| ons_mp_004 | 0.5 M / 5.1 M | 0.30 -> **0.13** | 2.59 -> **1.19** | 1,811 -> **750** | 3,140 -> **2,856** |
| sp_003_b | 5.0 M / 11.2 M | 2.58 -> **1.36** | 8.48 -> **3.62** | 4,315 -> **1,919** | 6,337 -> **5,545** |
| sp_004_b | 0.5 M / 7.7 M | 0.30 -> **0.21** | 4.24 -> **1.85** | 2,753 -> **1,114** | 3,703 -> **3,396** |
| sp_005 | 4.3 M / 23.4 M | 2.03 -> **1.52** | 24.23 -> **8.26** | 10,081 -> **4,222** | 12,253 -> **10,181** |
| sp_005_b | 4.3 M / 18.8 M | 2.00 -> **1.13** | 14.32 -> **5.18** | 7,906 -> **3,193** | 9,989 -> **8,169** |
| sp_006 | 5.9 M / 26.4 M | 3.65 -> **2.13** | 39.71 -> **8.87** | 12,507 -> **5,139** | 15,204 -> **11,862** |
| sp_007 | 4.4 M / 30.2 M | 3.27 -> **1.82** | 35.30 -> **11.14** | 12,774 -> **5,622** | 14,627 -> **9,652** |
| sp_008 | 0.9 M / 17.6 M | 0.81 -> **0.49** | 10.14 -> **4.56** | 6,200 -> **2,550** | 7,005 -> **4,828** |
| sp_008_b | 0.6 M / 13.0 M | 0.34 -> **0.22** | 7.83 -> **2.78** | 4,374 -> **1,764** | 5,082 -> **3,796** |
| sp_009 | 0.1 M / 1.7 M | 0.05 -> **0.03** | 0.84 -> **0.36** | 808 -> **332** | 1,365 -> **1,252** |
| sp_01 | 2.4 M / 16.0 M | 1.32 -> **1.09** | 10.76 -> **5.99** | 6,122 -> **2,653** | 7,367 -> **5,610** |
| sp_02 | 1.8 M / 17.1 M | 1.09 -> **0.48** | 14.51 -> **4.37** | 6,930 -> **2,741** | 8,036 -> **5,324** |
| sp_03 | 4.8 M / 22.1 M | 2.73 -> **1.70** | 18.07 -> **7.40** | 9,258 -> **4,224** | 11,271 -> **7,945** |
| sp_04 | 3.4 M / 22.3 M | 1.30 -> **0.79** | 13.95 -> **6.56** | 9,487 -> **3,931** | 11,181 -> **8,739** |

| | old Python | Rust | |
|---|---:|---:|---:|
| walkability, 30 stages | 234.3 s | **88.3 s** | 2.7x |
| the detailed pass's walkability, largest (sp_006, 26.4 M surfaces) | 39.7 s | **8.9 s** | 4.5x |
| the walkability's own peak, largest (sp_007, 30.2 M surfaces) | 12.8 GB | **5.6 GB** | 2.3x less |
| the whole build's peak, largest (sp_006) | 15.2 GB | **11.9 GB** | |
| whole builds, 15 levels | 483 s | **310 s** | |

**Where the peaks come from.** Before NK4 the detailed pass's walkability was the build's peak on 12 of 15 levels
(all but the three smallest). Its own peak is now 2.2-2.5 times lower, and on no level the build's peak, which is
now later in the detailed pass: the stage's areas, tracing and one-way portals, which NK5 moves (sp_04: the
walkability call 5.1 GB, the areas 6.2 GB, the one-way portals 6.6 GB, in working set). A stage's walkability now
holds: the run (33 bytes a surface and 8 a column: heights as `float64`, normals, flags, hit words), the high run's
heights and flags (9 a surface), the floods' level (the shared heights and columns, the gradients as `float64`, three
flags), the masks (a byte a surface each) and one `stair_slope` answer at a time (24 a surface); a version 3 file is
read a block of records at a time, never whole (before: the file's bytes beside the arrays, twice - the run and the
high run). The NumPy version held many `float64` / bool temporaries of every mask expression at once.

### Retired

`navwalk.walkable`'s body (its NumPy glue), `near_points`, `ladder_ends`, `objective_floor`, `coarse`'s loop and
`_column_picks`; `navedits.region_mask`'s NumPy / matplotlib body and `edits_for`'s mask loop; `navbuild.water_tops`',
`deep_water`'s and `outside_bounds`' bodies and `stage`'s walkability block. Python twins inside a living tool
(RETIRED.md rule 4: recorded here and in CHANGELOG, not as RETIRED.md rows); the last commit with them is `30f19b0`.
The functions stay as thin calls with the same arguments and answers; no command line went, and nothing outside
NavKit imports them (Project OPFOR has no caller of navwalk, navedits or navbuild), so `VERSION` stays (RELEASING.md).
navedits no longer imports matplotlib.

### What is left: NK5

NK5 as NK1-NK3 planned it: the stage (sector discovery, specs, border-node runs, carried seam nodes, the one-way
drop, the detail loop with `detail_pass`'s glue, graph jobs and ids, the meshes' assembly), `merge_detailed`, the
driver's `build_inputs`; then `navkit build` / `all` / `apply` on a NavKit base in Rust, the editor's AI workspace in
process, `navbuild.py` retired after its other callers move. For NK5 from here:

- `walk::stage` already returns the run and the floods' `Level` in Rust: NK5's stage can keep them there
  (`navbuild.Level`'s `y` / `col_of` / `terrain` and the `NavLevel` rebuilt with `with_walk` are copies the core
  already holds), and the run need not cross to Python at all (today it does, as `navsample.Samples` over the core's
  bytes, for `Level`, `column_sectors`, `merge_detailed`'s seed points and navvehicle).
- The build's peak is now the detailed pass's areas and one-way portals (above): NK5 should measure them the same way
  (`parity_nk4_walk.py`'s sampler).
- `merge_detailed` passes stage a's kept ground in the detailed sectors as Python tuples (0.26-0.85 M seed points);
  in Rust it is a mask.
## PR1b-0: the map editor's core before E4 (2026-11-22, branch `pr1b0`)

Audit AU5 section 6's PR1b-0: ALG-1, ALG-3, ALG-4, ARC-6, ARC-7, ALG-2, ALG-8, ALG-9. The rule: every result stays the
same, bit for bit. Measured on the development PC (16 threads, 15 GB) with other rounds' builds beside it; release
builds; `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=4`, build folder `rust-targets\bfbc2-tool-suite-pr1b0`. The "before"
column is main at 30f19b0 built from a throw-away worktree, run back to back with "after". Nothing was written to
`game\`; the game was never launched.

### The numbers (`examples/editor_core_bench.rs`, `ray_bench.rs`, `preview_bench.rs`)

`editor_core_bench <OPFOR> <level> 200 100`: the scene's clone, a document, 200 moves, three undos, three redos, History
to the middle, the base and the end (the first ray straight down after each), then 100 static group members moved and
100 deleted (each checked by `Doc::refusal` first, as the editor does) and one undo. The heap is counted by a counting
global allocator. Every state's digest (partition bytes, each edited group file, every transform, the deleted set)
and the run's whole digest are the same before and after: SP_01 `bb797793...`, Wake Island `145c4c0e...`.

| | SP_01 before | SP_01 after | Wake before | Wake after |
|---|---:|---:|---:|---:|
| base scene clone | 18.6 ms, 56.8 MB | 7.2 ms, 9.4 MB | 8.8 ms, 50.5 MB | 4.0 ms, 5.0 MB |
| undo / redo / History (9 steps) | 47 - 77 ms | 14 - 18 ms | 22 - 29 ms | 7 - 12 ms |
| first ray after each of them | 114 - 123 ms | 0.58 - 0.67 ms | 105 - 113 ms | 0.34 - 0.37 ms |
| heap after 200 moves (8 checkpoints) | +279 MB (peak +314) | +73 MB (peak +82) | +92 MB (peak +104) | +37 MB (peak +41) |
| document made | 257 ms, +106 MB | 172 ms, +41 MB | 67 ms, +66 MB | 53 ms, +14 MB |
| member delete: refusal check, median | 90.5 ms | 3.7 ms | 14.6 ms | 1.4 ms |
| member delete: commit, median | 72.1 ms | 0.5 ms | 9.8 ms | 0.1 ms |
| 100 refusals + 100 deletes | 17.98 s | 0.49 s | 2.54 s | 0.18 s |
| undo after the member edits | 1,117 ms | 59 ms | 285 ms | 33 ms |
| `members::group_entities` | one partition walk per member type | 2.2 ms | | 0.6 ms |

The heap after 200 moves is AU3 PF-4's and AU5 ALG-4's question: 8 checkpoints held a whole copy of the partition's
writer each (26 MB on SP_01) plus their entities. With the writers' elements in shared chunks a checkpoint holds the
chunks edited since the one before (a copy of the writer now costs 0.0 MB and 0.0 ms until it is edited), so the 8
checkpoints are bounded by what changed, not by the level's size: no further bound was needed.

Rays (`ray_bench <OPFOR> <level> 400`; digests the same: SP_01 `d8c898ee...`, Wake `770ecf88...`):

| | SP_01 before | after | Wake before | after |
|---|---:|---:|---:|---:|
| vertical ray (`ground_below`) | 0.581 ms | 0.604 ms | 0.493 ms | 0.492 ms |
| picking ray | 0.675 ms | 0.640 ms | 0.554 ms | 0.502 ms |
| `surface_below` | 0.293 ms | 0.290 ms | 0.231 ms | 0.202 ms |
| picking ray, terrain only | | 0.254 ms | | 0.249 ms |

The asset preview's orbit at 1200 x 900 (`preview_bench 1200 900 30`; digests the same): the guard tower 21.6 -> 6.9
ms a picture (median), the 29,048-triangle soldier 23.7 -> 7.8 ms, under a 60 Hz frame on the UI thread.

### 1. ALG-1: the terrain, its ray grid and the base scene shared

`Scene::terrain` and `Scene::heightfield` are `Option<Arc<..>>`, the scene's partition bytes an `Arc` (private); a
clone shares them. `RayCache` is shared by a scene's clones and keeps up to 4 terrain grids, each keyed by its mesh's
`Arc` (held weakly: a mesh no scene holds is freed, and its grid goes at the next look-up), so the base and every
undo's scene build the grid once. `Scene::with_entities` makes an undo's scene from the base without copying the
base's entities (it marks the moved ones changed as `set_transform` did). For E5: a stroke that edits the terrain
replaces the mesh's `Arc` (a new grid then); updating only the grid cells a stroke's tiles cover is E5's
(`raygrid::TerrainGrid` buckets triangles by plan cell, so the cells of a tile are known).

### 2. ALG-3: member edits applied once, the group entities found by one index

- `LevelFiles::partition` and `groups` hold `Arc<LevelWriter>`; `apply` stages on `Arc` clones and `Arc::make_mut`
  copies a writer only when an edit changes a shared one.
- `bc2_formats::dbx::edit::EditDbx` keeps its elements in chunks of 1,024 behind `Arc`s and its file bytes and string
  table in `Arc`s: a clone costs the chunk pointers, an edit copies one chunk.
- `Doc::refusal` keeps the files its trial made (the document's revision, the operation, the files); a commit of the
  same operation at the same revision takes them instead of applying it again.
- The base partition is parsed once per document (`Doc::base_writer`), not again on every replay from the base.
- `members::group_entities` found each member type with `LevelWriter::find`, a walk of every instance with a string
  upper-cased per instance: one walk per member type, per member edit. It now builds one guid index per call.
- Not done: one `Schema` per group file for `GroupField` edits (such an edit still scans its file; no tool makes them in
  bulk yet), and a group-entity index kept across calls (it would need the writer to say when an instance is added or
  removed; the per-call index already takes it from 169 ms to 2 ms on SP_01).

### 3. ALG-4: checkpoint memory measured and bounded

See the table: +279 MB -> +73 MB on SP_01 after 200 operations (Wake +92 -> +37 MB), from the chunked writers and the
shared scene. `CHECKPOINT_EVERY` (16) and `CHECKPOINTS` (8) are unchanged.

### 4. ARC-6: the tool trait

`bc2_editor::tool`:

```rust
pub trait ViewTool {
    fn id(&self) -> Tool;                                    // gizmo::Tool: what the actions and the toolbar name
    fn key(&self) -> Option<&'static str> { None }           // its toolbar key
    fn gizmo(&self) -> bool { false }                        // moves / turns with the shared gizmo drag
    fn boxes(&self) -> bool { true }                         // a left drag on empty ground draws a box
    fn input(&mut self, p: &Pointer, v: &ViewState) -> Option<Outcome> { None } // before the editing input
    fn click(&mut self, hit: Option<Hit>, snap_vertex: bool) -> Option<Outcome> { None }
    fn draw(&self, d: &Draw<'_>, layer: Layer) {}            // what it holds, at Layer::Edit or Layer::Tools
    fn takes_enter(&self) -> bool { false }
    fn enter(&mut self) {}
    fn transient(&self) -> bool { false }                    // Esc cancels it first
    fn cancel(&mut self) {}
    fn left(&mut self) {}                                    // another tool was chosen
}
pub enum Outcome { Done, Message(String), Select(Vec<usize>, String), Commit(Box<Op>, String) }
```

`Tools` holds every tool's state (`ViewState::tools`; `ViewState::tool` is the chosen id): Select, Move and Rotate
(the gizmo's drag, handles, snapping and surface placement stay shared in the views), Ruler (its points, read-out and
drawing) and Polygon (its corners, double click and Enter, its selection). The toolbar is built from the tools' keys;
Esc and the transient check ask every tool; a tool chosen tells the others. `ViewState::outline` (`OutlineDraw`) and
`Measures::ruler` are gone (CHANGELOG Removed). Tests: `tool::tests` (3), beside the editor's. Not moved (not view tools
yet): placing from the content browser and the gameplay tiles, the sink popup and the picker, the AI workspace's nine
`NavTool`s (in `navui`), the measuring path (M). E4's flags, spawns, M-COMs and teams should be new implementors and
a line in `Tools`; the AI tools are the next to move. `export::build` and `export_ui::window` were not split (ARC-6's
second half; E4 does not need it).

Game layer (the user's later targets: MOH 2010, Warfighter, BFBC1, BF3, BF4): the trait and `Task` assume no BFBC2
format. Two BFBC2-specific points sit in the editor core: `Outcome::Commit` carries `bc2_levelwrite::Op` (a game layer
would put an operation type per game behind the level API there), and tools see the `bc2_scene` scene through
`ViewState` (a game-neutral scene trait would go in `bc2_scene`).

### 5. ARC-7: one background-job type

`bc2_editor::task::Task<T>`: `spawn(what, body)` / `spawn_value`, a thread named `bc2-editor: <what>`, the body under
DF1's `guarded` (a panic is an error naming `what`), one answer on its own channel; `poll`, `wait`, `wait_for`; a
worker that ended without an answer is `EditorError::Worker`. On it: the comparison (`compare::DiffState`), the content
browser's catalogue, the game-accurate view's world (`GameLoad`), the asset preview's reader and its "Read the mesh
index" button, the export's preview (`diffui`) and round A1's auto layout (merged in with main 350ba30): 7 of the 19
thread spawns. Left with their own threads, each
already guarded or answering once: the export window's four (`export_ui`: they share `answer()`, and tests inject
their receivers through `running_from`), the level load and its two helpers in `app.rs`, the AI workspace's two
(`nav.rs`: a job with a cancel flag and streamed output) and `prepare`'s three (a tool's stdout and stderr read while
it runs: these stay threads). Nothing bounds how many run at once: the bounded pool is PR1b-2 item 11 (ALG-11).

### 6. ALG-2, ALG-8, ALG-9

- **ALG-2.** An object ray takes the blueprints' shapes under one lock per ray (it was a lock and a look-up per placed
  object) and, for an object its sphere does not reject, tests in the object's own space only the runs of 16
  triangles whose bounds (padded by 1e-4 m plus a millionth of their size) its path meets, placing those triangles as
  `world_triangles` does, without a list. The hits are the same (digests above). The gain is small (picking 0.675 ->
  0.640 ms on SP_01): what is left of an object ray is the loop over every placed collision mesh (5,924 on SP_01) and
  its blueprint's look-up by name; a grid of the objects' spheres needs the scene to say when an entity moved (E4's
  placement tools, with an edit generation on `Scene`).
- **ALG-8.** The DBX inspector's undo stack keeps at most `UNDO_BUDGET` (256 MB) of steps, the oldest going first (the
  file stays modified); an edit clones the file and serialises it only when the edit changed it; a project field's
  trial copies the file, not the inspector's undo, script and output. Test
  `dbxinspect::tests::the_undo_stack_keeps_its_budget`.
- **ALG-9.** `Canvas::fill_parts` draws every part in horizontal bands (32 rows or more, at most 64 bands) on all cores,
  each band running `Canvas::fill`'s triangle loop in the same order on its own rows: the same picture, depths and
  owners. The preview's orbit uses it; Save picture and the parity tests keep `fill`. The cache key is still a
  `format!` of the state (well under a millisecond).

### Proof of identical behaviour

- Tests: bc2-editor lib 76 and `editor` 53 run (25 game tier ignored); the game-tier tests this round touches, 13 of
  13: `xf1::undo_on_a_mode_project`, `group_members::wake_members_through_the_interface`,
  `real_level_editing::placement_on_real_terrain`, `dbx_inspector_window::`,
  `dbxedit_parity::the_inspector_gives_dbxeditguis_answers`, `editor_core::stroke_budgets_on_10000_strokes`,
  `view_toggles::` (3), `problems_panel::`, `inspector_parity::`, `every_level::`, `game_view_live::`; bc2-levelwrite's
  `export_xf1::` (2) and `model_group_members::` (2); bc2-formats, bc2-scene, bc2-levelwrite and bc2-convert's crate
  tests (the replay corpus's `project::tests::old_projects_load_and_replay_the_same` among them).
- Screenshots of the main views (`bfbc2-editor --screenshot`, 1600 x 950): the start screen dark, light and WW2, the
  demo level dark, light and with a selection, MP_005, SP_01's navigation view and Wake Island: byte for byte the same
  before and after. MP_005's differs between two runs of the same binary in 41 pixels of the status bar (a reading
  indicator); the second "after" run equals the "before" run byte for byte. The screenshots stayed in the session's scratch
  folder.
- The project-log replays: the bench's state digests through 200 moves, undo, redo, History and 200 member edits, and
  the corpus replay test above.

## A2: the auto layout's vehicles, control and the export's path-data step (2026-11-22, branch `a2`)

Round A2 (ROADMAP "A2"): the auto layout's items that need no path data - vehicles and control - and the export's
path-data step as far as NavKit's port allows (the rest is A2-rest). Nothing was written to `game\`; the game never
launched; no NavKit build ran.

### Vehicles: what DICE's need around their spawners (`bc2_levelwrite::vehicles`, `site`)

New site measures: a land vehicle's **way out** (`Site::way_out`: a flood fill on 2 m cells from the spawner's surface,
a step up to 1.2 m, a drop up to 2 m, nothing static 0.6 to 2.5 m above the surface, water up to 1.2 m), a boat's
**water reach** (`water_reach`: 4 m water cells at least 0.5 m deep, nothing static at the surface), an aircraft's
**open sky** (`open_sky`: the lowest collision 3 m or more above the spawner within a radius) and a plane's **take-off
run** (`runway`: the longest straight run on 16 bearings, a 10 m span, no step over 1 m, no water, nothing crossing 0.6
to 5 m up). `examples/vehicle_sites.rs` (bc2-editor) measured every vehicle spawner of the 27 levels A1 calibrated on
(707; 291 of vehicles that move):

| class | spawners | the tightest of DICE's | the rule's bound |
|---|---:|---|---|
| tank, IFV, IFV-AA, car, quad bike | 184 | every one drives 40 m (the measure's limit) | XVAL-PLACE-6: under 20 m |
| boat (where the water mesh is judged) | 44 | 30.5 m of open water (`mp_009gr`) | XVAL-PLACE-7: under 24 m |
| helicopter | 49 | no collision under 60 m within 3.5 m (one Vietnam pad, `nam_mp_005r`, has some 3.1 m up 7 m out) | XVAL-PLACE-8: under 12 m within 3.5 m |
| plane (WW2 ports) | 15 | a 34 m run (`guadal_canal`); one of OPFOR's planes on `iwo_jima_s` stands inside a hangar's solid collision, so the run counts crossings, not solids | XVAL-PLACE-8: under 30 m |
| **XVAL-PLACE-2 to 8 on the 27 levels** | | **0 findings** (`a1_calibrate`: 1,475 spawn points, 707 vehicle spawners; `vehicle_sites`) | |

DICE's spawners a capture point lists all spawn by themselves (`AutoSpawn` true on 172 of 172); 64 unlisted ones wait
for events. Flags' vehicles are neutral spawners or one per team; every stock Squad Deathmatch level's one IFV
belongs to the layer's neutral capture point, 5-19 m from it. New spec rules: GAME-VEH-2 (`engine/gameplay.md`),
XVAL-PLACE-6 to 8.

### Vehicles in the auto layout

`vehicles::plan`, on the laid-out layer: each capture point gets DICE's count (L1's medians, `l1_a1_params.json`: a
Conquest base 4, 8 on WW2; a flag 2; Rush's stage-1 attackers' base 3, defenders' 2; Squad Rush none; Squad
Deathmatch 1), from the level's own spawners no capture point of the layer owns and that spawn by themselves (DICE's
for the level: the base's own layer has gone), by team (a flag: a neutral spawner, else one of each team) and class in
DICE's order (L1's class totals), each moved once, then copies (`Op::Place`), each on the first good spot of rings 12
to 88 m out (boats 8 to 148 m, aircraft from 20 m) facing the layout's centre (a plane its run), joined to its capture
point (`Op::LinkSpawner`, which now takes vehicle spawners). The proposal commits as one operation (`Proposal::op`:
`Op::Files` with the moves, the copies, the links; the log's base check now reads a multi-part operation's parts in
order, so a link may name a copy made before it). `placement::measure` gains L1's vehicle pools (`cq.hq_vehicle_pool`,
`cq.flag_vehicle_pool`) and Rush's per-stage vehicles; the auto layout holds them against DICE's ranges.

**The 16 held-out levels** (`examples/a1_layout.rs`, as A1: every donor of the era and mode but the level's own map,
the best chosen without DICE's layout; release):

| level | mode | donor chosen | errors / warnings / notes | statistics inside DICE's ranges | vehicles (copies) | classes | s |
|---|---|---|---|---|---|---|---|
| MP_001 | CQ | MP_005 | 0 / 0 / 0 | 35 of 35 | 14 (6) | 8 ifv, 2 quad, 4 tank | 3.9 |
| MP_003 | CQ | MP_001 | 0 / 0 / 0 | 42 of 42 | 16 (10) | 12 ifv, 4 quad | 1.9 |
| MP_005 | CQ | MP_009CQ | 0 / 0 / 0 | 35 of 35 | 14 (2) | 2 car, 2 heli, 2 quad, 8 tank | 2.5 |
| MP_006CQ | CQ | MP_001 | 0 / 0 / 0 | 42 of 42 | 16 (14) | 16 quad | 2.8 |
| MP_009CQ | CQ | MP_001 | 0 / 0 / 0 | 39 of 39 | 4 (0) | 1 boat, 1 ifv, 2 quad | 2.3 |
| NAM_MP_002CQ | CQ | NAM_MP_005CQ | 0 / 0 / 0 | 33 of 33 | 6 (4) | 6 tank | 2.6 |
| MP_005GR | Rush | MP_008 | 0 / 0 / 0 | 37 of 37 | 5 (0) | 1 quad, 4 tank | 6.1 |
| MP_009GR | Rush | MP_004 | 0 / 0 / 0 | 37 of 37 | 3 (0) | 1 car, 1 ifv, 1 quad | 3.5 |
| MP_012GR | Rush | MP_008 | 0 / 0 / 0 | 37 of 37 | 5 (0) | 1 heli, 2 ifv, 1 quad, 1 tank | 6.2 |
| NAM_MP_005R | Rush | NAM_MP_003R | 0 / 0 / 1 | 37 of 37 | 3 (0) | 3 boat | 2.9 |
| MP_002SR | SR | MP_001SR | 0 / 0 / 0 | 19 of 19 | 0 | - | 1.7 |
| MP_005SR | SR | MP_003SR | 0 / 0 / 0 | 19 of 19 | 0 | - | 2.0 |
| MP_012SR | SR | MP_001SR | 0 / 0 / 0 | 19 of 19 | 0 | - | 1.5 |
| MP_004SDM | SDM | MP_SP_005SDM | 0 / 0 / 0 | 5 of 5 | 1 (0) | 1 ifv | 1.8 |
| MP_007SDM | SDM | MP_004SDM | 0 / 0 / 0 | 5 of 5 | 1 (0) | 1 ifv | 1.7 |
| MP_009SDM | SDM | MP_004SDM | 0 / 0 / 0 | 5 of 5 | 1 (0) | 1 ifv | 1.9 |

**16 of 16 levels: 0 errors, 0 warnings; 446 of 446 statistics inside DICE's ranges** (A1: 379 of 382; the 64 more
are the vehicle statistics: Conquest's base and flag pools, Rush's and Squad Rush's per-stage vehicles), **Squad
Deathmatch's vehicle placed** on all three (5 of 5 each, A1's 4 of 5); 1 note, A1's (`NAM_MP_005R`'s M-COMs 63 m apart
in height). Where a level's own spawners lack a team's vehicles that spawn by themselves the proposal says so and
places fewer (`MP_009CQ`: no Team 1 vehicle, flags without a spawner of each team; `NAM_MP_002CQ`: its HQs' helicopters
and tanks wait for events; Rush: one side's), all inside DICE's ranges. Two fixes the run found: a boat's spot needs
water over the terrain (XVAL-PLACE-3's "dry land", `NAM_MP_005R`'s river), and a flag short of one vehicle takes no pair.

### Control: several proposals, pins, the areas after Accept

- `autolayout::propose_all(.., pins, k)`: the best `k` of the 8 fitted candidates (distinct places or turns), each
  given its vehicles and measured again; `propose_with` is its first.
- **Pins** (`Op::Pin`, the project's organisation; **project format 8**, a version-7 file loads as it is): a pinned
  flag, base or M-COM keeps its place - the search runs over turns and scales only, the centre the one that puts the
  pins on their own places best, a pin left off costing 0.2 a metre - and its own spawns and markers follow it; a pinned
  spawn point or vehicle spawner is not moved and a pinned vehicle counts for its capture point.
- The panel: Seed, Proposals (1-4), Propose layouts; Pin selection / Unpin selection / Unpin all; a column per proposal
  (Show, Accept, warnings and notes, statistics inside, turn and scale, vehicles); the shown proposal's statistics,
  vehicles and notes, findings below. Proposals run on PR1b-0's `Task`.
- **VW1's areas follow the project** (`overlays::with_project`): the combat and capture areas are read again from the
  project's partition at each revision that edits it; the base's effects, lights, sounds and spawn checks stay.

The editor (`rust/bc2-editor/tests/auto_layout_panel.rs`, game tier, release, 16 s with A1's test): Laguna Alta with
Panama Canal's layer, three proposals in 1.5 s, each 0 warnings, 42 of 42 statistics inside, 16 vehicles (10 new);
after Accept every one of the 6 capture points has its capture area drawn where it stands, the combat area too, and
every vehicle joins its capture point; A1's test still passes (now with vehicles). Screenshots (Dark, Light;
`BC2_EDITOR_SHOTS`): `auto_layout_proposals_<theme>.png` (three columns, proposal 2 drawn), `auto_layout_areas_<theme>.png`
(the accepted layout's areas). Synthetic (`tests/auto_layout_vehicles.rs`, no game data): the three rules raised by a
tank in a courtyard, a boat in a pond, a helicopter under a roof and a plane in a courtyard and silent on the same in the
open; a Conquest layout's 14 vehicles (each HQ 4, each flag a neutral pair) from 5 own spawners (all moved, 9 copies),
the log replaying the operation, three distinct proposals, deterministic, pins kept.

### The export's path-data step (`bc2_levelwrite::pathdata`, opt-in)

`ExportSpec.path_data` (the Export window's "Stage the level's path data for bots"): `pathdata::ready` before anything
is built - the base's newest path data (`work\navkit\<base>`, else `work\navkit_offline\<base>`, else DICE's own file,
the ledger's original first) and no blocker: the project edits no collision (a static object moved, added or deleted;
a static group member edited; the terrain sculpted) and has no navigation edits - else `mapexport build` refuses
(XVAL-AI-1) naming why. The file is staged as `ai/infantry/pathdata.hpd` (`ai/vehicle/` with DICE's), recorded in
`stage.json`'s `path_data` with its install path `Output/win32/ai/levels/<id>/infantry/pathdata.hpd`. `mapexport check`
adds XVAL-AI-1 (the file recorded, Kynapse, every sector's nodes sorted: HPD-NODE-2 / AI-CRASH-4) and XVAL-PLACE-5 on
the staged file's route graph (`pathdata::routes_of`: nodes by id, islands by size, as the AI workspace makes them). The
AI workspace opens a project's staged path data when its last export staged one (and reads `work\navkit_offline`
builds too).

**Proof** (`examples/vw2_canaries.rs --path-data`, the six VW2 canaries, and the SG1 member-edit test):

| export | base | path data | XVAL-AI-1 | XVAL-PLACE-5 on the stage |
|---|---|---|---|---|
| Arica at dawn | MP_002 | NavKit's offline build staged (57 sectors, 22,700 nodes) | ok | 30 spawns / objectives of the first-guess layer off the main network |
| Atacama in fog, under Arica's sky, at night, in rain | MP_005 | NavKit's offline build staged (81 sectors, 32,664 nodes) | ok (4 of 4) | 9 off the main network each |
| Wake Island under Iwo Jima's sky | WAKE_ISLAND_S | refused: no NavKit build and no DICE file for it | - | - |
| SG1's member edits (Wake Island, MP_005) | | refused before the build: the static group members edited | - | - |

The canaries' layers are the target's first guess (A1: Arica's spawns stand in the harbour), so XVAL-PLACE-5 fails on
the stage as it should. The auto layout with the same path data (`a1_layout --routes`, its `propose_with`) puts every
spawn point, flag and M-COM on the main network: MP_002's best (MP_012CQ's layer) and MP_005's (MP_009CQ's) 0 off,
0 warnings, every statistic inside (DICE's own layouts: 13 and 7 off on these offline builds).

**Blocked on NavKit's port (A2-rest)**: sampling the edited level without the game (`navsample_offline` with the
stage's partition and group files: Python, and not a `navkit` step; NK4 ported walkability, not the sampler's partition
override), building in process with the project's spawns and objectives as seeds and objective floors (NK5's
`build_inputs`), applying navigation edits to a NavKit base (`navkit apply` on a NavKit base: Python until NK5), the
stock-MP AI layer for a staged level (`stockmp` by a stock level's name: HasAI, PathfindingTypes, SectorOffsets, AI
spawners and orders), and OPFOR's `mapedit_install.py` installing `path_data`. A later Rush stage's vehicles wait for
the stage events (E4). In-game behaviour (vehicles spawning, bots on the staged path data) is unproven until a playtest.
## NK5: NavKit's build in Rust (2026-11-22, branch `nk5`)

NK1's plan, the last stage of the build: everything `navbuild.build` did after walkability, and the driver around it.
A NavKit build now runs in Rust from the sample files to the path-data file; `bfbc2.exe navkit build`, `all` and
`apply` on a NavKit base answer without the Python.

### What moved

| Python (now a twin or a thin call) | Rust | where it runs |
|---|---|---|
| `navbuild.stage` after walkability: sector discovery and the 256 x 256 check, the specs (edge-sector tiles), the areas' order and layers, `area_of`, the border-node runs and ids, the carried seam nodes, the one-way pieces (`drop` / `keep` / `restore`), `detail_pass`'s glue (the cells by grid position, `eligible` from the portal sides, the broken outline sides taken out, `level.blocked`, `half_cells`), the meshes' assembly (area values, tile items, rectangle, bounds, the neighbour links), the graph jobs, the ids, `note_nodes` and the order and ladder counts | `bc2_navbuild::stage::stage` | `bc2_native.navkit_build` (`navbuild.build`, a thin call); `bfbc2.exe navkit build / all / apply` |
| `navbuild.merge_detailed`: the seed points (a's kept ground in the detailed sectors: no Python tuples now), the carried nodes, stage b, the links across mixed seams, the dropped seam nodes, the merge | `stage::detailed_seeds`, `carried_nodes`, `merge`; `build::build_graphs` | the same |
| `navbuild.build`'s log lines and `write_file`'s glue (anchors, the shadow islands' line, the order nodes, the clock words) | `bc2_navbuild::build` (`build`, `file_bytes`, `write_out`) | the same |
| `navvehicle.stage_on_grid` / `write` (`navbuild.stage(grid=...)`, `write_file`) | `build::grid_ground`, `stage::stage`, `build::file_bytes` | `bc2_native.navkit_stage_grid` / `navkit_write_stage` |
| `navkit.build`, `build_inputs` (`detailed_sectors` with its cache key, `stale_detailed_samples`, `source_stamps`), navregress's `level_water` / `reference_bounds` / `order_points_of` / `area_orders_of` / `ladders_of`, `np.load` of the triangle files, `navedits.regions_of` | `rust/bc2-cli/src/navkit/build.rs`, `bc2_navbuild::build::read_tris_npy` | `bfbc2.exe navkit build`, `apply` on a NavKit base (`apply.rs`), `all` (prepare, the sampling as a `navkit.py sample` child, build, check) |

The run and the floods' level stay in Rust from walkability on, as NK4 set up: `walk::stage`'s level becomes the
stage's (`Level::with_keep_terrain`), the sample run is dropped once its grid and terrain flags are read, and no array
crosses to Python.

### Python's order and floats

No order or float decision was needed: 15 of 15 levels are byte-identical with the same log lines. What the Python
left to dicts and sets, and how the port keeps it:

- the areas' order: `sorted` by (tile x, tile z, lowest height), stable (the floats compared as Python compares them);
- an area's cells in the detail pass: a dict by (i, j), first place and last value (`trace::Ordered`);
- the border runs: the window's order, the active runs in their list order (the first that links takes the surface);
- the carried nodes' ground: `min` over (small area, distance, surface) tuples; `math.hypot` as CPython's
  (`pymath::hypot`), `round()` half to even;
- `merge_detailed` iterated a Python set of sectors only to fill per-sector lists and sets: no order reaches the file;
- the broken sides (a set) are only tested for membership and counted;
- the node ids are given in the sectors' sorted order, as before.

### Parity

- **Every regression level, against the old Python** (`rust/parity_nk5_build.py`: `navregress.build_level` with a
  checkout of `eee12ba` - the Python stage - and with this branch, one level at a time, the build clock fixed;
  `compare`): **15 of 15 identical** - the path data's SHA-1 and size and every log line (seconds and the output path
  blanked). The 4 golden builds (sp_009, sp_004_b, sp_008_b, ons_mp_004) **as recorded**.
- **XNAV-BLD-3**: the files are the Python's bytes, so every navparity measure is unchanged by construction; no
  regression reference changes.
- **`bfbc2.exe navkit build` and `apply` on a NavKit base** (a scratch script: the old checkout's `navkit.py` against
  this branch's `bin\bfbc2.exe` on sp_009 and sp_004_b, the level's `work\navkit` files restored after; an apply with
  a region edit, a seed and a note): stdout the same line for line (seconds blanked), the built and the applied files
  the same once each sector's build date and time are zeroed, the apply reports the same.
- **NK1's apply parity**: **33 of 33 identical** (`rust/parity_nk1_apply.py`: the 18 DICE files and the 15 regression
  builds; the DICE base's edit stage is untouched, its code only split so the NavKit base shares its tail).
- **The tiers** (after merging main with A2): `run_tests.py quick -j 2` 133 of 133 (O2 among them); NavKit's game
  tier 13 of 13 (`apply`, `traversal`, `links`, `navbuild-native`, `vehicle-level`, `navwater-mesh`, `navtools-native`,
  `navdata`, `hpdtool-meshtrip`, `collision-backends`, `navmesh-cells-routes-native`, `navregress:small`,
  `cli-navkit-prepare-harness-texqa:live`); the editor's AI-workspace game tests (`gr2::ai_workspace_on_ons_mp_004`,
  `gr2::edit_build_install_restore_on_a_scratch_copy`: its Build now runs the Rust build through `bfbc2.exe`): pass.
- Tests: `stage_tests.rs` (6: Python's `repr` / `format(g)`, navwalk's trigonometric constants bit for bit, the
  `.npy` reader, the detailed settings, a whole stage on a synthetic two-sector ground read back as a file, the
  no-ground refusal); `test_navbuild_native.py`, `test_vehicle.py`, `test_apply.py`, `test_tool_edges.py`,
  `test_navkit_offline.py`, `test_detail_features.py`, `test_water_rule.py`, `test_reference_clip.py`,
  `test_navsample_offline.py`, `test_navislands.py`, `test_column_sectors.py`: pass.

### Time and memory

Each level's whole build, old and new (one process each, one at a time; other agents' work beside them; peak = the
process's peak private bytes, sampled every 10 ms; the detailed pass's columns are the largest private bytes between
its log lines: its walkability, then its stage).

The build peak is the regression's: each row is `navregress.build_level` of one level in its own process (the build that made NavKit regression's 15 GB peak), so navregress's peak per level falls the same way, 11.8 -> 6.9 GB at most (navparity's measures after each build were not part of the capture). Measured without `OPENBLAS_NUM_THREADS=1`: about 0.5 GB of each figure is NumPy's import in the Python process.

| level | build, old -> new (s) | build peak, old -> new (MB) | detailed pass: walkability, stage, old -> new (MB) |
|---|---:|---:|---:|
| ons_mp_002 | 9.1 -> **6.6** | 3,912 -> **2,543** | 2,474, 3,899 -> 2,411, 2,500 |
| ons_mp_004 | 4.8 -> **3.3** | 2,929 -> **1,926** | 1,464, 2,909 -> 1,357, 1,917 |
| sp_003_b | 16.0 -> **9.6** | 5,607 -> **3,051** | 3,948, 5,594 -> 3,051, 2,908 |
| sp_004_b | 5.8 -> 8.7 (beside a cargo build) | 3,516 -> **2,019** | 1,859, 3,481 -> 1,727, 1,939 |
| sp_005 | 31.4 -> **19.7** | 10,088 -> **5,280** | 6,389, 10,088 -> 5,280, 4,643 |
| sp_005_b | 20.4 -> **13.5** | 8,074 -> **4,242** | 5,319, 8,032 -> 4,242, 3,821 |
| sp_006 | 38.6 -> **19.1** | 11,760 -> **6,379** | 7,824, 11,714 -> 6,379, 5,189 |
| sp_007 | 26.8 -> **16.3** | 9,599 -> **6,876** | 7,452, 9,598 -> 6,876, 4,695 |
| sp_008 | 9.4 -> **6.5** | 4,905 -> **3,238** | 3,357, 4,890 -> 3,238, 2,670 |
| sp_008_b | 6.8 -> **4.8** | 3,775 -> **2,383** | 2,471, 3,766 -> 2,383, 2,276 |
| sp_009 | 1.4 -> **1.2** | 1,245 -> **945** | 886, 1,235 -> 861, 931 |
| sp_01 | 12.1 -> **8.2** | 5,525 -> **3,514** | 3,902, 5,525 -> 3,514, 2,930 |
| sp_02 | 10.5 -> **6.9** | 5,461 -> **3,465** | 3,837, 5,455 -> 3,465, 2,624 |
| sp_03 | 21.8 -> **15.6** | 7,880 -> **5,467** | 6,246, 7,880 -> 5,467, 3,984 |
| sp_04 | 19.5 -> **11.1** | 8,667 -> **4,829** | 5,648, 8,660 -> 4,829, 3,893 |

| | old (Python stage) | new (Rust) | |
|---|---:|---:|---:|
| whole builds, 15 levels | 234 s | **151 s** | 1.6x |
| the largest build peak | 11.8 GB (sp_006) | **6.9 GB** (sp_007) | 1.7x less |
| the detailed pass's stage (areas, one-way portals, detail, graphs), largest | 11.7 GB (sp_006) | **5.2 GB** (sp_006) | 2.3x less |

**Where the peaks come from now.** The stage's own peak (the areas and the one-way portals' tracing, every sector's
side kinds held at once on 16 threads) is no longer any level's build peak. On every level but ons_mp_002 the peak is
the detailed pass's walkability (NK4's `walk::stage` at pitch 0.2: the run, the high run, the masks, the floods'
level), with stage a's result held beside it (1.3-1.7 GB on the largest levels: its level, cells, meshes and graphs,
which the merge's links read). The NK4 plan's per-sector streaming was not needed to get the build to 6.9 GB; it is
the next step if a level grows past the budget (below).

### Retired

Nothing yet: `navbuild.stage`, `merge_detailed`, `detail_pass`, `half_cells`, `restore_one_way`, `drop_one_way` and
`write_file` stay as twins the tests and the NK2-NK4 parity scripts read; no build calls them. No command line went
and nothing outside NavKit imports these modules (Project OPFOR has no caller of navbuild, navkit build / apply or
navvehicle), so `VERSION` stays (RELEASING.md).

### What is left (NK5-rest)

1. **The editor's AI workspace in process.** Its Build button runs `bfbc2.exe navkit apply` as a child (`nav.rs`,
   `start_navkit`), which is Rust now. In process it needs `build_inputs` as a library (today `bc2-cli`'s, on `pyjson`
   / `pyos`: a `bc2_navbuild::inputs` with Python-exact JSON numbers, the detailed choice through
   `bc2_scene::collision`, the terrain unpack through `bc2_formats::fbrb::tool`) and, for a side file with graph edits
   or traversals, `graphedit` without `bc2-pyrt` (the editor has none since PR1a: its `Value` and Python's `round`);
   then a `Task<T>` with the build's lines streamed and a cancel flag.
2. **Retiring `navbuild.py`.** Its callers are thin now: `navregress.build_level` and `navsample_offline` call
   `navbuild.build` (Rust), `navvehicle.stage_on_grid` / `write` call `navkit_stage_grid` / `navkit_write_stage`,
   `navwater_mesh` and `navvehicle` call `water_tops` (Rust since NK4); `hpdcheck.py` and
   `bfbc2-navigation-studio/overlay.py` only name its constants in comments; `rust/bench_navkit.py` builds through
   `navregress`. What keeps the file: `BuildSettings` and the switches the bindings read (to a small settings module
   or into the bindings), and the twins the tests read - `test_navbuild_native.py` (E3h's recorded whole stages
   through `navbuild.stage`), `test_detail_features.py` (`detail_pass`, `line_walkable`), `test_portal_symmetry.py`,
   `test_bridge_union.py`, `test_area_link.py` (`trace`, `trace_edges`, `trace_emit`, `build_areas`, `Level`),
   `test_navislands.py`, `test_tool_edges.py`, `test_column_sectors.py`, and `rust/parity_nk2_file.py`,
   `parity_nk3_detail.py`, `parity_nk4_walk.py`. Each needs its checks moved to Rust tests (`stage_tests.rs` has the
   start) or onto `navkit_build` / `navkit_stage_grid`; then the file and its twins go (RETIRED.md rule 4) and the PYI
   rows change.
**For A2-rest** (the export's path data built in process): `bc2_navbuild::build::build(&Inputs, out, &BuildSettings, &Switches, &WalkSettings, (date, clock), &mut dyn stage::Log)` builds a level in process; `Inputs.seeds` (soldier spawns), `order_points` and `area_orders` are plain point lists, so the project's spawns and objectives go in as seeds and anchors; `build_graphs` + `file_bytes` give the bytes without a file. What it lacks is `build_inputs` as a library (item 1).

3. **Memory**, if a level grows past the budget: stage a reduced to what the merge reads (the normal sectors' area
   maps, tiles and border runs) before stage b, and stage b's tracing in chunks of sectors.

## PR1b-1: storage, I/O and memory, in part (2026-11-22, branch `pr1b1`)

Audit AU5 section 6's PR1b-1 (PR0 items 4-8, ALG-5, ALG-10, ARC-4, ARC-5). This round did the memory item, part of the
cache item and the DBX-document decision. The rest is listed under "What is left" below. Every output is the same:
WW2 rebuild 10,852 of 10,852, the O-series goldens, and the bench digests below.

Measuring: `rust\bench_memory.py` (new). It runs each batch reader in a child process and samples the child's
committed memory every 2 ms. `--command <program ...>` runs any command and reports the commit peak of the whole
process tree, the line printed before it and each process's share. Test peaks are `run_tests.py`'s own (job object
peak, `test_times.json`).

### 1. Memory

**NumPy's OpenBLAS (the largest finding).** `import numpy` commits about 490 MB in every Python process on this
16-thread PC: OpenBLAS's per-thread buffers (scipy-openblas 0.3.34, `MAX_THREADS=24`). The suite's array code does
not use BLAS threads. With `OPENBLAS_NUM_THREADS=1`, `import numpy` commits 19 MB (4 threads: 115 MB).
`suite_paths.py` and `bfbc2core/__init__.py` now set it with `os.environ.setdefault`, so a user's own value wins and
every process the suite starts inherits it. It is not in the `bfbc2core.envvars` registry, which only takes the
suite's own prefixes.

| Peak (run_tests, job object) | before | after |
|---|---:|---:|
| geometry-native:game (terrain, water, Havok readers; 3 workers) | 2,895 MB | 990 MB |
| navmesh-cells-routes-native:game | 5,434 MB | 1,560 MB |
| texture-helpers-native:game | 5,227 MB (726 s) | 4,553 MB (458 s) |
| model-tools-native:game | 5,522 MB (1,051 s) | 5,038 MB (1,171 s) |

The geometry entry measured by process at its peak: main 514 MB (NumPy), workers 765 / 680 / 663 MB before;
33 / 271 / 205 / 188 MB after. Reading each bundle's geometry in groups of 128 MB instead of all at once changed the
peak by 12 MB only (2,895 -> 2,883), so that change was not kept: the terrain readers' 2.9 GB was OpenBLAS, not the
readers.

WW2 rebuild (OPFOR main, the suite at `pr1b1`), each step's process peak (working set / commit), PR1a's rebuild log
against this one: the commit falls by about 480 MB in every step. The sum of the 55 steps' commit peaks is 79.9 ->
53.4 GB. The level builds' peaks are 4,352 / 4,356 / 4,296 -> 3,871 / 3,873 / 3,822 MB, material_grid 4,046 -> 3,564,
wake_level 2,816 -> 2,334 and bf1943_fx 2,575 -> 2,104. The whole rebuild took 22.0 -> 13.3 min. The time is not
attributed: the PR1a run's machine load is not known, and the bundle cache was warm here.

**The batch readers** (`bc2-py/src/window.rs`). `dbx_parse_many`, `dbx_to_xml_many`, `fbrb_read_many`,
`xenos_parse_many`, `granny_roots_many`, `granny_to_pc_many` and `texture_convert_many` decoded every input on all
cores and kept every Rust result. Only then did they build the Python objects, so all the Rust results and all the
Python objects were alive at once. They now decode in a window: at least two items per core, more while the inputs
stay under 64 MB, at most 4,096 items. The calling thread builds each result in input order while the window
decodes ahead, and drops each Rust result once its object exists. `fbrb_read_many` keeps one bundle per core in
flight. Each item is decoded alone by the same function, so the results are the same objects in the same order.
`bench_memory.py`, the game copy, the peak above the inputs held, before -> after (results' digest unchanged):

| Call | items | before | after |
|---|---:|---:|---:|
| `dbx_parse_many` (mp_common + sp_009 `.dbx`) | 10,075 | 686 MB, 1.6 s | 677 MB, 1.7 s |
| `dbx_to_xml_many` | 10,075 | 160 MB, 0.24 s | 120 MB, 0.21 s |
| `fbrb_read_many` (41 Dist level bundles' `.dbx`, cache off) | 41 | 1,002 MB, 2.1 s | 917 MB, 2.9 s |
| `xenos_parse_many` (every Xbox level's shaders) | 55,903 | 5,877 MB, 33.2 s | 5,859 MB, 15.8 s |
| `granny_roots_many` (mp_common) | 1,253 | 1,511 MB, 2.5 s | 937 MB, 2.8 s |

What remains is mostly the Python objects the caller keeps: the 55,903 Xbox shader trees are 5.8 GB of Python
objects, about 105 KB each. Bounding that is up to the callers (iterate in parts). The test that reads them already
uses 2 workers of about 360 MB. Times on this PC vary by about 30 % between runs while other rounds build.
`fbrb_read_many` gets slower when one large bundle dominates a window. `kymesh`'s `navkit_navdata_areas` /
`navkit_mesh_cells` parse a file's sector meshes at once: these are small, so they were left as they are.

**NavKit regression's 15 GB.** `navregress.py` builds one level at a time, so its 15.3 GB peak is one level's
`navbuild.build`. NK5 moved that build to Rust and measured the largest build's peak at 11.8 -> 6.9 GB. The
coordinator gave the regression peak to NK5 / NK5-rest; it was not measured here.

**ALG-10, the WW2 level builds' peaks attributed.** Iwo Jima's build was run one step at a time under
`bench_memory.py --command` (`ww2_level_build.py iwo_jima_s <step>`, after the rebuild; the oracle was checked again
afterwards: 10,852 of 10,852). The commit peak of each step:

| Step | peak | at |
|---|---:|---|
| fx | 797 MB | effectport's emitter copies |
| objects | 836 MB | |
| level | 2,346 MB | wake_level's partition pass (the level's DBX trees) |
| shaders | 1,920 MB | wake_shaders' merge from the stock levels |
| clone | 940 MB | |
| billboards | 796 MB | |
| translate | 778 MB | |
| grid | **3,659 MB** | material_grid: "5,859 partitions reached from the level in 9 steps", each kept as a Python tree |

The chain's 3.9 GB is the grid step. Wake's own `material_grid.py` step, at 3.56 GB, is the same walk. The bound
belongs to OPFOR in Rust (stage 0 / 2): material_grid should parse each partition, take its material names and drop
it (or walk the `dbxcatalogue` index) instead of keeping 5,859 trees. The level step comes next.

### 2. One cache layer (in part)

The mesh viewer's index `work\tools_out\meshview_index.pkl` (34 MB; written by the Python, `bfbc2.exe meshview` and
the editor's Asset preview) is a project cache now: `meshview`, suffix `meshview_index.pkl`, cap
`BFBC2_MESHVIEW_CACHE_MB` (default 256), in `caches.PROJECT_CACHES` and in bc2-cli's `budget::SPECS`. It is listed,
pruned, cleared and counted in the overall budget, so the editor's writes are budgeted too: any tool's budget pass
counts them. The `caches` help and the argparse recordings changed by that one name. `argfmt_golden.json`,
`abbrev_golden.json` and `cli_o3_caches_golden.json` were re-recorded from the Python and `bfbc2.exe` matches them.

Not done: one budget implementation (`caches.py`'s twin calling the Rust needs `budget` / `caches` moved out of
bc2-cli into a library: PR1b-2's `bc2-gamefs`), content keys for the payload, culltree-row, HLSL, collision and
shadercover caches, and `renderqa_truth.pkl` and OPFOR's pickles under the budget.

### 7. The one DBX document: decided

The two byte-preserving models:

- `bc2_formats::dbx::edit::EditDbx` (dbxbin's port, E4a). It is an arena of chunked `Arc` nodes with `NodeId`
  handles, and since PR1b-0 a clone shares everything until an edit. It keeps the string table and its area byte
  for byte while no string is added, each element's string indexes, container flags and trailing bytes. It is used
  by levelwrite's operations (12 files), stockmp, bc2-cli, bc2-py and the editor's edits.
- `bc2_formats::dbxedit::EditFile` (DbxEdit.cs' port, GR4). It is an owned recursive tree (`Vec<Element>`, children
  `Option<Vec<Element>>`), no handles, and a clone copies everything. It keeps i32 string indexes as stored (checked
  only where read: DbxEdit's faults), every value's bytes, the string area's offsets and padding, and XML-text
  files. It is used by DbxEdit's commands, import, leveldiff, check, and the editor's DBX inspector, panel and
  compare. GR4 proved it on 31,037 prepared-level files and 1,232 DbxEditGUI steps.

**Decision: one document with EditDbx's storage and EditFile's rules.** It lives in `bc2_formats::dbx::edit`. The
arena, the handles and the shared clones stay: the editor's undo, checkpoints and levelwrite's operations depend on
them (PR1b-0: 200 operations' checkpoints at +73 MB, not +279). The owned tree has none of this. EditFile's
preservation and failure rules move onto the arena: raw i32 string indexes with DbxEdit's lazy faults, every value's
stored bytes, the original offsets and padding, and the non-binary file case. With those, the arena keeps everything
either model keeps. DbxEdit's commands (`dbxedit::{editor, import, leveldiff, check, export}`) and the inspector then
run on the arena, and `EditFile` goes. The acceptance proof is the union of both proofs: GR4's 31,037 files and 1,232
steps, DbxEdit's recordings (`cli-fbrbtool-dbxedit`), and levelwrite's export canaries and stockmp's goldens. After
that come culltool's `Doc` and the culltree walker on the same document, and one varint (`dbx::cursor`'s) instead of
the six.

The merge is the PR1b-2 round's (too big for this round with its proofs). The order: (1) the rules onto the arena
with EditFile's tests run against it; (2) the dbxedit commands ported, EditFile deleted; (3) culltool `Doc` and
`bc2-levelwrite::culltree` on it (the Onslaught bundles' and the editor exports' cull trees byte for byte); (4) one
varint.

### What is left (precise)

- **2, one cache layer**: one budget implementation (needs PR1b-2's library crate), content keys (a bundle's gzip
  CRC-32 + ISIZE + compressed length for the payload cache, BLAKE3 of the inputs for the others; the cache file
  format version raised, so every cache is cold once), `renderqa_truth.pkl` and OPFOR's pickles under the budget.
- **3, restart points** (Rust half of S1): the disk tier still stores whole payloads. The plan is restart points at
  deflate block boundaries (bit offset, output offset, 32 KB window) indexed per bundle on disk and shared by
  processes. This needs an inflater that can start mid-stream: `netgzip`'s managed-inflater port can, and zlib-rs
  through `libz-rs-sys` (`inflatePrime` / `inflateSetDictionary`) can.
- **4, one BF1943 reader in bc2-formats**: `bc2-cli/src/bf1943src.rs` (the `fbrb1943.py` port) moved into
  bc2-formats with the restart points, bound in `bc2_native`, and `bf1943import.Source` / `RESTARTS` on it. Proof:
  the WW2 rebuild oracle.
- **5, ALG-5**: the export's reach and closure walked once with one session per bundle (needs 3).
- **6, sounds in Rust**: an XMA2 decoder in Rust, matched to ffmpeg's samples on the WW2 and console sound sets
  (none written).
- **7, the DBX merge**: steps (1)-(4) above.
- **8**: `dbxparity.py` / `portparity.py` stay live Python.
- **ALG-10's bound** (OPFOR in Rust): material_grid's walk streamed (3.66 GB).
## NK6: NavKit in the editor, the export's path data built in process (2026-11-22, branch `nk6`)

NK5-rest's first item and A2-rest: NavKit's level driver as a library, the editor's Build with these edits in
process, and the export's path-data step building the level itself. Nothing was written to `game\`; the game never
launched. `navbuild.py`'s retirement (NK5-rest's second item) was not reached: see "What is left".

### The level driver as a library (`bc2_navbuild::inputs`, `::apply`)

| was (bc2-cli, on `pyos` / `pyjson` / `Py`) | now | who runs it |
|---|---|---|
| `navkit/build.rs`: `build_inputs`, `detailed_sectors` and its cache, `stale_detailed_samples`, `level_water`, `build_level`, `edits_of` | `bc2_navbuild::inputs` | `bfbc2.exe navkit build / all / apply`, the editor, the export |
| `navkit/apply.rs`: `navkit apply` on a DICE base and on a NavKit base, the report | `bc2_navbuild::apply` (feature `graphedit`) | the same |
| `navkit/prepare.rs`: `heightfield_of` (the terrain unpack, stamped, under its lock), `reference_bounds` | `inputs::heightfield_of`, `inputs::reference_bounds` | the same, and `navkit prepare` |
| `navtools/detailed.rs`: `flagged_sectors`, `stacked_columns` | `inputs::flagged_sectors`, `inputs::stacked_columns` | the same, and `bfbc2.exe detailed` |
| `navshared.rs`: `original_file` (the ledger's original, the markers before it) | `inputs::original_file` | every NavKit tool |
| `bc2-pyrt::json` / `fmt::round_to` in `graphedit` | `bc2_navbuild::pyjson` (Python's `json`, `float_repr` from `bc2_shader::pyrepr`), `pymath::round_to` | `graphedit` no longer pulls `bc2-pyrt` |
| `pyos` | `bc2_navbuild::ntpath` (ntpath's join / relpath / abspath, `os.walk`, `glob`, Python's `OSError` texts) | |

The command line keeps only its edges (`navkit/build.rs`, 100 lines): the lines printed through `py`, the in-process
FbrbTool runs logged as the Python's tool runs are, the library's errors as the Python's exceptions (`Fail::Raised`
-> `PyErr`, a hand-off -> the Python). New in the library: a level prepared before `area_orders.json` existed has its
objectives read from its XML (`levelobjects.area_orders`, the Python's `navregress.area_orders_of`), which the command
line used to hand to the Python; `Root::samples_dir` (the offline sampler's or a stage's samples); the build's cancel
(`stage::Log::cancelled`, checked between walkability, the stage, the detailed pass's walkability and stage, the merge
and the file: `NavError::Cancelled`); `inputs::local_clock` (`time.localtime()`'s words for the sectors).

### The editor's Build in process

`nav::start_apply`: the Build button's job is `bc2_navbuild::apply::apply` on a `Task<()>` (PR1b-0's), its lines
pushed as they come (the panel shows them), **Cancel the build** setting its flag (the build stops at its next step and
removes its partial output; closing the editor does the same). Install and Restore stay `bfbc2.exe navkit` children.
A cancel on sp_006 asked 3 s in stopped 1.3 s later, after the walkability step, with no file left.

### Parity (`rust/parity_navkit_paths.py`)

- **The 15 regression levels through every path** (`build`): `bfbc2.exe navkit build`, the editor in process (an
  apply of an empty NavKit side file, `examples/nav_apply.rs`) and the export's step (`pathdata::build_bytes` on an
  unedited project, `bc2-levelwrite`'s `examples/pathdata_build.rs`), each in a stand-in OPFOR folder (junctions to
  the game, the prepared levels, the samples), and navregress's build with the golden clock: **15 of 15 identical**
  (the files' SHA-1 with each sector's build date and time zeroed); **the 4 golden builds as recorded** (sp_009,
  sp_004_b, sp_008_b, ons_mp_004).

| level | CLI s | editor s | export s | | level | CLI s | editor s | export s |
|---|---:|---:|---:|---|---|---:|---:|---:|
| ons_mp_002 | 7.6 | 6.0 | 6.4 | | sp_007 | 18.6 | 17.6 | 17.7 |
| ons_mp_004 | 5.6 | 3.9 | 4.6 | | sp_008 | 9.8 | 7.3 | 7.2 |
| sp_003_b | 9.9 | 8.8 | 8.9 | | sp_008_b | 6.8 | 5.6 | 5.8 |
| sp_004_b | 5.4 | 4.5 | 4.4 | | sp_009 | 2.9 | 1.8 | 1.9 |
| sp_005 | 19.5 | 18.2 | 17.9 | | sp_01 | 10.2 | 8.2 | 8.5 |
| sp_005_b | 14.3 | 12.9 | 13.7 | | sp_02 | 8.3 | 7.1 | 6.9 |
| sp_006 | 22.2 | 20.4 | 20.4 | | sp_03 | 14.1 | 12.6 | 12.6 |
| | | | | | sp_04 | 12.6 | 11.3 | 11.6 |

  (The CLI's second or so more is the process start and its log.)
- **NK1's apply set** (`dice`: the 18 DICE files and the 15 regression builds, NK1's generated side files of 36-39
  links, route edges, traversals and notes): the command line and the editor in process **33 of 33 identical** - the
  edited file, the traversal annotations, the report and every line. The command line's own answers against the
  retired Python stay the quick tier's recorded `navkit apply` cases (`core/tests/cli_o4_navkit.py`, O4): pass.
- **A NavKit base** (`navapply`: a walkable box, an unwalkable polygon, a seed, an unseed and 15 graph edits on
  sp_009, sp_004_b, ons_mp_004): **3 of 3 identical**, files (dated fields zeroed; the annotations' `pathdata_sha1`
  checked against each run's own file) and lines.

### The export's path-data step (A2-rest)

`pathdata::stage` now builds the level (`build_bytes`) whenever there are samples (`ready_at` -> `Plan`):

- the base level's prepared inputs and samples (in-game `work\navsample`, else `work\navsample_offline`);
- the staged level's spawns (`.SoldierSpawnEntityData`), AI orders (`Entity.AreaGeometryEntityData`) and objectives
  (`.AreaOrderEntityData`) in place of the base's lists when the project changed them, plus the spawn points (seeds)
  and the flags and M-COMs (objective floors) it moved or made - an unedited project's build is the base's own;
- the project's navigation edits: walkability in the build (`inputs::edits_of`), links and route edges by
  `graphedit` after it, traversals staged as `ai/infantry/<level>.traversal.json` beside the file;
- a project that changes the collision (static objects, static group members) is **sampled on its stage first**
  (`Plan::Sample`, `pathdata::sample_stage`: `navsample_offline.py stage <stage> --level --base`, a Python child -
  the sampler's 1,572 lines were not ported this round): the staged `level-00.fbrb` unpacked and converted into
  `work\navkit_stage\<level>` (a short path: the stage's own folder made unpacked names longer than 260 characters),
  the base's group files the stage does not carry copied in, `levelobjects` and `ladders` on the staged partition,
  the base's settings and terrain, both capsule runs at 0.8 m and at 0.2 m over the detailed sectors; then the build
  reads that tree (DICE's extent the base's);
- a sculpted terrain is refused (the stage sampler reads the base's terrain); a base NavKit cannot prepare is refused
  in words (the Package-only WW2 levels); a base without samples and a project that changes nothing it depends on
  stages the base's newest path data as before.

**Proof.**

| export | path data | XVAL-AI-1 | XVAL-PLACE-5 |
|---|---|---|---|
| Arica at dawn (VW2, MP_002) | built in process from the base's offline samples, 21,306 route nodes | ok | 30 off (the canary's first-guess layer: spawns in the harbour, as A2 found) |
| Atacama in fog / under Arica's sky / at night / in rain (VW2, MP_005) | built in process, 31,028 route nodes each | ok (4 of 4) | 9 off each (first guess, as A2) |
| Wake Island under Iwo Jima's sky (VW2) | refused: no samples, no build, no DICE file; NavKit cannot prepare a Package-only level | - | - |
| SG1 member edits, MP_005 (4 member edits) | the stage sampled offline (`work\navkit_stage\mapedit_sg1_atacama`, 965 MB: the unpacked bundle and the samples), then built in process: 31,819 route nodes (the SG1 game test, 218 s with both levels' exports) | ok | 9 off (the project's first-guess layer, as the canaries) |
| SG1 member edits, Wake Island | refused: the member edits change the collision and wake_island_s is not prepared for NavKit | - | - |
| the auto layout on MP_002 (MP_012CQ's layer; `a1_layout --export`) | built in process, 22,395 route nodes | ok | **ok: 0 off** (21,331 nodes on the main network) |
| the auto layout on MP_005 (MP_009CQ's layer) | built in process, 32,608 route nodes | ok | **ok: 0 off** |

So every stage whose base NavKit can prepare gets its path data, and XVAL-PLACE-5 passes at the stage wherever the
auto layout placed the gameplay. Project OPFOR's `mapedit_install.py` installs a stage's `path_data` through the
install ledger (owner `mapedit-ai:<level>`) and reverts it with the level (OPFOR commit `8306a7b`, its test
`test_path_data_through_the_install_ledger`). In-game behaviour (bots on the staged path data) is unproven until a
playtest.

### Tests

- `cargo test` of bc2-navbuild (85 + 53), bc2-levelwrite, bc2-cli, bc2-py: pass; `clippy --all-targets
  --all-features -D warnings` and `fmt` on the five crates; ruff and mypy on the changed Python.
- `run_tests.py quick -j 2`: 132 of 133, the one failure (the sorted-JSON rule on the stage sampler's two new
  `json.dump` calls) fixed and `tests/test_repo.py` passing after it; O2 passes.
- NavKit's game tier (`run_tests.py game --only navkit`, then `traversal`, `navdata`, `meshtrip`): 46 of 46,
  `navregress:small`, `apply`, `links`, `navbuild-native`, `vehicle-level`, `navwater-mesh`, `collision-backends`,
  `navmesh-cells-routes-native`, the live O2 / O4 / O5 among them.
- The editor's game tests: `gr2::ai_workspace_on_ons_mp_004`, `gr2::edit_build_install_restore_on_a_scratch_copy` (its
  Build now in process), `auto_layout_panel` (2): 4 of 4. The SG1 game test (`model_group_members`, its path-data
  branch new): pass. Project OPFOR: `test_mapedit_install` 6 of 6.
- `spec/trace.py --check`: ok (XNAV-INS-3's code reference moved with `original_file`). XNAV-BLD-3 and the parity
  target's measures are unchanged by construction: every build is the same bytes.

### What is left

- **NK5-rest 2: retiring `navbuild.py`** - not started this round: `BuildSettings` and the switches the bindings read
  (`_cfg_dict`, `_switches`; the tests patch the module's names) into a settings module, the eight tests
  (`test_navbuild_native` E3h, `test_detail_features`, `test_portal_symmetry`, `test_bridge_union`, `test_area_link`,
  `test_navislands`, `test_tool_edges`, `test_column_sectors`) and the NK2-NK4 parity scripts moved to Rust tests or
  the bindings, then the file and its twins deleted (RETIRED.md rule 4) with the PYI rows; `navkit.py`'s own `build` /
  `apply` (the command line's hand-off) need the thin `navbuild.build` call kept somewhere.
- **The offline sampler in Rust** (`navsample_offline.py`, 1,572 lines: the Havok bodies, the rasterised casts, the
  capsule test), then the stage step without Python; a sculpted terrain sampled from the stage's own terrain.
- **NavKit for Package-only levels** (the WW2 levels: Wake Island has no samples and `navkit prepare` lists Dist
  levels only).
- **The stock-MP AI layer for a staged level** (`stockmp` takes a stock level's name: reading the staged partition
  and path data instead, and writing the layer into the stage).
- **The rest of `mapedit_install` (IN1)**: the install plan's dry run naming the path data's ledger state, a stage
  re-export replacing installed path data, `navkit install` and `mapedit-ai` on the same level.
- NK5-rest 3 (memory past the budget) and A2-rest's other items (a later Rush stage's vehicles, engine limits, the
  level's memory, Xbox 360 meshes in a stage, L1-HT-001) as listed before.

## PR1b-1-rest: restart points, one BF1943 reader, the export's reads (2026-11-22, branch `pr1b1r`)

The rest of PR1b-1 (above, "What is left"): items 1-3 done, item 4 in part, items 5 and 6 not started (left as
round PR1b-1c, below). Every output is the same: the WW2 rebuild 10,852 of 10,852, the export canary's stage byte
for byte, the bundle replay's 731 digests. Measured on the development PC (16 threads, 15 GB) while NK6's and then
NKG1's NavKit build series ran beside it, so the times carry that load (both sides of each table were measured
under it, minutes apart).

### The restart points (`bc2_formats::restart`)

A deflate stream can only be decoded from a block boundary with the 32 KiB of output behind it. `restart::build`
decompresses a stream once with zlib-rs's `Z_BLOCK` stops (through `libz-rs-sys`, the C interface of the inflater
`flate2` already uses; `flate2` has no `Z_BLOCK`, `data_type` or `inflatePrime`) and records, at the first block
boundary after every 1 MiB of output, the compressed bytes used, the unused bits of the last one (`data_type & 7`),
the output so far and the window (`inflateGetDictionary`, kept deflated). `decode_span` primes a raw inflater with
those bits (`inflatePrime`), gives it the window (`inflateSetDictionary`) and inflates to the next point: zlib's
`zran.c`. Each point keeps the CRC-32 of its span and every decode checks it, so changed or damaged bytes are
refused (the reader then falls back to decompressing as the reference does), never answered as other bytes.
`read_ranges` decodes the spans a read needs on the worker threads, each with its own handle on the file. Tests:
every span and range against the whole stream (stored, fixed, dynamic blocks; points mid-byte), a byte changed
inside a span refused, the index's bytes round-tripped and damaged ones refused.

### Item 3: points on disk instead of whole payloads (`fbrb::cache`)

The disk tier wrote each large bundle's whole decompressed payload (`.payload`); it now writes a `.points` file
keyed by the bundle's content (length, payload start, SHA-1 of the first 64 KiB, the gzip trailer), so a level's
`Dist` and `Package` copies share one. The points are made by a second pass over the stream with the same inflater
and the same header and trailer checks, and only for a stream whose trailer checks: a stream the reference refuses
is never written, as before. `.payload` files are no longer read; `superseded` (`bfbc2 caches --prune`) lists them.

`rust\bench_fbrb_cache.py` (OPFOR's WW2 level-build reads replayed: 731 reads of 87 bundles; main's module against
this branch's, the same digests in every run):

| run | before: time, peak | after: time, peak | disk |
|---|---:|---:|---:|
| cold (one process, an empty cache) | 59.3 s, 339 MB | 29.2 s, 604 MB | 8.09 GiB of payloads -> 0.10 GiB of points |
| warm (a new process, that cache) | 11.8 s, 190 MB | 12.4 s, 529 MB | |
| memory only (disk cache off) | 150.6 s, 355 MB (68.9 GiB inflated) | 46.5 s, 548 MB (28.0 GiB) | |

The real cache folder held 16 GiB of payloads (at its cap); the WW2 rebuild's whole cache, as points, was 145 MB
(109 files). The peak is higher by the decoded spans (up to `BFBC2_FBRB_MEMORY_MB`, 128 MB) and the parsed points
(their packed windows, within the same budget), and the spans of one read decoded side by side. A first try that
kept the windows inflated in memory and decoded spans one at a time made the warm run 24 s: the points of the 87
bundles (416 MB of windows) did not fit the budget and were parsed again and again.

### Item 2 (ALG-5): one decompression per bundle per export

The export's walks read the large stock bundles step after step; a payload over the in-process budget (128 MB:
`mp_common`, `sp_01`, `ww2_common` ...) was decompressed from its start at every read when no disk file served it
(`BFBC2_FBRB_CACHE=0`, a first read, a cache turned off). Now such a read makes the payload's points in memory with
one pass, and every later read in the process inflates spans of them; the points go to disk at the bundle's next
read when the disk cache is on. The reach and the closure still walk separately (the reads they repeat are spans
kept in memory); one walk is left for PR1b-1c.

`mapexport build` of the cross-set canary (`rust\bc2-levelwrite\examples\export_canary.rs`: XF1's test project,
SP_01 as WW2 Conquest with Guadalcanal's radar bunker), main's `mapexport.exe` against this branch's, through
`rust\bench_memory.py --command`:

| run | before | after |
|---|---:|---:|
| `BFBC2_FBRB_CACHE=0` | 79.5 s, peak 896 MB | 34.8 s, peak 1,160 MB |
| cold (an empty disk cache) | 56.1 s, 878 MB | 29.6 s, 963 MB |
| warm (that cache) | 23.9 s, 432 MB | 16.9 s, 688 MB |
| the disk cache afterwards | 7.5 GB | 115 MB |

All six stages are byte for byte the same (the 8 bundles and files; `stage.json` and `report.txt` differ only in
their timing lines: "shaders: the reach and the merges in 25.4 s" -> 4.3 s).

### Item 1: one BF1943 bundle reader (`bc2_formats::bf1943`)

`bc2-cli`'s `bf1943src.rs` (O5's port of `fbrb1943.Bundle`'s check and `gzip.decompress`) moved into
`bc2_formats::bf1943`, with `Payloads`: an entry of a compressed payload read through restart points, made once by
any process and kept on disk by the payload's content (`cache\bf1943source\*.points`, beside the source index)
and in the process (`BF1943_RESTARTS_MB`, the decoded spans within `BF1943_SOURCE_MEMORY_MB`). A payload that is
not one plain gzip member as zlib reads it is decompressed whole on each read, `bf1943import.Source`'s rule.
`bfbc2 bf1943import` keeps only the Python-index and open logic and reads through it.

`bf1943import.Source` is on it through `bc2_native` (`bf1943_check` for the reader version it follows, `bf1943_read`,
`bf1943_restarts` / `bf1943_restarts_clear` behind `RESTARTS.held()` / `clear()`; registered backend
`bf1943source`): the index rows are the same dicts in the same key order, so the index pickle is the same bytes.
The Python reader stays as the reference (`BFBC2_PURE_PYTHON=1`); `test_bf1943import.py` runs its old checks on the
Python and new ones on the Rust: every entry in a random order against `WholeSource`, a two-member payload, a
stored one, a refused one, the points shared by a second `Source`, read from disk by a later process, none kept with
`BF1943_RESTARTS_MB=0`, four `Source`s on four threads, a bundle changed in place (same size and time) refused by
the span's CRC-32.

**WW2 rebuild** (OPFOR main, `OPFOR_TOOLKIT` a scratch folder whose `bfbc2-tool-suite` junctioned to this worktree,
`BFBC2_FBRB_CACHE` a scratch folder, `--jobs 2`): all 55 steps in 13.5 min (the 3 level chains side by side in
276 s). `compare_oracle`: **10,852 of 10,852 identical** (differ 0, missing 0, extra 0); `qa_checks.py` passed. The
peaks are PR1b-1's (level builds 4.16-4.25 GB commit, material_grid 3.79 GB).

### Item 4: the cache budget, in part

- the bundle cache's and the BF1943 source's points are keyed by content (above);
- `renderqa_truth.pkl` moved from the suite's `build\` folder (outside every cap) into a cache of its own,
  `renderqa` (`BFBC2_RENDERQA_CACHE`, cap `BFBC2_RENDERQA_CACHE_MB`, 64 MB), in `caches.CACHES` and `budget::SPECS`:
  listed, pruned, cleared and counted in the overall budget (the old file in `build\` is left for the user to
  delete);
- the caches' tables count `.points` (fbrb, bf1943source). `cli_o3_caches_golden.json`, `argfmt_golden.json` and
  `abbrev_golden.json` re-recorded from the Python (the help's list of names and the listing's rows), `argspecs.rs`
  regenerated; `bfbc2.exe` matches them.

OPFOR's pickles, for OPFOR (read-only here): `work\ww2\_qa\cache` (qa_checks' `stock_*` / `bundle_*` pickles,
2.1 GB in 148 files, no cap), `work\ww2\install_index.pkl` (61 MB), `install_templates.pkl` (14 MB),
`install_dbnames.pkl` (8 MB); and the suite's own `work\tools_out\dbx_catalogue.pkl` (70 MB, `dbxcatalogue.py`),
which could be a project cache as `meshview_index.pkl` is.

### What is left (PR1b-1c)

- **Item 4's single implementation**: `caches.py`'s pruning calling the Rust (`budget` / `caches` out of `bc2-cli`
  into PR1b-2's library crate; today they lean on `bc2-cli`'s `pyos` / `pylog` helpers); content keys for the
  culltree-row, HLSL, collision and shadercover caches (BLAKE3 of their inputs, each cache's format version raised);
  the OPFOR caches above and `dbx_catalogue.pkl` under the budget.
- **Item 5, sounds in Rust**: an XMA2 decoder (ffmpeg's `wmaprodec.c` / `xma` path: WMA Pro frames, the MDCT, the
  float output) matched to ffmpeg's PCM on the WW2 and console sound sets, then `audio.py`'s one ffmpeg per sound
  replaced. Not started: matching ffmpeg's float MDCT sample for sample is its own round.
- **Item 6**: `dbxparity.py` / `portparity.py` ported (their `--predict` passes need the Python `Porter`'s pools and
  `dbxcatalogue`'s pickle; `portparity` needs `meshport`'s Porter).
- **ALG-5's one walk**: the export's reach and closure walked once.

## AIS-rest: the bot AI's static RE, finished as far as it goes without the game (2026-11-22, branch `aisr`)

A specification round: no tool's behaviour changed. 12 new rules (`spec/engine/bot_ai.md` 52, `spec/model/ai_orders.md`
17; 69 in the two files: 5 verified, 55 observed, 9 inferred); BOT-RT-6, BOT-CTL-1, BOT-MOV-1 / 2, BOT-VEH-3, BOT-GS-1,
BOT-DIF-4, AI-NEED-5 and PERS-SCORE-1 corrected or narrowed.

**Method.**
- **Ghidra.** A scratch copy of OPFOR's analysed project (258 MB, headless, `-readOnly`, deleted at the end), six
  probe runs (decompiles, scalar searches for 0xD84 / 0xC2C, string and vtable listings), each 2-6 minutes.
- **Scans.** Python with pefile / capstone over the executable, read only: the 86 creator registrations (`call
  0x500660` after the type record's push), the behaviour / control instance vtables through their creators, linear
  sweeps of the AI code (0xE40000-0xF30000, 414,448 instructions) for calls through an object loaded from a +0xC field
  (the input object), +8 (the soldier interface) or +4, and for slot-2 / slot-3 calls with an action index of 0-12
  as their first argument; E8 / E9 target and 32-bit pointer scans for callers and vtables; the engine's event-name
  hash (djb2 with xor, from 0x10013E0) applied to event names and searched as immediates.
- No game launch.

**Findings for OPFOR's bot module:**

| Finding | Rule |
|---|---|
| the bot's +0x8C points at its proxy: +4 a body interface, +8 the soldier interface (vtable 0x014197B8), +0xC the input object; seat-entry and exit requests at +0x24 / +0x28 | BOT-RT-6, BOT-CTL-6 |
| DICE's AI writes reload and fire as digital actions, walks infantry by a move target (slot 15), not throttle / strafe; sprint, weapon slot (0 or last) and aim through their own slots; no site writes strafe, alt fire, zoom, jump or interact | BOT-CTL-7 |
| stance is the soldier interface's slot 20 (0x58DAB0 -> 0x69F5B0), never `EiaChangePose`; vehicle weapons by input slots 17 / 18; seats by 0x5725E0 / 0x56D480 | BOT-CTL-8 |
| cars and tanks write throttle 0, brake 2, yaw 3; turrets pitch 4 and roll 5; helicopters all five | BOT-VEH-5 |
| the kill handler's 12 AI tests gate stat events and the skill rating only; bots never score (0x86C12D, 0x86A7EE) and have no scoring record (0x865D7F) | BOT-SCO-1, BOT-SCO-2 |
| AI-NEED-5 against PERS-SCORE-1: the offline block was the 4-client scoring flag; with the patch a human's kill of a bot scores | BOT-SCO-3 |
| a player's name is the EASTL string at +0xC2C, set by 0x564AE0 (the AI spawn at 0x72D3FB, from its soldier's data) and sent with the AI flag by 0x5486D0 | BOT-SCO-4 |
| a capture point's owner +0x94, progress team +0x7C, progress +0x9C, captured flag +0xAC; 0x725630 / 0x711AA0 fire OnCaptured / OnLost | BOT-GS-3 |
| an offline match runs the AI at the session's +0xC8: 1 (Normal) unless a campaign level ran before | BOT-DIF-4 |
| one server order class; `IssueOrder` is the event hash 0xD2199312 checked in 0x58DD20 | AIO-RT-1 |

**Proof.** `spec/trace.py --emit` / `--check` clean (1,195 rules); `run_tests.py quick -j 2` green.

**What is left** (ROADMAP AIS-rest2; `spec/engine/bot_ai.md` "Reverse engineering still needed"): the in-game test of
the AI input object (its checklist is in `bot_ai.md`, "Play-test plan for item 1"); whether bots count in capture areas
and for tickets; the combat fire site (only scripted `EiaFire` writes were found); the Rush stage and M-COM state; the
bot object's class and senses; the order issue (0x5A5490) and the urgency sum; who sets the proxy's seat-entry request;
the soldier interface's state bits.

## NKG1: NavKit's path data, structure toward DICE's (2026-11-22, branch `nkg1`)

The first round of the NavKit gap list (`spec/transforms/kynapse_generation.md`): rules already known, low risk. Each
change follows a numbered rule, is measured with `kynapse_probe` on DICE's 35 files and NavKit's 15 regression builds,
and with navparity on the 15 levels (XNAV-BLD-3: nothing worse).

### What changed in NavKit's file

| Rule | Change | Switch (`navbuild.py`) |
|---|---|---|
| KYN-SEC-2 | the sector table's bounds are the box of the sector's written nodes (f32); a sector with no node, or whose box equals an earlier sector's (the loader's duplicate test), keeps the build's bounds | `NODE_BOUNDS` (on) |
| KYN-EDGE-4 (new, verified) | one edge per (source, target): `graphfile::arcs` writes each pair both ways once | always |
| KYN-LINK-1 / KYG-LINK-1 | a border portal's run is cut where it crosses the sector square's line (`trace::trace_emit_cut`); a piece beyond links to the diagonal neighbour whose square holds it, else to the axis neighbour | `DIAGONAL_LINKS` (on) |
| HPD-NODE-5 / KYG-EDGE-4 | edges only out of owner border copies: written, but **off** (below) | `OWNER_EDGES` (off) |

Not done: C records with DICE's "diagonal" / "up" direction values (KYN-C-1: the values change along one straight
run, are not geometric and are read in no test traced; not imitated); the lookup lists' tie order (KYRT-NODE-2 reads
the lists in order, but the ties are among NavKit's own nodes until placement follows DICE's: NKG2).

### Owner-only edges and the runtime

During the round, NKS-rest read the runtime's use of the owner bit (KYRT-PATH-3): the out-edge walk collects every
copy's edges, and drops edges into a border id with no owner copy. navparity now measures the graph that way
(`runtime_connectivity`, `strong_share`; the probe's `runtime.*` counts). NavKit's border nodes form one row 0.8 m into
the higher sector, which owns them all, so the edges on their non-owner copies (111,968) are NavKit's only edges from a
border node into the lower sector. Left out, the strongly connected share falls from 100 to 6.5 % (sp_004_b), 10.6 %
(sp_008_b) and 22.3 % (sp_009). DICE can keep a node's edges on its owner copy because its band has owned nodes on
both sides of each line (KYG-NODE-2). So `OWNER_EDGES` stays off until NKG2's band.

A second row 0.8 m into the lower sector (each row owned by its sector) was tried: with owner-only edges it gave a
92.7 % strongly connected share, but it changed the sector graphs. New 2-3 node islands appeared where the row's copy
in the higher sector found no pair (islands 402 -> 435 mean). On sp_007 a 1,609-node group split at a coarse /
detailed seam (0x0d0b | 0x0e0b), as the detailed graph re-pruned around the carried row. `connectivity` fell by up to
0.51 points on 5 levels and `over_connection` rose by up to 2.09 on 6, so the row was taken out and waits for the band.
Also measured on DICE's 35 files and verified: every border id has edges in two of its sectors or more (NavKit: 2,704 of
48,899 do not; KYG-NODE-2).

### Parity (15 regression levels, before = `work\navkit\regress` of 2026-11-20/22, after = this branch)

Every navparity measure is the same on every level (means): `ground` 98.22, `extra_ground` 8.31, `areas_same` 5.18,
`portals` 6.47, `outline` 97.99, `borders` 94.69, `costs` 95.91, `nodes_same` 0.64, `nodes_near` 47.63, `edges_same`
0.02, `connectivity` 98.79, `over_connection` 15.31, `ai_coverage` 99.16, `ai_pairwise` 92.67, D / F 50.53 / 18.84 /
34.02 / 33.46, `sectors_extra` 1.33, islands 402.47. `costs` is a share of `edges_same` (0-40 matched edges per
level, nodes matched within 5 cm), so at this size it is noise. The new directed measures, the same before and after:

| level | runtime_connectivity | strong_share | DICE's strong_share |
|---|---:|---:|---:|
| sp_01 | 97.89 | 99.28 | 96.71 |
| sp_02 | 99.44 | 99.86 | 98.12 |
| sp_03 | 97.58 | 99.10 | 96.69 |
| sp_04 | 99.69 | 99.93 | 97.52 |
| sp_003_b | 90.01 | 98.20 | 95.66 |
| sp_004_b | 100 | 100 | 97.91 |
| sp_005 | 99.23 | 99.72 | 96.84 |
| sp_005_b | 99.27 | 99.90 | 96.73 |
| sp_006 | 99.54 | 99.89 | 97.17 |
| sp_007 | 98.12 | 99.84 | 97.90 |
| sp_008 | 98.88 | 99.89 | 97.08 |
| sp_008_b | 98.50 | 99.92 | 97.19 |
| sp_009 | 100 | 100 | 98.07 |
| ons_mp_002 | 99.75 | 99.86 | 97.58 |
| ons_mp_004 | 98.19 | 99.97 | 98.30 |
| mean | 98.41 | 99.69 | 97.30 |

### Structure (probe, the 15 builds; DICE's infantry files of the same 15 levels in brackets)

| Measure | before | after | DICE |
|---|---:|---:|---:|
| table bounds equal to the node box | 0 of 2,494 | 2,489 of 2,494 | 2,474 of 2,474 |
| duplicate (source, target) edges | 6,916 | 0 | 0 |
| edges | 1,900,962 | 1,894,046 | 1,551,934 |
| link entries / to diagonal neighbours | 8,994 / 0 | 14,904 / 5,922 | 14,562 / 5,682 |
| link objects (all resolved) / cross-pitch | 45,522 / 5,383 | 51,995 / 6,614 | 49,600 / 8,875 |
| C records, middle in a diagonal neighbour's square (coarse / detailed) | 941 / 2,058 | 7,787 / 3,927 | 22,344 / 5,051 |
| edges on non-owner copies | 112,296 | 111,968 | 7,355 |

DICE's 35 files with the probe's new counts: one-way edges by their ends (owner border copy to non-owner copy 205,190,
ordinary to non-owner 5,683, the rest 8,627; KYN-EDGE-3), C records crossing a square line (coarse 30,036, detailed
11,566), strongly connected share as the runtime walks it 97.04 % over all 35.

### The goldens and the reference

The 4 golden builds of `bfbc2-auto-navkit/tests/navbuild_golden.json`, re-recorded (old -> new SHA-1, bytes); the
quick part is unchanged:

| level | old | new |
|---|---|---|
| sp_009 | 2971a232f861d0900ea92e15ba9a2c24306ea1f6 (406,216) | e9be0ef5ef1c45e71930516e9f6c1090552993be (408,096) |
| sp_004_b | bf711ee42f2f02b20f1a804fc84ebb2621b04924 (8,235,888) | bd44cedb25e7c88032ef29bf47c3fdddee72032f (8,242,112) |
| sp_008_b | 2d23c2131d7f14f1bbd09f4b9f251cba874860ad (8,477,720) | 2839e515d3802f501e4dc67513becc97591201a7 (8,484,708) |
| ons_mp_004 | 843ca19e5ee911e6ce6fdcfaa1240389cac9b236 (7,623,072) | 020d7019c061c2e1b91335d540b213f8e80598ed (7,627,988) |

The files grow by the diagonal-neighbour link entries and shrink by the duplicate edges. navregress's reference run
`regression` was saved again with this code (its measures are the old ones plus `runtime_connectivity` and
`strong_share`).

### Tests

- `run_tests.py quick -j 2`: 133 of 133 (O2 among them), after merging main (5fd65e8); O4's navtools golden
  re-recorded from the Python (only navparity's usage text and `show` table changed: the new docstring and the
  `runtime %` column, which `bfbc2.exe navparity show` now prints too).
- NavKit's game tier, 16 entries: `navbuild-native`, `navtools-native` (E4e's vehicle and parity-row recording
  re-recorded: ONS_MP_002's vehicle path data a5baa5ed8b714b67c6d0e33d4b5630c1ae74e14e (291,704 bytes) ->
  d630b57869bd951d788efffe2ced75181635db8d (292,600)), `navmesh-cells-routes-native`, `pathdata-native`,
  `hpdtool-roundtrip`, `hpdtool-meshtrip`, `navdata`, `hpdcheck`, `navwater-mesh`, `vehicle-level`, `links`,
  `traversal`, `apply`, `navregress:small`, `collision-backends`, O4 live: all pass.
- `kynapse_corpus` (game tier): 14 of 14, the two new tests among them.
- NK1's apply parity (`rust/parity_nk1_apply.py`, the pre-NK1 checkout 4a515a4 against this branch): 33 of 33
  identical (the 18 DICE files and the 15 new regression builds).
- `cargo test` (bc2-navbuild, bc2-formats, bc2-py), clippy `--all-targets -D warnings` (with bc2-cli), fmt, ruff and
  mypy on the changed Python, `spec/trace.py --check`: pass.

## SPG2-rest: what the specification's gap pass left (2026-11-22, branch `spg2r`)

Mostly specification, plus one reader fix in `bc2-render`. Nothing written to `game\`; the game never launched. The
probes read DICE's bundles of the game copy (`game/**/levels/*/*.fbrb` dated before 2013, each resource once) through
the worktree's own `bc2_native`.

### Coverage, before and after

| | rules | verified | implemented | observed | inferred | covered | thin |
|---|---:|---:|---:|---:|---:|---:|---:|
| before (main at 5674dc8, NKS-rest included) | 1,183 | 531 | 116 | 487 | 49 | 34 | 2 |
| after | 1,231 | 605 | 126 | 453 | 47 | 36 | 0 |

48 new rules: TERR-HF-9 - 11, TERR-MM-2 - 4, VEG-UG-5, MAT-TEX-5 - 19 (15), 26 MENU rules; 74 rules observed ->
verified by the game-tier check.

### 1. The main height-field block's tail

The "n offsets, A bytes, fixed part" split was the same bytes cut in the wrong places. The tail is:

| part | size | |
|---|---|---|
| row offsets | 4n | where each depth-map row's runs start (u16 units) |
| depth runs | 4 bytes a run | (count, value) u16 pairs, maximal, a row adding up to n; the editor's `<T>-DepthMap.png` |
| padding | 0 - 15 | to 16 bytes from the heights' start; A (0x2D) = offsets + runs + padding |
| min / max tree | 10,922 / 43,690 u16 | root first, rows along z, down to 16-sample cells, each node one sample wider |
| lower envelope | 22,363 / 88,412 u16 | grids of 2, 3, 5, ..., n / 8 + 1 a side, coarsest first |

Offsets, runs, padding and tree re-encode byte for byte on 30 of 30 DICE main blocks. The envelope lies under every
height sample on 15 terrains and under at least 99.48 % on the rest (presumably heights edited after it was built);
DICE's generator was not reproduced (greedy lowering matches 17 - 24 % of the vertices), so a writer keeps or builds a
surface under the ground (TEDIT-HF-3). Depth maps: mostly 255 on 26 terrains, mostly 0 on MP_001, MP_004, SP_01 and
NAM_MP_006; 0 under MP_005's town buildings.

### 2. The physics material map

Blocks of 512 x 256 cells tile a width x width map (8 x 16 blocks at 4,096, 4 x 8 at 2,048); a row is bytes of two
4-bit cells (low nibble first), a byte equal to the one before followed by a count c meaning c + 1 pairs; the
block's 256 u16 are the rows' byte lengths. All 30 maps re-encode byte for byte. The map covers the main block with
rows along z (SeaBed against height: r = -0.55 to -0.89 on six terrains, against -0.21 to +0.60 transposed or spread
twice as wide): 0.5 m cells on 27 terrains, 0.25 m on MP_001, MP_002, MP_008. Index 0 is `defaultmaterial` on all;
1 - 10 names, each a `MaterialData` of the level's grid (31 of 31 paired grids hold them, never at the same index: the
engine resolves by name).

### 3. The NAM_MP_003 reader fix

`bc2_render::terrain::parse_visual` skipped one byte where an undergrowth entry has a string; NAM_MP_003's 30 entries
name `<mesh>/<guid>` there, so the reader ran out of step ("runs past the end at 0x2f65c") and the four nam_mp_003
levels (cq, r, sdm, sr) drew no terrain patches or road strips in the game-accurate view. Fixed, the entries kept as
`PatchShader::meshes` (`UndergrowthMesh`, compared by bits so `PatchShader` stays `Eq`). Old and new reader over the
game copy's 34 visual terrains (DICE's 30 and OPFOR's four WW2): the same shaders, maps and roads on 33, NAM_MP_003
read now; `Terrain::load` of nam_mp_003cq prepares its 53 patch shaders. Tests: a unit test, and
`tests/visual_terrains.rs` (30 terrains, 1,515 patch shaders, 3,069 entries, 0.7 s with the game copy). Left alone:
`bc2-convert`'s walk and console-convert's `terrain.py` read the same field as one byte (their inputs never set it).

### 4. The undergrowth numbers

`BFBC2Game.exe`'s type information has `TerrainUndergrowthType` (0x4C bytes, a field of `TerrainDecalMaterial`'s
`UndergrowthTypes`): `Mesh`, `MeshShaderSet`, `CacheShader`, `Density`, `Variation`, `MinMask`, `MaxMask`,
`WindScale`, `ViewDistance`, two-float `MinScale`, `MaxScale`, `MinMaskScaleFactor`, flags `SkewEnable`,
`RotateEnable`, `Enable`: exactly the entry's 3 strings and 12 floats. Matched by order and DICE's ranges (VEG-UG-5,
inferred): density, variation, min / max mask, min scale (x, y), max scale (x, y), mask scale factor (x, y), wind
scale, view distance; 01 00 presumably rotate on, skew off.

### 5. Grime / camouflage and the menus' runtime

Written with two helper agents, each confined to its spec file and probe. Findings that change planned work:

- Vehicle camouflage is shader set 1 (desert) or 2 (snow) of the same mesh, picked by the level's spawners; soldier
  and weapon camouflage is bound per level by the level's compiled shader database (38 of 58 soldier entries differ
  between databases), not by any DBX: a modder's new level needs its database built with the right camo.
- `DirtColor` / `DirtFactor` are per-object values the engine writes (placeholders in the databases); the terrain
  materials carry `DirtTriggerFactor` / `Color` (MAT-TEX-6, 7).
- The movies' interpreter dispatches through a 185-entry table at 0x1545B00 with no bound check; 132 opcodes have a
  handler, and four the suite names (0x57, 0x68, 0x8D, 0xB9) have none (MENU-RT-1, 2, MENU-ED-2: no tool checks that a
  written movie avoids them). aptgraft's comment names 0x1311570 a binary-operator handler; the table says GetMember.

### 6. Every probe a game-tier check

`core/tests/test_spec_probes.py` (`run_tests.py game`): the terrain tails and material maps, SPG's five print-only
probes by the output lines that hold their rules' counts, and `grime.py` / `menus.py` by their `checks()`: 172 checks,
all passing, 3 min 24 s (the material maps' Python decode about 30 s of it).

| probe | run alone |
|---|---:|
| `weapons.py` | 5 s |
| `vehicles.py` | 17 s |
| `materials.py` | 19 s |
| `destruction.py` | 16 s |
| `lighting.py` | 20 s |
| `grime.py` | 35 - 45 s |
| `menus.py` | 9 - 12 s |

### Left

`spec/COVERAGE.md` section 2 (queued as SPG2-rest2): the terrain rules' playtest, the undergrowth loader, the RE list
(emitter processors, mounts, `OnlyTakeableByTeam`, the award evaluator, dirt build-up, the menus' loading roots), the
other two readers' one-byte undergrowth field.

## PR1b-2: the crate shape, in part (2026-11-22, branch `pr1b2`)

Audit AU5 section 6's PR1b-2 (PR0 items 9-12; ARC-2, ARC-7, ARC-12, ALG-6, ALG-11, NAM-8) and PR1b-1's DBX merge plan.
Every output is the same (WW2 rebuild, the export canaries, DbxEditGUI's recording, the corpus round trip). Version
0.17.0: public Rust API moved or removed (CHANGELOG).

### 1. The one DBX document

PR1b-1's plan, steps 1, 2 and 4 (step 3 is left). `EditFile`'s rules are on `EditDbx`'s arena:

- every value keeps its stored bytes, a text value's string-index varint too (`EditDbx::payload`; `raw()` keeps
  dbxbin's meaning, empty for text). The dbxbin parse keeps them as well, so a non-shortest index now comes back as
  stored (it was re-encoded; no DICE file has one: the corpus round trips were already identical);
- `set_string` (DbxEdit's `replace` / `replaceprefix`): the table written anew, the intern index rebuilt lazily so
  interning finds the first entry of the table as it is now;
- DbxEdit's reader (`dbxedit::read`) builds the same arena under its rules: C# `int` string indexes checked where
  read (`dbxedit::Table`), wrapping varints, C# `int` numbers sizes, nesting past 256 refused, a non-binary file kept
  as its bytes, strings decoded as .NET does (U+FFFD for a malformed sequence; only seen once the table is written
  anew). `dbxedit::serialize` checks DbxEdit's limits in file order, then `EditDbx::write`.
- the commands (`editor`, `import`, `check`, `export`, `leveldiff`), the DBX inspector, its panel and Compare run on
  `NodeId`s; the inspector's Import XML uses the open file's clone as its reference (no serialise and re-read), a
  refused Apply puts the clone back.

One varint (step 4): `bc2_formats::varint` (`put`, `read_u64`, `read_saturating`, `read_i32`); `dbx::cursor`,
`dbx::edit`, `dbxedit`, `culltree` and `culltool` (twice) call it, each with its reader's rule for a varint the game
never writes.

| Proof | result |
|---|---|
| DbxEditGUI's recording (`dbxedit_parity::the_inspector_gives_dbxeditguis_answers`) | 19 of 19 files, **1,232 of 1,232 steps** |
| every prepared level's binary `.dbx` through the inspector and back (`every_level_file_round_trips_through_the_inspector`) | **31,037** of 89,337 files, **0 differ** (43 s) |
| the project partition's edits (`project_partition_edits_are_project_operations`) | ok |
| levelwrite's whole-level tests and export canaries (quick + game tier, 38) | 37 ok; `model_group_members` needed `bc2_native` built in the worktree (passed once built) |
| DbxEdit's robustness (proptest), the editor's DBX unit tests | ok |
| DbxEdit.exe's recorded answers (`dbxedit:game`): 160 edit fingerprints; 1,592 corpus files, 12,464 command runs | all identical |

### 2. `bc2-formats` split: `bc2-gamefs` and `bc2-tools`

| crate | what |
|---|---|
| `bc2-formats` | the formats: bytes in, bytes out, files read; no file written, no cache, no system call |
| `bc2-gamefs` (new) | `game` (`Game::Bfbc2`, `find_game`), `atomic`, `ledger` (`LedgerError`), `liveness` (`pid_alive`, `host_name`: the crate's only Win32), `sha256`, `bundles` (the bundle cache, the cached reads, `pack_path` / `pack_many`), `points` (the BF1943 restart points on disk), `meshindex`, `budget`, `workers` |
| `bc2-tools` (new) | `fbrbtool` (FbrbTool's command line), `unpack_path` / `unpack_bytes`, `dbxedit` (DbxEdit's command line) |

`bc2_formats::fbrb` keeps `read_path_uncached` / `extract_path_uncached` / `entries_path` and `fbrb::stream` (the
chunk, gzip member, collector and pack pieces the cache and FbrbTool build on). `bc2_formats::bf1943` keeps the
points' format and takes a `PointsStore` (the seam: the reader never writes a file). Every caller moved (paths only);
the specification's citations re-cited by content (`spec/trace.py --emit`, 1,183 rules).

The game layer for the multi-game goal: `Game` names the game (its executable, a copy's check); a later game is a
variant and its answers. Nothing per-game was invented beyond what BFBC2 uses today.

**One cache budget** (`bc2_gamefs::budget`): the table of 13 caches, folders, caps, scan, the folder pruning (stale
temporary files and markers, the bundle cache's superseded points first, then least recently used, `keep`, the stamp
checked again under the lock), clear, usage, the overall pruning. `bfbc2.exe caches` / `budget` call it (their two
copies deleted; `pyos::files`' stat, scandir and delete are its); `bfbc2core.caches`' `prune`, `maybe_prune`,
`prune_total` and `clear` call it through `bc2_native.caches_prune` / `caches_prune_total` / `caches_clear` (the
Python is kept for a missing module). The caches group's recorded answers: 184 of 184 (the overall pruning's members need the project folder found as
before: one fix during the round). `run_tests.py quick -j 2` after merging main: **134 of 134**.

### 3. The census a check

`rust\native_census.py --check` (quick tier, `native-census`): 264 exports, 0 without a caller. `xas_encode` (no
caller) removed; the two classes only returned (`AptLines`, `NavStageGraphs`) are listed with why. `--record` keeps
OPFOR's callers (8 names) in `rust\native_census.json` for a checkout without OPFOR.

### 4. One worker pool and knob

`BFBC2_WORKERS` (`bc2_gamefs::workers`): rayon's global pool built with it at the start of `bc2_native`, `bfbc2.exe`,
the editor and `mapexport.exe` (else `RAYON_NUM_THREADS`, else the cores); `bfbc2core.workers(tool_env, default)`
for the Python tools (pcbuild).

The mesh index (ALG-6): each bundle on the pool (its header and one read of all its Dx9 databases), the parts
merged in the bundles' order with the reference's first-wins rules and its failure order (an entry's failure counts
only when the entry is new, as before).

The editor's jobs on `Task` (ARC-7): the start screen's catalog, a level's load, its render meshes, the export's
profiles, preview and run, NavKit's comparison read (8 of the 12 left by PR1b-0). Kept: the install's run (its
answer a status and text, its DF1 test hands it a receiver), the preparation and the two pipe readers (they stream
lines).

### WW2 rebuild

OPFOR main `2ffc684`, `OPFOR_TOOLKIT` a scratch folder whose `bfbc2-tool-suite` junctioned to this worktree (its fresh
`bc2_native.pyd` and `binfbc2.exe`, version 0.17.0), `BFBC2_FBRB_CACHE` a scratch folder, `--jobs 2`: all 55 steps
in 20.5 min (the 3 level chains side by side in 314 s; the machine shared with NKG1's game tier). `compare_oracle`:
**10,852 of 10,852 identical** (differ 0, missing 0, extra 0); `qa_checks.py` passed. OPFOR's `SUITE_REQUIRES`
(< 0.17.0) warned about the new version, as expected; OPFOR was not changed and nothing was installed. The reader
(the bundle cache in `bc2-gamefs`), the BF1943 points store and the mesh index the rebuild reads through are this
round's.

### What is left (PR1b-2-rest)

- **Merge step 3**: culltool's `Doc` (2,622 lines of the Python emulation) and the export's cull-tree walker on
  `EditDbx`, proved by the Onslaught bundles' and the editor exports' cull trees byte for byte.
- **bc2-cli's tool logic into libraries** (the rest of ARC-2, NAM-8): the many `find_opfor` copies, the groups that
  return `Fail` / `PyErr` (levelwrite's culltool, bc2-menu); LevelDiff's command line (still
  `bc2_formats::dbxedit::leveldiff::run`); rayon in bc2-cli's own CPU work.
- **Content keys** for the culltree-row, HLSL, collision and shadercover caches (each format's version raised, with
  the Python readers that share them), OPFOR's caches under the budget (PR1b-1c).
- **Sounds decoded in process** (PR0 12, PR1b-1c's XMA2 decoder).
- **Item 5, not started**: the mesh viewer's Python API through `bc2_native` (ARC-4: `meshview.Index` / `load_mesh` /
  `render` on `bc2_convert::meshview`, OPFOR's 103 calls proved by the WW2 oracle, then `meshview.py` and the pickle
  cache retired); the anim and model tools (`granny.py`'s curve_keys / sample / put, `decimate.py`, `shapes.py`,
  `texwrite.py`'s packing).
- The install run's thread on `Task`.

## NK5-rest: navbuild.py retired (2026-11-22, branch `nk5r`)

NK5's and NK6's "What is left" item: `bfbc2-auto-navkit/tools/navbuild.py` (1,486 lines) goes, now that the whole
build runs in Rust. Nothing was written to `game\`; the game never launched.

### What moved

| was (navbuild.py) | now |
|---|---|
| `BuildSettings` (a frozen dataclass), `DEFAULTS`, `grid_settings`, `_cfg_dict` | the Rust defaults, `bc2_navbuild::stage::BuildSettings::default()` / `at_pitch` / `detailed`; the bindings (`navkit_build`, `navkit_stage_grid`, `navkit_write_stage`) take a dict of **overrides** or `None` (a `pitch` key first moves the grid, then single fields; an unknown key is refused); `bc2_native.navkit_settings()` lists the defaults |
| the module switches (`AREA_LINK`, `LINK_RULE`, `SYMMETRIC_PORTALS`, `EDGE_SECTOR_*`, `GABRIEL_EDGES`, `SORT_NODES_BY_ID`, `BORDER_OWNER`, `OWNER_EDGES`, `NODE_BOUNDS`, `DIAGONAL_LINKS`, `ORDER_*`, `DETAIL_*`, `HEADER`, `_switches`) and the ones it read from navislands / navdetail | `bc2_navbuild::stage::Switches::default()` (overrides as above); navwalk passes only its own two (`roof_prune`, `objective_floor`: the stage's log line) |
| `build` (the thin call), `water_tops`, `outside_bounds`, `deep_water`, `MAX_WATER_DEPTH`, `_clock` | `navwalk.build`, `water_tops`, `outside_bounds`, `deep_water`, `MAX_WATER_DEPTH`, `clock_words` (navkit.py's hand-off `build` / `apply`, navregress, navsample_offline, navvehicle, navwater_mesh call them) |
| `navkit.py --no-detail`: `replace(DEFAULTS, detail_features=False)` | `{'detail_features': False}` |
| `navvehicle.vehicle_settings()`: a `BuildSettings` | the same settings as overrides (pitch 1, 50-sample tiles, the vehicle radius ...) |
| the Python stage, `merge_detailed`, `detail_pass`, `half_cells`, `drop_one_way`, `restore_one_way`, `write_file`, `Level`, `trace*`, `build_graph`, `link_scope` / `sector_links` and the rest of the glue | deleted (the build has run `bc2_navbuild::stage` since NK5); the tests' calls of the Rust core are in `test_navbuild_native.py` itself; `NavLevel.drop_one_way` / `detail_pass` (the Rust stage's own, now `pub`) and `navkit_stage_answer` (a whole stage with everything it made) give the tests the Rust stage's answers |
| the command line `python navbuild.py <samples> <forbid> <XOffset> <ZOffset> <out.hpd> ...` | removed: `navkit.py build` / `bfbc2.exe navkit build` (version **0.17.0**, RELEASING.md "A merge that removes something") |

Project OPFOR imports none of it (no caller of navbuild, navwalk, navvehicle or the bindings), so its
`SUITE_REQUIRES` only follows the new version.

### The tests

| test | now |
|---|---|
| `test_column_sectors.py` | Rust `column_sectors_as_dice_places_them` (every 10 m from -3,000 to 3,000 at 0.8 and 0.2, SP_008's line column) |
| `test_area_link.py` | Rust `area_link_rule` (`Level::connected` raw / slope / terrain, `components`) |
| `test_portal_symmetry.py` | Rust `portal_symmetry_and_one_way_pieces` (side by side, stacked 'keep' / 'drop', 'restore' through `restore_one_way`, a shared surface across sectors) |
| `test_bridge_union.py` | Rust `bridge_outlines_are_closed` (600 random patterns, their own generator: 300+ with bridges, 50+ overlapping; a lone bridge's diagonals and sides) |
| `test_detail_features.py` | navdetail's checks stay Python; the detail pass's (sides out of the outline in pairs, the detailed settings) and the route lines' D-side checks are Rust (`the_detail_pass_moves_outline_sides_in_pairs`, `route_lines_do_not_step_across_a_d_side`) |
| `test_tool_edges.py` | the settings' checks are Rust (`the_default_settings_are_the_normal_grid`); `--no-detail`'s overrides checked in Python |
| `test_navislands.py` | the file through `navkit_write_graphs` (the build's writer) directly |
| `test_navbuild_native.py` | on the bindings: E2a 331 and E3c 456 of the retired Python's recorded answers, **unchanged** (E3c's tracing now through the Rust stage's `drop_one_way` and detail pass); the 10 whole stages through `navkit_stage_answer` |
| `test_reference_clip.py`, `test_water_rule.py`, `test_navkit_offline.py`, `test_review_fixes.py`, `test_native_required.py` | navwalk's names |

**The whole stages.** The old digests hashed navbuild.py's stage result with Python's dict insertion orders (the
border nodes' sectors in the order the stage first met them, `DETAIL_STATS`' keys in the order a pass first counted
them), which the Rust stage does not keep. They are recorded again in a canonical form (sectors sorted, counts as
numbers), from **navbuild.py's own stage** - an archive of `bda5b8d` run with this branch's module (whose old
bindings still take full dicts) - and the Rust stage gives the same on **10 of 10** (every part: meshes, areas,
graphs, border nodes, ids, dropped nodes, order nodes, D / F counts, D sides), with the diagonal-neighbour links off
as that stage built them. `navbuild_golden.json`: `retired.stage` (10) new, `retired.e3h` 39 -> 29.

**The NK2-NK4 parity scripts** (`parity_nk2_file.py`, `parity_nk3_detail.py`, `parity_nk4_walk.py`) hook into
navbuild.py's Python stage, so they stay runnable against **pinned checkouts**: both suites at `bda5b8d` or earlier
(the last with navbuild.py); they refuse a checkout without it. The current build is held to their results by the
goldens and the byte-identical regression below. `parity_nk5_build.py` and `bench_navkit.py` run unchanged
(navregress builds through `navwalk.build`).

### Parity

- **15 of 15 regression levels byte-identical** to main's build (navregress.build_level with the golden clock, each
  in its own process, against OPFOR's `work\navkit\regress\<level>.hpd` - NKG1's reference run - with each sector's
  build date and time zeroed); **the 4 golden builds as recorded** (sp_009, sp_004_b, sp_008_b, ons_mp_004).

| level | s | | level | s | | level | s |
|---|---:|---|---|---:|---|---|---:|
| ons_mp_002 | 5.5 | | sp_005_b | 12.9 | | sp_009 | 1.5 |
| ons_mp_004 | 3.5 | | sp_006 | 19.1 | | sp_01 | 8.7 |
| sp_003_b | 9.1 | | sp_007 | 16.0 | | sp_02 | 6.8 |
| sp_004_b | 3.9 | | sp_008 | 6.6 | | sp_03 | 13.7 |
| sp_005 | 16.4 | | sp_008_b | 5.3 | | sp_04 | 12.8 |

- **NK1's apply: 33 of 33 identical** (`rust/parity_nk1_apply.py`, an archive of 4a515a4 against this branch's
  `bin\bfbc2.exe`).
- XNAV-BLD-3: the files are main's bytes, so every navparity measure is unchanged by construction.

### Tests

- `cargo test` of bc2-navbuild (93, the 7 new among them) and bc2-py; clippy `--all-targets --all-features -D warnings`
  on bc2-navbuild and bc2-py (and bc2-cli, bc2-levelwrite on the changed crate); `fmt`; ruff and mypy (every folder).
- Release builds: `rust\build_native.py` (the module, `bfbc2.exe`) and `--editor` (`bfbc2-editor.exe`, `mapexport.exe`).
- NavKit's game tier (`run_tests.py game --only navkit -j 1`): 35 of 37 at first - the two O4 entries failed only on
  usage texts this round changed (navkit.py's `--no-detail` line, hpdcheck's and ladders.py's docstrings, which the
  Rust command lines print); `cli_o4_navkit_golden.json` / `cli_o4_navtools_golden.json` re-recorded from the Python
  (`test_cli_o4.py --record`: only those usage lines changed), then `--live --game` for both groups: all pass (37 of
  37). `navbuild-native:game` (sp_009, sp_004_b as recorded), `navregress:small`, `apply`, `links`, `vehicle-level`,
  `navwater-mesh`, `navtools-native` among them.
- `run_tests.py quick -j 2`: 129 of 129 (133 less the four entries ported to Rust), O2 among them.
- `spec/trace.py --check`: ok.

### What is left

- NK5-rest2 (ROADMAP): memory past the budget, if a level grows (stage a reduced before stage b, stage b's tracing
  in chunks).
- `navislands.py` and `navdetail.py` stay for their own callers (prune, features for the tests); the build reads the
  Rust defaults of their settings, so their module names no longer change a build.

## MM1: the OPFOR mod manager (2026-11-22, branch `mm1`, with Project OPFOR's `mm1-migrate`)

`bfbc2 mods` (`rust/bc2-install/src/mods`, the `mods` tool group) turns Project OPFOR's mods on and off one at a time
on the install ledger of round DF2, without a rebuild; the hub has a Mods tab on it. Spec `transforms/install.md`
section 5 (XINST-MOD-1 to -9).

**The model.** A mod is the ledger owners its tools name (`opforcore.stages.owner_of`), the level folders installed
beside the ledger with their own record, or a setting: `ww2` (`wake`, `minimap-edges`, the WW2 level folders),
`pacific-menus` (`pacificmenu`; needs `ww2` and `vietnam`), `vietnam` (`opfor.ini [patches] vietnam`, on when
missing), `onslaught:<level>` (the 14 console-port stages and the Napisal-level fixes on that level; `onslaught` the
shared `loadset` / `loading`), `mapedit` (`mapedit`, `mapedit-ai:<lv>`, the exported level folders),
`navkit:<lv>`, `stockmp:<lv>`, `base` (`import`, `manifestfix`).

- **disable** takes the mod's layers out top first through the ledger (`Ledger::uninstall_files`, new: some of an
  owner's files), after the layers built on them - their mods, and the mods needing it, named and taken only with
  `--yes` (exit 2 otherwise, nothing written). What each layer wrote is kept (`<game>\opfor\mods\blobs`, a hard link of
  the file the ledger replaces: no copy) with its write time; the level folders are moved aside (a rename: write
  times kept, which the DLL's `.opfor-tips` / `.opfor-stings` markers compare). `wake`'s own `opfor.ini` keys go back,
  the launcher's later edits kept (the WW2 install's `ini_refresh` rule, as a restate of the prior first).
- **enable** moves the folders back first, then installs the kept bytes last-out-first (`install_keep`) and sets each
  file's write time back: byte for byte and time for time. A kept layer goes back only on the file it was built on;
  else refused in words naming what to enable first - except an `.ini`, which gets the keys changed meanwhile carried
  over. The mods disabled in the same operation come back with it (`--only`: not).
- **uninstall** is a disable dropping the kept copies (the WW2 opt-out's bundleManifest repair kept as `manifestfix`).
- **restore-original** `<file> ...` takes every layer off those files (owners' other files kept), `--all` every mod
  but the settings: DICE's files (each bottom layer's prior), the installs kept for `enable`.
- **vietnam** is set through the file, every prior and value the ledger keeps (`Ledger::edit_through`, new: one
  journalled commit) and every kept copy, so no revert or enable takes the setting back.
- **verify**: each ledger file's stack (top write = file, each write = the next prior, every prior - DICE's original at
  the bottom - intact by sha256), each level folder against its record, each kept copy, files with old markers, and,
  after `bfbc2 mods baseline`, every other game file.
- **migrate**: `opforcore\migrate.py` in Rust (`Ledger::write_migrated`, new).
- **No interruption loses anything**: every check, then the kept bytes linked, then the operation journal
  (`.op.json`, every step with its data) before the first game write; each step re-runnable; the next command (or
  `resume`) finishes a killed one. `BC2_MODS_CRASH_AT=<n>`: exit 9 before the n-th write step (its own and the
  ledger's).

**Found and fixed.** OPFOR's `migrate.py` (and the port) refused a file for good after a run killed while archiving its
markers once one old backup had moved: the backups left were fewer than the records written. A file is now refused
only when a backup's owner holds no record (OPFOR `mm1-migrate` a2d6d69: `test_an_interruption_at_every_marker_move_resumes`
fails without the fix, passes with it).

### Proof on scratch copies (never `game\`)

The author's game copy's affected files - the 23 files with markers from before the ledger and their markers, the 5
WW2 level folders, the main menu bundle and its marker: 182 files, 4.19 GB - copied twice with their write times into a
temporary folder (A, B).

- **The migration, Rust against OPFOR's Python**: `bfbc2 mods migrate` on A, OPFOR's `migrate.py` on B: the plans'
  36 lines identical (23 files, 23 migratable, 0 refused), the applies' lines identical, then 288 files of each copy
  (game files, ledger records less their id and time, blobs, archived markers) identical, write times included.
  3.9 s against 3.8 s.
- **Round trips on A**, compared with B (untouched after its migration) file by file, bytes and write times:

  | Operation (then enable) | Mods taken | Game files after the enable |
  |---|---|---|
  | `disable ww2` (refused without `--yes`: nothing written) | ww2, pacific-menus | identical (2.4 s / 1.0 s) |
  | `disable onslaught` | the 6 Onslaught mods, ww2, pacific-menus | identical (24 s / 32 s: 41 layers on 40 MB bundles, each a journalled commit) |
  | `disable pacific-menus`, `disable base` | as named; base takes onslaught, ww2, pacific-menus | identical |
  | `disable vietnam` | vietnam, pacific-menus; opfor.ini `vietnam=0` | identical bytes (opfor.ini rewritten twice: its write time new) |
  | `restore-original --all` | every mod but vietnam | the 22 ledger files each its original's sha256, no WW2 folder; identical after `enable ww2` (the whole group) |
  | `restore-original Dist/win32/package.mft` | the file's 3 layers | DICE's file; `enable ww2 --only` refused in words first; identical after base, onslaught, ww2 |

- **A real kill at every write step** (`BC2_MODS_CRASH_AT`, `bfbc2.exe` ended with exit 9): `disable ww2` has 55 write
  steps (the journal, 2 kept records, the restate, 2 ledger commits, 30 level files moved, ...), `enable ww2` 72; killed
  before each, then `bfbc2 mods resume` and the command again: every disabled state the uninterrupted one's (bytes),
  every enable back to B, bytes and write times; no lock left.
- **Uninstall** (`uninstall ww2 --yes`): wake's and minimap-edges' files back, the 5 folders and their kept copies gone,
  `manifestfix` kept, `verify` 18 checked, 0 problems. Against OR1's opt-out (`bfbc2 install revert` with the last
  install plan, its paths moved onto a third copy C of B): 23 game files, 21 identical; the 2 others are the WW2 content
  the opt-out leaves - `minimap-edges`' layer on the startup bundle (a WW2 install of 2026-11-12, migrated as its own
  owner, which `install revert` does not revert) and the Pacific menus (`menu_install.py --revert`'s, outside OR1).
- **The hub's Mods tab** on A (`BFBC2_MODS_GAME`): the list, Disable / Enable of `onslaught:ons_mp_008` through the
  tab's methods, the migration's plan.
- **The real game copy**: 1,582 files, every path, size, write time and sha256 the same before and after the round.

### Tests

- `bc2-install` `tests/mods.rs` (16: every mod's round trip byte for byte and time for time on a fake game copy, the
  confirmations, the refusals, restore-original per file and whole, the setting, the ini carry-over, uninstall, verify,
  a held lock, the migration and its resume; an in-process kill at every step of a disable, an enable, a
  restore-original, a setting and the migration), the unit tests (`catalog`, `ini`, `migrate`); `bc2-cli`
  `tests/mods.rs` (the command lines' exit codes; a real kill at every step of a disable and of an enable);
  `bc2-gamefs` as before. clippy `--all-targets -D warnings` on bc2-gamefs, bc2-install, bc2-cli; every tool group of
  `bfbc2.exe` alone (18, `mods` new, in CI's list); `fmt`.
- OPFOR (`mm1-migrate`): opforcore 84 of 84 (6 skipped: no game copy in a worktree).

### Left

- The editor's Help > Diagnostics entry: waits for IN1's doctor.
- One map editor export disabled while the others stay: the shared files are one owner's patch for every export
  (`mapedit`), so `mapedit` goes as a whole; per level needs `mapedit_install.py`'s rebuild or IN1's slots.
- A kept layer goes back only on the file it was built on (a settings file excepted): re-basing a bundle or a
  registration text onto a changed file is IN1's registration slots.
- `mods install ww2` (the staged plan through `bfbc2 install apply --staged`) and `mods install onslaught:<level>`
  (OPFOR's `console_port.py`) hand over to the installers; neither was run here (they write the real game copy).
- In game: nothing played (in-game tests paused); the write times of the DLL's markers are kept by construction.
## OR3a: OPFOR in Rust, stage 3a - the shader Merger (2026-11-22, branch `or3a`, with Project OPFOR's `or3a`)

### What moved

The Python `shaderdb.Merger` (audit AU3 AR-5, ARC-9; found too big for QW2) is retired. `bc2_formats::shaderdb::merge`
had `add_donor` and `finish` (round SD1's command-line merge, XF1's export); it now also has:

- `clone_entry` / `clone_entry_from_self`: the Python `clone_entry` rule for rule - the checks first (the entry, each
  program's indices, record lengths, new ids, fragments, layouts, the texture renames against both parameter blocks),
  with the same messages and order, then the programs with ids MD5(new name + old id), keys naming the new entry and
  retextured blocks; atomic.
- `retexture` (simultaneous, whole-name renames in a field's span; hits counted only when every rename fits),
  `name_guid`.
- `add_part` / `add_part_from_self`, `add_donor_kept`.
- Donors with a lifetime of their own: what a merge takes from a donor read for one call (records, shaders, layouts,
  fragment and entry GUIDs, keys) is copied into a `Store`; a donor that lives as long as the file (`add_donor`: the
  command line, the export) is still borrowed.

`bc2_native.ShaderMerger` (`rust/bc2-py/src/shaderdb_merge.rs`), made by `shaderdb.merger(base, need, all_names)`,
holds the merger and its arena (bumpalo) in one value (self_cell) with a copy of the base. It keeps the Python
merger's contract with the base, which OPFOR relies on: every record a call adds is appended to the base's own lists
at once (OPFOR reads `base.entries`, `base.layouts` and `entry_textures(base, ...)` between calls in `_autoclone`,
`register_parts`, `wake_translate.add_foreign_entries`), the programs, keys and groups replaced at `finish`; a donor
that *is* the base reads the base as merged so far (the Python's `donor.entries` held the new entries while its
`groups` did not: a clone of an entry cloned earlier in the same merge has no programs - kept for parity, see
below). Donors: read from `to_bytes()`, the last 4 kept, each known by its object (a weak reference: the merger keeps
no donor alive, so `wake_shaders --clone`'s one-level-at-a-time reading still frees each level) and a stamp of its
section lists and items by identity (a donor changed between calls is read again). `shaderdb.merge` (the
`add_donor` loop) runs on it.

Project OPFOR: the 21 sites (`wake_install` 10, `wake_translate` 2, `wake_shaders`, `ww2_level_build`,
`detail_layers`, `minimap_edges`, `rg_projectiles`, `shingunto`, `type100`, `type38`, `console_markv`) call
`S.merger(base)`; `test_ww2_billboards` patches `merger`.

### Proof

- **The Python merger's recorded answers** (`bfbc2-shader-tools/tests/test_shaderdb_merge.py`): recorded from
  `shaderdb.Merger` at `8ce3cd9` before the deletion (loaded beside the checkout's `shaderdb.py`), then answered by
  the Rust merger: each call's value or error (type and message), then the base's bytes and the merger's lists
  (added, report, conflicts, part_conflicts, shared_programs, entry_names, part_names) after every call, the merged
  bytes and the donors' (unchanged) after `finish`.
  - built Dx9 / Dx10 / Dx11 / Xbox 360 databases: clones with a swap and a chain of renames, from the base itself
    and of an entry cloned in the same merge, every refusal; parts (new, again, another layout, no layout, from the
    base); donors with shared and conflicting programs and part conflicts, `all_names`; a kind mismatch; the program
    limit at 0xfffe + 1 / + 2;
  - damaged donors (11 kinds): program indices, a parameter block index and entry groups past their tables, keys
    naming fragments / layouts no file has, a duplicated program, two entries of a name, a fragment named twice; and
    three a file cannot hold (a shader's table past its end, a short key, a short program);
  - 240 seeded random cases (random kinds, entries, textures, damage and 2-9 calls each).

  **271 cases, 1,709 calls: identical**, but for three kinds of answer decided against the Python (each allowed by
  name in the comparison): an `IndexError`'s message (the Python's was Python's own), a name that is not latin-1
  (`ShaderDbError` in words; the Python raised `UnicodeEncodeError`), and a donor only memory holds (read from its
  bytes and refused whole with a `ShaderDbError`; for those cases only `clone_entry` is compared - the Python took
  the layouts and entries of such a donor as far as they went).
  The game copy (`--game`): ONS_MP_008 with 12 of mp_008cq's entries cloned (swapped or shortened texture names), 4
  of its own entries cloned from itself, 20 part layouts and the flags' `add_donor`, Dx9 / Dx10 / Dx11: **3 cases,
  114 calls identical**.
- Rust tests (`merge.rs`): retexture's renames, a clone's ids / keys / textures, every refusal adding nothing, the
  self clone's empty entry, parts, a donor dropped after its calls.
- Only the Rust merger (`native_only`): the base holds a new entry before `finish`, a donor changed between calls is
  read again, no donor is kept alive, a part name that is not latin-1 is refused with nothing added, no call after
  `finish`.
- **WW2 rebuild**: OPFOR's main checkout on its branch `or3a` (the 21 sites moved; 6d8a98f), `OPFOR_TOOLKIT`
  a scratch folder of junctions (`bfbc2-tool-suite` to this worktree with its fresh `bc2_native.pyd` and `bin\bfbc2.exe`,
  version 0.18.0; `bf1943-tool-suite`, `operation-rising-sun`), `--jobs 2`: all 55 steps in 18.9 min (the 3 level
  chains side by side in 410 s; other rounds building beside it). `compare_oracle`: **10,852 of 10,852 identical**
  (differ 0, missing 0, extra 0); `qa_checks.py` passed. The steps that merge: `wake_shaders` (10 s), `detail_layers`
  (14 s), `wake_shaders --clone` (59 s), the weapon ports' dry runs, `wake_translate`, `minimap_edges` and the level
  builds' shaders steps.
- **Install oracle**: `wake_install.py --plan` (the coordinator's conditions: its code path read first - the build stages under
  `work\ww2`, `bfbc2 install plan` writes nothing, the game copy has no ledger to roll back - and the game copy's 1,582
  files, 11.6 GB, hashed with path, size, time and sha256 before the rebuild and after the plan: identical), 6.7 min:
  `compare_oracle --install` **21,197 of 21,197 identical** - wake_install's 10 merges (undergrowth stand-ins, tree
  billboards, the clone lists, the partly covered meshes, terrain materials, autoclone's two passes, the edge
  half-instancing re-sort, part registration, grafts, layout fixes) on all four levels.

### Timings

The game cases' calls alone (ONS_MP_008 + mp_008cq: 12 clones, 4 clones from the base, 20 parts, one donor,
`finish`; best of 3, the loads left out): Python merger 0.23 / 0.23 / 0.24 s (Dx9 / 10 / 11), Rust 0.15 / 0.36 /
0.20 s. At this size the calls are not where the time goes: the Rust merger reads the base and each donor through
`to_bytes()` and a parse, and appends what it adds to the base's Python lists, which costs what the Python's indexing
cost. The gain is memory and one implementation (the command line, the export and OPFOR's 21 sites on the same
code); OPFOR's merging steps took the same time in the rebuild as in PR1b-2's.

### Decided, and found

- **A clone of an entry cloned in the same merge has no programs.** The Python read `base.entries` (the new entry
  there) and `base.groups` (its programs not yet) - an entry that draws nothing. Kept for the oracle; OPFOR's
  `_autoclone` can reach it (a material found among `m.entry_names` and cloned from the base in the same pass). Left
  for a DICE decision (PY1): refuse it, or finish before the second pass's clones.
- The level cover index stays Python: its pickle cache is one of the caches PR1b-2-rest gives content keys
  (`shadercover`); moving it now would change the cache format twice. `retexture` stays Python (OPFOR's own block
  edits in `wake_terrain` / `ww2_char_shaders`; the merge's is the Rust one).

### Tests

- `cargo test -p bc2-formats`: 190 unit tests (6 new in `merge.rs`) and the integration tests pass; clippy
  `--all-targets -D warnings` on bc2-formats, bc2-py, bc2-cli, bc2-levelwrite; each of `bfbc2.exe`'s 17 tool groups
  alone (`--no-default-features --features <g>`): clean.
- `test_shaderdb_merge.py` (quick, and `--game`), `test_shaderdb.py` (its clone and merge checks on the new merger;
  the program-limit check moved to the merge test's real 0xfffe cases).
- `python run_tests.py quick -j 2`: 131 of 131 passed in 1,452 s (the new `shaderdb-merge` entry 1.4 s), O2 among them.
- `spec/trace.py --check`: 1,247 rules (SHDB-LVL-3 new: a cloned entry); ruff and mypy clean on the changed Python.
- Project OPFOR (`or3a`): OPFOR's quick tests from a worktree of `or3a` with this suite (`OPFOR_TOOLKIT`): 361 passed, 6 failed, 8 skipped. The 6 are the worktree's: 4 need the game copy and `work\` (a worktree has neither: `nam_common` and the loading pictures missing), and `test_types` onslaught / ww2 run mypy with `pyproject.toml`'s `mypy_path` fixed at `..\bfbc2-tool-suite` (main, 0.17.0, no `shaderdb.merger`); mypy with that path pointed at this worktree: onslaught 20 files and ww2 82 files, no issues. `test_ww2_billboards` (the patched `merger`) and every merging test pass. ruff clean on the changed files.

### What is left

- The level cover index (`level_cover_prefixes`, `donor_levels`) with PR1b-2-rest's content keys; `retexture`'s
  Python twin; the rest of stage 3 (OPFOR's shader glue in `wake_shaders` / `wake_translate` / `ww2_char_shaders`).
- The self-clone accident above (PY1).
## CI1: GitHub's checks green (2026-11-22, branch `ci1`)

GitHub's `checks` run (`python run_tests.py quick --no-game --ci-steps -j 2` on windows-latest) failed five entries
on f4071e3. Their causes, found by reproducing the runner on the development PC: a fresh `git clone` on another drive
(`subst Q:`) than an 8.3-short TEMP on C:, no Project OPFOR above it, no `OPFOR_` / `BFBC2_` / `BC2_` / `BF1943_`
variables, no ffmpeg on PATH, CI's variables, and `OPENBLAS_CORETYPE=Haswell`. That simulation failed the same five
entries with the same counts as the runner's annotations (routes 4950 of 10000, texture.check 1090 of 1200).

| Entry | Root cause | Fix |
|---|---|---|
| green crates (`test_rust_skip.py`) | `os.path.relpath` across drives raises (checkout D:, stand-in workspace in TEMP on C:) | `test_rust._input_name`: falls back to the checkout |
| O5 stockmp | validate's log records name the temporary folder: the PC's long path recorded, the runner's 8.3 `RUNNER~1` | `<temp>` in long and short spellings; goldens rewritten |
| F2a texture.check | `np.corrcoef` is a BLAS dot: OpenBLAS's kernel (SkylakeX here, Haswell / Zen there) changes its last bits | exact correlation in the core; 135 + 744 answers re-recorded |
| E3o routes | `Route.straight` was `np.linalg.norm` (a BLAS `ddot`), same cause | the estimate's formula (`navpath.distance`) |
| O3 audio | ffmpeg missing on the runner | not this round's: PR1b-1c's Rust XMA2 decoder |

The OpenBLAS kernel is the root cause, not the C runtime: NumPy 2.5's own float64 ufuncs dispatch only up to X86_V3 on
Windows (no AVX-512 / SVML paths), and `np.exp` / `log` / `log10` / `sin` / `cos` / `arctan2` / `power` / `hypot` /
`tanh` / `arccos` gave the same bits with AVX-512 disabled; `np.corrcoef` (random integer arrays) and `np.linalg.norm`
of 200,000 random 3-vectors changed bits with `OPENBLAS_CORETYPE` (SkylakeX = this PC's default; Haswell = Zen;
Prescott, Nehalem each different). The Rust module imports from the C runtime only `fma`, `pow`, `powf`, `atan2`,
`atan`, `sin`, `cos` and `fmod` (its import table); `fma` (f64::mul_add without the `fma` target feature) defined
NavKit's route estimate, now `libm::fma`. None of the others reached a failing entry.

Re-recording: `texture.check_reference`'s replayed answers were rewritten only where the exact correlation differs
from the recording, and each rewritten answer was first proven to be the old recording with NumPy's corrcoef put back
(the worst, same and total unchanged): 135 of 1,192 quick answers, 744 of 8,582 game answers (a texture against
itself: exact 1.0 where this PC's OpenBLAS gave a value a few units in the last place from it). The recorded routes
did not change: the estimate's formula is np.linalg.norm's bits on the recording PC.

### Tests

- The runner simulation on the branch: 123 of 124, the one failure O3's audio group (182 lines; PR1b-1c's).
- `run_tests.py quick -j 2` in the worktree: 129 of 130; the one failure `dbx-write-native`'s DbxEdit round trip
  reading a work\ww2 file another round's WW2 rebuild was rewriting at the time (the entry passes alone).
- texture.check with `OPENBLAS_CORETYPE` default, Haswell and Prescott: 1,200 of 1,200, 0 wrong replays each; E3o's
  routes 10,000 of 10,000 with default and Haswell; O5 stockmp 73 of 73; green crates with TEMP on another drive.
- cargo fmt; clippy `--all-targets -D warnings` on bc2-convert, bc2-navbuild, bc2-py, bc2-cli and each of
  bfbc2.exe's 17 tool groups alone; cargo test of those crates (texcheck's new `correlation_exact` among them);
  ruff, mypy; `spec/trace.py --check` (XCONS-PAR-2 new).
## PR1b-2-rest: the mesh viewer on its Rust port, typed errors, the anim and model tools in part (2026-11-22, branch `pr1b2r`)

What PR1b-2 left ("PR1b-2", "What is left"), less sounds in process and the cache content keys / OPFOR's caches
(round PR1b-1c's). Version **0.19.0** (0.18.0 went to rounds OR3a and MM1, merged first; removals: the pickle cache, `meshview.py`'s Python index / reader / renderer,
`bc2-convert`'s `pickle-cache` feature, `granny.py`'s curve reader, LevelDiff's command line moved; CHANGELOG).
Nothing was written to OPFOR's `game\`; the game never launched.

### 1. The mesh viewer's Python API through `bc2_native` (audit AU5 ARC-4)

| was (`meshview.py`) | now |
|---|---|
| `Index._build` / `_build_reference` / `_add_database`, the pickle `work\tools_out\meshview_index.pkl` (`CACHE_VERSION` 3) | `bc2_native.meshview_open(bundles, cache, say)` -> `MeshViewIndex` (`bc2_gamefs::meshindex::load`): the cache `meshview_index.bin` (magic `BC2MESHIX`, version 4, the bundles' signature, the four tables, the databases read, a SHA-1 of it all), built under its lock on all cores, not cached when something was left out; `where` / `parts` / `entries` the same dicts, made on first use (`MeshViewIndex.tables()`); `Index.locate` / `partitions_of` in Rust |
| `load_mesh`, `_read_part` (+ `meshview_part`), `_values`, `_texture`, `mesh_surfaces` | `MeshViewIndex.load_mesh` / `mesh_surfaces` (`bc2_convert::meshview`); the dicts' arrays made from its bytes (writable copies, as before) |
| `stack_models`, `render` (NumPy, triangle by triangle) | `bc2_native.meshview_stack` / `meshview_render` (`bc2_convert::meshview::render`) |
| kept in Python | `_bundles` (OPFOR's step tracker sees the walk and the bundle list in `meshview_open`'s arguments), `read` / `read_many` with the partition cache (they read through `fbrb`, already Rust), `page()` (Project OPFOR's WW2 preview pages: `attachments.py`, `scopes.py`), the command line |

`bc2_gamefs::meshindex::build` now leaves out what it cannot read and names it (`skipped`): the Python reference
skipped an unreadable bundle or database but raised on a damaged database record; neither happens on DICE's files.
`bfbc2.exe meshview` / `dbxcatalogue` build a missing index in Rust too (they handed that to the Python).

**Parity on the real game copy** (a scratch script: the retired `meshview.py` archived from `8ce3cd9`, its pickle
read as OPFOR's tools had it, against the new module; the new index built into a scratch cache):

| what | result |
|---|---|
| the index (`where`, `parts`, `entries`: 166,054 / 26,846 / 9,776 rows) | the same items in the same order |
| `load_mesh` of 405 meshes (5 fixed - the Garand, the Type 5, the M18, a soldier, the M1A2 - and 400 sampled with a seed), LODs 0 and 1 | **1,347 of 1,347 parts identical** (names, surfaces, textures, every array's dtype, shape and bytes, the images); `mesh_surfaces` the same |
| `stack_models` of 120 groups of 1-3 meshes | identical offsets |
| `render` side / top / front at 300 / 400 / 640 | **360 of 360 pictures identical** after the fix below |
| time | index from its cache 0.3 s (the pickle 0.2 s), built 2.1 s; `load_mesh` 12.1 s vs 33.2 s; `render` 8.6 s vs 196.2 s for 360 pictures |

The renderer's shading was one level off on some triangles (9 of 360 parity pictures, 1 to 28 pixels each): the facing's length
is NumPy's `np.linalg.norm` of a float32 vector - OpenBLAS' `sdot`, each float32 square added in a double - and its
dot with the light `ddot` with fused multiply-adds; the port summed in float32 and multiplied plainly. Found on
19,045 real triangles (all equal now; the float32 sum missed 1,070 lengths, the plain dot 5,968); the editor's asset
preview draws with the same code. A part whose positions are float64 (OPFOR builds some) is drawn in float32 now:
59 of 120 such test pictures differ from NumPy's float64 drawing (previews only; none is a build output).

Removed: `tests/test_f4_native.py`'s `meshview` area and its golden (the part reader it compared is the port's
whole). `core/tests/cli_o3_meshview.py` writes the `.bin` from the format (an independent writer of it);
`cli_o3_meshview` / `cli_o3_bf1943` goldens re-recorded from the Python command lines (only the cache file changed):
98 of 98 and the bf1943 group as recorded.

### 2. Typed errors, LevelDiff, one OPFOR finder

- `bc2_levelwrite::culltool`: `CullError` (`CullTree`, `Io`, `Mkdir`, `Damaged`, `Sink` for a caller's log that
  failed) and `CullResult` in place of the Python emulation's `Fail`; `From<CullError> for Fail` gives bfbc2.exe's
  culltree the same exceptions. The cull group: **190 of 190** as recorded.
- LevelDiff's command line (`Run`, `parse_args`, `run`, `USAGE`) in `bc2_tools::leveldiff` (the JSON through
  `bc2_gamefs::atomic`); the comparison stays in `bc2_formats::dbxedit::leveldiff` (the editor's Diff panel).
- `bc2_gamefs::game::find_opfor` / `opfor_candidates` / `walk_up` (`Beside::Only`, `Beside::Above`,
  `OpforNotFound`): bc2-cli's gameindex, shader, navshared (navkit / navtools / harness), stockmp, bc2-menu and the
  editor call it (six copies gone); each keeps its own message where the Python's differed.

### 3. The anim and model tools

- `granny.curve_keys` / `sample`: `bc2_convert::granny_edit::curve_keys` / `sample` (`bc2_native.granny_curve_keys`,
  `granny_sample`); `_trunc_scale`, `_quat`, `bits_of` removed. `bfbc2-anim-tools/tests/test_granny_curves.py`
  keeps the retired Python as the reference: built curves of every format (3,607 curves, 42,192 samples) and, with
  `--game`, the WW2 tools' animations and every 8th other one of the game copy: see Tests.
- `texwrite.py`'s packing (`resource`, `texture`'s header, `wrapped`, `l8_bytes`): `bc2_convert::texwrite::resource`
  / `wrapped` / `l8_bytes`. Measured: `sample` through `bc2_native` costs 16 us a call against the Python's 3 us (the
  keys are converted on every call; OPFOR samples a few tracks a frame, so the rebuild's time did not move): a batch
  call (keys once, many times) is the fix if it ever matters. The DXT encoders' principal axes stay `numpy.linalg.eigh`'s (OpenBLAS, processor
  dependent: round F2a).

### 4. The editor's install run on `Task`

`export_ui::Install`'s run is a `Task<(bool, String)>` (its DF1 test hands it a receiver of `Result`s); a panic
reads "The install: internal error (...)". The last of PR1b-0's hand-made workers but the two pipe readers.

### WW2 rebuild

OPFOR main `031d80e` (OPFOR_ROOT that checkout, `OPFOR_TOOLKIT` a scratch folder whose `bfbc2-tool-suite` junctioned
to this worktree - its fresh `bc2_native.pyd` and `bfbc2.exe`, version 0.18.0 - and `operation-rising-sun` /
`bf1943-tool-suite` to the real ones; `BFBC2_FBRB_CACHE` a scratch folder), `--jobs 2`: all 55 steps in 39.5 min
(the 3 level chains side by side in 477 s; the machine shared with a cargo test build). `compare_oracle`: **10,852 of
10,852 identical** (differ 0, missing 0, extra 0); `qa_checks.py` passed. Every WW2 tool's `meshview.Index`,
`load_mesh`, `render` / `stack_models`, `granny.curve_keys` / `sample` and `texwrite` packing ran on this round's
Rust. OPFOR's `SUITE_REQUIRES` (< 0.18.0) warned, as expected (OPFOR branch `pr1b2r` moves it to 0.18.0 - 0.19.0).

### Tests

- `cargo fmt`; `cargo clippy --all-targets -D warnings` on bc2-gamefs, bc2-convert, bc2-tools, bc2-formats,
  bc2-levelwrite, bc2-menu, bc2-py, bc2-cli, bc2-editor; CI's per-group clippy of `bfbc2.exe` (all 17 groups in
  `[features] default`, each alone): clean.
- `cargo test` of those crates (but bc2-py): 30 test binaries, all pass (`meshindex`' cache round trip and damage,
  `game`'s finder, `texwrite`'s packing, the editor's DF1 install test on `Task`).
- `run_tests.py quick -j 2`: 129 of 130 at first - the repository rules wanted `test_granny_curves.py` in a tier
  (added: `granny-curves`, `granny-curves:game`); then both pass (131 entries).
- `test_cli_o3.py`: the meshview group 98 of 98 and the bf1943 group as re-recorded from the Python command lines
  (only the cache file changed: `REKEY` takes the `.bin`'s SHA-1 trailer out, which hashes the case folder's paths);
  the cull group 190 of 190 and the caches group as recorded (unchanged).
- Game tier by hand: `test_meshview.py` (the Garand's parts, the soldier's coordinates, the picture), `test_granny.py`
  (shifts, big-endian), `test_granny_curves.py --game` (1,126 animations - the WW2 tools' and every 8th other - 231,340 curves, 2,233,706 samples: all equal, after one fix: Python's `q ** 2` in `_quat` is the C library's `pow`, which differs from `q * q` in the last bit for some values - 600 of the sampled orientation curves were off with `x * x`; the port calls `powf` with the exponent kept opaque from LLVM, which would turn it back into `x * x`; the WW2 rebuild above ran before this fix and still matched the oracle), `test_texwrite.py`, `test_model_f2a_native.py`
  (6 of 6), `test_f4_native.py` (7 of 7), `test_deep_scan_fixes.py` (the unreadable bundle said, not cached).
- `rust\native_census.py --check`: 273 exports, 0 without a caller (recorded again: OPFOR no longer calls
  `meshview_index`); `python_status.py` regenerated; `spec/trace.py --check` ok (stockmp's citation moved with the
  finder); ruff and mypy on the changed Python.

### What is left (PR1b-2-rest2)

- **Merge step 3**: culltool's `Doc` and the export's cull-tree walker on `EditDbx` (not started: 2,600 lines of the
  Python emulation's codec to re-seat, proved by the Onslaught bundles' and the editor exports' cull trees).
- **bc2-menu's typed errors** (220 uses of the Python emulation's `Fail` in the menu tools' library and aptgraft).
- **rayon in bc2-cli's own CPU work** (not measured this round).
- **`decimate.py`, `shapes.py`**: their quadric costs and tangents are NumPy matrix products through OpenBLAS; the
  bits must be matched first (as the renderer's were here), with a parity test on OPFOR's weapon LODs.
- **`Granny.put`**: a write into the Python reader's buffer; it moves when the Granny object is Rust's.


## PR1b-1c: sounds decoded in Rust, content keys, the export's one walk (2026-11-22, branch `pr1b1c`)

What PR1b-1-rest left (above, "What is left"): items 1, 3 and 4 done, item 2 not started (left, below).

### Item 1: an XMA2 decoder (`rust/bc2-xma`)

FFmpeg 9.0's `xma2` decoder ported to Rust, operation for operation: the WAV demuxer's packets (2,048-byte XMA
packets grouped by `ff_pcm_default_packet_size`, so a damaged packet takes the rest of its demuxer packet with it,
as in FFmpeg), the XMA wrapper (`xma_decode_packet`: its sample FIFO, the 64 samples `decode.c` skips, the trims at
the end), `wmaprodec.c` (tiles, channel transform, scale factors, the vector and run-level coefficient codes, the
frame buffer with its stale bytes past a frame, the safe bit reader's clamping), FFmpeg's C inverse MDCT
(`ff_tx_mdct_inv_float_c` over the split-radix `fft{n}_ns` codelets, same tables, same operation order, no fused
multiply-adds), the sine windows, and libswresample's float to 16-bit conversion. The Huffman tables are generated
from `wmaprodata.h`. Being a translation of LGPL code, the crate is under FFmpeg's licence (`LGPL-2.1-or-later`,
`rust/bc2-xma/README.md`), not the suite's MIT.

**Against ffmpeg**, every XMA stream of the two sound sets (BFBC2's unpacked Xbox bundles: 1,847 streams;
Battlefield 1943's Waves with their streamed segments: 2,810), 1,039,009,766 samples, through
`rust/bc2-xma/examples/xma_compare.rs` (ffmpeg's outputs kept in a scratch folder, deleted after):

| reference | streams identical | samples that differ | largest difference |
|---|---:|---:|---:|
| `ffmpeg -cpuflags 0` (its C code) | 4,657 of 4,657 | 0 | 0 |
| `ffmpeg` (its default run: SIMD transforms on this CPU) | 364 of 4,657 | 169,447 (0.016 %) | 1 |

ffmpeg is not deterministic across machines: its default run picks AVX2 / FMA FFT codelets by the CPU, which add the
same terms in another order; on a 47-stream sample its own default and C-path outputs differed on 1,069 of 7.8
million samples, each by 1. The C path is the reference (spec rule XMA-DEC-1, coordinator's ruling 2026-11-22): the
game decodes in the Xbox 360's XMA hardware, which no tool reproduces, and a difference of one in the last bit is far
below the error of EA-XAS's 4-bit ADPCM that the samples are encoded into.

**Whole conversions** (every Wave of both sets; `test_audio_native.py --game` checks the same with ffmpeg present): `audio.convert` (the Rust core, the
in-process decoder) against `audio.convert_reference(..., ffmpeg=True)` (the Python, ffmpeg's C path): 4,430 PC Waves
byte for byte the same, 34,133 Waves the same `AudioError` (streamed stubs without their stream, codecs other than
EA-XMA). Against the conversions recorded with ffmpeg's default run (`tests\audio_golden.json`), 1,104 of the 4,430
Waves now give other bytes (each where a sample moved by one changed the encoder's choice somewhere); the others are
the same bytes. The recording's "now" answers hold the 1,104.

**Speed:** the 1.04 billion samples decode in 18.2 s on one thread (57 M samples/s), where one ffmpeg process per
stream took about 2 s of process start and pipes each.

**Where it runs:** `bc2_convert::wavport::decode_xma_part` (the EA framing's `convert` / `decode`), `bc2_native`'s
`audio_convert` / `audio_decode` (their `ffmpeg` argument and the `'ffmpeg'` answer gone) and `audio_xma_decode`
(new: `audio.xma_decode`), `bfbc2.exe audio` (its ffmpeg lookup and pipes gone). `audio.py` runs no ffmpeg unless
asked: `decode_xma` / `decode_reference` / `convert_reference` take `ffmpeg=True` for the tests' reference, and
`ffmpeg_decode` runs ffmpeg with `-cpuflags 0`. A stream that decodes to nothing is silence padded to the header's
count (the ffmpeg run raised an `AudioError` for an empty output). `AudioError.what_to_do` no longer says ffmpeg.

**The recordings:** `core/tests/cli_o3_audio_golden.json` (182 command lines) recorded again from `audio.py` with no
ffmpeg on `PATH` and no `$FFMPEG` (GitHub's runner has none; 60 of the 182 had needed ffmpeg); the four ffmpeg cases
renamed (`no ffmpeg on PATH`, `FFMPEG names nothing` ...): they convert now. `test_audio_native.py`'s game recording
("now" answers) recorded again from the new decoder after the proof above; with ffmpeg present its `--game` also
checks each conversion against `convert_reference` on ffmpeg's C path.

**WW2 rebuild** (OPFOR main 7347817 with branch `pr1b1c` - its two cache-budget calls - checked out detached,
`OPFOR_TOOLKIT` a scratch folder whose `bfbc2-tool-suite` is junctioned to this worktree, `BFBC2_FBRB_CACHE` a scratch
folder, `--jobs 2`, while other rounds' builds ran): all 55 steps in 31.2 min, `qa_checks.py` passed.
`compare_oracle` against the recorded oracle: 10,548 of 10,852 identical, 304 differ, 0 missing, 0 extra. The 10,051
files that are not sound Waves: all identical. The 304 that differ are all `*_wave.res` Waves, and every one of the
rebuild's 801 Waves has the SHA-1 of a conversion proved equal to `convert_reference` on ffmpeg's C path (above). The
oracle's sound Waves were recorded again (OPFOR, coordinator's ruling 2026-11-22: the old ones came from ffmpeg's
CPU-dependent SIMD run; the old oracle kept as `data\ww2_rebuild_oracle_2026-11-22_ffmpeg-simd.json`): 10,852 of 10,852
identical since. The install oracle (`wake_install --plan`'s staging, 21,197 files) likewise: a `--plan` with main's
suite, the rebuild's Waves put back to their old bytes, reproduced it 21,197 of 21,197; against that staging this
branch's differs in 1,044 Waves and the 24 bundles and reports holding them - the 35 bundles' 92,255 entries the same
but 1,056 Wave entries (each the C-path conversion), the reports only in the bundles' byte counts - and those 1,068
were recorded again (old oracle kept, dated, `ffmpeg-simd`). `game\` was sha256-identical (1,582 files) before and
after each of the three `--plan` runs.

### Item 3: content keys and the last pickles under the budget

| cache | key before | key now |
|---|---|---|
| `culltree` (reference rows; `culltreelib._RowCache`, `bc2_levelwrite::cullcache`) | level and tree file: size, `st_mtime_ns` | size and the first 128 bits of the file's SHA-256 (a signed little-endian int: the pickle's shape unchanged), `CACHE_VERSION` 1 -> 2; the Python (`hashlib`) and `bfbc2.exe culltree` (`bc2_gamefs::sha256`, SHA-NI) compute the same key and share the entries |
| `shadercover` (`shaderdb.level_cover_prefixes`) | `(COVER_VERSION, path, size, st_mtime_ns, dx)` | the file named by `(COVER_VERSION, path, dx)`, its key `(COVER_VERSION, SHA-256 of the bundle, dx)` (hashed once per process per file), `COVER_VERSION` 1 -> 2 |
| `hlsl` (`pcbuild.compile_hlsl`) | already by content since E3d: SHA-256 of the source, entry, profile, defines, name, flags and the compiler DLL's bytes | unchanged |
| `collision` (the mesh-alias cache, `bc2_scene::alias_cache`) | each XML file's and its `.dbx`'s size and time | unchanged, decided: the cache exists to index an unchanged level without opening its thousands of small XML files; a content key reads every file, which is the whole cost it saves (the reference it stores is one field of the file). A stamp miss only reads the file again, never a wrong answer kept past a change of size or time |

SHA-256 rather than BLAKE3 (the plan's): the Python reads and writes both keyed caches, `hashlib` has SHA-256 (no
new dependency, no native module needed for the key), and `bc2_gamefs::sha256` already runs it on the SHA
extensions at about 2 GB/s.

Under the budget now (`caches.PROJECT_CACHES`, `budget::SPECS`; listed, pruned, cleared, counted in
`BFBC2_CACHE_TOTAL_GB`): `dbxcatalogue` (`work\tools_out\dbx_catalogue.pkl`, 256 MB; `dbxcatalogue.load` marks it used
and prunes after writing it), `ww2qa` (Project OPFOR's `work\ww2\_qa\cache`, 4,096 MB: `qa_checks.py`'s stock and
bundle pickles, keyed by bundle stamps, so every changed bundle left an orphan behind; 2.1 GB in 148 files) and
`ww2install` (`work\ww2\install_index.pkl`, `install_templates.pkl`, `install_dbnames.pkl`, 256 MB). OPFOR's two
scripts mark a read entry used and prune after a write (OPFOR commit below); its rebuild hashes leave `_qa` and the
files at `work\ww2`'s top out, so the pruning's markers there touch no oracle file. The caches' help, its argparse
recordings and `argspecs.rs` changed by the three names (`cli_o3_caches_golden.json`, `argfmt_golden.json`,
`abbrev_golden.json` recorded again from the Python; `bfbc2.exe` matches them).

### Item 4 (ALG-5): the export's reach and closure walked once

`mapexport build`'s closure (round E3d: what the level and its base's own bundles name, copied in when hidden or
missing) and its shader reach (round XF1: every mesh the level can draw) each walked the level's partitions: the
reach read and parsed again every partition the closure had walked. The closure now parses each partition once and
keeps what the reach takes of it (`shaders::Memo`: the partitions it names, the mesh it gives entries); the reach
reads and parses only what the closure does not walk (the shared bundles' partitions). The reach's steps, order and
first-namer rule are unchanged, so its answer is the same map; a name the level's own entries hold twice is left
out of the memo (the reach takes the last, the closure walks both).

**Proof:** the cross-set export canary (`rust\bc2-levelwrite\examples\export_canary.rs`), `mapexport build` with
`BFBC2_FBRB_CACHE=0`, main's (8ce3cd9, built in a scratch worktree) against this branch's: all five bundles,
`closure.txt`, `registration.txt`, the description partition and `vo` byte for byte the same; `stage.json` and
`report.txt` differ only in their timing lines. The closure walked 1,322 partitions of the level's own and its base's
bundles (and copied 1,673 entries in), which the reach now takes from the memo; the reach's time is spent on the
shared bundles' partitions and the three databases' merges ("shaders: the reach and the merges" 16.3 s before, 16.4 s
after, within the run-to-run spread on a PC with other rounds running), so the gain here is the reads and parses
saved, not the step's time.

### What is left (PR1b-1d)

- **Item 2, not started**: `dbxparity.py` / `portparity.py` ported behind `bfbc2 dbxparity|portparity` (today their
  command lines are Rust and a run is handed to the Python; their O3 recordings pass, 102 of 102). A port needs:
  DBX values printed as `dbx.value` prints them over the element tree `dbx.parse` gives, and the BF1943 index
  (compare); for `--predict`, the Rust `dbxport` with fair pools, `BoneTables.ske01` and the catalogue's class
  defaults swapped per partition (`fair_catalogue`); for `portparity`, meshport's whole `Porter` (only its index and
  search are Rust), `mesh.parse`, `meshset.parse` and texture decoding. Each is proved by its report on the real
  files against the Python's.
- **The collision alias cache** stays keyed by file stamps (decided above); the culltree folder listing
  (`list-<sha1>.pkl`, bfbc2.exe's own) likewise: it exists to skip reading files whose size and time are unchanged.

## SPG2-rest2: reverse engineering for the specification (2026-11-22, branch `spg2r2`)

Static reading of `BFBC2Game.exe` (PC build 795745) with a scratch copy of Project OPFOR's analysed Ghidra project
(read only, headless, about 20 s a batch) and capstone sweeps of the executable's code; DICE's data through the suite's
readers. No game run (in-game tests are paused). Per item:

| Item | Found | Rules |
|---|---|---|
| `OnlyTakeableByTeam` | data +0x84, read once, by the capture update 0x726B60: the point is skipped while its progress belongs to the named team; the progress's team becomes the lowering team at neutral, so a point the named team lowers freezes at neutral (the 2026-10-29 playtest's three results); the data class's generated getters (0x6F5740 -) are dead, the reads inlined | ONS-CAP-3, GAME-CQ-10; BOT-GS-3 corrected (progress 0 - 100) |
| bots in captures | the area list 0x724060 has no AI-flag test | BOT-GS-5 |
| dirt build-up | 0x884C20: at 5 m/s and up on a material with `MaterialPropertyTerrainData`, factor += `DirtTriggerFactor` x dt (clamped 0 - 1), colour blended towards `DirtTriggerColor`; nothing lowers it; drawn as `dirtFactor` / `dirtColor` through the instance's handle table | MAT-TEX-20 |
| mount functions | 0x5657B0 fills a table (0x4E48E0 is its grow call); 0x464B10 mounts: a 0x238-byte bundle object indexing names by a case-insensitive djb2-xor hash | STRM-MNT-3 |
| award evaluator | the scoring object (made at every level start) loads `MP_Awards` / `SP_Awards` and links criteria to stats; awards move only on the per-player stat events (humans only, scoring flag) | PERS-AWD-4 |
| menus' loading roots | `LevelDescription.LoadingMovie` is a Bink video (40 of 74 DICE levels), not the Flash root; the roots stay unnamed; a `_loading` movie gets one extra player call | MENU-LOAD-1 (probe check `level_loading_movies`) |
| emitter processors | the 37 processor and 12 evaluator types and the polynomial's fields from the type information; the particle update not found | FX-PROC-4 |
| undergrowth loader | the type's field order (pairs first) is not the file's; no code touches the type's flag bytes in the terrain module: not found | VEG-UG-6 |
| one-byte undergrowth reading | `bc2-convert`'s walk reads the second string; DICE's PC visual terrains 29 -> 30 of 30 (main refuses NAM_MP_003 "runs past the end at 0x2f65c"); the 20 console twins and BF1943's three terrains convert as before | VEG-UG-2, TERR-VT-3 (tests `undergrowth_second_string_is_a_string`, `pc_visual_terrains`) |
| terrain playtest | the plan, 8 steps, `spec/transforms/terrain_edit.md` section 5 | - |

Rules 1,258 -> 1,267 (verified 619 -> 620, observed 465 -> 473).

### What is left (SPG2-rest3)

The undergrowth entry's parser (start from the `VisualTerrain` vtable 0x144FCF0's 19 slots), the particle update
(a reader of a polynomial's `Coefficients` beside `ScaleValue`; the 37-way switch 0x11DD76A is unread), the Flash
loading roots and the menus' event queue / FSCommand callback, who drains the mount table, the award listener
classes, which entities run the dirt build-up; and every playtest: the terrain plan, ONS-CAP-3's patch, a RenderDoc
capture for MAT-TEX-20, bots on a flag (BOT-GS-5).
## PR1b-1d: the parity tools in Rust (2026-11-22, branch `pr1b1d`, with Project OPFOR's `pr1b1d`)

What PR1b-1c left (above, "What is left"), item 2: `dbxparity.py` and `portparity.py` ported behind
`bfbc2 dbxparity|portparity`, then retired. Version **0.21.0** (a removal: `RELEASING.md`; not tagged here).

### What runs where

| Python (retired) | Rust | Reused |
|---|---|---|
| the command lines (`argparse`) | `rust/bc2-cli/src/bf1943/argp.rs`: Python 3.13's `_parse_known_args` for the three options and `names` in full (abbreviations and their ambiguity, `--x=v`, `-oX`, `-ho`, `--`, negative numbers, positionals around options) | `argfmt` (the texts) |
| `bf1943import.Source`, `meshview.Index` | `bf1943src::Source::open_reading` (a stale source index is read, not written: its pickle is the Python's), `gameindex::load_full` | `bc2_formats::bf1943`, `bc2_convert::meshview::Index` (`rows`, `contains`, `part_names` added) |
| `compare`, `flatten`, `instances` | `bf1943/dbxparity.rs` (Python's dicts and Counters as `OrderedMap`s, `str.strip`'s whitespace) | `bc2_formats::dbx::{parse, value}` |
| `predict`, `fair_catalogue`, `BoneTables.ske01`, `Templates.from_index`, `guids_of` | `bf1943/dbxparity.rs` (the catalogue built in process and copied as `bc2_native.DbxCatalogue` copies it; the class defaults swapped per partition) | `bc2_convert::dbxport::port` and `rules::Rules::default()`, `dbxcatalogue::build`, `granny::bone_names` (new) |
| `meshport.Porter.port` (no renames, donors, poses or shader plan: the tool reads none) | `bc2_convert::meshport::Porter` (new, over a `Source` trait) | `meshport::Index::search`, `meshconv::console_layouts`, `granny::bone_map` (new), `texture::convert` |
| `mesh.convert(b, pairs, reorder='drop', bones=...)` | `bc2_convert::meshconv::convert` (new; the Python glue keeps calling `mesh_vertices`) | `mesh::convert_vertices`, `meshdata::{parse_mesh, write_mesh}`, `strip_to_list` |
| `meshset.parse` / `write` / `lod_size`, `texture.levels` / `decode` | - | `meshdata::{parse_set, write_set}`, `texcheck::{levels, decode}` |
| NumPy in `compare_part` / `compare_texture` | `bf1943/portparity.rs`: float64 from halves and singles, row sums in order, the picture's MSE by NumPy's pairwise summation, `np.round` (multiply, round half even, divide), `.4g` / `.3f` / `repr` as Python formats them | `pyfmt::fixed`, `float_repr`, `json::dumps` |

### Proof

On the user's BF1943 copy (Project Rising Sun's `dist\xenon`) and game copy, each tool run by the Python (0434c22)
and by `bin\bfbc2.exe` with the same arguments:

| run | what was compared | result |
|---|---|---|
| `dbxparity` | 2,353 partitions, 16,249 instances: the report (229,707 bytes), `dbxparity_enums.json`, stdout | byte for byte |
| `dbxparity --predict` | 173 weapon and vehicle partitions ported three ways: the report, stdout | byte for byte |
| `portparity` | 148 models (147 ported, 1 PortError row, 1 not compared): the report, `portparity.json`, the 148 progress lines | byte for byte |
| the 147 ports themselves | every LOD's MeshData, the mesh set, every texture (SHA-256, against `meshport.Porter.port`) | identical |
| 8 filtered runs (`zero`, `corsair kurogane --predict`, names that match nothing, `uibackground parachute`, an upper-case filter, default `-o`) | files and both streams in an empty folder | the same |

`bfbc2-bf1943-import\tests\test_parity_reports.py` (game tier, `parityreports:game`) keeps the digests
(`parity_reports_golden.json`, the Python's) and checks every run is answered in Rust. The library moved to
`dbxparitylib.py` gives `instances` the same answers on 2,730 stock partitions (every `weather/` one and the beach
vehicles': one SHA-256 over the reprs, old module against new).

**O3's bf1943 group** (`core\tests\test_cli_o3.py --group bf1943 --who`): 102 of 102 as recorded; dbxparity 36 of 36
and portparity 36 of 36 answered in Rust (Python 0; 24 of their 72 were handed over before: runs, `--`, attached values and
positionals around options were handed over). Left with the Python: dbxcatalogue's 8 lines that build the catalogue's
cache (a stale, damaged or old cache, a stale index) - that tool's pickle build, not this round's. `test_argfmt` 943 of
943, `test_abbrev` 193 of 193 (all `dbxparity` / `portparity` lines Rust). `argp.rs`'s 1,200 random command lines
(`argp_cases.json`) give argparse's answers.

### Decided against DICE (PY1's rule)

- **A float2 element** (format 0x02, 8 bytes) is compared as two singles (spec XWW2-PAR-3). The Python's `DTYPE` read
  it as three (12 bytes, the next element's first four with it). No compared part has one at stream 0: run with the
  Python's table changed, its reader saw 0 float2 elements and the report was the same.

### Speed (release `bfbc2.exe`, the same PC, other rounds building)

| run | Python | Rust |
|---|---:|---:|
| `dbxparity` | 14 s | 4.9 s |
| `dbxparity --predict` | 12 s (the catalogue from its pickle cache) | 26.7 s (the catalogue built in process: about 20 s) |
| `portparity` | 7 s | 6.7 s |

`--predict` is slower: the Python unpickles the catalogue cache (`work\tools_out\dbx_catalogue.pkl`), the Rust builds
the catalogue from the game copy each run (the pickle holds Python `dbx.Element` objects; reading it in Rust is left).

### What is left

- **dbxcatalogue's cache build** in Rust (the 8 O3 lines above): the pickle of `Catalogue` with `dbx.Element` trees,
  which Project OPFOR's modules unpickle; with it `--predict` could read the cache instead of building.
- **`meshport.py`'s Porter** stays the WW2 builds' (renames, donors, poses, the shader plan); the Rust `Porter` covers
  what the parity tool reads. Moving OPFOR's ports onto it is OPFOR in Rust stage 2.

## PR1b-1e: the DBX catalogue's cache in Rust (2026-11-22, branch `pr1b1e`, with Project OPFOR's `pr1b1e`)

What PR1b-1d left ("What is left", first item). Version **0.22.0** (a file format removed, the pickle; and
`bc2_gamefs::budget::maybe_prune`'s signature: `RELEASING.md`; not tagged here).

### The file

`work\tools_out\dbx_catalogue.bin`, `bc2_convert::dbxcatalogue::cache` (format version 1): the magic, the version, the
key, one string table (736,760 strings on the user's copy), `Built`'s tables in their insertion order, the 463
partitions whose elements the catalogue keeps (65.0 MB as they are, each deflated at the fast level), the executable's
identifiers, a SHA-256 of all before it. 78.5 MB on the user's copy (the pickle: 72.8 MB; without the deflate 116.0
MB). Read by `bfbc2 dbxcatalogue`, `bfbc2 dbxparity --predict` and `dbxcatalogue.load` (through the new
`bc2_native.catalogue_load`: the same `Catalogue`, its tables and `dbx.Element` trees made as `catalogue_build` makes
them); written by whichever builds it first, under the file's lock (`bc2_gamefs::atomic::locked`, which excludes the
Python's `atomicio.locked`), then the `dbxcatalogue` cache folder and the overall budget pruned. An incomplete
catalogue (a read problem) is used and not cached, as before.

**The key, by content** (as PR1b-1c keyed the culltree rows and the cover index, but not by hashing whole bundles: the
catalogue's 143 bundles are several GB, more than the catalogue costs to build): SHA-256 over each bundle's path below
the project folder, the entry names the index gives it (in order) and its content key - its length, the SHA-256 of its
first 64 KiB or its whole compressed header if longer, and its last 8 bytes, which are the gzip trailer (the CRC-32 of
the whole decompressed payload and its length) - and the executable's SHA-256. Checked on the user's copy: of the 143
bundles 142 are zipped with the gzip member ending the file (each decompressed to its end), one is stored, and a
stored bundle (or anything that is not a bundle) is hashed whole. The key takes 0.06 s. A copied or moved game copy
keeps its catalogue (the paths are relative); a bundle rewritten with other bytes at the old time does not. The bundle
cache's points files are keyed the same way (`bc2_gamefs::bundles::ContentKey`).

The project folder is never a native call's argument: `bc2_native.catalogue_load` / `catalogue_key` take it from the
cache file's path, because Project OPFOR's rebuild tracker (`stepstamps_hook`) stamps every file under any folder a
native call is given (it skips `work\tools_out`).

### The 8 O3 lines: answered in Rust

A chunk the pass cannot read made the Rust pass refuse, so the Python's reference ran and wrote its problem. The pass
now writes the reference's problem itself - `<bundle>: <n> partitions not read (<exception>: <text>)`, the exception as
`bc2_py`'s `fbrb_error` and PyO3's `io::Error` conversion raise it (`ValueError` for a damaged bundle, `KeyError` with
Python's `repr` for a missing entry, named in the order the reference asked; `FileNotFoundError`, `PermissionError`
... for the operating system's errors) - in the walk's order among the parse problems.
`core\tests\test_cli_o3.py --group bf1943 --who`: dbxcatalogue 32 of 32 Rust, Python 0 (the 8 lines were Python's);
104 of 104 as recorded, `--live` the same both ways. The catalogue's lines recorded again from the Python
(`--record --only catalogue`, 32: its cache file is the new one; every stdout and stderr the same text as before, now
through the Rust pass), one line added (`catalogue, only the retired pickle`); the 72 parity lines' `files` digest moved
with the case folder's cache file (they read none of it; their recordings are the retired Python's otherwise).

### Proof

- **The same catalogue**: `dbxcatalogue.load` through the new file against the retired pickle (main's, current for the
  copy), every table equal and in the same order (`classes`, `order`, `shapes` / `texts` / `numeric` with their
  Counters' order, `guids`, `primary`, `exe_names`, `partitions`, `problems`), every kept element equal as a tree
  (`protos`, `defaults`, `others`), and the 341 prototypes inside a kept instance the same objects there in both.
  `bfbc2 dbxcatalogue`'s counts are the Python's on the pickle: 84,739 partitions, 631 classes, 607,100 guids, 9,657
  paths, 17,218 executable names.
- **The reports**: `test_parity_reports.py` (game tier) passes - `dbxparity`, `dbxparity --predict`, `portparity`
  byte for byte the retired Python's (PR1b-1d's digests), every run Rust. The test now runs `bfbc2 dbxcatalogue` first
  (the recorded runs read a current cache: a build's lines on stderr are not theirs).
- `test_dbxcatalogue_native.py` (seeded games with damaged bundles: every table and problem the reference's),
  `test_dbxport_rules.py` (an unreadable bundle reported, the catalogue not cached), the cache's Rust tests (round
  trip, damage refused, keys follow content and not place).

### Speed (release `bfbc2.exe`, this PC, other rounds building; seconds)

| run | before | after |
|---|---:|---:|
| `bfbc2 dbxparity --predict` | 14.9, 16.8 (main 0.21.0 today; 26.7 at PR1b-1d; the Python 12) | 4.6, 5.2, 5.3 (the cache read) |
| `bfbc2 dbxcatalogue`, cache current | 3.6, 3.5 (main: the pickle read with `bc2_pyrt::pickle`) | 1.5, 1.7 |
| `bfbc2 dbxcatalogue`, cache stale | handed to the Python | 13.9, 17.2 (the pass 12, then the file written) |
| `dbxcatalogue.load` (Python), cache current | 0.9 (unpickling) | 2.7 - 3.8 |

The Python's load is slower than unpickling: the file is read in 0.8 s (`from_cache_bytes`: the digest, the string
table, the deflated partitions on all cores), and the rest is making the Python objects (607,100 guids, the Counters,
the element trees), which a pickle makes faster. A WW2 rebuild loads the catalogue in a handful of steps; worth a
later look (a lazily made `Catalogue` on the Rust tables) if it shows.

### What is left

- The Python `Catalogue` itself (OPFOR's WW2 modules and dbxport's Python ask it) and `build_reference`: OPFOR in Rust
  stage 2. Without the Rust core `dbxcatalogue.load` builds each time (no cache).
- Unproven until the next WW2 rebuild: none of its outputs can change (the catalogue is the pickle's, table for table
  and element for element), so no rebuild was run for this round.
## E4: the map editor's gameplay tools (2026-11-22, branch `e4`)

Round E4 (ROADMAP item 7): flags, spawn points and M-COMs added, carried and deleted with their units, teams and the
round's rules, on the editor's tool trait; operations `place-unit`, `link-point`, `set-rules` (project format 9).
Nothing was written to `game\`; the game never launched.

### What the data says (measured on the game copy, read only)

- **A flag's unit is its own** (GAME-CQ-11): on 19 Conquest levels of every set, none of 67 flags' units (what moves
  with it plus what only its references and events reach) shares an instance with another capture point's; 246 team
  filters sit in them. A first check by event pattern found no shared announcer glue: each flag has its own two team
  filters per team, its own capture messages (`HUD_MP_CQ_TOOK_BASE_<letter>`) and voice-over entities. So a new flag is
  a copy of a whole unit, and "copy only the connections inside the copy" (E2b's placement) would have left it mute.
- **Rush stage counters count their M-COMs** (GAME-RUSH-10): 221 counters on 31 Rush and Squad Rush levels, every
  `TriggerAmount` equal to the M-COMs whose `onDestroyed` reaches it (91 of 91 first on OPFOR's XML dumps).
- **No server preset sets tickets** (GAME-RULES-1): the executable's `Server.*` settings are hardcore, friendly fire,
  kill cam, minimap, spotting, crosshair, third-person vehicle cameras, autobalance; the round's rules live in the
  mode layer's counters and flags, so the "preset options" are DICE's own values (Conquest 200 / 250 / 350 tickets,
  20 / 25 s; WW2 130, 10 s; Rush 100 or 75 against 1,500; Squad Rush 20 against 1,000; Squad Deathmatch 50).

### Proofs

- Valparaiso (`MP_001`): flag C's unit (31 instances) copied 60 m off - every one of the template unit's 15 event
  connections has its copy (members mapped, shared ends kept), no connection more; deleted again, the instances and
  connections are the level's. Port Valdez Rush (`MP_012GR`): an M-COM added to stage 1 (unit: two interactions, an
  award, three delay triggers, marker, alarm, six messages, seven voice-over lines), the stage's counters count 3, the
  play order unchanged; taken out, the level's instances again. Every partition written and read back byte for byte.
- The export: Operation Aurora (`SP_01`) as BFBC2 Conquest with a flag E, a spawn point on its spawner and the rules
  250 tickets / 25 s, built by `export::build` in 24-26 s and held by `export::check`; the staged `Levels/...dbx`
  reads back byte for byte with 5 flags (letters B, D, A, C, E), the point and the rules. This export first failed:
  the base's schema (a campaign level) has no evidence for the grafted layer's numbers (`Index` refused but 0); a
  field set now takes the type of the instance's own nonzero value there (XLOG-APP-5's rule, applied to it).
- In the editor (headless, demo level, both themes): the tool's click adds a flag (one operation, selected, its zone
  drawn), Delete takes its unit, Ctrl+Z brings it back, Carry moves the flag with its ring and marker, Esc drops it, a
  spawn point with no spawner is refused in words (`tests/gameplay_tools.rs`; screenshots `gameplay_flag_<theme>`,
  `gameplay_panel_<theme>` with `BC2_EDITOR_SHOTS`, kept in the session's scratch folder).
- Synthetic level (no game data, CI): `the_tools_on_a_synthetic_level` - unit, wiring to a shared voice-over line,
  carried spawn points, delete, rules, version 8 -> 9.

### The object grid for rays (PR1b-0's leftover): not built

The Gameplay tool casts one ray per frame for its ghost and one per click. PR1b-0 measured a pick at 0.64 ms on
SP_01 (5,924 collision meshes): under a millisecond a frame, so a grid of the objects' spheres (which needs an edit
generation on `Scene`) buys nothing visible for E4. It stays a candidate for E5's brushes, which cast many rays a
frame.

### What is left (E4-rest)

- A whole Rush stage added or taken out (its counters, bases, combat areas and the enable chain); the tools refuse a
  stage's last M-COM.
- A new flag's own texts: its capture messages keep the template's `HUD_MP_CQ_TOOK_BASE_<letter>` keys and its spawns'
  names the template's; letters past the level's need loc keys.
- Soldier spawners added to a flag or base by the tool (the asset browser's tile places one; `link-spawner` links it).
- PR1b-0's leftovers: the AI workspace's nine `NavTool`s and placing from the content browser onto `ViewTool`; the
  other 12 workers onto `Task`.
- Unproven until a playtest: whether the game spawns at, captures and announces a copied flag (and shows a fifth
  letter), whether an added M-COM arms and ends its stage, and the rules' effect on a round.

## E4-rest: whole Rush stages, a new flag's texts, spawners (2026-11-22, branch `e4r`)

What round E4 left, items 1 to 3 of 4 (ROADMAP E4-rest). Nothing was written to `game\`; the game never launched.

### What DICE's data says (read only)

- **Rush stage wiring** (GAME-RUSH-11; `examples/rush_facts.rs`, `examples/rush_wiring.rs`): on 30 of the 31 Rush and
  Squad Rush levels every stage has exactly one base counter (the stage counter whose `OnTrigger` sends
  `DestructBase` to the defenders' life counter), `BaseCount` equals the stages on 30 of 30, and the chain is exact:
  stage 1's base counter enabled, every later one disabled and enabled by the stage before's alone, the last enabling
  none. The other counters (team triggers) are enabled from the start (78 of 81 later ones). A middle base counter
  enables the next stage's forward capture point (54) and combat area (43) and disables its own (57 / 36); the last
  disables no combat area. A stage's defenders' base comes in through the new base counter's `OnEnabled`, the
  attackers' area through a hold-back timer, and a combat area's `OnEnabled` disables the one before. `ons_mp_005gr`
  (the Onslaught install's) has no defenders' life counter: refused. MP_006's last counter switches on a sixth
  forward base for a stage the level does not have.
- **Flag texts** (GAME-CQ-12): capture messages are `HUD_MP_CQ_TOOK_BASE_<letter>` / `LOST_BASE_<letter>`; DICE's
  localisation has A to F in all eight languages (FOXTROT kept in six); spawn names (`LocationNameSid`) are per-level
  keys ending in the letter (`MP_001_A` = "A: COMM CENTER"). On every stock Conquest level of each set: 67 lettered flags, 208 of 208 capture
  messages name their flag's letter, 103 of 103 spawn keys end in it, all with a text.
- The play model (`modelayer::rush_play`, L1's) follows a stage's end in no time; a timer of the stage's M-COMs that
  enables a spawn can land after the base counter's disable of it. `rushstage::settled` gives the base counter the
  last word, so "what is in play during one stage only" is DICE's intent on every level.

### What was built

- `bc2_levelwrite::rushstage`: `stages` (the parts, refusing other wiring), `add_stage`, `remove_stage`,
  `counterparts`, `settled`; `LevelWriter::connect` and the operation `connect` (project format 10);
  `Ownership::owned_from_except`; `modelayer::clusters` (one graph for many clusters).
- `gameplay::add_flag` re-letters the copied capture messages and spawn keys; `rename_flag`; `add_spawner`;
  `flag_texts` + `names::flag_message_texts` run as the export's step 4b, their texts in the record's `strings`
  (`mapexport overlay` and OPFOR's install add them, built from the overlay's backup; `--remove` takes out
  `<LEVEL>_A` - `_Z`).
- Editor: the Gameplay tool's Add stage / Add spawner, the panel's Remove stage, Add stage after the last and the
  selected flag's name; `app::PanelRequest` for the interface tests.

### Proofs

- Synthetic Rush level (CI): three stages, a fourth added (13 copies), played last; taken out again; every stage taken
  out; the log's version 10; refusals (no stages, the only stage). `connect` added, refused twice, taken out.
- Port Valdez (`MP_012GR`, 258 copies, 27 wires) and its Squad Rush (`MP_012SR`): through the log, the partition read
  back byte for byte.
- Game tier, every Rush and Squad Rush level of each set (`every_rush_level_is_wired_stage_by_stage`, 427 s): 30
  levels, 87 stages accepted, `ons_mp_005gr` refused. On each a stage added after the last (72 - 323 copies, 7 - 62
  connections beside the copies'; 0 left unshifted on most) plays last, the others as before, its copies as the last
  stage's objects; removed again, every instance and stage state is back (MP_006's dormant forward base goes with
  it), the last counter keeping 1 - 35 switches. Each of the 87 stages taken out leaves the others in order with their
  states, but one: on `nam_mp_002r`, with stage 2 out, the attackers' hold-back area `CombatArea7` (switched on by
  stage 2's area going off) stays on in stage 3.
- Valparaiso: flags E, F, G added; E's messages `..._E` and spawn key `MP_001_E`, renamed by the export to
  `<LEVEL>_E` "E: FLAG E"; G's messages worded "WE HAVE TAKEN POSITION GOLF", German "WIR HABEN POSITION GOLF
  ÜBERNOMMEN!", Russian left to the English. A spawner added to flag A (12 copies, wired as the flag's own) and
  deleted: the level as it was (the delete now takes a spawner's points although the flag's cluster lists them).
- Aurora (`SP_01`) as Conquest with E4's edits (`aurora_conquest_with_the_tools_edits_exports`): built in 28-39 s,
  `export::check` holds it; the new flag E's spawner keyed `MAPEDIT_GAMEPLAY_AURORA_E` with "E: FLAG E" in the
  record's `strings` (the stage checks of the level's own names now look at the name keys only: a first run failed
  XVAL-NAME-8/9 when the flag texts made `strings` non-empty).
- Editor (headless, both themes): the demo flag renamed from the panel (one operation, undone), a spawner and a stage
  refused in words; on `MP_012GR` a fifth stage by a click on the ground ("Add Rush stage 5"), removed from the panel,
  undone, redone, stage 2 removed. Screenshots `gameplay_rename_<theme>`, `gameplay_stage_<theme>` (session scratch).

### What is left (E4-rest2)

- PR1b-0's leftovers (item 4, not started): the AI workspace's nine `NavTool`s and placing from the content browser
  onto `ViewTool`; the other 12 workers onto `Task`.
- A stage inserted between two (a stage is added after the last; insertion is a removal and adds).
- Removing an added stage leaves the last counter with the switches it gained (as DICE's middle counters have them):
  semantically the level's, not byte for byte; undo is the exact way back.
- Taking out a Vietnam Rush stage can leave a hold-back combat area on (`nam_mp_002r`, stage 2: `CombatArea7`).
- Vehicle spawners DICE stops by `DisableSpawnPointFunctionality` that are shared between stages are not stopped by a
  new stage's end where no counterpart is found (counted as "unshifted": 0 on most levels, up to 9 on `nam_mp_005r`).
- Unproven until a playtest: an added or shortened stage chain playing (bases, areas, hold-back timers,
  reinforcements, the round's end), the new texts on the HUD and the deploy screen, a spawner added to a flag.

## NKG2: NavKit's graph toward DICE's - the spanner, the owner rule and its band (2026-11-22, branch `nkg2`)

The graph-and-areas half of the NavKit gap list (`spec/transforms/kynapse_generation.md`). Every candidate change was
measured with navparity on the 15 regression levels (navregress's builds into a scratch folder, one level at a time)
and with `kynapse_probe` / `kynapse_rules` on the builds and on DICE's files, and kept only where no measure got worse
(XNAV-BLD-3). Two rules went in; six items were measured and stayed out, each with its evidence.

### What changed in NavKit's file

| Rule | Change | Switch |
|---|---|---|
| KYG-EDGE-3 | each sector's walkable pairs pruned to a greedy 1.2-spanner (shortest first, ties by pair order), both pitches; the Gabriel rule on detailed sectors is no longer used | `Switches::spanner` = `Some(SPANNER_STRETCH)` (on) |
| KYG-EDGE-4 / KYG-NODE-2 | every edge on its source's owner copy: an edge out of a copy moves to the node's owner (the sector whose square holds it), which gets a copy of the far end (an ordinary far end becomes a border node), until nothing moves; a sector writes the nodes its own edges name. This forms DICE's border band | `Switches::owner_edges` (on; `graphfile::relocate_owner_edges`) |
| KYG-RECT-1 | a build of a map without DICE path data keeps the sample columns inside the spawns' box grown by 24.5 m (`navregress.spawn_bounds`, `inputs::spawn_bounds`; the editor's stage export when its base level has no DICE data) | always (DICE's levels keep DICE's extent) |

### Parity (15 regression levels, means; before = main's build, `work\navkit\regress` of NKG1 rebuilt byte-identical)

Every navparity measure is unchanged but these:

| Measure | before | after |
|---|---:|---:|
| `runtime_connectivity` | 98.41 | 98.80 |
| `strong_share` (DICE's own files 97.30) | 99.69 | 100.00 |
| `nodes_same` | 0.64 | 0.66 |
| `costs` | 95.91 | 95.29 |
| islands (count) | 402.47 | 402.53 |

`costs` is the share of edges matched to DICE's (both ends within 5 cm) whose cost word equals DICE's. The band
matches more edges, and the new ones' lengths differ by a sixteenth: sp_006 12 of 12 -> 12 of 14 matched edges,
sp_003_b 76 of 84 -> 92 of 103. No edge that matched before lost its cost, so the drop is the sample growing, not a
regression; it is the only measure below the reference.

| level | runtime_connectivity | | strong_share | |
|---|---:|---:|---:|---:|
| sp_01 | 97.89 | 99.10 | 99.28 | 100 |
| sp_02 | 99.44 | 99.56 | 99.86 | 100 |
| sp_03 | 97.58 | 98.59 | 99.10 | 100 |
| sp_04 | 99.69 | 99.80 | 99.93 | 100 |
| sp_003_b | 90.01 | 92.67 | 98.20 | 100 |
| sp_004_b | 100 | 100 | 100 | 100 |
| sp_005 | 99.23 | 99.38 | 99.72 | 100 |
| sp_005_b | 99.27 | 99.37 | 99.90 | 100 |
| sp_006 | 99.54 | 99.63 | 99.89 | 100 |
| sp_007 | 98.12 | 98.25 | 99.84 | 100 |
| sp_008 | 98.88 | 98.96 | 99.89 | 100 |
| sp_008_b | 98.50 | 98.50 | 99.92 | 100 |
| sp_009 | 100 | 100 | 100 | 100 |
| ons_mp_002 | 99.75 | 100 | 99.86 | 100 |
| ons_mp_004 | 98.19 | 98.23 | 99.97 | 100 |

### Structure (probe; DICE = DICE's infantry files of the same 15 levels)

| Measure | before | after | DICE |
|---|---:|---:|---:|
| border ids | 48,899 | 125,145 | 103,085 |
| ids in 3 / 4 sectors | 66 / 222 | 3,967 / 2,272 | 4,799 / 3,553 |
| border ids with no owner copy | 346 | 0 | 3,308 |
| edges out of non-owner copies | 111,968 | 0 | 7,355 |
| nodes outside their sector's mesh rectangle | 3,522 | 53,577 | 90,809 |
| coarse nodes / edges | 249,059 / 651,516 | 277,100 / 520,405 | 350,902 / 1,016,192 |
| detailed nodes / edges | 423,734 / 1,242,530 | 462,629 / 1,872,847 | 196,287 / 535,742 |
| ordinary ids | 574,485 | 498,233 | 329,114 |
| strongly / weakly connected node pairs as the runtime walks | 99.80 % | 100 % | 97.04 % (all 35) |
| table bounds the node box | 2,489 | 2,484 | 2,474 |
| link entries / objects | 14,904 / 51,995 | 14,904 / 51,995 | 14,562 / 49,600 |

The detailed graphs gain edges because the spanner keeps more pairs than the Gabriel rule among NavKit's dense
detailed nodes (2.4 x DICE's: its 710 detailed sectors and 4 m coverage nodes); coarse edges per node fall to 1.9
(DICE 2.9: DICE's nodes are denser, KYG-NODE-5). `kynapse_rules --only pairs`: coarse pairs in sight without an edge
or a route within 1.2 x, 46,951 -> 40,050 (DICE's 51,668). Five sectors' node boxes now equal an earlier sector's
(nodes shared across the line), so they keep the build's bounds (KYN-SEC-2's duplicate rule): 2,489 -> 2,484.
`kynapse_rules --only cover` (KYG-NODE-5, ground seeing a node within 7.5 m): coarse 73.69 -> 73.15 %, detailed
97.54 -> 97.56 % (DICE's same files 96.41 / 98.27 %).

### Measured and left out

| Item | What was tried | Result |
|---|---|---|
| DICE's detailed sectors (KYG-DET-1) | DICE's list on DICE's levels (`DICE_DETAILED`, written, off) | 40 of DICE's 502 never sampled at 0.2 (sp_02 `connectivity` 99.49 -> 74.5 % when built from the run anyway); with the 462 sampled, 8 means worse: `nodes_near` 47.63 -> 45.51, `connectivity` -0.05, `runtime_connectivity` -0.07, `portals` -0.12, `outline` -0.11, `df_d_precision` -0.61 (better: `ground` +0.16, D / F recall +3.5 / +6.5) |
| A second border row (NKG1's) with the owner rule | a row 0.8 m into the lower sector | not needed with the relocation; `connectivity` -0.02, `over_connection` +0.23, `ai_pairwise` -0.12 |
| Nodes off the grid (KYG-NODE-1) | each new node moved toward its share's centroid, inside its cell | `ai_pairwise` 92.73 -> 92.33, `nodes_same` -0.08, `connectivity` -0.04 |
| Coarse coverage nodes (KYG-NODE-5) | `node_cover` 7.5 m at pitch 0.8 | `nodes_near` 47.63 -> 50.14, but `over_connection` +1.75 / +2.15 on sp_03 / sp_003_b, ONS_MP_004 `connectivity` 97.90 -> 97.46 |
| The rectangle on DICE's levels (KYG-RECT-1) | the spawns' box in place of DICE's extent | `extra_ground` 8.31 -> 11.03, `sectors_extra` 1.33 -> 2.60 (sp_008_b 23.3 -> 57.6 %, sp_006 0 -> 19 sectors): DICE's ground ends before the box on 7 sides |
| Midline cut, off-grid portals (KYG-AREA-2), lookup ties (KYG-NODE-3) | not built | the cut's trigger and the tie order are not decoded (named exceptions); the off-grid portals carry routes until the area and link rules agree |

### Tests

- `cargo test -p bc2-navbuild --lib`: the spanner (unit and proptest: every pair left out has a route within 1.2 x,
  the kept pairs join what all pairs join) and the owner rule's relocation (`graphfile` tests); the switch defaults.
- `test_navkit_offline.py`: `build_detailed` with and without `DICE_DETAILED`.
- The goldens re-recorded: `test_navbuild_native.py --record --game` (sp_009, sp_004_b, sp_008_b, ons_mp_004) and
  `test_navtools_native.py --record --game --only vehicle,measures` (ONS_MP_002's vehicle path data, the parity rows of
  sp_009 / sp_004_b); navregress's reference run `regression` saved again (the old one kept as
  `regression_before_2026-11-22_nkg2.json`).

The golden builds (`navbuild_golden.json`, old -> new SHA-1, bytes; the files grow by the band's copies and the
detailed sectors' spanner edges); the 10 whole-stage answers ('stage', built with the switch defaults) re-recorded
with them, the E2a / E3c / E3h answers unchanged:

| level | old | new |
|---|---|---|
| sp_009 | e9be0ef5ef1c45e71930516e9f6c1090552993be (408,096) | 7f5f994c100e439d42810258295a8edc0f6329a5 (453,264) |
| sp_004_b | bd44cedb25e7c88032ef29bf47c3fdddee72032f (8,242,112) | 469a8b98dc0b44833b9318a1fc8120386b5e0931 (8,556,872) |
| sp_008_b | 2839e515d3802f501e4dc67513becc97591201a7 (8,484,708) | 01b978717db04edd7e92aaf29553a951f0a647e9 (8,842,992) |
| ons_mp_004 | 020d7019c061c2e1b91335d540b213f8e80598ed (7,627,988) | 76caa81b6c2577afcf722ba4a1621ab8bec56ef5 (8,020,340) |

## SPG2-rest3: reverse engineering for the specification, with AIS-rest2's static half (2026-11-22, branch `spg2r3`)

Static reading of `BFBC2Game.exe` (PC build 795745) with a scratch copy of Project OPFOR's analysed Ghidra project
(read only, headless, about 25 s a batch, 32 batches) and capstone sweeps of a scratch copy of the executable; DICE's
data through the suite's readers. No game run (in-game tests are paused): every exe rule stays observed until a
playtest (bot_ai.md's plan, steps 8 - 10 new). Per item:

| Item | Found | Rules |
|---|---|---|
| ticket counter (AIS-rest2) | found: tickets a float at +0x80; a death message (channel 0x10, type 0x11) whose victim is on its team takes 1, a revive (0x21) gives 1 back, captures / losses move its point value; the bleed 0x684150 read to its formula; no AI test anywhere: a bot's death costs a ticket | BOT-GS-6, BOT-GS-7 |
| mode classes | corrected: AIS-rest's "ticket counter" (0x66E190) is the mission objective, its "mission objective" (0x6740D0) the kill counter, its "life counter" vtable a cutscene object's; the real ticket counter 0x66E880, life counter 0x67A4B0 | BOT-GS-4 |
| Rush stage, M-COM (AIS-rest2) | partly: the life counter's lives, reinforcement and bases left (the run-time stage); an M-COM's arming is a `ServerInteractionEntity` with `GameInteractionEntityData`'s fields; where the armed state lives not found | BOT-GS-8, BOT-GS-9 |
| bot object and senses (AIS-rest2) | found: a 0xF4-byte structure (constructor 0xE6C0B0), all made up front by the bot manager 0xE6C480; its parts and the senses at +0x6C (five sensing tasks); the target record layout not read | BOT-RT-6 |
| combat fire site (AIS-rest2) | not found; narrowed: not input slots 17 / 18 (vehicles only), no direct AI call of the state or weapon functions, `BehaviourFire` hands its update to an action object at +0x10 | bot_ai.md RE item 8 |
| mount table drain | found: the game object's constructor 0x56EDE0 under `GameSettings.UseBundles`; the record is `BundleMountPoint` {`VfsPath`, `Bundle`, `IsResourcePath`} (round SPG2-rest2 had the strings swapped) | STRM-MNT-3 |
| dirt build-up's entities | found: `ClientVehicleEntity` only (vtable slot 125) | MAT-TEX-20 |
| undergrowth parser | not found: none of `VisualTerrain`'s 19 slots parses (job queues, draws, getters) | VEG-UG-6 |
| particle update | not found; 0x11DD76A is the C runtime's code-page `toupper`; of the 16 switches of 36+ cases, those read are not the particle system's | FX-PROC-4 |
| award listeners, career save, Flash roots, anim tree | not reached | - |

The probe `spec/probes/modes.py` (game tier, about 20 s): DICE's 84,143 level-bundle partitions, the 74 with a
counter type parsed; the 14 event names the rules give have their hashes in the functions named (12 of them are
DICE's PC event names; `DecreaseLife` and `OnReachZero` come from the game copy's other graphs); 30 attacker and 30
defender life counters. `core/tests/test_spec_probes.py` checks its 9 answers.

Rules 1,269 -> 1,273 (BOT-GS-6 to BOT-GS-9 new; BOT-GS-4, BOT-RT-6, STRM-MNT-3, MAT-TEX-20, VEG-UG-6, FX-PROC-4
changed).

### What is left (SPG2-rest4, AIS-rest2)

The undergrowth entry's parser (the resource factory registered with the type at 0x1375A20), the particle update,
the award listeners and the career save, the Flash loading roots / event queue / FSCommand callback, the compiled anim
tree; for the bots: the fire site (the fire behaviour's action object), the M-COM's armed state, the bot's unnamed
parts; and every playtest.

## SPV1: the specification's observed rules verified on DICE's files (2026-11-22, branch `spv1`)

Of the 473 observed rules, every one was classed by what would verify it (`spec/COVERAGE.md` section 4); the ones a
count over DICE's PC files settles got a game-tier check. The corpus is DICE's: every `.fbrb` under the game copy dated
before 2013 (1,021 bundles; OPFOR's installs and the bundles it changed are newer), each check read-only.

| Check | Reads | Time | Rules verified |
|---|---|---|---|
| `rust/bc2-formats/tests/spec_corpus.rs` (12 tests, one shared pass) | 1,021 bundles: headers and raw gzip members, every entry (stored bundles where each entry is), 124,458 DBX files (the raw element walk and `DbxTree`), 3,826 textures, 4,265 MeshData and their mesh sets, 876 movies | about 35 - 70 s release, 3 threads | 56: FBRB 9, RES 3, DBX 7, TYPE 16, INST / REF 7, TEX 4, MESH / MSET 6, TERR 2, HAVOK 1, BIGF 1 |
| `spec/probes/level.py` (`checks`) | DICE's 74 level partitions (Package's copy where DICE's), their two cull trees (148, 516,718 leaves) | about 70 - 90 s | 15: LEVEL 10, CULL 3, VEG-CLS-1, DECAL-SRC-2 |
| `spec/probes/registry.py` (`checks`) | the stock `package.mft` and `bundleManifest` (OPFOR's `.opfor-orig` backups) and which bundles exist | under 1 s | 8: REG 4, BNDL 3, STRM-REG-1 |
| SPG's probes' printed lines (`EXPECT`, now with `<head>§<line>` for a line inside one block) | the weapon, vehicle, material, destruction and lighting probes' output | as before | 48: WPN 23, VEH 11, MAT 4, DSTR 4, FX 2 (one through `emitters`), LIGHT 1, WTH 3 |

`test_spec_probes.py` grows from 172 to about 700 checks (7 - 8 minutes on this PC with other rounds' builds
running). Rules: verified 622 -> 749, observed 473 -> 346.

Contradictions found (each corrected in place, the text saying so, then verified): FBRB-HDR-10 (the build overlays are
stored), FBRB-DEL-2 (DICE's marker-only bundles end with a 20-byte empty gzip member; FbrbTool's with none),
TYPE-ATTR-3 (14 namespaces), LEVEL-ENV-1 (0 - 3 environment maps per level), WPN-DMG-3, WPN-AMMO-1, WPN-SWAY-2,
VEH-HP-4, VEH-MAT-1 (numbers in their tables). Counts that differed only because the earlier survey read the whole
game copy (OPFOR's files in it) got DICE-only `[data: ...]` lines: DBX-STR-3 (no unused string in DICE's files),
MSET-LOD-2 (no exception in DICE's), VEG-CLS-1 (Package's level copies).

### What is left

`spec/COVERAGE.md` section 4b: 67 rules a data probe over DICE's files could count (shader databases, animation,
persistence, the terrain probe's strips and trees, the weather classification, the levels' descriptions), 23 AI value
lists for `ai_corpus.rs`, 24 placement conventions (a re-run of the L1 study as a check), 23 rules half data and half
RE, 32 measured on other corpora, 21 that need the game running, 119 read from the executable; the 37 Kynapse /
NavKit rules are round NKG2's.

## SPV1-fix: DICE's empty gzip member in the packer; the WW2 ports against SPV1's corrected rules (2026-11-22, branch `spv1fix`)

**What DICE writes.** Read-only over DICE's 1,021 bundles (the game copy's `.fbrb` dated before 2013): every zipped
bundle (1,012) has a payload gzip member right after the header region, and nothing after it; the 105 whose payload
is empty (104 of `*deleted*` markers only, 1 with no entries; payload length field 0) have the gzip member of no data,
20 bytes `1F 8B 08 00 00000000 00 0B 03 00 00000000 00000000` (zlib's header, the empty fixed block, zero CRC and
length). No stored bundle has an empty payload; no other bundle kind carries an extra member. FbrbTool's .NET
`GZipStream` writes nothing for no data, and the suite's packer followed it.

**The fix.** `bc2_formats::fbrb::pack`'s payload writer appends `EMPTY_GZIP_MEMBER` (DICE's 20 bytes) when a zipped
payload is empty; every pack path goes through it (`pack`, `pack_dir`, `pack_rows*`, `bfbc2core.fbrb.pack` /
`pack_many`, `bfbc2 fbrbtool pack`, the level writer's bundles). The header member keeps .NET's `xfl 4 / os 0`
(FBRB-HDR-4, every bundle), so a repack of DICE's bundle differs from DICE's file in those two bytes only.

| Proof | Result |
|---|---|
| `spec_corpus.rs::fbrb_entries` (game tier): each of the 105 packed from its header's entries | 105 of 105: DICE's file byte for byte but for bytes 16-17 (xfl / os); the 20-byte member at DICE's place, byte for byte |
| `test_fbrb_pack_native.py`'s game check over the 105 (unpack in Rust, pack in Rust) | 105 of 105 "identical but the header's gzip header, the empty payload member DICE's byte for byte" |
| `pack.rs::empty_zipped_payload_is_dices_member` | zipped markers-only / no entries: the member, valid gzip of no data, reads back; stored: nothing after the header; a live payload: no extra member |

**FbrbTool's goldens (DICE's bytes win).** Re-recorded only where the packer now differs: `fbrbtool_golden.json`
`pack_built` "empty payload, zipped" and "no entries, zipped", `cases` "pack: a marker with an empty file" and
"pack: only the first line"; `fbrbtool_golden_game.json` 56 pack records (the 105 bundles' distinct bytes, 54, and two
temporary test bundles of a logged run, `<temp>/fbrb_unpack_01dh3d_5/b1` and `b2`, zipped with an empty payload: their
unpacked folders rebuilt and checked against the recorded unpack digests); `cli_o1_golden.json` the files digest of
the four `pack:` command lines with such a bundle (alone and on one pipe). Each new record was checked to be
FbrbTool's recorded bundle + the 20 bytes (SHA-1 of the new bundle less its last 20 bytes = the recorded SHA-1; for
`cli_o1`, the folder digest with the bundle so cut = the recorded digest, every stream and log line unchanged) before
it was written; each file's `meta.rerecorded` says so.

**The game.** It reads DICE's form (it ships it). FbrbTool's form (no member) has not been tried in game: no bundle
OPFOR installs is one of markers only (65 OPFOR-dated bundles in the game copy, none without a live entry), so nothing
OPFOR has shipped used it. Untested in game either way until a playtest needs one.

**The WW2 oracles.** The rebuild oracle holds no bundle, and none of the install oracle's 35 staged bundles is
without a live entry. A full rebuild on this branch (OPFOR main, OPFOR_TOOLKIT junctioned to the worktree): 55 steps
in 19.2 min, 10,852 of 10,852 identical, QA checks ok. `wake_install.py --plan`: the install staging 21,197 of 21,197 identical to the install oracle; `game\` sha256
snapshot before and after: the same 1,582 files and bytes.

**SPV1's five corrected rules and the WW2 ports.** No OPFOR or suite code cites WPN-DMG-3, WPN-AMMO-1, WPN-SWAY-2,
VEH-HP-4 or VEH-MAT-1, and the rule texts (2026-11-22, round SPG) postdate every value the ports set (class overrides
2026-11-10, carried ammo 2026-11-13, rifle-grenade values 2026-11-06, the material grid's stand-ins 2026-11-09,
dbxport's ammo conventions 2026-11-09). The values are copied from DICE's partitions at build time: the K98 /
Springfield class fields from the M24 field for field (`bf1943_weapons.CLASS_OVERRIDES`), the Type 5 / Nambu / Type 97
carried ammo from their US counterparts' firings (`weapons.FINITE_FIRING`), dbxport's magazines and pickups from the
same-class template, the wrench from the power tool, the rifle grenades' anti-armour values from the bazooka / RPG
firings, VEH material stand-ins from DICE's grids. `test_ww2_balance.py` on the rebuilt outputs: 138 class values of
the K98 and Springfield inside DICE's five bolt-action rifles' ranges; the wrench at the power tool's rates. Nothing
re-derived; no WW2 output changed for this item.

## OR3b: OPFOR in Rust, stage 3b - the level cover index, `retexture`, the level scan (2026-11-23, branch `or3b`, with Project OPFOR's `or3b`)

What OR3a left ("What is left"): the level cover index with content keys, `retexture`'s Python twin, and the shader
glue's heavy work in Project OPFOR's `wake_shaders` / `wake_translate` / `ww2_char_shaders`. Version **0.23.0**
(removals: the cover index's pickles, a file format; the Python index; `retexture`'s Python body - RELEASING.md; not
tagged here; a round merged first may take the number). Nothing was written to OPFOR's `game\`; the game never
launched.

### What moved

- **The level cover index** (`shaderdb.level_cover_prefixes` / `level_coverage` / `donor_levels`):
  `bc2_convert::shaderlevels::levels` through `bc2_native.shadercover_levels`. A level's database is found as
  `load_levels` finds it (`level_database`: the bundle's databases, `pick_database`, the bundle cache, `open` with
  the label and messages of `ShaderDb.parse`), its prefixes by `bc2_formats::shaderdb::levels::cover_prefixes`, the
  levels read in parallel. The cache `shadercover`: one `<24 hex>.bin` per level bundle and Dx (named by the bundle's
  path and the Dx; magic, version 3, the Dx, the key, the prefixes in their order or the error, a SHA-256 of all
  before it), **keyed by the bundle's content key** (`dbxcatalogue::cache::bundle_key`: its length, its head and its
  gzip trailer - the CRC-32 of the whole payload; the pickles hashed every byte of every bundle). A damaged file, or
  one of another version, Dx or content, is not read and is written again. Python passes the bundles' paths (never
  the project folder: OPFOR's rebuild tracker stamps every file under a folder a native call is given).
- **`retexture`**: `bc2_formats::shaderdb::merge::retexture` (the merge's own) through
  `bc2_native.shaderdb_retexture`; `hits` added to in the order the Python met the names.
- **`shaderdb.level_scan(levels, dx)`** (new): per level, what `entry_signatures`, `entry_textures`, `entry_layouts`,
  the main-pass skinning kinds (key byte 0x33 where byte 0x30 is 0) and the parts give on the loaded database -
  `bc2_formats::shaderdb::levels::scan` on the parsed file, the levels in parallel, the Python dictionaries made once.
- Project OPFOR: `wake_shaders --clone`'s donor scan (pass 1: every stock level, Dx9 / 10 / 11) and
  `detail_layers.index_levels` on `level_scan`; `wake_shaders.donor_dbs` chooses its donors from the cover index
  (`donor_levels` with the base's covered meshes taken out: the same choice) and loads only the chosen levels;
  `wake_shaders.signatures` / `entry_layouts` are the suite's `entry_signatures` / `entry_layouts` (the Rust handle;
  they are also what `detail_layers`, `kitbags` and `wake_install` call). `ww2_char_shaders.retexture` and
  `wake_terrain` reach the Rust `retexture` through `S.retexture`. SUITE_REQUIRES 0.23.0 to below 0.24.0.

### Proof

- **Recorded before the deletion** (`bfbc2-shader-tools/tests/test_shaderdb_levels.py --record`, from the Python at
  `4534f26`, `levels_golden.json`): 418 `retexture` cases (18 built: swaps, chains, a name that is the tail of a
  longer one, an empty name, at the block's start and end, latin-1 names, too long; 400 seeded random blocks and
  renames) - the bytes, the error, the hits; the cover index on 24 seeded stand-in game copies (150 levels: Dist and
  Package, Package winning, no Dx9 database, two, a damaged one, the level's own among several, unknown levels, mixed
  case) - each level's prefixes in order or its error, the index's hits and loads on a first, a second and an
  uncached pass, `level_coverage`, `donor_levels`, and a damaged bundle's exception. `levels_golden_game.json`: every
  level of the game copy (89), its prefixes' digest or error.
- The Rust answers: **418 of 418** retextures (12 refusals naming a texture that is not ASCII decided: named as
  latin-1, as every other shaderdb message; the Python read them as UTF-8, or raised `UnicodeDecodeError` making the
  message), **the 24 copies identical** (every row, hit and load count), the game copy **89 of 89**. What only the
  new index does: one `.bin` per bundle, a bundle rewritten in place with its old time indexed again, damaged files
  read from the bundle again and rewritten. The level scan against the Python queries: 450 stand-in databases and
  the game copy's 89 levels x Dx9 / 10 / 11, every dictionary and its order identical. Rust unit tests: the prefixes'
  order and edge cases, the index file's round trip and damage, names by place and keys by content.
- On the game copy, OPFOR's old pass 1 (its own `signatures` / `entry_layouts` / `entry_kinds` over `S.load`) against
  the new loop over `level_scan`: the six tables equal for Dx9, Dx10 and Dx11; `donor_dbs` old against new with
  `needs_bfbc2.txt` (166 meshes) on bases mp_005 and mp_002: the same donors (12 / 11 per Dx) and the same database
  bytes.
- **WW2 rebuild**: OPFOR's main checkout detached at `or3b` (b89e92d), `OPFOR_TOOLKIT` a scratch folder of junctions
  (this worktree, version 0.23.0, fresh module and `bin\bfbc2.exe`), `OPFOR_SUITE_STRICT=1`, `--jobs 2`: all 55 steps
  in 15.4 min; `compare_oracle` **10,852 of 10,852 identical**; `qa_checks.py` passed. **Install oracle**:
  `wake_install.py --plan` 5.0 min, `compare_oracle --install` **21,197 of 21,197 identical**; the game copy's 1,582
  files (11.6 GB) hashed (path, size, time, SHA-256) before the rebuild, before the plan and after it: identical.

### Timings

| | Python | Rust |
|---|---:|---:|
| cover index, the game copy's 89 levels, empty index | 6.1 s | 1.2 s |
| cover index, from the index | 1.74 s | 0.29 s |
| `wake_shaders --clone` pass 1, 79 stock levels (Dx9 / 10 / 11) | 8.1 / 10.5 / 8.2 s | 2.8 / 3.1 / 2.9 s |
| `detail_layers.index_levels` (79 levels, Dx9) | 5.9 s | 1.4 s |
| level scan vs the Python queries, 89 levels x 3 (test) | 23.8 s | 6.7 s |
| `donor_dbs`, base mp_005 / mp_002 | 5.6 / 5.2 s | 4.4 / 2.2 s |

In the rebuild: `wake_shaders` 11 s (10 s in OR3a's), `wake_shaders --clone` 49 s (59 s), `wake_translate` 18 s,
the three level chains side by side 277 s (410 s in OR3a's, other rounds building beside it then). `detail_layers`
took 56 s (14 s in OR3a's rebuild) though its index alone takes 1.4 s against the old 5.9 s on the same copy: the
step's other work (its clones and merges) was not changed; not explained here - to look at in the next rebuild.

### Decided

- `retexture`'s refusal names its textures as latin-1 (above).

### What is left (stage 3)

- `wake_translate`'s translation driver (`translate_db`, `foreign_entries`, `add_missing_entries`, the terrain patch
  and strip programs, `translate_batch`'s orchestration around the Rust translator and d3dcompiler): ~1,300 lines of
  Python glue over Rust queries; the time is in HLSL compiles (cached) and the per-program Python.
- `ww2_char_shaders`' soldier glue (`match_entries`, `perms`, `part_users`, `offer_kinds`, `entry_blocks`): small,
  per database; it calls `translate_db`.
- `wake_install.index_templates`' Dx9 scan (needs the layout records too: `level_scan` with the layouts' bodies),
  with the install oracle.
- OR3a's self-clone accident (PY1).

### Tests

- `test_shaderdb_levels.py` (quick, new entry `shaderdb-levels`; `--game`: `shaderdb-levels:game`).
- cargo fmt; clippy `--all-targets -D warnings` on bc2-formats, bc2-convert, bc2-py, bc2-gamefs, bc2-cli and each of
  `bfbc2.exe`'s tool groups alone; `cargo test` of the new modules (results in the round's report).

## PR1b-2-rest2: merge step 3, the menu tools' typed errors, decimate and shapes in Rust (2026-11-23, branch `pr1b2r2`)

What PR1b-2-rest left ("PR1b-2-rest", "What is left"): items 1, 2 and 4 done; 3 and 5 left (below). Removals
(culltool's `Doc` / `El`, bc2-menu's `Fail` results): the merge releases **0.24.0** (0.23.0 went to round OR3b, merged first; CHANGELOG). Nothing was
written to OPFOR's `game\`; the game never launched.

### 1. Merge step 3: the cull tools on the one DBX document

| was | now |
|---|---|
| `bc2_levelwrite::culltool::Doc` / `El`: `culltree.py`'s own byte-keeping codec (an owned tree of elements, values copied out of the file), built from `bc2_formats::culltree::elements` | `EditDbx` (the suite's one editable document since PR1b-2), read by `bc2_formats::culltree::document` under `Doc`'s rules: string indexes kept as stored and checked where read (`IndexError` -> the reference's own exception, as before), every value a range of the shared file bytes, the trailer kept; written by `EditDbx::write` |
| `stream_root`: a second copy of the element loop materialising `El`s one root child at a time (round CT1's streamed readers) | `bc2_formats::culltree::stream`: the same walk on a scratch `EditDbx` cleared after each root child or streamed item (`Sink`: `child` / `streamed` / `item`) |
| three element loops (`elements`, `Doc.parse`'s tree, `stream_root`) | one: `bc2_formats::culltree::walk` (a `Visit` per element); `elements`, `document` and `stream` are visitors |
| `El` methods (`attr`, `name`, `child`, `numbers`, `set_numbers`), `Doc::instances` / `instance` | functions over `(&EditDbx, NodeId)` (`attr`, `name_of`, `child_index`, `numbers`, `set_numbers`, `instances`); a node cloned with its string indexes as stored (`clone_stored`), so the written tree's bytes are the template's |

A string index past 32 bits (the one document keeps 32-bit indexes; no DICE file has one) is
`ElementError::Wide`, which culltool reports as the reference's own exception (`Damaged`), as an index outside the
table always was. The export's cull-tree walker (`bc2_levelwrite::culltree`, the editor's export) was already on
`EditDbx` (round XF1); it is unchanged.

**Proof**, old `bfbc2.exe` (main `6b71953`) against new, `BFBC2_CULLTREE_CACHE=0`:

| what | result |
|---|---|
| Onslaught: `culltree add <tree> <level> --auto --misplaced 2.0 --ref <Linux level> --ref-dir work\levels\*` (console_cull.py's command line) on the four console ports' levels and trees (`work\onslaught\build_<lv>\out`) | 4 of 4 the same (report, exit code; ons_mp_005 adds: tree and level byte for byte); the kept post-install inputs (`work\console_cull\<lv>\pkg`, 3 levels) "nothing to add" on both |
| `add --auto --misplaced 2.0 --ref-dir <its folder>` on every level partition with an exclusive tree beside it in OPFOR's `work\levels` | 89 of 89 the same; 25 wrote a tree and a level, every one byte for byte |
| `info` and `verify` on every cull tree of `work\levels` | 360 of 360 runs the same (stdout, stderr, exit code) |
| CT1's game-tier test (`culltool_ct1::streamed_readers_answer_as_the_whole_document`), now also: every file the document reads written back byte for byte | **87 level partitions and 260 trees and descriptions**: every one written back byte for byte, the streamed and whole readers the same (damaged copies too; a damaged copy's non-shortest varint is written shortest, as `Doc.tobytes` wrote it, so the round trip is checked on the real files) |
| `cli-caches-stockmp-graft-meshview-cull` (the cull group's 190 recorded command lines) | **190 of 190** (the cull group) as recorded; graft 228 of 228; every O3 group passes |

### 2. The menu tools' typed errors

`bc2_menu::error::MenuError`: `Menu` (`MenuToolError`), `Apt` (`AptError`), `NotFound` (`FileNotFoundError` of a
named movie, bundle or exe), `Io` (a file's `OSError`), `Latin1` (`UnicodeEncodeError`), `Reference` (an input the
Python reference fails on with its own exception: the hand-off); `MenuResult` everywhere the library returned
`PyResult` (common.rs, graft/mod.rs, graft/edit.rs, graft/crosshair.rs: 220 uses). `From<MenuError> for Fail`
gives `bfbc2.exe` the Python's exceptions (kind and words), and its matches on `MenuToolError` /
`FileNotFoundError` are matches on the variants (`MenuError::is_file_not_found`: a named file missing or `open()`'s
`ENOENT`, as before). The library reads files with its own `read_file` / `read_text`.

| proof | result |
|---|---|
| `cli-menu-crash-navkit-convert` (O2: the menu group's recorded command lines) | all groups as recorded (the menu group's lines among them): ALL PASSED |
| `cli-caches-stockmp-graft-meshview-cull` (O3: aptgraft's) | **190 of 190** (the cull group) as recorded; graft 228 of 228; every O3 group passes |
| `apt-model-check-refs-native`, `apt-decode-graft-native`, `aptgraft` | in `run_tests.py quick` (below) |

### 3. rayon in bc2-cli's own CPU work: not done

bc2-cli uses no rayon. The one CPU-heavy loop of its own found is texqa's analyses, already on scoped threads (at
most 4, texqa.py's worker processes' rows in order); moving it to the pool is a refactor with no win measured. Left.

### 4. `decimate.py` and `shapes.py` in Rust: OpenBLAS' bits matched

NumPy's products go through OpenBLAS 0.3.34 (`DYNAMIC_ARCH`); on the reference machine (AMD Ryzen 7 8845HS, Zen 4)
it picks the `SkylakeX` kernels (`openblas_get_corename`). Found by trial against NumPy (Python's `math.fma` for the
fused forms; `scratchpad` probes, not kept):

| NumPy | OpenBLAS | the bits (all equal) |
|---|---|---|
| `np.dot` / `np.linalg.norm` of 3- and 4-vectors | `ddot` (under 16 elements: its fused scalar tail) | `a0 b0`, then `fma(ai, bi, s)`: 200,000 of 200,000 |
| `x @ M`, x a 4-vector, M 4 x 4 C order | `dgemv` | `y_j = fma(x3, M3j, fma(x2, M2j, fma(x0, M0j, x1 M1j)))`: 3,000 of 3,000 vectors (the other 119 orders and plain sums tried: none) |
| `x @ M @ x` (decimate's quadric cost) | `dgemv` then `ddot` | 100,000 of 100,000 |
| `(n, 3) @ (3, 3)`, n >= 2 (`Geo.moved`) | `dgemm` | `fma(a2, b2, fma(a1, b1, a0 b0))`: every element of n = 2 ... 1,000 |
| `(1, 3) @ (3, 3)` | `dgemv` | `fma(a2, b2, fma(a0, b0, a1 b1))` |
| `np.sum(x, axis=1)`, `norm(x, axis=1)` of 3 columns | (NumPy's pairwise sum) | `((0 + x0) + x1) + x2` (three -0 give +0) |
| every BLAS product | the output zeroed first (`beta` 0, `ddot`'s sum from 0.0) | a zero result is +0 |

On another processor NumPy's own last bits differ (round CI1's lesson), so the port writes these kernels out rather
than calling a BLAS: it is deterministic on any processor and gives the oracle recorded on this PC (spec
XWW2-MOD-1). Matching NumPy is only possible where NumPy runs the SkylakeX kernels; elsewhere the recorded digests
are the test. `decimate` and the Python's dict
and set orders: the heap's tuples order every pop, sets only decide all-or-nothing tests, and the open edges'
quadrics are added in the dict's insertion order, as the port does.

| proof | result |
|---|---|
| `test_decimate.py`: the sphere tests (seams closed, bones apart, max_cost) on the port | pass (500 / 600 triangles, closed, bones apart, caps) |
| the port's arrays against `decimate_reference` (bit for bit) and against digests recorded from it: spheres, a shapes-built rifle (lathes, an extrusion, a turned bolt on a second bone) at 6 targets / caps and a boundary weight, 4 jittered spheres | **19 of 19** cases bit for bit (also a 48-segment rifle: 576 triangles at three targets / caps); digests recorded from the reference (`tests/decimate_golden.json`) |
| shapes: `moved` (40 rotations, whole and 1-274-vertex parts), `tangents`, `fix_winding`, `outward`, `_facing` against the NumPy originals; extrude's windings | **240 of 240** results bit for bit, after one fix: OpenBLAS adds each product to a zeroed output, so a zero result is +0 (the plain chain gave -0 for 7 normals of a rotated part: `+ 0.0` on every product now) |
| WW2 rebuild (the Type 38's and Winchester 1897's LODs, the shapes-built parts) | OPFOR main `6ad7a88` (read-only; `OPFOR_TOOLKIT` a scratch folder junctioned to this worktree, version 0.22.0 at the time; `BFBC2_FBRB_CACHE` a scratch folder), `--jobs 2`: all 55 steps in 26.8 min; `compare_oracle` **10,852 of 10,852 identical** (differ 0, missing 0, extra 0); `qa_checks.py` passed |

### 5. `Granny.put`: not done

It writes into the Python reader's buffer and moves when the Granny object is Rust's (a port of `granny.py`'s
`Granny` class: 1,179 lines, its reader's object tree and the writers' field walk). Left.

### Tests

- `cargo fmt`; `cargo clippy --all-targets -D warnings` on bc2-formats, bc2-levelwrite, bc2-menu, bc2-convert,
  bc2-py, bc2-cli, bc2-editor, bc2-stockmp (after merging main): clean; CI's per-group clippy of `bfbc2.exe` (all 18
  groups of `[features] default`, each alone): clean (exit 0; cargo's own unused-dependency notes only).
- `cargo test` of bc2-formats, bc2-menu, bc2-convert, bc2-levelwrite, bc2-cli: all pass (new:
  `culltree::document_writes_back`, `decimate::blas_forms` / `a_square_of_four_triangles`,
  `shapes::products_and_windings`); `culltool_ct1` (game tier) as above.
- `test_cli_o2.py`: ALL PASSED; `test_cli_o3.py`: every group as recorded (cull 190 of 190, graft 228 of 228).
- `test_decimate.py`: the unit checks, 19 of 19 decimations and 240 of 240 shapes results against the NumPy
  originals (this machine: OpenBLAS core SkylakeX), the 19 recorded digests.
- `run_tests.py quick -j 2` after merging main (version 0.24.0, module and `bfbc2.exe` rebuilt): **133 of 133**.
- `rust\native_census.py --check`: 282 exports, 0 without a caller; `python_status.py` regenerated (`decimate.py` a
  twin, `shapes.py` partly); `spec/trace.py --check` ok (1,289 rules; XWW2-MOD-1 new, unused on main); ruff and mypy
  on `bfbc2-model-tools`.

### What is left (PR1b-2-rest3)

- **rayon in bc2-cli's own CPU work** (measure first: texqa's scoped threads the candidate).
- **`Granny.put`** with the Granny object made Rust's.
- `shapes.py`'s builders (`lathe`, `extrude`, `_triangulate`) stay Python by decision: plain Python floats (`math.cos`,
  `math.hypot`), no BLAS; a port would have to match CPython's `hypot` and the C library's `cos` for nothing gained.

## NKG3: NavKit's step data toward DICE's - the terrain rule for D sides (2026-11-22, branch `nkg3`)

The step-data half of the NavKit gap list (`spec/transforms/kynapse_generation.md`, item 6): DICE's D lines and F
cells. The rule was fitted on DICE's files with a new probe, implemented in `bc2-navbuild`, and measured with
navparity on the 15 regression levels (navregress's builds into a scratch folder, one level at a time). It raised D
precision and recall but lowered one level's D recall (sp_02, -0.33); the user ruled it on (2026-11-22) with that dip
a named exception (`spec/transforms/navkit.md`, "Parity target"). navregress's reference and the goldens were
re-recorded.

### Where DICE's D / F sides stand in NavKit's builds (before)

`kynapse_df --cand work\navkit\regress` (NKG2's builds): of DICE's 323,071 normal-pitch D / F sides, 117,629 (36 %)
are matched; 93,963 (29 %) have both cells in one NavKit area with no break (the rule); 52,138 (16 %) one cell not
NavKit ground; 27,763 (9 %) lie where NavKit builds the other pitch (its detailed sectors, KYG-DET-1); 23,687 (7 %)
neither cell ground; 3,989 split between areas. The mean `df_d_recall` of 18.8 % also counts DICE's detailed lines,
which NavKit never writes (sp_009: 0 %).

### The rule (KYG-DF-1)

Every in-area side of DICE's normal-pitch areas whose two cells are terrain samples, against DICE's label, 15 levels:

| Rule | broken and DICE's | false | missed | precision | recall | D found | F found |
|---|---:|---:|---:|---:|---:|---:|---:|
| centres' step > 0.6 m (NavKit today) | 178,348 | 50,572 | 103,895 | 77.9 % | 63.2 % | 124,842 | 53,506 |
| the terrain's slope across the side > 0.77 | 199,918 | 18,025 | 82,325 | 91.7 % | 70.8 % | 147,795 | 52,123 |

Ruled out as the deciding measure (F1 on sp_01 / sp_004_b / sp_03): the sample normals' slopes (0.51), the step
corrected by the normals, the full 3-D gradient (0.49), footprint min / max steps (0.53), the high-capsule and
clearance flags; the step across the side over +-0.05 to +-0.2 m acts as the slope (0.816), wider spans fall back
towards the centres' step. The threshold holds out of sample (sp_02, sp_006, ons_mp_002: best at 0.767-0.77, 89.8 /
71.9 %). F cells follow the steps (the step finds more F sides), so the rule NavKit implements splits: D sides
between terrain cells by the slope, F cells by the step.

### What changed in NavKit

| Rule | Change | Switch |
|---|---|---|
| KYG-DF-1 | D sides between two terrain cells of a normal-pitch area by `stage::terrain_slope` (central difference on the level's height field at the side's midpoint) above `DF_TERRAIN_SLOPE` (0.77); F cells and the route edges' blocks by the step (`detail::detail_features_by`, `features_split`) | `Switches::df_slope` / `navwalk.DF_SLOPE` (on, 0.77) |
| - | the build takes the level's height field: `build::Inputs.terrain`, `StageArgs.terrain`; `navwalk.build(heightfield=...)` (navregress, `navkit.py build`), `inputs::level_terrain` (bfbc2.exe, the editor) | always (no height field: the step rule) |

### Parity (15 regression levels; before = NKG2's reference `regression`)

Every navparity measure unchanged but these (means):

| Measure | before | after |
|---|---:|---:|
| `df_d_precision` | 50.53 | 56.71 |
| `df_d_recall` | 18.84 | 23.89 |
| `df_d_recall_n` (normal pitch) | 32.80 | 40.60 |
| `df_d_recall_shared` | 21.34 | 26.96 |
| D sides written (`df_d_candidate`) | 17,225 | 20,960 |

| level | D precision | | D recall | | D sides | |
|---|---:|---:|---:|---:|---:|---:|
| sp_01 | 44.70 | 51.06 | 22.84 | 29.45 | 12,924 | 15,442 |
| sp_02 | 40.43 | 53.35 | 21.86 | **21.53** | 7,054 | 5,664 |
| sp_03 | 50.28 | 57.11 | 28.48 | 38.69 | 36,814 | 46,706 |
| sp_04 | 49.54 | 49.53 | 27.96 | 34.80 | 19,347 | 24,296 |
| sp_003_b | 50.10 | 56.21 | 31.25 | 42.07 | 47,189 | 59,521 |
| sp_004_b | 59.30 | 61.93 | 19.05 | 24.24 | 1,320 | 1,618 |
| sp_005 | 53.88 | 57.44 | 28.49 | 37.40 | 45,212 | 56,904 |
| sp_005_b | 54.86 | 57.65 | 25.97 | 34.89 | 38,741 | 50,363 |
| sp_006 | 45.56 | 54.27 | 28.89 | 31.19 | 30,171 | 28,627 |
| sp_007 | 53.29 | 56.36 | 20.88 | 27.65 | 15,141 | 19,848 |
| sp_008 | 57.95 | 61.78 | 13.63 | 19.14 | 1,866 | 2,538 |
| sp_008_b | 34.88 | 60.00 | 2.11 | 3.80 | 62 | 72 |
| sp_009 | 100 | 100 | 0 | 0 | 0 | 0 |
| ons_mp_002 | 49.73 | 55.52 | 10.35 | 11.89 | 2,124 | 2,238 |
| ons_mp_004 | 13.50 | 18.48 | 0.84 | 1.55 | 414 | 564 |

sp_02's D recall falls by 43 of DICE's 12,932 sides: `kynapse_df --before` shows the sides lost at slopes of
0.60-0.77 with centre steps of 0.60-0.65 m (239 of DICE's D / F side keys lost, 206 of them at slopes 0.70-0.77; 233
gained, all at slopes over 0.77 or with steps under 0.6 m), the band where DICE breaks only 17-34 % of sides. This is not a larger matched sample (NKG2's `costs` case) but a real loss on
one level; the user accepted it as a named exception (2026-11-22). No other measure moves.

### Measured and left out

| Item | What was tried | Result |
|---|---|---|
| The slope for F too | every break by the slope | `df_f_recall` 33.46 -> 31.35 (9 levels worse; sp_02 37.45 -> 26.37), F precision 34.0 -> 47.8 |
| Route edges off the slope's D sides | the slope's D sides block edges (DICE's graph never crosses a D line) | ONS_MP_004 `connectivity` 97.90 -> 97.76, `runtime_connectivity` 98.23 -> 98.07; `over_connection` sp_003_b 29.35 -> 29.60 |
| D by slope or step | the union | `df_d_precision` 50.53 -> 49.94 (12 levels worse), recall 25.43 |
| A lower threshold | 0.755 | `df_d_precision` 50.53 -> 46.09 (11 levels worse) |
| Detailed D / F | the slope (0.915) or the step (0.175 m) on DICE's own detailed areas | precision 29 / 22 %, recall 43 / 43 %: no rule; stays off |
| Vehicle D / F | - | needs an in-game vehicle-shaped sample run (KYG-DF-2); a playtest item |

### Tests

- `cargo test -p bc2-navbuild --lib`: `detail_tests::a_side_test_decides_where_it_answers`,
  `detail_tests::split_rule_f_by_the_step_d_by_the_test` (and with a test that never answers, the step rule's answer),
  `stage::tests::terrain_slope_across_a_side`, the switch default.
- With the switch off the four golden builds came out as recorded (`test_navbuild_native.py --game`); with it on
  they were re-recorded (below), and navregress's reference run saved again (the old one kept as
  `regression_before_2026-11-22_nkg3.json`).
- The probe: `cargo run --release -p bc2-navbuild --example kynapse_df -- [<level>...] [--cand <folder>] [--before
  <folder>]`.

The golden builds (`navbuild_golden.json`, old -> new SHA-1, bytes; the files grow by the slope rule's D lines; sp_009
has no normal-pitch terrain D side and is unchanged):

| level | old | new |
|---|---|---|
| sp_009 | 7f5f994c100e439d42810258295a8edc0f6329a5 (453,264) | unchanged |
| sp_004_b | 469a8b98dc0b44833b9318a1fc8120386b5e0931 (8,556,872) | 8d0e8305700314ad62eda32c896472f1bb5fbfb1 (8,565,240) |
| sp_008_b | 01b978717db04edd7e92aaf29553a951f0a647e9 (8,842,992) | 19c2ac0ff612d3fb8db61b8b37b7a19158ed13ed (8,843,220) |
| ons_mp_004 | 76caa81b6c2577afcf722ba4a1621ab8bec56ef5 (8,020,340) | 326b162973db3fc5d41d072df7655db8f6fb75b3 (8,023,856) |

`test_navtools_native.py --record --game --only vehicle,measures`: ONS_MP_002's vehicle path data unchanged (vehicle
builds write no D / F); sp_004_b's parity row re-recorded (its candidate is the new golden build). navregress's
reference `regression` saved again (exit 1 only for the ruled exception, sp_02 `df_d_recall` 21.86 -> 21.53); the old
one kept as `regression_before_2026-11-22_nkg3.json`.

## NKS-deep C: NavKit's offline sampler in Rust, at 0.2 m, DICE's detailed sectors and a vehicle run (2026-11-23, branch `nksdc`)

Part C of NKS-deep (ROADMAP "NKS-deep"), A2-rest's sampler brought forward: `navsample_offline.py`'s sampling ported
to Rust (`bc2_navbuild::sampler`), run at 0.2 m over the level's and DICE's detailed sectors and for a vehicle-shaped
entity, exposed as `bfbc2 navkit sample <level> --offline` and through the bindings. Nothing was written to `game\`;
the game never launched. Measured on the development PC (16 threads, 15 GB) beside other rounds' builds.

### The port (`bc2_navbuild::sampler`)

| Python (`navsample_offline.py`) | Rust |
|---|---|
| `Grid`, `from_box`, `like` | `sampler::Grid` (`from_box` in `float32` as `navsample.cpp`'s `Begin()`, the 1 m rectangle mask) |
| `level_geometry`, `destroyed_parts`, `part_names`, `load_level` | `sampler::level` (the suite's collision index, `hulls::asset_bodies`, the reference's regexes) |
| `raster_hits`, `_edge_dist2`, `_line_tri_interval`, the barycentric test | `sampler::geom` (one pair at a time, NumPy's order of operations) |
| `cast_columns`, `Intervals`, `capsule_blocked`, `terrain_blocks`, `sample_grid` | `sampler::sample` (bands of 8 rows on all cores) |
| `write` / `file_bytes` | `Samples::write_to` (streamed, `write_v3`'s bytes) |
| `compare` | `sampler::compare` (a column at a time) |
| `sample_level`, `like` / `validate`, `stage` | `sampler::level::sample_level`, `like`; `bfbc2 navkit sample --offline [--like / --stage]` |

**Bit for bit.** Each (triangle, column) pair's arithmetic is NumPy's: `np.minimum` / `maximum` / `clip` keep a NaN,
`np.cross` is `a1 b2 - a2 b1`, a row's `sum` / `linalg.norm` over three values is `(x0 + x1) + x2` (NumPy 2.5's
contiguous reduction, measured on 200,000 rows: 100 % against 88 % for `x0 + (x1 + x2)`), the placed object's matrix
the fused chain `hulls` measured (round F4). The cast's and the capsule's float keys (`column * 4096 + height`) are
computed as the reference computes them, so every search lands on the same hit; hits tied in height keep the
reference's order (mesh, convex, terrain; each by triangle index). The work runs in bands of rows: every answer the
reference computes for a column depends only on that column's pairs (its keys never reach another column's), so the
order of the work changes nothing.

### Parity with the Python (the reference, NumPy)

| run | files | identical |
|---|---:|---:|
| pitch 0.8, both capsule runs, the 15 regression levels | 30 | **30** |
| pitch 0.2 over the level's detailed sectors (`sample --pitch 0.2`'s crop): sp_009, sp_004_b, ons_mp_004 | 6 | **6** |
| the in-game files' grids (`like`, masked detailed runs included): sp_009, sp_004_b at 0.8 and 0.2, both runs | 8 | **8** |
| the same through `bfbc2 navkit sample sp_004_b --offline` (0.8 + 0.2) | 4 | **4** |
| an export's stage (`mapedit_aurora_cq`, base sp_01; `navsample_offline.py stage` against `bfbc2 navkit sample --offline --stage`, stand-in OPFOR folders): the prepared tree - objects, group members, triangle lists, ladders, settings - and the four sample files | 17 | **17** (Python 26.3 s / 3.19 GB commit, Rust 16.6 s / 0.70 GB) |

One difference was found and decided (NAVS-SMP-2): `sample_level` and `stage` cropped the geometry to the level's box
before the 0.2 m run, so a detailed sector outside it lost its objects (sp_009: 6 flagged sectors east of x 500, the
box ends at x 100; 1,678,603 against 1,723,700 surfaces, 20,768 columns). The geometry now covers every square a run
samples (the box's sector squares, the flagged sectors, DICE's); `sample --pitch 0.2` already cropped to the detailed
box and is what the table compares against. The Python's `sample_level` and `stage` were fixed the same way.

### Speed and memory (whole process; Python: the run in a warm interpreter)

| level | Python 0.8 s | GB | Rust 0.8 s | GB | | level | Python 0.8 s | GB | Rust 0.8 s | GB |
|---|---:|---:|---:|---:|---|---|---:|---:|---:|---:|
| sp_01 | 30.5 | 4.98 | 1.4 | 0.19 | | sp_006 | 30.2 | 3.67 | 2.2 | 0.35 |
| sp_02 | 14.8 | 2.53 | 1.2 | 0.16 | | sp_007 | 26.1 | 3.45 | 1.8 | 0.35 |
| sp_03 | 25.9 | 3.46 | 1.9 | 0.31 | | sp_008 | 14.0 | 2.74 | 0.9 | 0.14 |
| sp_04 | 11.2 | 2.04 | 1.4 | 0.19 | | sp_008_b | 12.9 | 2.63 | 0.7 | 0.11 |
| sp_003_b | 18.5 | 3.01 | 1.8 | 0.27 | | sp_009 | 0.9 | 0.21 | 0.4 | 0.05 |
| sp_004_b | 3.2 | 0.79 | 0.6 | 0.09 | | ons_mp_002 | 8.3 | 1.69 | 1.1 | 0.11 |
| sp_005 | 21.5 | 3.30 | 1.8 | 0.27 | | ons_mp_004 | 4.1 | 0.91 | 0.8 | 0.09 |
| sp_005_b | 25.3 | 3.30 | 1.9 | 0.26 | | | | | | |

At 0.2 m (the detailed run alone in Python; the 0.8 m and 0.2 m runs together in Rust): sp_009 15.7 s / 2.48 GB ->
1.4 s / 0.11 GB, sp_004_b 63.0 s / 4.52 GB -> 2.8 s / 0.25 GB, ons_mp_004 61.8 s / 3.90 GB -> 2.5 s / 0.20 GB. On
the in-game grids (four files): sp_009 32.2 s / 2.66 GB -> 1.2 s / 0.11 GB, sp_004_b 165.3 s / 5.55 GB -> 5.6 s /
0.31 GB. Peak memory (working set; commit) is the geometry, the band results and one copy of the surfaces: the runs
share the cast's surfaces, the files are streamed, the comparison reads a column at a time (an in-game detailed file
is up to 84 million columns and 670 MB). The whole level with `--dice --vehicle` (0.8 m pair, 0.2 m pair over the
level's and DICE's detailed sectors, vehicle run):

| level | s | GB | | level | s | GB | | level | s | GB |
|---|---:|---:|---|---|---:|---:|---|---|---:|---:|
| sp_01 | 17.3 | 0.57 | | sp_005 | 23.8 | 0.71 | | sp_008_b | 9.4 | 0.41 |
| sp_02 | 13.0 | 0.54 | | sp_005_b | 20.9 | 0.58 | | sp_009 | 1.9 | 0.11 |
| sp_03 | 20.5 | 0.66 | | sp_006 | 29.4 | 0.85 | | ons_mp_002 | 7.4 | 0.29 |
| sp_04 | 14.1 | 0.63 | | sp_007 | 25.4 | 0.85 | | ons_mp_004 | 4.3 | 0.21 |
| sp_003_b | 13.1 | 0.40 | | sp_008 | 19.8 | 0.48 | | sp_004_b | 3.5 | 0.26 |

Every level stays far under the 3 GB budget (the largest commit 1.26 GB, sp_006).

### Agreement with the in-game sampler (NAVS-SMP-4)

`bfbc2 navkit sample <level> --offline --like <in-game files>` on OPFOR's `work\navsample` (read only): per in-game
surface a partner within 0.1 m (found), per offline surface one in game (real), the walkable and blocked flags on
matched surfaces (normal run; the high run's blocked agreement in brackets), the object surfaces found. The 0.2 m
rows are the in-game detailed runs, on the columns they sampled.

| level | 0.8 m columns | found / real | walk / blocked agree | objects found | 0.2 m columns | found / real | walk / blocked agree | objects found |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| sp_01 | 2,148,328 | 97.72 / 98.22 | 97.27 / 98.35 (98.63) | 82.40 | 12,675,605 | 96.63 / 96.10 | 97.86 / 97.39 (97.81) | 87.62 |
| sp_02 | 1,612,334 | 98.46 / 97.54 | 98.53 / 98.41 (98.27) | 88.16 | 13,517,006 | 97.33 / 95.77 | 98.18 / 97.35 (96.82) | 89.72 |
| sp_03 | 4,514,346 | 98.65 / 98.69 | 97.99 / 98.10 (98.35) | 85.06 | 17,038,780 | 95.35 / 95.46 | 97.82 / 96.62 (97.39) | 85.02 |
| sp_04 | 3,239,189 | 99.70 / 99.65 | 99.28 / 98.50 (98.51) | 97.07 | 18,906,555 | 99.29 / 99.13 | 98.65 / 98.28 (98.15) | 97.24 |
| sp_003_b | 4,873,328 | 99.37 / 99.48 | 96.31 / 98.38 (98.37) | 85.42 | 8,884,605 | 95.44 / 96.20 | 97.93 / 97.56 (97.76) | 85.48 |
| sp_004_b | 505,318 | 99.27 / 99.02 | 97.06 / 98.82 (98.66) | 95.50 | 6,866,066 | 99.18 / 98.89 | 98.52 / 98.71 (98.56) | 96.56 |
| sp_005 | 4,010,827 | 99.30 / 99.20 | 97.60 / 98.88 (99.14) | 92.28 | 19,366,545 | 97.97 / 97.72 | 98.52 / 97.99 (98.59) | 92.40 |
| sp_005_b | 4,069,150 | 99.47 / 99.29 | 97.95 / 98.68 (98.85) | 92.98 | 14,518,280 | 98.06 / 97.44 | 98.10 / 97.68 (98.15) | 92.98 |
| sp_006 | 5,512,128 | 99.25 / 99.19 | 98.94 / 99.49 (99.53) | 90.22 | 20,850,328 | 97.45 / 97.24 | 98.16 / 98.15 (98.32) | 90.41 |
| sp_007 | 4,108,715 | 98.88 / 98.59 | 96.49 / 97.71 (97.54) | 86.57 | 25,609,545 | 97.43 / 96.75 | 98.10 / 96.68 (97.14) | 86.64 |
| sp_008 | 548,464 | 95.32 / 95.16 | 96.93 / 97.52 (97.47) | 90.67 | 10,909,105 | 95.72 / 95.67 | 97.62 / 97.58 (97.28) | 91.36 |
| sp_008_b | 400,939 | 97.20 / 96.16 | 97.75 / 98.00 (97.62) | 93.91 | 7,948,471 | 96.37 / 95.39 | 97.58 / 97.64 (97.28) | 92.79 |
| sp_009 | 118,664 | 99.26 / 99.66 | 99.68 / 99.55 (99.68) | 93.33 | 1,556,330 | 98.80 / 99.24 | 99.31 / 99.18 (99.48) | 93.11 |
| ons_mp_002 | 675,736 | 96.90 / 96.29 | 96.99 / 97.74 (98.35) | 85.84 | 7,509,726 | 95.42 / 94.95 | 97.75 / 96.93 (97.82) | 85.15 |
| ons_mp_004 | 490,326 | 98.72 / 98.48 | 98.80 / 98.80 (99.00) | 91.07 | 4,578,061 | 97.98 / 97.71 | 98.23 / 98.25 (98.60) | 90.71 |

The 0.2 m agreement is within 0.5-3 points of the 0.8 m one on every level, so the offline samples stand in for the
in-game ones where the game never sampled. What stays unproven until a playtest: whether a build from offline
samples matches one from in-game samples (`navsample_offline.py parity` measured that before, on the Python; not
re-run this round).

### DICE's detailed sectors at 0.2 m (KYG-DET-1)

`--dice` adds DICE's own detailed sectors to the level's choice. Of DICE's 502 detailed sectors on the 15 regression
levels, the in-game 0.2 m runs sampled 462 (NKG2's 40 never sampled); the offline sampler now samples **all 502**:

| level | DICE's | sampled in game | never in game (now offline) |
|---|---:|---:|---|
| sp_01 | 41 | 33 | (2, 5) (3, 2) (3, 5) (10, 4) (10, 5) (13, 3) (13, 4) (13, 5) |
| sp_02 | 38 | 35 | (4, 11) (6, 1) (6, 9) |
| sp_04 | 55 | 45 | (3, 1) (3, 3) (4, 13) (7, 5) (8, 1) (8, 2) (9, 11) (11, 10) (12, 8) (12, 9) |
| sp_005 | 42 | 41 | (6, 8) |
| sp_006 | 51 | 42 | (13, 15) (13, 16) (13, 17) (14, 13) (14, 14) (14, 17) (14, 18) (14, 19) (15, 9) |
| sp_007 | 47 | 46 | (5, 4) |
| sp_008 | 19 | 18 | (6, 7) |
| sp_008_b | 23 | 20 | (2, 1) (6, 4) (7, 4) |
| ons_mp_004 | 21 | 17 | (2, 1) (2, 9) (3, 6) (3, 9) |
| sp_03, sp_003_b, sp_004_b, sp_005_b, sp_009, ons_mp_002 | 50, 28, 19, 41, 4, 23 | all | - |

(sp_02's (6, 1) is in the level's own offline choice - its stacked columns in the offline 0.8 m samples - so `--dice`
lists 39 sectors as DICE's only.) The in-game list counts a sector sampled when its in-game 0.2 m file has samples in
at least half of its square. The build's `DICE_DETAILED` switch (NKG2, off) can now be measured with every one of
DICE's sectors at 0.2: part B's.

### The vehicle run (NAVS-SMP-3, inferred)

At the vehicle pitch (1.0 m) the same casts, each surface tested with a vertical cylinder of radius 2.5 from 0.4 to
4 m (KYG-PAR-2) against the collision triangles (a 32-sided prism), and the height field's rise above the surface's
plane against the step. Blocked share of the surfaces: sp_009 40 %, sp_004_b 45 %, sp_006 24 %, sp_04 38 %, sp_03 59
%, ONS_MP_002 55 %. Not validated: no in-game vehicle-shaped run exists (KYG-DF-2, a playtest item); it is the input
the vehicle D / F fit (NKG3's left item) needs.

### Bindings and callers

- `bc2_native.navkit_offline_sample` (geometry and height field in memory): `navsample_offline.sample_grid` calls it;
  its NumPy (the casts, the capsule test, `Intervals`, `terrain_at`) is gone. `navkit_offline_sample_level`:
  `navkit.py sample <level> --offline`.
- `bc2_levelwrite::pathdata::sample_stage` (the export's stage step) runs `bfbc2.exe navkit sample <level> --offline
  --stage <stage> --base <base>` (was `navsample_offline.py stage`).
- The editor's sculpted-terrain sampling (A2-rest) can call `sampler::sample` with its own `Field` and `Geometry`.

### What is left

- **Retiring `navsample_offline.py`**: not done. Its research drivers (`parity`, `stockmp`, `build`, `check`,
  `all` with the Package overlay and the combat-area box of a map without path data) and the reference wrappers the
  F3 / F4 / collision-backend tests call (`asset_bodies_reference`, `_write_args`, `level_geometry`) still use it. A
  removal bumps the minor version (CHANGELOG rule); nothing was removed that another project imports.
- The vehicle run's rule is unvalidated until an in-game vehicle-shaped run.
- A sculpted terrain on the stage (A2-rest): the stage still copies the base's terrain.
## SPG3: options, input, scriptability and dependency graphs in the specification (2026-11-23, branch `spg3`)

A specification round (no Rust or tool code changed): four new files, 43 rules (17 verified, 2 implemented, 16
observed, 8 inferred), three read-only probes held by the game-tier check. Static analysis only: `BFBC2Game.exe`
(build 795745) on a scratch copy (string and reference searches, a call index of every `E8` / `E9` target, capstone
disassembly), DICE's partitions through the suite's readers, OPFOR's step records; nothing launched, no settings file
read beyond its key names.

| Probe | Reads | Time | Checks |
|---|---|---|---|
| `spec/probes/options.py` | the startup bundle's `Settings/GameSettings` and `Input/*`, `mp_common`'s `Input/InputMappings/*`, the exe | 0.1 s | 16 |
| `spec/probes/scripting.py` | DICE's 74 level partitions, the exe's type names | 15 s | 13 |
| `spec/probes/dependencies.py` | every `.dbx` string table of DICE's 1,021 bundles (124,458 entries; no tree parse) | 22 s | 12 |
| `spec/probes/dependencies.py --steps` | OPFOR's 55 step records (63 MB of JSON) | 0.4 s | print only |

The asset graph from string tables alone (an external reference is a string `<path>/<guid>`, DBX-STR-1) costs 22 s
for the whole of DICE's data, against minutes for a tree parse: the way for an editor's "what uses this".

**Findings.**

- `settings.ini` is 4 sections and 29 keys in one static table; `GameSettings.ini` is the defaults partition's 45
  user-exposed settings plus the ones the code asks for by name (`ToggleCrouch`, `HoldToZoom`, `ColorBlindMode`,
  `KitConfig_0` - `7`, `CurrentKit`, the server browser's two); the 14 hidden ones are never saved (SET-GS-1 - 4).
- The per-user binding files are DBX in XML instance form, used only when `MapDataVersion` >= 11, saved whole on
  every confirmed or cancelled key change (INP-FILE-1); the console's XInput device and rumble have no caller on PC
  (INP-PAD-1).
- DICE's 57,667 event connections use 77 classes and 186 event names; 107 of the exe's 180 entity data classes are
  never wired; `GameSettingEntityData` / `ConsoleCommandEntityData` are never placed, but 104 debug-menu items are,
  in 21 levels (SCR-EVT-1, SCR-EVT-2, SCR-CMD-1). No developer console in the client; the dedicated server is not on
  this PC (SCR-DEV-1, SCR-SRV-1).
- 113,597 of 266,618 partition references (43 %) name partitions DICE never shipped (textures' and materials'
  partitions mostly); 11 small cycles (XDEP-REF-2, XDEP-REF-3).
- **The WW2 rebuild has three read-before-write steps**: `wake_translate` reads 33 of `material_grid`'s and 2 of
  `bf1943_jp_bazooka`'s outputs and `material_grid` 2 of `wake_vo`'s, all written later in the order - the previous
  rebuild's files. The oracle passes because the outputs are at a fixed point; a rebuild from an empty `work\ww2`
  would need these steps twice (XDEP-STEP-3). For OPFOR to fix or declare before `rebuild_all` becomes a step graph.

**Unproven until a playtest** (in-game tests paused): what the game does with bad settings values; what the debug-menu
entities do with `IsMenuDevOptsEnabled` set; the handlers of the settings and console-command entities.

### What is left (SPG3-rest)

`spec/COVERAGE.md` section 2, items 1 - 4, and ROADMAP's SPG3-rest.

## TG0: terrain generator feasibility and spec (2026-11-23, branch `tg0`)

A feasibility and specification round for the user's goal of our own random height-map and terrain generator built
from BFBC2's assets (no Rust or tool code changed). `spec/transforms/terrain_gen.md`: 32 rules (16 verified, 2
implemented, 3 observed, 11 inferred, ids TGEN-*); a read-only probe of DICE's terrain shapes, biomes and playable
areas; a throw-away prototype generator and writer (Python, NumPy) that TG1 replaces in Rust. Both run in the game
tier (`core/tests/test_spec_probes.py`: `terrain_shapes`, `terrain_generator_prototype`).

| Run | Reads / writes | Time | Peak RAM |
|---|---|---|---|
| `spec/probes/terrain_shape.py` | DICE's 30 terrains (heights, material maps, visual terrains, water meshes) and 74 level partitions | 80 s | 474 MB |
| `spec/probes/terrain_gen_proto.py` (n 1,024, donor MP_005) | generates, writes and reads back 3 terrains (seed, again, seed + 1) and a visual terrain | 17 s | 187 MB |
| `spec/probes/terrain_gen_proto.py --n 2048` | the same at 2,048 samples | 54 s | 615 MB |
| the writer canary (`checks()` with the game copy) | DICE's 30 main blocks written again from their heights, depth maps and header words | 126 s | 654 MB |

**What a generated terrain proved (offline, the suite's readers):** a 1,024 and a 2,048 main block read in the Rust
reader (every height within one u16 step, the tail taken as a main block's), the depth runs, padding and min / max
tree re-encoded by the DICE-checked reader, the built lower envelope under every sample; the material map decoded and
re-encoded byte for byte; the water mesh read; a one-block visual terrain with MP_005's 50 patch shaders and every cell
on its rule-only shader read back; same seed same bytes. The canary: DICE's 30 main blocks written again are byte for
byte DICE's with their own envelope (30 of 30) and identical up to the envelope with a built one (30 of 30), the built
envelope under every sample (30 of 30). The 2,048 file is 8,669,245 bytes, the size of DICE's.

**What DICE's terrains are (the numbers a generator matches):** 30 main blocks of 1,024 (11) / 2,048 (19) samples of 1
m, centred on the origin from height 0; five biomes by the levels' vegetation sets (arid 5, jungle 9, snow 6, temperate
5, vietnam 5; the material maps agree on 30 of 30); median slope 20.4 degrees (biome medians 8 - 30.5), a steep tail
(99th percentile 8 - 84 degrees), hypsometric integral 0.12 - 0.57, spectrum slope beta 3.2 - 4.6; 27 of 30 with ground
under water (0 - 83 %); borders a median 19 m above the centre. Combat areas (61 levels): Conquest 44 - 479 thousand m2
(median 99, the placement study's figure), at most 22 % of the terrain in any mode, convexity median 82 - 97 %, flatter
inside than outside on 52 of 61 levels; 50 Conquest flags on the ground at 1.5 degrees median slope (92 % under 10) at
any height of the combat area.

**The prototype against DICE:** spectral synthesis at the biome's beta, relief scaled to the biome's median slope,
thermal erosion, objective pads, the water level at the biome's under-water share: every biome's beta within 0.1,
median slope within 0.1 degree and under-water share exactly (seed 7, 1,024 samples); its 99th-percentile slope (20 -
56 degrees) and 66 m relief (0.9 - 3.2 m) fall short of DICE's biome medians (53 - 75 degrees, 2 - 13 m): hydraulic erosion and
height remapping are TG1's research item (TGEN-GEN-3).

### What is left (TG1 - TG5)

TGEN-PLAN-1: TG1 the Rust generator and writers (ring, visual water, the canary, erosion and remapping tuned to the
biome statistics); TG2 the visual side (a donor's patch and undergrowth shaders renamed into the new terrain's
databases, the colour / occlusion map, the game-accurate view); TG3 E7's Create Level wired to the generator, the export
with the level's own terrain bundle, `export::check`, NavKit from the offline sampler; TG4 vegetation, rocks and props
with E5's sculpt and paint; TG5 gameplay through A3 and the WW2 (pacific) biome. Unproven until in-game tests resume:
the engine with a generated envelope, a donor's rule layers on other heights, a level without its base's terrain
bundle, our own water mesh, craters on a depth map of 255.
## SPG4: the specification's census - everything in DICE's game files against the rules (2026-11-23, branch `spg4`)

A specification round (no Rust or tool code changed): the census, two read-only probes, 13 new spec files, 72 rules
(1,348 -> 1,420). Static analysis only: DICE's files through the suite's readers, `BFBC2Game.exe` (build 795745) read
as data on a scratch copy (its `typeinfo` section, strings, capstone around single references); nothing launched.

| Probe | Reads | Time | Checks |
|---|---|---|---|
| `spec/probes/census.py --measure` | the game folder's file list, every entry header of DICE's 1,021 bundles, every DBX partition once (84,641, tree parse), the exe's `typeinfo` section | 117 s (2 workers) | 4 in the game tier (files, res, dbx, engine equal to `census.json`) |
| `spec/probes/census.py --check` | `census.json` and the spec's markdown | 0.5 s | the quick tier's gate |
| `spec/probes/resources.py` | the distinct OccluderMesh, SootMesh, AnimTreeInfo, ImpulseResponse, RagdollResource and BinkMemory entries, the 33 movies, the loose files | 5 s | 36 |

**Census, before -> after** (a rule covers an item when its text names it as a whole word; catalogue rules excepted):

| Category | Items | Covered before | Covered after | Out of scope | None after |
|---|---|---|---|---|---|
| file kinds outside the bundles | 16 (1,317 files) | 6 | 13 | 3 (DirectX, `winui.dll`, `Support`) | 0 |
| bundle entry types | 37 | 28 | 37 | 0 | 0 |
| DBX instance classes | 630 (2,888,042 instances) | 304 | 630 | 0 | 0 |
| typeinfo modules | 22 | 19 | 22 | 0 | 0 |
| typeinfo records (not gated) | 1,900 | 419 | 799 | 0 | 1,101 |

**Findings.**

- The executable's reflection: a `typeinfo` record is 24 bytes (28 for value types and enumerations, with their
  member table in `fieldinf`); kind = (flags >> 4) & 0x1F; every DBX class is one of its 1,362 classes (ENG-TI-1, 2).
- The PC's `SootMesh` has a 16-byte float vertex form in 6 of 40 files, which the console converter's guard ("every
  BFBC2 soot mesh has" the 12-byte form) does not allow for (SOOT-VTX-1); its tail is the box and triangle counts.
- `AnimTreeInfo` is the tree compiler's memory image: a base address and three blocks, every pointer absolute in that
  image (ATREE-PTR-1) - which is why ANIM-TREE-3 saw pointer shifts between trees.
- `ImpulseResponse` is the one big-endian PC resource (`irf1`, FFT partitions of 1,024 frames, IR-HDR-1).
- The movies outside the bundles carry the RES header with zero length words; `BinkTexture` ones have 8 bytes more
  (BINK-OUT-1). `database.dbmanifest` is DICE's editor database manifest, a copy of the build overlays' entry;
  `dfe`, `InstallationData.xml`, `UpdateHistory.log` and the dialog logo are named by no game file.
- Only SP_006 has `HasFlyingAI`, and it is the only level with an `AerialHeightmap.res` (GFILE-AHM-1).
- The spotting cones are per weapon (`...WeaponEffectsData`: `ActiveSpotConeAngle`, `FocusConeAngle`,
  `OrderConeAngle`); the kill cam has no data class, only a texture atlas.

**Unproven until a playtest** (in-game tests paused): the FOV's composition (SOLD-FOV-1), the aerial height map's
loader condition, whether the start-up object reads `database.dbmanifest`.

### What is left (SPG4-rest)

The 1,101 typeinfo records no rule names (`spec/CENSUS.md` section 5), the run-time semantics behind the data-side
files' rules (each file's open questions), and the opaque insides of OccluderMesh's triangle records, AnimTreeInfo's
blocks and the impulse responses' partitions.

## NKS-deep B: DICE's node placement decoded and built, the remaining generator rules (2026-11-23, branch `nksdb`)

Part B of NKS-deep (the old NKS-rest3), in the ranked order NKS-deep A left. The generator's own code in
`BFBC2Game.exe` (Kynapse's `kymapbuilderrgg.cpp` and `kymapbuilderobstacles.cpp`, 0x01140000-0x011B8000) was read with
headless Ghidra on a scratch copy of Project OPFOR's analysed project and capstone on a scratch copy of the executable
(scripts in scratch, deleted); no game run, nothing written to OPFOR's `game\`.

### What the executable says

| Rule | Finding | Status |
|---|---|---|
| KYG-NODE-7 (new) | the node generator propagates breadth first from the seed points (the exploration's own seed list): each node sends **NbSectors** probes (32 coarse, 128 detailed, 64 vehicle; KYG-PAR-2 had NbSectors "not decoded") one every 2 pi / NbSectors from +x, DistPropagation + 2 pitches along the mesh, pulled back 2 pitches at an obstacle; farthest first, a probe's end becomes a node unless a node is within DistDensityMin, or within DistDensityMax and in a straight walk | verified: of the node pairs exactly one step apart, 21,709 of 21,884 coarse, 2,272 of 2,407 detailed, 341 of 359 vehicle lie on the NbSectors directions (control 243 of 15,521) |
| KYG-NODE-8 (new) | dense edges within ConnectionDistance (unset by DICE: Kynapse takes 1.2 x DistPropagation, 4.02 m), then edges collapsed into their middles shortest first while every neighbour still walks straight to the middle within DistEdgeMax (leaves, locked nodes and two-edge apexes kept; dirty edges / OverSimplify's short edges with a 5 m-cell lock and rerouting / all edges, repeated) | observed |
| KYG-EDGE-5 (new) | the 1.2 rule is a **removal** pass over the simplified graph's edges in order (A* inside a sphere of 1.2 x the length), not a greedy addition over every pair in sight: KYG-EDGE-3's 37 % of coarse pairs without an edge are pairs never adjacent in the simplified graph or edges whose route a later removal broke | observed |
| KYG-AREA-4 (new) | the midline cut's trigger: an area whose built record reaches MaxLocalMapBinarySize (Kynapse's default 8,192 bytes) is dealt into four by its cells' box midlines ((min + max) >> 1), its tile and the four side neighbours rebuilt, until none does | observed (the size is the unsimplified record's: not fitted from the stored files) |
| KYG-NODE-1 / KYG-NODE-4 | explained: nodes are propagated and then merged into middles of middles (off every grid); the step pairs are propagation nodes that never merged | - |
| KYG-PAR-3 | StepMax / StepLength x pitch as a step tested on the raw samples (`kynapse_df`): detailed 0.156 m finds 14,616 of DICE's terrain breaks (0.175 m: 14,010) for 824,630 false ones (48,791); normal-pitch object sides 787 found / 2,180 false (0.6 m: 865 / 2,331); KYG-AREA-1 puts the link step under 0.62 m. NavKit keeps 0.6 / 0.175 m and the 0.77 slope | data added |

Probes: `cargo run --release -p bc2-formats --example kynapse_place -- --only angles` (35 files, about 40 s) and the
game-tier `kynapse_corpus.rs::step_pairs_lie_on_the_nbsectors_probe_directions` (1.2 s in release).

### What changed in NavKit

`bc2_navbuild::propagate` builds a sector's route graph the map builder's way on NavKit's sample grid: the sector's
border nodes and the AI order positions in it as locked seeds (DICE has a node within 0.2 m of each order position,
KYG-NODE-1), the probes over the sector's kept surfaces with the area link rule and the route edges' D sides, a
point's height its cell's surface, then the dense edges, the simplification and the removal pass, nodes left without
an edge dropped; the ladder links, group joins and order nodes follow as before. Behind `Switches::placement`
(`navwalk.PLACEMENT`), **on** by the user's ruling (2026-11-23): the regressions below are named exceptions under
`spec/transforms/navkit.md`'s parity target, sp_007's real `over_connection` rise handed to round NKS-deep D. A
level builds in 8-86 s.

### Measured: the 15 regression levels (scratch builds, one at a time; before = the reference `regression`)

| Measure (mean) | before | placement |
|---|---:|---:|
| `nodes_near` | 47.63 | 56.21 |
| `ai_pairwise` | 92.67 | 95.57 |
| `connectivity` | 98.79 | 99.06 |
| `runtime_connectivity` | 98.80 | 99.06 |
| `over_connection` (lower is better) | 15.31 | 15.74 |
| `costs` | 95.29 | 84.92 |
| `nodes_same` | 0.66 | 0.63 |
| `edges_same` | 0.02 | 0.01 |
| ground, extra_ground, areas, portals, outline, borders, D / F, `ai_coverage`, `strong_share` | | unchanged |

| level | nodes_near | ai_pairwise | connectivity | runtime_connectivity | over_connection | costs | nodes_same |
|---|---:|---:|---:|---:|---:|---:|---:|
| sp_01 | 46.12 -> 59.13 | 96.4 -> 98.9 | 98.66 -> 99.1 | 99.1 -> 99.39 | 59.94 -> 53.53 | 95.45 -> 73.33 | 1.18 -> 0.92 |
| sp_02 | 43.51 -> 52.6 | 96.1 -> 97.7 | 99.49 -> 99.45 | 99.56 -> 99.46 | 7.78 -> 6.85 | 80 -> 100 | 0.43 -> 0.37 |
| sp_03 | 43.37 -> 60.51 | 97.9 -> 98.1 | 98.46 -> 99.15 | 98.59 -> 99.3 | 20.7 -> 23.2 | 89.77 -> 96.43 | 1.26 -> 0.99 |
| sp_04 | 40.63 -> 48.48 | 97.3 -> 97.3 | 99.61 -> 99.58 | 99.8 -> 99.58 | 7.57 -> 7.38 | 100 -> 100 | 0.54 -> 0.53 |
| sp_003_b | 40.48 -> 60.23 | 70.3 -> 74.4 | 93.67 -> 95.28 | 92.67 -> 94.52 | 29.35 -> 37.02 | 89.32 -> 94.37 | 1.51 -> 1.2 |
| sp_004_b | 56.47 -> 62.2 | 84.1 -> 88.3 | 100 -> 99.9 | 100 -> 99.87 | 18.21 -> 17.62 | 100 -> 100 | 0.41 -> 0.38 |
| sp_005 | 41.56 -> 57.4 | 98.6 -> 98.6 | 99.42 -> 99.56 | 99.38 -> 99.54 | 14.06 -> 12.53 | 100 -> 86.44 | 1.04 -> 0.94 |
| sp_005_b | 42.85 -> 61.93 | 95.6 -> 98.5 | 99.36 -> 99.59 | 99.37 -> 99.59 | 23.96 -> 23.34 | 96.3 -> 85.71 | 1.04 -> 0.92 |
| sp_006 | 31.77 -> 39.97 | 94.7 -> 96.4 | 99.46 -> 99.59 | 99.63 -> 99.72 | 7.75 -> 6.65 | 85.71 -> 50 | 0.37 -> 0.34 |
| sp_007 | 49.23 -> 58.43 | 92.7 -> 94.4 | 98.43 -> 98.22 | 98.25 -> 98 | 10.87 -> 17.47 | 92.86 -> 87.5 | 0.78 -> 0.73 |
| sp_008 | 59.91 -> 68.59 | 96.2 -> 95.5 | 98.63 -> 98.97 | 98.96 -> 99.17 | 6.68 -> 6.12 | 100 -> 100 | 0.56 -> 0.58 |
| sp_008_b | 61.82 -> 59.16 | 98.3 -> 98.3 | 98.69 -> 99.66 | 98.5 -> 99.56 | 11.75 -> 13.12 | 100 -> 100 | 0.38 -> 0.57 |
| sp_009 | 41.08 -> 32.43 | 71.8 -> 97.2 | 100 -> 100 | 100 -> 100 | 0.5 -> 0.53 | 100 -> 100 | 0 -> 0.21 |
| ons_mp_002 | 54.91 -> 56.93 | 100 -> 100 | 100 -> 99.85 | 100 -> 99.81 | 2.22 -> 2.1 | 100 -> 0 | 0.14 -> 0.3 |
| ons_mp_004 | 60.81 -> 65.18 | 100 -> 100 | 97.9 -> 98.04 | 98.23 -> 98.42 | 8.38 -> 8.68 | 100 -> 100 | 0.26 -> 0.4 |

Structure (scratch counts, sp_01; DICE / before / placement): coarse ordinary nodes 10,207 / 9,623 / 11,853, nearest
ordinary node median 3.14 / 1.35 / 2.90 m, edges per node 2.72 / 1.88 / 2.73; detailed ordinary nodes 9,456 / 17,189
/ 14,990, edges 36,903 / 88,665 / 50,102. sp_009: coarse edges per node 3.70 / 1.65 / 3.64, detailed nearest-node
median 5.20 / 4.44 / 5.19 m. Both ways within 2 m (sp_01): DICE nodes with a NavKit node near 44.8 -> 58.8 %, NavKit
nodes with a DICE node near 38.4 -> 50.8 %.

**What got worse, with counts (named exceptions by the user's ruling, 2026-11-23):**
- `over_connection`: mostly the matched sample growing. On sp_003_b the DICE group pairs NavKit wrongly joins fall
  8,622 -> 7,629 and on sp_03 5,442 -> 4,753, while the matched DICE nodes grow 19,318 -> 29,999 and 21,654 -> 31,379
  (the measure weighs node pairs, so more matched nodes in the same wrong joins count more). sp_007 is a real rise:
  147 new joins between DICE groups both builds matched (942 -> 987 joined pairs), led by one 142-node DICE island at
  (595, 150, 476) that NavKit's detailed ground joins to the main group through edges of 0.3-1.5 m beside DICE's own
  island boundary (NavKit's ground there, not the placement, decides it); sp_008_b the same 6 joined pairs with fewer
  matched nodes.
- `costs`, `nodes_same`, `edges_same`: tiny samples. `costs` is the share of edges matched within 5 cm whose cost
  word equals DICE's (a handful of edges per level); `nodes_same` counts nodes on DICE's exact positions, which
  NavKit's old nodes met only where DICE kept a cell-centre node (KYG-NODE-1's 4 %).
- per level: `connectivity` sp_007 -0.21, ons_mp_002 -0.15, sp_004_b -0.10, sp_02 / sp_04 -0.04 / -0.03;
  `runtime_connectivity` likewise (sp_04 -0.22); `nodes_near` sp_009 41.08 -> 32.43 (the old build had 2.6 x DICE's
  nodes there: the share of NavKit's nodes near a DICE node rose 23.0 -> 32.4 %) and sp_008_b 61.82 -> 59.16;
  `ai_pairwise` sp_008 96.2 -> 95.5.

### Left

- KYG-AREA-4 in NavKit: needs NavKit's records sized as Kynapse's unsimplified ones, or the size fitted against
  DICE's cut groups on part C's 0.2 m samples.
- DICE's detailed sectors (KYG-DET-1): part C's offline sampler (on main after this round's runs) samples all 502 at
  0.2 m; the `DICE_DETAILED` measurement is to be repeated with it, with the placement on and off.
- KYG-NODE-8's seeds and mesh queries are NavKit's stand-ins (border nodes, order positions; the sample grid); vehicle
  builds keep NavKit's placement (no vehicle sample run).
- Object-side D / F: no rule appeared (the 0.6 m step stays).

### Tests

- `cargo test -p bc2-navbuild --lib`: 100 pass (`propagate_tests`: DICE's parameters, the probes' step and
  directions, the density limit, open ground simplified within DistEdgeMax into one graph, the removal pass, a wall
  passed through its gap only).
- `kynapse_corpus` (game tier): `step_pairs_lie_on_the_nbsectors_probe_directions` new.
- The switch on by default (the user's ruling), for the infantry pitches (0.8, 0.2); vehicle builds (pitch 1 m) keep
  NavKit's own nodes (ONS_MP_002's vehicle path data unchanged). navregress's reference `regression` saved again with
  it (the old one kept as `regression_before_2026-11-23_nksdb.json`; the new rows equal the scratch run above, every
  measure); the 10 whole-stage answers re-recorded; `test_navtools_native.py --record --game --only measures` (the
  parity rows of sp_009 / sp_004_b, whose candidates are the new golden builds).

The golden builds (`navbuild_golden.json`, old -> new SHA-1, bytes):

| level | old | new |
|---|---|---|
| sp_009 | 7f5f994c100e439d42810258295a8edc0f6329a5 (453,264) | 65e1f51d18c32350f6f549f86536081da2a69735 (331,212) |
| sp_004_b | 8d0e8305700314ad62eda32c896472f1bb5fbfb1 (8,565,240) | 20f9b284871d45babd98d92d232de96693601421 (8,046,096) |
| sp_008_b | 19c2ac0ff612d3fb8db61b8b37b7a19158ed13ed (8,843,220) | 2774bb3f1f7ed3797c545d99c75140a77b05720a (7,977,168) |
| ons_mp_004 | 326b162973db3fc5d41d072df7655db8f6fb75b3 (8,023,856) | dc870cc8b87ef4c500c6303c719f663850487173 (7,297,064) |

The files shrink with NavKit's detailed nodes and edges (sp_01's detailed edges 88,665 -> 50,102).

## WW2-ORD1: the WW2 rebuild settles in one pass from an empty work\ww2 (2026-11-23, branch `ww2ord1`, with Project OPFOR's `ww2ord1`)

An OPFOR round with a specification update here (no suite code changed). It fixes SPG3's XDEP-STEP-3: three steps
of OPFOR's WW2 rebuild read files that only a later step wrote. OPFOR's commits:
- **The order**: `bf1943_jp_bazooka`, `wake_vo`, `anims`, then `material_grid`, all before `wake_translate`. Both
  readers take every OURS root, and `material_grid` stopped on an empty folder. A test holds every OURS producer
  before both.
- **The tracker** (stepstamps):
  - the install indexes (`install_*.pkl`) and rebuild_all's own logs are left out;
  - the files under a folder handed to the Rust core go through the hook's skips;
  - a listing of a folder that the step or a later step fills, at any depth, is not judged.
- **`wake_ambience`** makes its preview folder before drawing into it.
- **`qa_checks`** no longer fails when there is no install report.

| Run | Steps | Time | Oracle (10,852) | `--changed --dry` after | Probe: read-before-write |
|---|---|---|---|---|---|
| before, OPFOR's records of 2026-11-22 / 23 | 55 | - | 10,852 (main's folder) | - | 3 edges, 37 files |
| from seeded-empty (599 external input files), rebuild 4 | 55 in one pass | 21.5 min | 7,921 identical, 28 differ, 2,903 missing | 0 of 55 | 0 (362 read-after-write edges, 7 write-after-write) |
| OPFOR's main checkout, `--changed` on the new order | 55 (tracker code changed) | 19.4 min | 10,852 of 10,852 | 0 of 55 | 0 (336 read-after-write edges, 9 write-after-write) |

The first empty-folder runs exposed the fixes above. Run 1 stopped at `wake_ambience`. Runs 2 and 3 never settled:
`material_grid` recorded the step logs as inputs, and two listings looked changed.

**Findings.**
- **XDEP-STEP-5**: the rebuild reads inputs under `work\ww2` that no step writes:
  - hand-placed files: `fonts\`, `weapon_icons\confirmed`;
  - the install plan's staging: `install_ports*`, `install_<lv>\level\objects\vegetation`. That staging is a rebuild
    -> plan -> rebuild cycle.
- **XDEP-STEP-6**: the oracle holds the folder's history, and every difference from empty falls in one of these:
  - 2,903 relics no step writes (2,798 debug HLSL sources from manual `--hlsl` runs, 105 textures);
  - 18 L8 textures that the clone pass keeps from a 2026-11-05 converter. They have unpadded smallest mips against
    TEX-FMT-2 and flag bit 1 against TEX-FLAG-1; the current converter follows both rules;
  - 4 effects caches with equal content but different pickle bytes;
  - 6 emitter-atlas reports that count the relics.

  Regenerating the oracle is for the user to rule on (round WW2-ORC1).

**Unproven until a playtest** (in-game tests paused): nothing is installed by this round. The 18 malformed textures
are in the last install, and whether they show is unknown.

## WW2-ORC1: the WW2 rebuild oracle regenerated cleanly (2026-11-23, branch `orc1`, with Project OPFOR's `orc1`)

An OPFOR round, done on the user's ruling of 2026-11-23 ("regenerate it cleanly"), with a specification update here
(no suite code changed): XDEP-STEP-5 and XDEP-STEP-6 implemented, XDEP-STEP-7 new. OPFOR's commits:
- **The 18 textures**: `wake_shaders.convert_textures` converts every BF1943 texture on every run and writes it when
  the file differs. grime.py's weathering of today's conversion is kept. The 18 kept conversions of 2026-11-05 were
  BF1943's console DXT5A masks (format 3, flags 0) written as L8 with flags 2 and smallest levels 16 / 4 / 1. They are
  now 16 / 8 / 4 with flags 0, as DICE's L8 textures are (TEX-FMT-2, TEX-FLAG-1). A test converts synthetic console
  textures and checks both rules.
- **The caches**: `rebuild_all.CACHES` (`effects_cache.pickle`) is left out of the hashes.
- **External inputs**: `rebuild_all.EXTERNAL_INPUTS` (14 paths: the font and its licence, the weapon icons'
  sign-off, the install plan's staging). A run stops on a missing one, naming its source; `--without-inputs` runs
  anyway.
- **`archive_relics.py`**: moves the files a seeded-empty rebuild does not make to `work\ww2_relics_<date>\`, with a
  manifest.
- **The tracker** (version 3) keeps a step's in-place edits with the version it read; a run sent back to that step
  goes back to the version's writer (XDEP-STEP-7).
- **`rifle_grenades.py`** no longer writes a json into `rg_projectiles`' output folder (it never settled).
- **The oracles**: re-recorded; the old ones are kept dated.

| Run | Steps | Time | Result | `--changed --dry` after |
|---|---|---|---|---|
| seeded from empty (14 declared inputs, 599 files), worktree | 55 | 19.0 min | 7,945 files; vs the old oracle 7,921 identical, differ 24 (18 textures, 6 atlas reports), missing 2,907 (2,903 relics, 4 caches) | 0 of 55 |
| main: `archive_relics.py --apply` | - | - | 2,903 moved (2,798 `wake\shaders_hlsl`, 78 `wake\textures_bf1943`, 13 + 14 `textures_bf1943\Tinted`), 12 emptied folders removed | - |
| main: `--changed` | 30 of 55 | 16.8 min | 7,945 of 7,945 = the seeded run | 1 (`rifle_grenades`, fixed) |
| main: `--changed` again (before the tracker fix) | 18 | 18.3 min | 9 files differ: the clone pass rerun alone on the finished databases (XDEP-STEP-7) | 17 |
| main: full rebuild | 55 | 15.8 min | 7,945 of 7,945 = the seeded run | 0 of 55 |
| main: `--changed` | 0 | 0.3 min | 7,945 of 7,945 | - |
| `wake_install.py --plan` | - | - | 21,117 of 21,197 against the old install oracle: differ 32, missing 48 (below); new install oracle 21,149 | - |

**The new oracles.**
- Rebuild: 7,945 files (was 10,852), from main's rebuild, identical to the seeded-empty rebuild.
- Install: 21,149 (was 21,197). The changes:
  - 48 relic textures that no staged file of their level names are gone (21 Wake, 13 Guadalcanal, 14 Iwo Jima);
  - 17 of the 18 textures changed;
  - `Sky_Beach_Generic_RGBA16f` is DICE's PC copy (mp_common's) in all four levels. A relic BF1943 conversion of
    2026-11-07 had overridden it;
  - 4 level bundles, 3 manifests and 4 reports changed with them.

Entry by entry against the game copy's installed bundles, the new level bundles differ only there and in the Waves
(the installed copy predates PR1b-1c). The plan left `game\` unchanged: 0 of 1,582 files by sha256.

**Unproven until a playtest** (in-game tests paused):
- nothing is installed by this round;
- a reinstall changes Wake's 17 textures (row padding and a header flag only);
- it changes the sky cube of all four WW2 levels to DICE's own copy, a visible change.

**Proposed, not done** (XDEP-STEP-5): the plan's porting as a rebuild step (`wake_install.py --ports` before
`wake_translate` and each level's chain), which would turn the rebuild -> plan -> rebuild cycle into an order.

**Note for the merge**: tracker version 3 makes the next `--changed` on OPFOR's main checkout rerun all 55 steps
(expected 7,945 of 7,945).

## TG1: the terrain generator's core in Rust (2026-11-23, branch `tg1`)

New crate `rust/bc2-terragen` (bc2-formats and rayon only) and `bfbc2 terrain generate` (new bfbc2.exe tool group
`terrain`): a main height field from a seed shaped to one of DICE's five biomes, its material map painted from slope
and height, its water mesh and a `terrain.json` (seed, settings, `generator_version` 1, the statistics against the
biome's DICE medians). The blocks are written through round E5's `bc2_formats::terrain_codec` (merged from `e5`,
a85e0514): E5's corpus test is the writer canary in Rust (DICE's 30 main blocks and 30 material maps byte for byte).
Spec: TGEN-GEN-3 implemented, TGEN-GEN-4 (the shapes) and TGEN-GEN-5 (the library and the command line) new,
TGEN-GEN-1, TGEN-OUT-1 - 3, TGEN-PLAY-3 and TGEN-MAT-1 / -2 cite the Rust.

**The pipeline** (`bc2_terragen::generate`): spectral synthesis at the biome's beta (own radix-2 FFT); five rounds of
hypsometric remapping and *slope matching* - each sample's gradient given the magnitude its rank takes in the biome's
slope distribution (DICE's p10 / p50 / p90 / p99), the surface rebuilt by a periodic Poisson solve, the aimed
distribution corrected by the miss measured as the probe measures (central differences); hydraulic erosion (125
droplets per 1,000 samples); the border bowl corrected to the biome's border-minus-centre; the sea floor flattened by
bisection where the 10th slope percentile stays over 2 degrees above the biome's; the hypsometric integral remapped
again, the median slope scaled, the border corrected again; pads; the range rounded up to 50 m; the water level by
share. Every random value is a hash of (seed, step, index); the bytes are the same on 1 and 4 threads (test).

What closed the prototype's gap, step by step (seed 7, 1,024, the generator's own statistics):

| Step added | arid p99 | jungle p99 | snow HI | jungle p10 | Note |
|---|---|---|---|---|---|
| TG0 prototype (thermal erosion, median-slope scale) | 20.0 | 45.9 | 0.60 | 9.0 | near-Gaussian slopes |
| slope matching + remapping (4 rounds), erosion, border | 50.9 | 75.1 | 0.49 | 5.2 | the tail reached; HI drifts in the last steps |
| + a final hypsometric remap after the border | 51.6 | 74.0 | 0.41 | 5.3 | HI on target on all five |
| + the sea floor flattened (bisection, 2 degree slack) | 51.7 | 76.0 | 0.41 | 3.5 | p10 down; without the slack snow's p90 rose to 64.6 |
| + the border corrected after the scale | 51.9 | 76.0 | 0.41 | 3.9 | border within 0.2 m on all five |

**The shapes** (`spec/probes/terrain_shape.py`'s `shape()` on the written main blocks; TGEN-GEN-4 has the full
table): 99th-percentile slope arid 51.9 / jungle 76.0 / snow 74.6 / temperate 75.3 / vietnam 62.7 degrees (DICE's
medians 52.8 / 75.0 / 71.1 / 75.1 / 64.6; prototype 20.0 / 45.9 / 55.9 / 48.3 / 33.5); 66 m relief 1.63 / 4.67 / 4.55 /
4.43 / 2.36 m (DICE 1.98 / 4.67 / 4.91 / 13.16 / 3.35; prototype 0.87 / 2.45 / 3.21 / 2.80 / 1.52); HI 0.29 / 0.42 /
0.41 / 0.36 / 0.22 (DICE 0.27 / 0.41 / 0.41 / 0.36 / 0.21; prototype 0.58 - 0.60). The Rust statistics equal the
probe's on the same files to the printed digit.

| Run (release, this PC, other rounds building) | Time | Peak RAM |
|---|---|---|
| `bfbc2 terrain generate`, 1,024 (five biomes, seed 7) | 1.1 - 2.4 s | 126 MB |
| `bfbc2 terrain generate`, 2,048 (five biomes, seed 11) | 6.8 - 9.8 s | 486 MB |
| `cargo test -p bc2-terragen` (dev profile: two whole 1,024 terrains, one on 1 and 4 threads, five biomes) | 11.5 s | - |

Files: a 1,024 main block is 2,171,963 bytes and a 2,048 one 8,669,245 bytes (DICE's sizes); material maps 0.5 - 4 MB.

### What is left

- The sub-terrain ring and the visual water (TGEN-PLAN-1's TG1 row; moved to TG2 with the visual terrain).
- Temperate's rough relief (10 m / 66 m: 0.94 / 4.43 m against DICE's median 4.77 / 13.16, which three rocky terrains
  set): the ridged / terraced option of TGEN-GEN-3.
- Jungle's 10th slope percentile (3.9 against 0.6; DICE's range 0 - 7).
- The playable area kept flatter (TGEN-PLAY-1) once A3 / TG5 give its region; shores and fords.
- `spec/probes/terrain_gen_proto.py` is not retired: its visual-terrain writer (TGEN-VIS-1) has no Rust twin yet and
  its checks are the Python readers' view of a generated terrain; TG2 retires it with the visual writer.
- Unproven until a playtest: everything TGEN-OUT-5 lists (a generated block in game, its envelope, the depth map of
  255, the water mesh).

## E5: terrain sculpt and paint (2026-11-23, branch `e5`)

The map editor's Terrain workspace works: sculpt brushes and the material paint, saved as project operations,
exported into the level's terrain bundle and checked. ROADMAP row E4-E7's E5; spec `transforms/terrain_edit.md`
(TEDIT-HF-3 and TEDIT-MM-1 implemented, TEDIT-BR-1 and TEDIT-NAV-1 new), `transforms/validation.md` (XVAL-TERR-2
new, XVAL-EDIT-5 now for the visual paint only).

**The codec** (`bc2_formats::terrain_codec`, owned by E5 and used by TG1's generator; the coordinator's ruling):

| On DICE's terrains (`rust/bc2-formats/tests/terrain_corpus.rs`, release) | Result |
|---|---|
| 30 main blocks decoded and encoded | 30 of 30 byte for byte; computed min / max tree = stored on 30 |
| 288 sub-terrain blocks | computed quadtree = stored on 288 |
| 30 material maps decoded and encoded | 30 of 30 byte for byte |
| the whole pass | 1.7 s |

Finding: MP_005's main block is 2,048 samples, not 1,024 as `terrain_edit.md` section 5 said (corrected there).

**The writers** (`bc2_levelwrite::terrainedit`, `rust/bc2-levelwrite/tests/terrain_edits.rs`, release):

| Level | Block | Edit | Written in | Envelope | Material map |
|---|---|---|---|---|---|
| MP_005 | 2,048 | a 20 m hill, a 10 m pit | 0.015 s | 233 vertices lowered; under share 1.0000 (DICE's 1.0000) | 4,096 cells, 7 names, a checkerboard painted |
| SP_01 | 2,048 | the same | 0.016 s | 299 vertices lowered; 0.9961 (DICE's 0.9961: DICE's own poke-throughs kept) | 4,096 cells, 9 names |

Nothing edited writes DICE's bytes (both files, both levels). The edited files read back with `terrain::parse`,
`MainBlock::decode` and `MaterialMap::decode`; their tree is their heights', their edited samples lie over the
envelope, the depth map, bounds and names are DICE's.

**The export** (`sculpted_export_stages_and_checks`): EX1's canary (Atacama's Rush as Conquest) with a sculpt stroke
and a material stroke (tiles beside the project, project format 11, saved and read back) built in 29.5 s;
`export::check` passes; XVAL-TERR-1 holds for the terrain bundle's other entry, XVAL-TERR-2 for the two rewritten.
MP_005GR's own terrain bundle holds neither resource (it loads MP_005's), so the export adds them to the staged
terrain bundle, which the level loads first. `export_rules` / `export_rules_stage` unchanged (7 of 7).

**The editor** (`rust/bc2-editor/tests/terrain_tools.rs`):

- through the interface, both themes, on the demo level with a synthetic 256 m height field and material map: Raise
  dragged is one `stroke` of at most 4 tiles; the heights and the mesh rise under it, nothing far away moves; Ctrl+Z /
  Ctrl+Y; a rebuild from the tiles gives the same heights; Flatten, Smooth, Noise, a two-click Ramp, the Material
  brush (format 11); the Paint layer greyed (E5-rest); Select back when the workspace is left;
- on MP_005 and SP_01 as loaded (game tier): a raise dab and a material dab committed as strokes, undone to DICE's
  heights, redone. A 20 m raise dab and its stroke (tiles kept, the log applied, the scene's heights and mesh
  made again) 0.040 s on MP_005 and 0.057 s on SP_01; a material stroke on the 4,096-cell map 0.086 / 0.092 s
  (debug build at opt-level 1).

The stroke budget (`editor_core::stroke_budgets`, 400 strokes on a 2,048 height field): commit median 3.65 ms
(OP1's 2.6 ms plus the scene following: the stroke's tiles copied into the scene's heights and the mesh rows over
them), p95 6.38 ms; undo 20.3 ms, redo 15.9 ms (budgets 16 / 100 ms). A first version that compared the whole raster
and copied the height field and mesh after every stroke took 6.9 ms alone and passed 16 ms with the other editor
tests running; the sync now reads only the stroke's tiles and changes the scene's height field and mesh in place
(`Arc::make_mut`, a new allocation so the ray grid is rebuilt).

Live drawing: a dab writes only the vertex rows it touched into the simplified view's terrain buffer; the committed
stroke re-uploads the terrain once and rebuilds the ray grid on the next ray. The game-accurate view takes the new
height map at each committed stroke and undo (not during the drag).

**Path data**: a sculpt counts as a collision change, so the export's path-data step samples the stage
(`bfbc2 navkit sample --offline --stage`). The stage sampler now writes the staged height field over the base's
terrain unpack before sampling. Not run end to end in this round (it needs a prepared base level and a built
`bfbc2.exe`).

**Left (E5-rest)**:
- the occlusion / colour paint layer and "re-shade sculpted area" (the map is a DXT1 / L8 texture of the level bundle);
- the visual paint layers (patch maps, atlas tiles, patch shader entries);
- widening the height range: a sculpt is held to it and says so;
- a material the map lacks (the grid check), the visual / material pairing, the material map drawn in the views;
- strokes over the sub-terrain ring;
- the game-accurate view during a drag;
- the path-data sampling of a sculpted stage end to end;
- the playtest (`terrain_edit.md` section 5).

**Unproven until a playtest**: everything the game does with an edited terrain - drawing, collision, LOD, the
envelope, the depth map's meaning, the material under a contact. Nothing was installed or launched.

## PR1b-2-rest3: the Granny object in Rust; texqa's analyses on rayon measured (2026-11-23, branch `pr1b2r3`)

What PR1b-2-rest2 left ("PR1b-2-rest2", "What is left"): both items. Removals (`bc2_native.granny_header`,
`granny_index`, `granny_with_crc`; `granny.Granny` a function, no longer a class): the merge bumps MINOR (CHANGELOG
"Removed"). Nothing was written to OPFOR's `game\`; the game never launched.

### 1. rayon in bc2-cli's own CPU work: measured, not kept

texqa's analyses (`bc2-cli/src/texqa/run.rs`: scoped threads, at most 4) were moved onto rayon's one pool
(`par_iter` in order; `--workers` / `BFBC2_TEXQA_WORKERS` a pool of that size) and timed against the old
`bfbc2.exe` on OPFOR's `work\ww2\install` (789 textures, `--worst 0 --no-bf1943 --no-dice`, cold; the report and
`texqa.json` the same bytes every run; other rounds building on the PC, ~30% CPU):

| run | wall (s) |
|---|---|
| old: scoped threads (4) | 8.57, 8.37, 8.46, 8.47 (median 8.46) |
| rayon, the pool's 16 threads | 8.86, 8.29, 8.64, 8.19, 8.47 (median 8.47) |
| rayon, `BFBC2_WORKERS` 1 / 2 / 3 / 4 / 6 / 8 / 16 | 24.11 / 13.23 / 9.53 / 8.70 / 7.42 / 7.56 / 8.47 |
| every texture cached (the serial part: finding, hashing, the report) | 0.98 |

Not faster at the default pool, so not kept (the scoped threads stay). Root cause of the ceiling: the analyses are
allocation-bound, not thread-bound - CPU time of one run, user + kernel: 1 thread 16.9 + 6.7 s, 4 threads 22.8 +
8.1 s, 16 threads 62.2 + 30.1 s for the same work (the largest texture alone, Corsair's 4,096 x 2,048 colour map, takes
0.5 s). Each analysis allocates whole-picture float buffers (`bc2_pyrt`'s arrays) whose pages Windows commits and
zeroes per allocation; more threads contend on that. Left (below): buffers reused across a texture's levels and
checks, then the pool.

### 2. `Granny.put`: the Granny object is Rust's

| was | now |
|---|---|
| `granny.Granny`: a Python class; with the Rust core its tables came from `granny_header` (sections, offsets, relocations made on first use by `granny_index`) and `root()` from `granny_root` over a copy of the bytes; `put` wrote the Python's `bytearray` and its per-section copies, after which every read ran in Python (`_written`); `to_bytes` took the CRC from `granny_with_crc` | `granny.Granny(b, where)` returns `bc2_native.Granny` (`rust/bc2-py/src/granny.rs`): it holds the resource's bytes and its tables (read once), and answers `root` / `read` / `members` / `size` / `type_name` / `name`, `field` / `array_at` (`bc2_formats::granny::Granny::member_offset`), `put` (struct's packing, little-endian) and `to_bytes` (`bc2_convert::granny_edit::with_crc`); reads after a write take the same tables over the written bytes (`Granny::over`); the attributes are the old ones (`where`, `e`, `strings`, `data`, `sections`, `section_at`, `reloc`, ...) |
| the Python reader, writer and the Rust hand-offs in one class | `granny.GrannyReference`: the Python reader and writer only (the twin: `BFBC2_PURE_PYTHON`, `use_native(False)`, resources the Rust reader refuses) |

Decided against the Python twin (spec GRANNY-EDIT-2, DICE's data: PC Granny files are little-endian throughout and
their sections never overlap - 4,534 of the game copy's PC Granny resources, 0 with sections, relocation tables,
marshalling lists or the header overlapping, `scratchpad` probe):

| the Python | now |
|---|---|
| `put` packed any `struct` format (`>`, native `@` sizes) | `<` first, else `struct.error`; nothing written |
| `put` wrote the resource's bytes, then failed on a section copy too short for the place | the place is checked first: nothing written |
| later trees read from per-section copies (two sections over the same bytes would not share a write) | one copy of the bytes, the tables over it |
| a damaged resource whose tree failed in Rust was re-read by the Python (its own `struct.error` / `IndexError` / ...) | a `GrannyError` saying why (`<where>: <why>`) |

**Proof** (the module and `bfbc2.exe` built from this branch):

| what | result |
|---|---|
| `test_granny_native.py` (quick): `writes` - 20 rounds of 30 writes into the built resource of every member type (in range, out of range, the wrong count, past a section, no such section; shuffled), `field` / `array_at` (a member that is not there), `to_bytes`, `read`, against `GrannyReference` | **600 of 600**: the same bytes, sections and tree after each write, the same refusals (type; `GrannyError` messages word for word), the same `to_bytes`; a big-endian resource read only both ways; `>I`, `I`, `=I`, `!I` refused, nothing written |
| the same: built resources (both byte orders) and 2,184 damaged copies, tables and trees both ways; `to_pc` against the recording | all the same, **2,193 of 2,193** conversions as recorded; 263 damaged copies the Rust core refuses with a `GrannyError` where the Python failed with its own exception |
| `test_granny_native.py --game`: every Granny resource of the Xbox mp_common / sp_common (each converted, the PC result read too), the game copy's PC ones, Battlefield 1943's | **2,660 / 4,534 / 756** read the same both ways (Rust 12.6 / 17.8 / 2.9 s, Python 40.2 / 42.1 / 10.9 s); 2,660 and 1,495 conversions as recorded |
| `test_granny_f2a_native.py` (quick): the WW2 rifles' 57 animations - every position curve moved (ramp, rest), every quantized orientation curve rewritten, `to_bytes`, tracks added, damaged copies; to_bytes of 300 headers with changed sizes / CRCs / cut short | 57 / 57 / 114 / 114 identical; 300 of 300 (93 the same error); 8 damaged copies refused (a `GrannyError` where the Python failed); edits Rust 0.67 s, Python 1.00 s |
| `test_granny_f2a_native.py --game`: every GrannyAnimation of the game copy's bundles the same way (shift, orientations, to_bytes, add_tracks; copies cut short) | **4,889 / 4,889** edited and **4,889 / 4,889** grown identically, all answered in Rust; 9,778 damaged and 9,778 refusal cases the same; 52 damaged copies a `GrannyError` where the Python failed; edits Rust 376 s / Python 460 s, add_tracks Rust 45 s / Python 278 s (each with its reads) |
| `test_granny.py`, `test_granny_curves.py`, `test_granny_port.py` | pass |

### Tests

- `cargo fmt`; `cargo clippy --all-targets -D warnings` on bc2-formats and bc2-py: clean; CI's per-group clippy of
  `bfbc2.exe` (all 19 groups of `[features] default`, each alone): clean.
- `cargo test` of bc2-formats and bc2-py: all pass (new: `granny::member_offsets_and_another_copy`).
- `run_tests.py quick -j 2` (module and `bfbc2.exe` rebuilt from this branch): **134 of 134**. O2 passes.
- `rust\native_census.py --check`: 285 exports, 0 without a caller; `python_status.py` regenerated (`granny.py` still
  partly: the twins left); `spec/trace.py --check` ok (1,468 rules; GRANNY-EDIT-2 new, unused on main); ruff and
  mypy on `bfbc2-anim-tools`.

### What is left

- texqa's analyses: allocation-bound (above) - reuse a texture's picture buffers across its levels and checks, then
  measure the pool again.
- `GrannyReference` and the `*_reference` functions (`shift_position_reference`, `set_orientations_reference`,
  `add_tracks_reference`, `_Graft`): the Python twins, kept for parity; retiring them needs their answers recorded
  (as `granny_golden.json` holds `to_pc`'s) - PY1.

## NKS-deep E: the measures over every node and edge, DICE's seeds, the midline cut's size, DICE's detailed sectors (2026-11-23, branch `nksde`)

Part E of NKS-deep (ROADMAP "NKS-deep"). The executable was read with headless Ghidra on a scratch copy of Project
OPFOR's analysed project (deleted); no game run, nothing written to OPFOR's `game\`. Every build was measured on the
15 regression levels (one level at a time, free RAM checked before each); the DICE-side rules on all 35 DICE files.

### 1. The measure rework (XNAV-MEAS-1)

`navdiff.paired_graph` / `bc2_navbuild::measures::paired_graph` (Python and Rust give the same counts on 40 random
file pairs): per sector every DICE node paired one to one with a NavKit node, nearest first, within half a
propagation step in plan (0.425 m detailed, 1.675 m coarse, 7.675 m vehicle) and 1 m in height. Over all DICE
nodes / edges: `nodes_paired`, `nodes_paired_candidate` (of NavKit's nodes: extra nodes earn nothing),
`edges_paired` (the partners hold the edge), `costs_paired` (and its cost is within a tenth), `node_distance` (each
DICE node's nearest NavKit node in plan, binned). The old measures stay beside them for this round.

The current build (B's placement on; the reference re-recorded, every old measure equal to B's):

| level | nodes_same (old) | edges_same (old) | costs (old, of matched) | nodes_near | nodes_paired | nodes_paired_candidate | edges_paired | costs_paired |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| sp_01 | 0.92 | 0.02 | 73.33 | 59.13 | 28.75 | 23.55 | 7.16 | 2.38 |
| sp_02 | 0.37 | 0.01 | 100.0 | 52.60 | 19.03 | 16.81 | 3.05 | 1.22 |
| sp_03 | 0.99 | 0.03 | 96.43 | 60.51 | 33.76 | 27.80 | 8.63 | 2.93 |
| sp_04 | 0.53 | 0.01 | 100.0 | 48.48 | 20.95 | 18.84 | 3.69 | 1.25 |
| sp_003_b | 1.20 | 0.04 | 94.37 | 60.23 | 36.74 | 31.28 | 9.76 | 3.31 |
| sp_004_b | 0.38 | 0.01 | 100.0 | 62.20 | 17.63 | 11.71 | 2.38 | 0.91 |
| sp_005 | 0.94 | 0.03 | 86.44 | 57.40 | 33.67 | 26.67 | 8.14 | 2.75 |
| sp_005_b | 0.92 | 0.02 | 85.71 | 61.93 | 32.15 | 26.05 | 7.59 | 2.60 |
| sp_006 | 0.34 | 0.00 | 50.0 | 39.97 | 18.53 | 16.02 | 2.60 | 1.03 |
| sp_007 | 0.73 | 0.01 | 87.5 | 58.43 | 27.10 | 20.60 | 5.31 | 1.82 |
| sp_008 | 0.58 | 0.00 | 100.0 | 68.59 | 26.55 | 16.28 | 6.06 | 2.15 |
| sp_008_b | 0.57 | 0.00 | 100.0 | 59.16 | 14.49 | 9.23 | 2.17 | 0.80 |
| sp_009 | 0.21 | 0.00 | 100.0 | 32.43 | 9.70 | 7.81 | 0.84 | 0.69 |
| ons_mp_002 | 0.30 | 0.00 | 0.0 | 56.93 | 15.64 | 14.76 | 2.05 | 0.81 |
| ons_mp_004 | 0.40 | 0.00 | 100.0 | 65.18 | 14.75 | 14.85 | 2.21 | 0.87 |
| **mean** | 0.63 | 0.01 | 84.92 | 56.21 | 23.30 | 18.82 | 4.78 | 1.70 |

Nearest NavKit node of each DICE node (mean shares): within 0.05 m 0.59 %, 0.1 m 0.59, 0.25 m 3.37, 0.5 m 8.07, 1 m
15.94, 2 m 23.29, 4 m 21.89, none within 4 m 26.27 %. The old `costs` is a share of the 0-5 edges a level matched
within 5 cm (ons_mp_002's 0 % is one edge); `costs_paired` covers every DICE edge.

Old and new definitions on B's placement (the same offline 0.2 m samples both ways, the only pair of runs with the
placement off): the old measures hardly move; the new ones show every graph measure rising.

| measure | placement off | placement on |
|---|---:|---:|
| `costs` (old) | 88.64 | 90.32 |
| `nodes_same` / `edges_same` (old) | 0.67 / 0.02 | 0.66 / 0.02 |
| `nodes_near` | 47.77 | 56.90 |
| `nodes_paired` | 15.23 | 23.48 |
| `nodes_paired_candidate` | 10.34 | 18.04 |
| `edges_paired` | 1.61 | 4.80 |
| `costs_paired` | 0.42 | 1.69 |

### 2. DICE's seed list (KYG-SEED-2)

Decoded: a sector's seeds come from its MapBuilder configuration - the spawn seeds in the sector's box (the
sectors maker), then its `.autoseed` perimeter points; BC2 snaps them to the sample grid and drops them onto the
ground; they become nodes unlocked, after any seed-graph nodes (none on DICE's levels). Checked on the 35 files
(`kynapse_seeds`): of DICE's detailed ordinary nodes on the sample grid, 584 of 1,610 lie on the lines two pitches
inside a square (0.8 % of the grid's points do); coarse on-grid nodes are not enriched there (279 of 7,562 against
3.2 %): coarse seeds merge, detailed ones often survive.

Built behind `Switches::dice_seeds` (the spawns and the perimeter cells two pitches inside the square, unlocked,
after NavKit's border nodes and order positions). Screens on sp_009, sp_004_b, ons_mp_004 (means; before = B):

| variant | nodes_near | nodes_paired | edges_paired | connectivity | over_connection | ai_pairwise | NavKit nodes, sp_004_b (DICE 8,971) |
|---|---:|---:|---:|---:|---:|---:|---:|
| before | 53.27 | 14.03 | 1.81 | 99.31 | 8.94 | 95.17 | 13,509 |
| spawns + perimeter (DICE's) | 52.81 | 14.33 | 1.75 | 99.16 | 9.33 | 96.10 | 14,093 |
| perimeter on terrain only | 52.78 | 14.46 | 1.77 | 99.14 | 9.33 | 95.17 | - |
| without the order positions | 52.53 | 13.87 | 1.79 | 99.18 | 9.45 | 89.90 | - |
| border nodes held back until the sector's seeds propagated | 53.28 | 13.90 | 1.72 | 99.18 | 9.20 | 96.10 | - |
| the seeds locked | 59.32 | 19.48 | 1.85 | 99.24 | 9.20 | 96.60 | 52,147 |

The locked variant's gains come with 4-7 times the nodes (rejected on that alone). DICE's list on the full set: the
6 levels built before sp_005's build passed 50 minutes (stopped): `nodes_near` 57.19 -> 58.39, `nodes_paired` 26.14
-> 27.70, `edges_paired` 5.78 -> 6.14, `nodes_same` 0.73 -> 0.86, but `connectivity` 98.74 -> 98.60 (5 of 6 levels
lower, sp_04 -0.27), `over_connection` 24.27 -> 24.60, `ai_pairwise` sp_04 97.3 -> 96.6 and sp_01 98.9 -> 98.6,
`costs` (old) 94.0 -> 80.5. Off (XNAV-BLD-3). NavKit's sectors have no margins and no linking stage, so its border
nodes stand in for what DICE's linking joins, and DICE's literal list starts a lattice that NavKit's border band then
crowds.

### 3. The midline cut (KYG-AREA-4, KYG-AREA-5)

Decoded: the size test serialises each area into Kynapse's in-place mesh without a buffer: 100 bytes + 28 per
portal segment + 52 per C entry + 20 per D / E chain + 12 per segment + 32 per F polygon + 12 per segment, after the
per-area chain simplification (round B's "before the outlines are simplified" corrected). Fitted on the 35 files
with the stored counts (`kynapse_cut`; C portals not counted): grouped by the union's box midline, 807 of 872 coarse
cut groups reach 8,192 bytes merged back (median 15.2 KB) against 1,211 of 115,713 uncut areas (p99 8.3 KB): a
partial separation, since the stored layout is not the in-place one; grouped by the tile midline (KYG-AREA-2's
groups) 311 of 1,064 stay under. The detailed and vehicle straight portals on a box midline are mostly not size
cuts (227 detailed groups, median 2.5 KB).

Built: `stage::stage_cut` (`Switches::midline_cut`): the stage built again while an area's record by the formula
reaches 8,192 bytes, that area dealt into four on its cells' box midlines (`Level::split_box`, each group's
components an area). NavKit's records are not DICE-sized (largest coarse / detailed on sp_01: 42 / 48 KB, DICE 13 /
15 KB; 166 / 358 areas over the limit against DICE's 84 / 11), so it fires on hundreds of areas a level. The 15
levels (before = B):

| measure | before | cut |
|---|---:|---:|
| areas (DICE 10,960) | 14,267 | 27,400 |
| detailed areas (DICE 3,099) | 8,605 | 18,105 |
| `areas_same`, `portals`, `outline` | 5.18, 6.48, 97.99 | unchanged |
| `nodes_near` / `nodes_paired` / `edges_paired` | 56.21 / 23.30 / 4.78 | 56.33 / 23.39 / 4.81 |
| `ai_coverage` | 99.16 | 97.27 |
| `ai_pairwise` | 95.57 | 92.99 |
| `df_d_recall` / `df_f_recall` | 23.89 / 33.46 | 23.18 / 32.17 |

Off: the cut needs NavKit's own records DICE-sized first (its outlines and per-cell portals). A level builds 1.5-2.5
times slower with it (each extra stage pass).

### 4. DICE's detailed sectors with the offline 0.2 m samples (KYG-DET-1)

`bfbc2 navkit sample <lv> --offline --dice` (0.8 + 0.2 m, 3-19 s a level) covers all 502 of DICE's detailed
sectors; the builds take the 0.2 m pair from it and the 0.8 m in-game samples as before. Control: NavKit's own
choice from the same offline 0.2 m run. Means over the 15 levels:

| measure | in-game 0.2 m, NavKit's choice (reference) | offline, NavKit's | offline, DICE's 502 | offline, NavKit's, placement off | offline, DICE's, placement off |
|---|---:|---:|---:|---:|---:|
| `ground` | 98.22 | 98.14 | 98.38 | 98.14 | 98.38 |
| `areas_same` | 5.18 | 5.16 | 5.40 | 5.16 | 5.40 |
| `portals` | 6.48 | 6.43 | 6.16 | 6.43 | 6.16 |
| `outline` | 97.99 | 97.96 | 97.57 | 97.96 | 97.57 |
| `nodes_near` | 56.21 | 56.90 | 57.05 | 47.77 | 46.38 |
| `nodes_paired` | 23.30 | 23.48 | 23.30 | 15.23 | 13.95 |
| `nodes_paired_candidate` | 18.82 | 18.04 | 18.99 | 10.34 | 10.44 |
| `edges_paired` | 4.78 | 4.80 | 4.80 | 1.61 | 1.40 |
| `connectivity` | 99.06 | 98.33 | 98.36 | 98.03 | 98.07 |
| `ai_coverage` | 99.16 | 98.10 | 98.30 | 98.10 | 98.30 |
| `ai_pairwise` | 95.57 | 93.84 | 94.38 | 90.62 | 91.19 |
| `df_d_recall` / `df_f_recall` | 23.89 / 33.46 | 23.89 / 33.46 | 28.40 / 39.96 | 23.89 / 33.46 | 28.40 / 39.96 |
| `over_connection` | 15.74 | 15.70 | 15.56 | 15.64 | 15.40 |
| detailed areas (DICE 3,099) | 8,605 | 9,913 | 8,261 | 9,913 | 8,261 |

Against its control, DICE's choice is better on 11 means and worse on `portals`, `outline`, `costs` and
`nodes_paired` (placement on); placement off it lowers the node measures. The offline 0.2 m samples are worse than
the in-game ones for the same choice (`connectivity` 99.06 -> 98.33, `ai_pairwise` 95.57 -> 93.84; sp_03 99.15 ->
95.96, ons_mp_002 99.85 -> 97.30). `DICE_DETAILED` stays off: a ruling for the user, and the offline / in-game
0.2 m gap is an item of its own (NAVS-SMP-4 measured surfaces, not builds).

### What changed, what did not

The build's output is unchanged (both switches off): the 15 levels rebuilt give every measure of B's reference; the
reference run was re-recorded with the new measures (the old one kept as `regression_before_2026-11-23_nksde.json`);
the golden builds are unchanged. Tests: `cargo test -p bc2-navbuild --lib` (new:
`split_box_deals_an_area_into_four_on_its_box_midlines`, `inplace_size_counts_the_in_place_record`,
`the_midline_cut_leaves_no_area_at_the_size_limit`); `test_navtools_native.py` quick (navdiff's Python against the
Rust on 40 random pairs, the new counts included).

### Left

- DICE's seeds need NavKit's sectors built with DICE's margins and a linking stage (KYG-SEC-2), and a faster
  propagation over thousands of seeds (sp_005 passed 50 minutes).
- The midline cut needs NavKit's area records DICE-sized (outline simplification and portal runs as DICE's), and
  `ai_coverage`'s loss under the cut traced.
- `DICE_DETAILED`: a ruling; the offline sampler's 0.2 m builds against the in-game ones.
- The old exact-match measures (`nodes_same`, `edges_same`, `costs`) out of navregress's failing set after this
  round.

## SPG2-rest4: reverse engineering for the specification (2026-11-23, branch `spg2r4`)

Static reading of `BFBC2Game.exe` (PC build 795745), read only. REA's Ghidra session could not start: its private
runtime folder was locked, and a reopen left it there. So the reading was done with capstone over a scratch image
of the executable, as in round SPG2-rest3's sweeps. DICE's data was read through the suite's readers. Nothing was
run in the game, and nothing was written to `game\`.

| Item | Found | Rules |
|---|---|---|
| undergrowth parser (VEG-UG-5) | found. The registration at 0x1375A20 only makes the type object. The `VisualTerrain` resource factory 0xA2F440 ("`.visual`", 0x2500 bytes, constructor 0xA27D80) calls the loader 0xA2BA20, which reads each entry into a 0x50-byte record: the 12 floats in file order, two bytes, and 1 / (MaxMask - MinMask) at +0x30. The cell generator 0xA0DDC0 names every float but `WindScale` by its use. The field layout went to the coordinator for TG2 as soon as it was known, with a correction (an exactly equal mask pair ramps over a width of 1; DICE's smallest spread is 0.0001). | VEG-UG-5 (inferred -> verified), VEG-UG-6 (corrected), VEG-UG-7 new |
| particle update (FX-PROC-4) | found the evaluation: the dispatcher 0xB268E0 (11 evaluator types through the table 0xB26E98). The polynomial 0xB24FD0 is clamp(c.x t^3 + c.y t^2 + c.z t + c.w) x ScaleValue x the selected component; t is the normalised age for `EfNormTime`. | FX-PROC-5 new |
| menus (MENU-LOAD-1) | the FSCommand callback 0xFA5380 and its ten commands, installed by 0xFA6BC0; the rest go to the screen's vtable +0x30. The event queue's drain 0x918E50: once a frame, delayed events deduplicated, values typed as strings, integers and floats. The loading roots: the main menu's root comes from a screen-class virtual (0x753660), and the loading screens start on `onStartLoading` / `onStartLoadingSP`; their own names were still not found. | MENU-RT-6, MENU-EVT-4 new; MENU-EVT-2, MENU-LOAD-1 updated |
| award listeners (PERS-AWD-4) | found: one 0xBD4-byte listener per player (0x8531B0, registered at the join), its handler 0x85F530, and the grant 0x852CB0. A finished award is the stats `<Code>_00` (count) and `<Code>_01` (days since 2008-01-01), plus the `AwardType` handling. | PERS-AWD-5 new |
| career save drops kills (PERS-SCORE-1) | found: kills, deaths, teamkills and the score buckets go through the scoring record's stats pointer (+0x24). The constructor sets it to 0, and only 0x866240 sets it, called only from the stats-arrived handler, which needs EA's answer. OPFOR's fill never attaches the stats, so nothing reaches the saved object. `score` is DICE's derived template, so OPFOR's log always prints 0. | PERS-SCORE-2 new |
| anim tree (ANIM-TREE-3) | found the outer layout from the loader 0xF47360's three relocation walkers. A: a node table and 26 node types with fixed pointer places. B: flat data behind one pointer; OPFOR's durations lie here. C: 0x60-byte records. | ATREE-BLK-1 to 3 new (verified), ANIM-TREE-3 updated |

Probes and checks (game tier; nothing of EA's is kept, only counts and types):

- `spec/probes/resources.py` (`anim_tree_blocks`): every one of DICE's 1,926 trees has its node table, every node
  and typed pointer in block A, block B's head in B, and block C's records' pointers in C (or at the block's end);
  59,725 nodes of types 1 - 26.
- `spec/probes/menus.py`: the callback's install bytes, its command table as the start-up's code gives it, and the
  drain's frame counter. Four of the six FSCommands missing from the name table are the callback's own.
- `core/tests/test_spec_probes.py::visual_terrains`: MaxMask over MinMask on all 3,069 entries, by 0.0001 at least.

Rules went from 1,468 to 1,477. By status: verified 869, implemented 139, observed 401, inferred 68.

### Playtests still pending (in-game tests are paused)

- `transforms/terrain_edit.md` section 5 (E5 / E7).
- ONS-CAP-3's patch.
- MAT-TEX-20's RenderDoc capture.
- BOT-GS-5.
- bot_ai.md's plan, including the kill probe.
- New this round: after OPFOR attaches the filled stats to the scoring record (PERS-SCORE-2), a round with kills and
  deaths whose save keeps them.
- The award stats `<Code>_00` / `_01` in a save (PERS-AWD-5).

### What is left (SPG2-rest5)

- The undergrowth's `WindScale` and second flag byte, and the render settings that scale density and view distance.
- The other particle evaluators and the processors' use of the value; the `EfVelocity` path.
- The loading screens' root names; who sets an event's delay; the screens' FSCommand handlers.
- The award criteria classes; whether DICE's `Final` templates hold the `<Code>_00` names.
- The 26 node types; blocks B and C beyond their pointers.
- OPFOR's fix for PERS-SCORE-2, as an OPFOR round.
## TG2: the generated terrain's ring and visual side (2026-11-23, branch `tg2`)

What TG1 left (the ring, the visual water, temperate's rough relief, the prototype's retirement) and the generated
terrain's visual side from one donor level, each part proved on DICE's data first. Spec: TGEN-GEN-6, TGEN-RING-1,
TGEN-VIS-3, TGEN-VIS-4, TGEN-WATER-1, TGEN-OCC-1, TERR-HF-12, TERR-VT-8, WATER-VIS-6 new; TGEN-REQ-4, TGEN-VIS-2
verified. Generator version 2.

**The writers on DICE's data** (`bc2_formats`, release, the game copy):

| Writer | DICE's data | Result |
|---|---|---|
| `terrain_codec::SubBlock` | 288 sub-terrain blocks | 288 of 288 decoded and encoded byte for byte (`terrain_corpus.rs`, 5.9 s with the main blocks) |
| `visual_terrain` | 30 visual terrains | 30 of 30 byte for byte |
| `water::VisualWaterFile` | 30 visual waters | 30 of 30 byte for byte (far plane 8192, 6000 on SP_004_B) |
| `water::open_water_vertex` | open-water vertices (normal up, tangent x) | 23,820 of 23,820 byte for byte: (x, y, z, 1), (0, 1, 0, 0), (1, 0, 0, 0), (x / 512, z / 512), zeros |
| `itexture` (DXT1 / L8, full mip chain) | 13 terrain colour / occlusion maps (SP_04's format 2 not written) | level sizes and headers DICE's on 13 of 13; level 0 encoded again within a mean of 0.00 (L8) - 0.90 (DXT1) levels, worst 25 |
| `shaderdb::terrain::clone_terrain` | MP_005's 98 patch, undergrowth, cache and water entries into its own Dx9 / 10 / 11 databases | 98 of 98 with the donor's program keys and shaders, the occlusion map renamed in 51, the donor's entries unchanged; 0.38 / 0.09 / 0.06 s |
| (DICE's precedent) | SP_008 -> SP_008_B patch entries | 16 of 16 shared combinations with SP_008's program keys under SP_008_B's name, 15 sampling SP_008_B's own occlusion map |

**The ring** (`bc2_terragen::ring`, TGEN-RING-1): DICE's 12 single-ring terrains all have 12 blocks of 64 samples in
one order and one stitching (TERR-HF-12: 512 of 512 seam samples equal the main block's knots, 768 of 768 ring-to-ring
samples their neighbour's, the main edges straight between knots within 0.5 units); the generated ring passes the same
checker (arid 1,024 and snow 2,048), its blocks decode and encode byte for byte; ring median 6 - 63 m from the border's
(DICE -77 .. +79), 10 - 90 % spread 40 - 372 m (DICE 25 - 432).

**Temperate's rough relief** (TGEN-GEN-6): DICE's rough terrains are rough in few places (1.5 - 2.3 % of 2 m samples
over 5 m off their 10 m mean, by up to 138 - 201 m on MP_004, SP_005, SP_005_B). Tried at seed 7, 1,024:

| Rough step | Slope p10 / p50 / p90 / p99 | Relief 10 m / 66 m | beta |
|---|---|---|---|
| TG1 (none) | 6.6 / 25.9 / 56.4 / 75.3 | 0.94 / 4.43 | 3.76 |
| terraces of 14 m over 55 % (before the relief scale) | 1.4 / 25.9 / 67.1 / 82.9 | 1.67 / 5.74 | 3.35 |
| blurred 2 - 8 m before the scale | 7.8 / 25.8 / 47 - 52 / 64 - 71 | 0.19 - 0.70 / 4.5 - 4.8 | 3.8 - 5.7 |
| a riser on every 60 - 120 m contour | 6.3 / 26.0 / 63 - 65 / 83 - 86 | 1.6 - 2.6 / 5.3 - 7.5 | 2.9 - 3.3 |
| mesas on a beta-3 mask (its face frays over a tenth of the ground) | 5.8 / 26.1 / 86.6 / 87.7 | 6.36 / 14.41 | 2.88 |
| **one escarpment of 120 m over 30 %, beta-6 mask, after the scale (kept)** | **6.5 / 26.1 / 57.1 / 79.0** | **4.24 / 13.70** | **3.13** |
| DICE's temperate median (SP_005) | 6.2 / 26.0 / 55.7 / 75.1 | 4.77 / 13.16 | 3.71 (3.16) |

The other four biomes measure as in TG1.

**The visual side** (`bc2_terragen::visual::build`, from MP_005 on a 1,024 arid terrain): 13 blocks, MP_005's 50
patch shaders renamed, 4,096 main cells drawn by `Dunes_Destruction` 1,735, `Dunes_Mountain_Destruction` 1,792,
`Dunes_Seafloor_Destruction` 569; the colour / occlusion map 2,048 x 2,048 DXT1 with MP_005's red p10 / p50 / p90
(153 / 173 / 206); 99 entries cloned into each database; built in 2.2 s. From MP_008 (snow 2,048): L8 grey map, 81
entries.

**The cell rule against DICE** (TGEN-VIS-3, DICE's heights and water): the sea floor where DICE put it on 38,713 of
47,177 rule-only cells (MP_005 0.80, MP_001 0.97, MP_009 0.89, MP_003 0.96, MP_004 0.67); DICE's slope layer is
painted (on MP_005 no slope statistic or threshold beats "never", 0.73). DICE's colour / occlusion maps are painted
too: sky visibility (2 - 256 m), slope and curvature explain r2 0.27 / 0.22 / 0.03 / 0.12 of MP_005 / MP_009 /
MP_001 / SP_01's red, with coefficients of other signs on each - so the generated map takes the donor's values.

**The game-accurate view** (`bc2-render`'s `generated_terrain` test: the generated files over OPFOR's prepared MP_005,
`World::load_generated`, MP_005's weather; one device a world - a device keeps the engine's height map by name):
every patch shader has a program; every sampled ray on the terrain drawn at its height (above 506 - 2,113, oblique
1,222 - 2,458 rays). The look per height band against MP_005's own (plain rule-only cells' dry ground per 20 m over
the water, sea-floor cells per 5 m under it; luminance ratio, hue within 0.035 everywhere):

| Variant | Dry ground, first 20 m | Dry ground, higher | Sea floor |
|---|---|---|---|
| identity (MP_005's heights through the generated visual side) | 1.05 | 0.53 - 0.74 (the computed map darker than DICE's painting) | 1.01 - 1.15 |
| generated, water at MP_005's 58 m | 0.89 | 0.85 - 1.00 | 1.74 (0 - 5 m: the flattened floor) |
| generated, heights from 0 (water 2.7 m) | 1.54 - 1.62 | 3.10 - 3.23 | 0.80 |

So the rule layers shade by the ground's height (TGEN-VIS-2): the generator takes the donor's water level
(`--donor` sets it). Loading a generated world over MP_005's files: 0.2 - 0.9 s.

| Run (release, this PC, other rounds building) | Time | Peak RAM |
|---|---|---|
| `bfbc2 terrain generate`, temperate 1,024 (ring, escarpment) | 2.2 s | 126 MB |
| `bfbc2 terrain generate --donor mp_005`, arid 2,048 (reads the game copy's MP_005 bundles) | 11.4 - 22.6 s | 566 MB |
| `bfbc2 terrain generate --donor mp_008`, snow 2,048 | 14.0 s | 570 MB |
| `cargo test --release -p bc2-formats --test terrain_visual_corpus -- --ignored` | 7.7 s | - |
| `cargo test --release -p bc2-terragen --test ring -- --include-ignored` | 15.3 s | - |
| `cargo test --release -p bc2-render --test render -- --ignored generated_terrain` | 21 s | - |

### What is left

- Shores: DICE's slot-0 wake strips along the coast; the generated shallow sea floor draws 1.74 times as bright as
  MP_005's (TGEN-VIS-2's open question).
- Tiled layers and their atlas tiles (TG4 / E5-rest); undergrowth is not drawn by the game-accurate view.
- Rings of more than one ring (25 - 49 blocks: MP_002, SP_04, SP_004_B, BC1_Harvest_Day) are not measured.
- Jungle's 10th slope percentile (3.9 against 0.6), the playable area kept flatter (TG5).
- Create Level's "Generate" and the export with the level's own terrain bundle (TG3).
- **Unproven until a playtest**: everything the engine does with a generated terrain - the ring and its seams, the
  visual terrain, the cloned entries, the visual water, the colour / occlusion map, the donor's rule layers in game.
  Nothing was installed or launched.

## OR3c: OPFOR in Rust, stage 3c - the shader translation driver (2026-11-23, branch `or3c`, with Project OPFOR's `or3c`)

What OR3b left ("OR3b", "What is left"): `wake_translate`'s translation driver, `ww2_char_shaders`' soldier glue and
`wake_install.index_templates`' scan. Version **0.25.0** (added: the driver; nothing of the suite removed; a round
merged first may take the number). Nothing was written to OPFOR's `game\`; the game never launched.

### What moved

- **`bc2_convert::shadertranslate`** (new), through `bc2_native.shadertranslate_*` and `shaderdb.py`'s thin wrappers:
  - `translate_plan` (`plan`): `translate_db`'s todo list (filters, excludes, `only`, the materials kept on their
    donor, the meshes drawn with DICE's shaders) and per entry BF1943's programs (main by point light, depth-only),
    the layouts it is drawn with (its mesh's parts, the strip layout, a terrain patch's two layouts, the parts the
    install will register - asked of the caller once), and per layout and permutation BF1943's program: by layout
    (`element_map`, each fetch in the layout's own Xenos format), by role (`by_role`) or by element order, scored by
    mesh kind, variant without the fade, no fade, rank and completeness; its vertex inputs, renderer fragment
    (`mesh_fragment` / the terrain's by name), key (`program_key`) and id. BF1943's vertex shaders are read with
    `bc2_shader::xenos::parse` (their usage indices) and their fetch records. Where the Python stopped an entry the
    plan puts the exception it raised (`TranslateError`, `XenosError`, `KeyError` of the GUID / format / offset,
    `ValueError`), after the programs before it; what the Python could not have run is an error of the call.
  - the soldiers (`soldier=True`, which replaces `ww2_char_shaders.soldier_wiring`'s patching of `element_map`):
    `soldier_layouts`, `soldier_element_map`; the glue `match_entries`, `perms`, `part_users`, `offer_kinds` (the
    copies returned, applied by the wrapper), `entry_blocks`, `entry_mesh`.
  - `missing_entries` and `foreign_entries`' choices (the clones stay the Rust merger's), `other_level_variant`, and
    `discards_code`: a compiled Dx10 / Dx11 shader's `discard_z` / `discard_nz` found in its SHDR / SHEX chunk - what
    `'discard' in pcbuild.disassemble(code)` answered, without `d3dcompiler`.
- **`level_scan`** gives each entry's layout record bodies (`LevelScan.layout_records`, the last record of a GUID).
- **Project OPFOR**: `wake_translate.translate_db` walks the plan (pass 1: the translation jobs; pass 2: compiles,
  blocks, records), keeping the translation (`xenos_hlsl`), the compiles and parameter blocks (`pcbuild`) and OPFOR's
  render-state rules (`wake_samplers`, `wake_material_states`, `wake_shore`); `add_missing_entries` /
  `foreign_entries` take their choices from the suite; `ww2_char_shaders`' glue and `wake_install.index_templates`
  are the suite's. `source_fragment` / `translated_program_key` retired (the plan makes the key).
  SUITE_REQUIRES 0.25.0 to below 0.26.0.

### Proof

- **Twins side by side** (the Python at OPFOR `02278c8` against the new, in one process, nothing written):
  Wake's translation as `main()` runs it - Dx10 and Dx11 (the objects, the foreign entries of the other BF1943
  levels, the 8 terrain strips and 34 patches), then Dx9's foreign entries with the Dx10 templates: the databases
  byte for byte, the entries kept, the wiring record, the templates chosen and the log identical. 44 damaged copies of
  the Dx11 input (seeded: layouts, fragments, BF1943's fragments, entry groups both sides, mesh parts, half-cut
  vertex shaders, keys' kind / variant / point light, part layouts, BF1943's layout records, fetch records; objects
  and terrain): every outcome identical (31 databases, 12 exceptions). `ww2_char_shaders.translate` on Wake's staged
  Dx10 / Dx11 databases (42 soldier entries) and the WW2 levels' builds: identical. `index_templates` on the game
  copy's 79 stock levels: the same 141 signatures and records. `discards` against the disassembler: **40,744 of
  40,744** distinct shaders (Wake's databases and every stock level's Dx10 / Dx11 database). Recorded as digests in
  OPFOR's `src\tools\tests\data\ww2_translate_golden.json` (`test_ww2_translate_parity`, SLOW) before the deletion.
- Suite: `test_shadertranslate.py` (quick, `shadertranslate`: a built BF1943 / PC pair with a seeded Xenos vertex
  shader - the plan, its wiring, fragment, key and id; its stops; the entries left out; missing / foreign entries;
  the soldier glue); `test_shaderdb_levels.py` checks the layout records against the old query on its 450 stand-in
  databases; 14 Rust unit tests.
- **WW2 rebuild** from OPFOR's main checkout detached at `or3c`, `OPFOR_TOOLKIT` a scratch folder of junctions (this
  worktree, the module of `58489ba4`), `OPFOR_ROOT` the main checkout (a scratch toolkit has no OPFOR beside it:
  `bf1943_fx`'s `find_opfor(None)` stops step 0 without it), `OPFOR_SUITE_STRICT=1`, `--jobs 2`: all 55 steps in 24.3 min; `compare_oracle`
  **7,945 of 7,945 identical**. QA: one regression outside this round (below).
- **Install oracle**: `wake_install.py --plan` 7.3 min, `compare_oracle --install` **21,149 of 21,149 identical**.
- `game\`: its 1,582 files (11.6 GB) hashed (path, size, time, SHA-256) before the rebuild, before the plan and after
  it: identical. OPFOR's main checkout back on `main` afterwards.

### Timings

| | Python | Rust plan |
|---|---:|---:|
| Wake's Dx10 translation (objects, foreign, strips, patches; compiles cached) | 18.1 s | 4.4 s |
| Wake's Dx11 translation | 10.2 s | 3.3 s |
| `index_templates`, 79 stock levels | 9.1 s | 1.9 s |

In the rebuild `wake_translate` took 39 s (18 s in OR3b's): the step also loads the stock index and converts the
named textures (`convert_named`), and this rebuild's `install_index.pkl` had been rebuilt by a parity run the same
day; not looked into further - the translation itself is the table's.

### Decided

- Nothing against DICE: the plan is the Python's rule for rule (a Python-only quirk kept as it is: the terrain
  fragment message's "stripes", from `f'{kind}es'`).

### What is left (stage 3)

- OR3a's self-clone accident (PY1).
- The HLSL compiles, the parameter blocks and binding tables stay `pcbuild` (Python over `d3dcompiler_47`), the
  render-state rules OPFOR's (`wake_samplers`, `wake_material_states`, `wake_shore`).
- `bfbc2-shader-tools`' Python: renderqa, xdiff, pcbuild, xenos_exec (and `shaderdb.py`'s / `xenos.py`'s /
  `xenos_hlsl.py`'s thin wrappers).
- The QA texture regression this rebuild reported (`install/level/textures/sky/sky_beach_generic_rgba16f.res`,
  psnr against BF1943's source 99.0 -> 21.22 dB): round WW2-ORC1 put DICE's PC copy of Sky_Beach_Generic in the
  levels instead of the relic BF1943 conversion; the QA baseline still holds the old texture. Not this round's.

### Tests

- cargo fmt; clippy `--all-targets -D warnings` on bc2-formats, bc2-convert, bc2-py and each of `bfbc2.exe`'s tool
  groups alone; `cargo test` of the new modules; the quick tier; ruff and mypy (results in the round's report).

## OR2: OPFOR in Rust, stage 2 - the level build (2026-11-23, branch `or2`, with Project OPFOR's `or2`)

What OR1 left ("Left for stage 2"): `wake_install.build` and its parts that are not the shader glue. Version
**0.26.0** (removals: `terrain._Walk` / `ROAD_VERTEX`, the `install_index.pkl` cache; RELEASING.md). The round was
paused once (the user, 2026-11-23) and resumed; the WIP commits stay in the history. Nothing was written to OPFOR's
`game\`; the game never launched.

### What moved

The new module `bc2_install::levelbuild` (feature `levelbuild`, which `bc2-py` enables; `bfbc2.exe`'s installer does
not build it) and its bindings `rust/bc2-py/src/ww2build.rs` (`bc2_native.ww2build_*`, `Ww2Index`, `Ww2PackJob`).
OPFOR's `wake_install.py` keeps its entry points as thin callers that pass its decisions as data (the level's
bindings, its roots and skips, REMAP, WEAPON_PERSISTENCE, TEAMS, KIT_VOICE, ROOT_FILES, UG_STANDIN, the loaded and
masked bundles): about 1,500 lines of its Python retired.

| Project OPFOR (`wake_install.py`, Python until OR2) | `bc2_install::levelbuild` |
|---|---|
| `stock_bundles`, `ondemand_bundles`, `stock_index` (the pickle `install_index.pkl`), `masked_names`, `live_entries`, `our_files` (`file_sha`, `same_file_content`), `primary_type` | `index` (the stock index read on all cores and kept for the process while every bundle keeps its size and date) |
| `closure` | `closure` |
| `write_stage` (`_STAGED`), `fetch_stock`, `pack_level` / `finish_packs` (the packing thread), `patch_bundle`, `build_ww2_common` / `common_signature` | `stage` |
| `model_group_check` (`havok_parts`, `group_ids`, `group_members`, `group_expects`, `member_mismatch`, `GroupError`), `loaded_names`, `level_names`, `icon_gate` (on `bc2_menu::graft`) | `gates` |
| `description`, `air_superiority_mode`, `level_list_items` / `level_list_xml` / `level_list_query` (DbxEdit in process) | `describe` |
| `edit_level` (`team_refs`, `vo_from`, the test aids `test_drop` / `test_spawn`), `remap_targets`, `patch_refs`, `persistence_ids`, `weapon_persistence`, `patch_ours`, `kit_voices`, `sky_patch` (its own branch; the sky's Option B stays `wake_sky`'s), `root_files`, `mesh_flags`, `ondemand_waves`, `emitter_atlas`, `undergrowth_files` | `levelfiles` |
| `terrain_files`, `undergrowth_scales`, `undergrowth_blocks`, `_undergrowth_uses` (and the suite's `terrain._Walk` / `ROAD_VERTEX`, retired) | `terrainfiles` |
| `port_missing` (`port_mesh`, `_PORTED`) | `ports` on `meshport::Porter::port_with` (new: the shader plan - each part's layout choice, the materials -, and a bare mesh skinned to no soldier) and `granny::to_pc`; PORT_PARTS' template ports stay `dbxport.port`'s (the suite's Rust core already) |

Still Python in `wake_install.py`: the shader glue (stage 3: `shader_dbs`, `autoclone`, `fix_layouts`,
`register_parts`, `zonly_programs`, `index_templates` - OR3c's), `res_type`, `stage_textures`, `ww2_common_files`,
`build_shared`'s gathering, `terrain_names`, `drop_instances` (wake_vo uses it) and `build()`'s orchestration.

### Decided (Python-only accidents against DICE's data)

- The closure's Python set orders (the partition names in our mesh sets, the reached partitions' order in the take,
  `level_names`' partitions) are first-seen and sorted orders: every staged file and report line came out the
  same (below), so no output depended on them.
- A model group's `Enums` holding something that is not a number is a group problem (reported, the gate fails); the
  Python stopped the build with a `ValueError`.
- The staging record knows a bytes source by the object's identity kept alive with the record, as `rec[0] is src`
  did.

### Proof

- **Wake, Rust against Python** (`scratchpad\or2\parity_build.py`: `wake_install.build` and the shared patches into a
  scratch folder, OPFOR main's Python with main's suite against this branch; the game copy and `work\ww2` read only):
  **6,260 of 6,260 staged files identical** (the level, terrain, ingame, loader and vo bundles and folders, the root
  files, the description, the level list, ww2_common, the startup / overlay / ondemand_awards patches, the patched
  copies, report.json); report.txt identical but for the scratch folder's own name in the DbxEdit check line. Python
  290 s, Rust 239 s (the shader pass, still Python, is most of both).
- **WW2 rebuild** (OPFOR main detached at `or2`, `OPFOR_TOOLKIT` a scratch folder of junctions to this worktree,
  `OPFOR_SUITE_STRICT=1`, `--jobs 2`): all steps in 24.5 min; `compare_oracle` **7,945 of 7,945 identical**. `rebuild_all` exits 1 on one QA texture check only (`sky_beach_generic_rgba16f.res` psnr 21.22 dB against the BF1943 source: DICE's PC sky since WW2-ORC1, a stale QA baseline, not this round's).
- **Install oracle** (`wake_install.py --plan` on the four levels): 4.7 min; `compare_oracle --install` **21,149 of 21,149 identical** (the install-time ports in Rust: 11 for Wake, the plans.json among them).
- `game\`: its 1,582 files (11.6 GB) hashed (path, size, time, SHA-256) before the rebuild, before the plan and after it: identical; OPFOR main back on main (02278c8).
- Tests: `bc2-install/tests/levelbuild.rs` (7 on a fake game copy: the stock bundles' order and the index, our files,
  the closure with every case, the staging and patches, ww2_common, the model groups, the level edits and remaps,
  the description and level list through DbxEdit, mesh flags and on-demand waves) and the module's unit tests;
  OPFOR's test_model_groups, test_ww2_packing, test_ww2_install, test_ww2_terrain_strips, test_ww2_loading on the
  Rust core.

### Left

- Stage 3c (OR3c, in parallel) and the shader glue named above; stages 4 and 5.
- `meshport`'s renames, donors and poses (OPFOR's soldier and kit builds: stage 4).

## OR5: OPFOR in Rust, stage 5 - Onslaught, menus, testrun (2026-11-27, branch `or5`, with Project OPFOR's `or5`)

Version 0.27.0 (OPFOR's `SUITE_REQUIRES` 0.27.0 to below 0.28.0).

### Inventory

| OPFOR / suite Python | Lines | What it did | Before OR5 | OR5 |
|---|---|---|---|---|
| `src\tools\apt\AptTool.cs` (C#, built by build_menus.bat) | 412 | the menu patch files applied; a disassembler | `menucommon.parse_patch` / `apply_patch` had Rust twins (aptcheck lint) | `bfbc2 aptpatch` (`bc2_menu::aptpatch`); retired (the dump is `bfbc2 apt dump`) |
| `src\tools\menus\onslaught_art.py` | 507 | the ONSLAUGHT mode picture (numpy, Pillow) | stfs / APT readers in Rust (F0, E3n) | `bfbc2 menuart onslaught` (`bc2_menu::modeart`, `bc2_menu::pil`); retired; `menuart.py` (48 lines) keeps the Pacific War step's atlas pieces on `bc2_native.menuart_*` |
| `src\tools\onslaught\console_onslaught.py` | 532 | the console Onslaught import (stage 1 of `console_port.py`) | stfs in Rust; DbxEdit / FbrbTool in process | build and install preparation in `bc2_install::onslaught`; 106 lines left (the command line, the ledger commit) |
| `src\tools\testrun\testrun.py` | 493 | starts a level, watches the log, restores opfor.ini | - | opfor.ini's edits and the log's notes in `bc2_harness::testrun`; the process handling stays Python (below) |
| `src\tools\testrun\playtest.py` | 783 | a scenario driver (game input, screenshots, log checks) | - | stays Python (below) |
| `console_onslaught`'s other stages (`console_*.py`, `apply_*.py`) | ~5,000 | `console_port.py`'s stages 2-14 and the Napisal-level fixes | they run DbxEdit / FbrbTool / culltree / aptgraft in Rust already | left (below) |
| `bfbc2-console-readers` (`apt.py`, `stfs.py`, `xisolib.py`) | ~840 | the Xbox 360 readers | Rust twins (F0) | OPFOR no longer imports them; aptgraft / aptcheck keep `apt.py`, the tests `stfs.py` / `xisolib.py` |
| `bfbc2-menu-tools` (aptgraft, bc2loc, aptcheck, menucommon) | ~4,400 | the menu tools | command lines Rust (O2 / O3), models Rust (E4c) | unchanged: their Python API is what OPFOR's WW2 menu tools (OR4's area) edit movies and texts with |

About 1,400 lines of OPFOR's code retired (AptTool.cs, onslaught_art.py, most of console_onslaught.py (532 -> 106 lines), testrun's ini
and log code).

**Why the test run's process handling stays Python.** `testrun.py` starts `OPFOR BFBC2.exe`, polls `tasklist`, closes
the game with `taskkill` and runs the crash reader; `playtest.py` sends the game keyboard and pad input and matches the
user's scenario regexes (Python's `re`, which the scenario files are written in). None of it can be proven without
launching the game, which the rounds may not do, and moving the regexes to Rust would change what a scenario means.
The logic that can be proven - opfor.ini's edits and the log's notes - is Rust.

### Decided against the Python (spec MENU-ED-7, MENU-ED-8, XCONS-ONS-1)

- `aptpatch` refuses what AptTool let through: a malformed line, a label defined twice, a member named twice in the
  archive, and an archive cut short (the readers' slice would shorten the member silently; AptTool failed on it too).
- The mode picture's cache is compared by its pixels: Pillow's and the png crate's PNG bytes differ.
- The import copies the shared files in name order and searches the campaign bundles in CAMPAIGN's order (the Python
  followed a set's and a dict's order; no output depended on them).
- Kept for parity, named in the spec: the cut-out's float32 luminance is numpy's (OpenBLAS sums green first, then red
  and blue each fused); a stripped bundle's printed marker count is the Python's (it subtracts the renamed copies).

### Proof

- **Menus** (`scratchpad\or5\menus_parity.py`: DICE's pristine main menu bundle unpacked into scratch; AptTool.exe
  compiled from OPFOR 169944d's AptTool.cs and `onslaught_art.py` from 169944d against this branch's `bfbc2.exe`):
  **12 of 12 patched movies identical**; the mode picture from the cache, from the user's Xbox 360 Update #3 package and
  from the fallback composition: **every file identical**, the cache's pixels identical; the whole bundle (cache
  source, then console_hud's loading screen step, packed) **byte for byte the same** (sha256 d544869c...). The Pacific
  War menu step, unchanged code, was left out (it reads OPFOR main's work folder).
- **Damaged patches and movies** (`aptpatch_damaged.py`, 400 cases: changed expected bytes and offsets, broken arrows,
  repeated, dropped and swapped lines, movies cut short): 145 written identically, 255 refused by both, 0 different.
- **Picture operations** (`rust\parity_menuart.py`, 200 random cases each): resize, cut-out, fit, paste and composite,
  flip, crop, DXT5 decode, DXT5 encode, Xbox 360 untiling: **1,800 of 1,800 identical** to Pillow 12.3 / numpy 2.5 and
  the retired script; `bc2_menu::pil`'s unit tests hold recorded digests of the same operations.
- **The Onslaught import** (`scratchpad\or5\ons_parity.py`: the retired `console_onslaught.py` with the ledger commit
  captured, against `bc2_native.onslaught_build` / `onslaught_install_plan`; the user's Update #3 package, OPFOR
  main's `game\` read only, scratch work folders): **ONS_MP_008 / MP_008: 94,645 of 94,645 work files identical**, the
  added files, the 13 writes (paths, order, bytes) and the printed lines identical (`level.cmd` differs only in the
  scratch work folder's own name in its graft lines); **ONS_MP_002 / MP_002: 95,949 of 95,949** the same way. Python
  84-85 s, Rust 83 s a level (the unpacks and DbxEdit's XML, Rust in both, are most of it). A package cut short stops
  both at the same unpack with the same message. `game\`'s registration files unchanged.
- **testrun** (`testrun_parity.py`: random, damaged and built opfor.ini texts - CRLF / LF / CR, case, Unicode
  whitespace and separators, empty and repeated sections; random log lines): ini_set, ini_get, ini_unset, ini_restore
  20,000 of 20,000 each, the log notes 5,000 of 5,000.
- No WW2 rebuild: nothing the rebuild runs changed (`bc2-menu`'s existing code, which `levelbuild` uses, is untouched:
  OR5 only adds modules; `menu_install.py`, whose import line changed, is not run by `rebuild_all`).

### What is left

- `console_port.py`'s stages 2-14 (`console_linux_base`, `console_mode`, `console_loadset`, `console_vehicles`,
  `console_soldiers`, `console_shaders`, `console_markv`, `console_input`, `console_deps`, `console_cull`,
  `console_loading`, `console_hud`, `console_icons`) and the Napisal-level fixes (`apply_*.py`): about 5,000 lines,
  their tools already Rust; next: a round per two or three stages, each against the Python's work files.
- `playtest.py` and testrun's process handling (above).
- `bfbc2-menu-tools`' and `bfbc2-console-readers`' Python: retired when OPFOR's WW2 menu tools move off aptgraft's
  and bc2loc's Python API (PR1c), and `stfs.py` / `xisolib.py` with their tests on recorded answers.
## OR4: OPFOR in Rust, stage 4 - the material grid and the pictures (2026-11-27, branch `or4`, with Project OPFOR's `or4`)

What OR2 and OR3c left for stage 4 ("OR2", "Left"; ROADMAP "OPFOR in Rust"): the material grid, the pictures and the
texture glue (`res_type`, `stage_textures`). Version **0.27.0** (removed: `meshview.page()`, `PAGE`, `_png()`; a round
merged first may take the number). Nothing was written to OPFOR's `game\`; the game never launched.

### Inventory (OPFOR's `src\tools\ww2` and what it calls in the suite)

| Python until OR4 | now | |
|---|---|---|
| `material_grid.py`: `Grid`, `key`, `clone`, `redirect`, `build_roots`, `wake_names`, `main`'s merge and check | `bc2_install::levelbuild::materialgrid` | ported |
| `physmat.py`: `havok_lists`, `list_hashes`, `set_parts`, `group_faces` | `bc2_convert::physmat` | ported (thin callers kept: four modules use them) |
| `wake_install.py`: `res_type`, `stage_textures` | `bc2_install::levelbuild::textures` | ported |
| `pictures.py`: `render`, `framed`, `bleed`, `down` | `bc2_convert::pictures::weapon` | ported |
| `ww2_loading.py`: `picture`, `level_picture`, `check`, `level_check` | `bc2_convert::pictures::loading` | ported |
| `weapon_icons.py`: `dxt5_alpha`, `picture`, `silhouette`, `fit` | `bc2_convert::pictures::icons` (`texwrite::alpha5`) | ported |
| `minimap_icons.py`: `box_colours`, `paint` | `bc2_convert::pictures::icons` | ported |
| suite `meshview.page()` (OPFOR's `attachments.py` / `scopes.py` preview pages) | `bc2_convert::meshview::page` (`attachments.preview_page`) | ported, Python retired |
| the DXT encoders (`texwrite.dxt1` / `dxt5`: `numpy.linalg.eigh`'s axes) | stay the suite's `texwrite` | not this stage |
| `effectport.port_many` (the grid's effects, cached 35 min), `wake_level.port_sounds`, `dbxport.port` | stay (the suite's importer; `dbxport` is Rust already) | effectport: PY1 / the importer |
| `type95_icon.silhouette` (Pillow's `ImageDraw.polygon`; OR5's `console_icons` uses it too) | stays | left: Pillow's polygon fill |
| `weapon_icons`' movie edits (`aptgraft`), `menu_*` art, `ww2_crosshairs` | stay | stage 5 (menus) |
| the preview sheets (Pillow's text), `wake_preview.py` / `weapon_preview.py` (they launch `shaderview.exe`; one opens a window) | stay | process launching and GUI: not Rust's |
| `bf1943_hud_icons.py`, `grime_sheet.py`, `emitter_atlas.py` (already `bc2_convert::emitteratlas`) | stay | |

### Proof

- **Twins side by side** (the Python at OPFOR main `169944d` loaded by path against the new, one process, nothing
  written to `work\ww2`):
  - loadout pictures: the 34 pictures `pictures.py` draws (from the WW2 builds' parts), size1 / size2 / size3:
    **102 of 102 identical**; 30 seeded part sets (float32 and float64 positions, normals or none, uvs and images of
    3 / 4 channels or none, bad triangles): 60 of 60. The NumPy took 0.9 - 3.9 s a picture, the Rust 0.2 - 0.5 s.
  - Pillow's resize: 400 seeded pictures (L, RGB, RGBA with alphas 0 / 128 / 255; Lanczos and box): identical; the four
    levels' loading and map pictures, their checks (wrapped bytes, PSNR to the tenth), 6 damaged cases (headers,
    format, a far picture, a load image of another format): identical; every weapon icon at 4 heights and in 4 boxes,
    100 random fits, 3 cut pictures, the minimap's 4 boxes and 60 random boxes: **169 of 169**.
  - physmat: 500 seeded stock physics assets and the WW2 builds' (lists, hashes, 5 part edits each - an existing
    material, every part, a new one, a missing part, a material without a hash -, face remaps, a wrong member count,
    an unknown face) and 120 damaged copies: **5,075 of 5,075 outcomes**.
  - `res_type` on 9,635 names (ours, stock, BF1943's, made-up), 3,000 without types, `stage_textures` 6 x 930
    textures: identical; the preview page under 2 titles (13 textures): the text identical, every PNG's pixels identical.
  - the material grid: `material_grid.main` old and new into scratch folders (the effects caches copied): Wake **333 of
    333** files, Guadalcanal 342 of 342, Iwo Jima 342 of 342, Coral Sea 305 of 305; 13 damaged grids (Isla Inocentes' grid with instances dropped, names changed, relations nulled; BF1943's cut): every outcome the
    Python's (the same exception and message).
  - Recorded as digests in OPFOR's `src\tools\tests\data\ww2_pictures_golden.json` (`record_ww2_pictures_golden.py`,
    `test_ww2_pictures_parity`, SLOW) before the deletion: 5 of 5 pass on the Rust.
- Suite: `pictures-native` (quick), `physmat-native` (quick: 600 seeded resources), `bc2-install/tests/materialgrid.rs`,
  the modules' unit tests.
- **WW2 rebuild** from OPFOR's main checkout detached at `or4` (`OPFOR_TOOLKIT` a scratch folder of junctions to this
  worktree, `OPFOR_ROOT` the main checkout, `OPFOR_SUITE_STRICT=1`, `--jobs 2`): all 55 steps in 14.2 min;
  `compare_oracle` **7,945 of 7,945 identical**; `qa_checks` exit 0, all 8 checks ok. Peaks: `material_grid.py` 3.7 GB,
  each level chain 4.1 - 4.4 GB.
- **Install oracle**: `wake_install.py --plan` exit 0; `compare_oracle --install` **21,149 of 21,149 identical**.
- `game\`: its 1,582 files (11.6 GB) hashed (path, size, time, SHA-256) before and after: identical; OPFOR's main
  checkout back on `main` (169944d).

### Decided

- Nothing against DICE: every output is the Python's byte for byte. The preview pages' PNG files are the `png` crate's
  (XWW2-PIC-4: a preview, no game data, no oracle file). A material-grid check failure now raises before the grid is
  written (the Python wrote it, then raised): no output of a run that fails is kept either way.

### What is left (stage 4)

- `effectport` (the grid's ported effects; the suite's importer, its 35-minute template scan) and the sound porter.
- `type95_icon`'s silhouette: Pillow's polygon rasteriser (shared with OR5's Onslaught icons).
- The DXT encoders' principal axes (`numpy.linalg.eigh`) in `texwrite`.
- The preview tools (`wake_preview`, `weapon_preview`: process launching and a window) and the preview sheets.

## E5-rest: the terrain's other layers (2026-11-27, branch `e5r`)

What round E5 left ("E5", "Left"): the visual paint layers, the colour / occlusion paint and its re-shade, widening the
height range, a material the map lacks and the visual / material pairing, the material map in the views, strokes on
the sub-terrain ring, the game-accurate view during a drag, and a sculpted export's path data end to end. Built on
TG2's pieces (`itexture`, `visual_terrain`, `shaderdb::terrain::clone_entries`, `sky_visibility`). Spec TEDIT-VT-2,
TEDIT-MM-2, TEDIT-RING-1, XVAL-TERR-3 new; TEDIT-SH-1, TEDIT-OCC-1 implemented; TEDIT-NAV-1 verified; TEDIT-HF-2,
TEDIT-VT-1, TEDIT-ATL-1 / -2, TEDIT-MM-1, TEDIT-BR-1, XVAL-EDIT-5, XVAL-TERR-2 extended. Paused 2026-11-23, resumed
2026-11-27.

**What was written** (one API for the Terrain workspace and a second tool, round RD1's roads):

| Layer | Writer | Log / editor |
|---|---|---|
| visual paint | `terrainpaint::paint`: a terrain's layers from its patch shader names (`LayerModel`), each reached 16 m cell's new shader (its own combination's; else a wider one with empty extra masks; else a clone of a same-kinds shader with one tiled layer's own textures retargeted), one tile per aligned block and layer in free atlas slots | `Target::Paint`; Paint layer brush |
| pairing | `terrainpaint::pairing`: the material under each tiled layer's strong mask | "Paired material": the material painted in the same stroke (`also`) |
| colour / occlusion | `terrainpaint::write_colour` (through `itexture::splice`), `reshade` (ratio of `sky_visibility_over` on the new and DICE's ground) | `Target::Colour`; Shade brush, "Re-shade sculpted area" |
| height range | `terrainedit::write_heights_in`, `rescale` | `terrain-range`; "Widen range" |
| materials | `terrainedit::write_materials_named`, `grid_terrain_materials` | `material-add`; "Add material" |
| ring | `terrainedit::ring_raster`, `split_ring`, `write_ring` (DICE's stitching) | `Target::Ring`; sculpt dabs past the main block's edge |

Project format 12. The export makes every terrain resource from the log (`terrain_writes`), clones new patch shaders
into each staged database, stages the colour map in the level's own bundle; `mapexport check` reads them back
(XVAL-TERR-2 with the ring, XVAL-TERR-3). The editor draws the material map around the brush and gives the
game-accurate view a stroke's heights every 150 ms while it is made.

**On DICE's data** (`rust/bc2-levelwrite/tests/terrain_layers.rs`, game tier, `Dist\win32`'s levels):

| What | Result |
|---|---|
| terrains whose layers have one kind each (paintable) | 7 of 23 (MP_005, MP_006, MP_007, SP_005, SP_005_B, SP_006, SP_04); 6 names do not split, 10 use a layer as tiled and as a rule layer |
| tile slot order = tiled layers in the name's order | 1,131 of 1,131 aligned blocks and layers (reversed: 1,063 of 1,463) |
| atlas border texels = nearest inside texel | 94.9 % (270,434 of 284,928) |
| material map names in their level's grid (Terrain / Default / Construction) | 155 of 155 |
| MP_005 pairing | Asphalt -> Tarmac 85 %, Dirtfloor -> Dirt 63 %, DryGrass -> Sand 98 %, Rubble -> Sand 98 %, Trash -> Gravel 99 % |
| MP_005, a tiled layer on each shader's cells | 67 own combination, 19 wider shader, 7 cloned, 55 refused (DryGrass, Rubble: no texture of their own) |
| MP_005, three paints | 0.08 s; 47 cells, 8 tiles, 2 clones; only the written tiles differ; `check_visual` clean |
| colour / occlusion maps (DXT1 / L8) | 18: nothing painted = DICE's bytes; a 64 x 64 shade rewrites 611 - 4,096 level-0 bytes; SP_04's and SP_006's are another format |
| single rings | 7: nothing edited = DICE's 13 blocks; a 30 m raise on the west edge: 2 of 12 blocks rewritten, seams 512 / 512 and 768 / 768, edges 0.50 units off straight |
| range x 1.5 | every sample within 0.0038 - 0.0076 m (under a stored unit of the new range) |

**The exports** (`terrain_edits.rs`, game tier):

| Test | Result |
|---|---|
| `painted_export_stages_and_checks` (MP_005GR as Conquest: a sculpt, range 500 -> 650 m, `Metal` added from the grid, DryGrass with it under it in one stroke, a 64 x 64 shade) | built in 30 - 45 s; XVAL-TERR-1 / -2 / -3 pass; the sculpted ground within a stored unit in the wider range; 28 cells rewritten, 2 tiles |
| `sculpted_path_data_end_to_end` (MP_005 as Conquest, a hill and a pit, `path_data: true`) | the stage sampled offline and its path data built in process: 34 - 59 s, 25 sectors, 8,384 route nodes; every check but XVAL-PLACE-5, which the same layer fails unsculpted too (10 points against 11) |
| `sculpted_export_stages_and_checks` (round E5's) | still passes (39 s) |

The editor's `terrain_tools::terrain_brushes_through_the_interface` (both themes, synthetic visual terrain, atlas,
colour map and ring) runs the Paint layer brush with and without its pair, Shade, a raise reaching the ring (one
operation, two rasters), the re-shade, the range widened and undone.

### Left

- The 16 terrains whose layers are not one kind each: a per-shader tiled-slot model (the shader's tiled layers from
  its cells' flags) before they can be painted; layers with no texture of their own cannot be cloned (MP_005's 55).
- Growing the mask atlas (a paint that needs more tiles is refused); clones have no undergrowth.
- SP_04's and SP_006's colour maps (another format).
- The stage sampler's main route network on MP_005 (7,341 nodes against the base samples' 15,090): a NavKit question,
  not measured unsculpted on the stage sampler.
- XVAL-PLACE-5 on MP_005's Conquest layer (10 - 11 points off the main network), the layer's, not the sculpt's.
- **Unproven until a playtest** (`spec/transforms/terrain_edit.md` section 5): everything in game - painted cells and
  their tiles, cloned patch shaders' look, the colour map, the ring's seams, a widened range, the paired materials.
  Nothing was installed or launched.

### Learned

- Patch shader layer names hold underscores (`Cliff_m`, `SP_03_Cliff`): split by the stored layer count.
- DICE's material maps name construction materials (`Stone`, `Concrete`) beside the terrain ones.
- `build_native.py --help` starts a real build.


## NKS-deep D: walkability and extent - the rectangle's spawns, the autoseeds, four switches for a ruling (2026-11-27, branch `nksdd`)

Part D of NKS-deep (ROADMAP "NKS-deep"): the measures no other part covers - `extra_ground`, `over_connection`,
`ground`, `ai_pairwise`. The executable was read with headless Ghidra on a scratch copy of Project OPFOR's analysed
project (deleted); no game run, nothing written to OPFOR's game folder. Paused 2026-11-23 to 2026-11-27 (the user's
ruling), resumed on main 0ed792f7. Every build was a scratch build of the 15 regression levels (two at a time,
`navregress.build_level` into a scratch folder, `navparity.run_level`, compared with `navregress.compare`), against
the reference `regression`; this branch's baseline gives every measure of it (scratch run `nksdd_base`).

### 1. Findings (spec)

| Rule | Finding | Status |
|---|---|---|
| KYG-RECT-1 (corrected) | DICE's rectangle is the soldier and vehicle spawns' box **without the player's alternate spawns** (`AlternateSpawnEntityData`): all 52 campaign sides within -0.39 .. +0.41 m of the box + 24.5 m (45 of 52 with the alternates, 37 without team 1's spawns). The "7 sides where the ground ends on its own" (NKG2) were alternates ("New Player Position", "preston", "Haggard", ...); sp_006's strip beyond is open terrain like the inside | verified (`kynapse_rect.rs` asserts every side) |
| KYG-SEED-3 (new) | the autoseed step: `AutoSeedGeneration` "always" (BC2 sets none), perimeter candidates, BC2's three filters - terrain body, slope within GroundSlopeMax (ray +-2 m), nothing above within 100 m; FilterFloorMode / strongly forbidden propagation off. On the autoseed lines 1.1 % of DICE's detailed on-grid nodes lie under a surface and 4.3 % on objects, against 6.2 / 9.4 % elsewhere | observed |
| KYG-PAR-4 (new) | the map builder's derived values: Pitch from EntityRadius, StepLength from Pitch, **HoleMax := -StepMax** (DICE's HoleMax 0.28 has no effect), cos of both slope limits, StepMax / StepLength and HoleMax / StepLength stored (KYG-PAR-3's 0.778 is computed; where the exploration reads it is not traced) | observed |
| KYG-NODE-9 (new) | the probes walk the mesh: sp_007's 142-node island is DICE's deck area with no portal to the ground beside it; NavKit's placement crossed by diagonal link-rule steps | inferred |
| XNAV-BLD-5 (new) | the terrain under a sampling hole: under a 34 m forbidden slab on sp_003_b the casts stop on its top (in game and offline), a strip with no terrain cuts DICE's one area in two | implemented (off) |

### 2. Causes traced (scratch tools `extra`, `fringe`, `slope`, `over`, `bridge`, `spot`, `pairs`, `holes`)

- `extra_ground` (8.31 %): 66 % of NavKit's extra cells lie within one cell of DICE's ground and 74-78 % (coarse)
  are NavKit's own boundary cells; DICE's missing cells are 77-80 % its boundary. Both files speckle at the slope
  limit: by the sampled normal DICE's kept share falls at 40 degrees as NavKit's, but DICE keeps 10-20 % less at
  37-40 degrees and more above 41 (sp_005, sp_03 frontier terrain). No single rule separates them; the exploration
  that reads KYG-PAR-4's ratios is the next thing to trace.
- `over_connection` (15.74 %): mostly joins inside one sector. sp_01's 53.5 % is one gap in DICE's graph at (-309,
  100, -415) in detailed sector 0x602 (two DICE areas 0.4 m apart around a 50-degree object ridge NavKit keeps as a
  lip); sp_03's joins lie where DICE's graph has no node though its areas are there; sp_007's 142-node island:
  KYG-NODE-9.
- `ai_pairwise` (95.57 %): sp_003_b (74.4 %) breaks at the slab's strip (XNAV-BLD-5) and at a sector-line / tile-line
  corner (-100, 139, 659); many other failures are AI positions 4-5 m over the ground whose nearest node differs (the
  measure's sensitivity).

### 3. The four switches (means over the 15 levels; reference `regression`)

| measure | reference | `RECT_ON_DICE_LEVELS` | `terrain_fill` | `mesh_corners` | `lip_object_flat_all` |
|---|---:|---:|---:|---:|---:|
| `ground` | 98.217 | 98.219 | 98.219 | 98.217 | 98.213 |
| `extra_ground` (lower) | 8.308 | 8.277 | 8.395 | 8.308 | 8.289 |
| `areas_same` / `portals` / `outline` | 5.18 / 6.48 / 97.99 | 5.23 / 6.79 / 98.01 | unchanged | unchanged | 5.18 / 6.48 / 97.99 |
| `nodes_near` | 56.21 | 56.20 | 56.20 | 55.93 | 56.34 |
| `nodes_paired` / `edges_paired` | 23.30 / 4.78 | 23.29 / 4.78 | 23.30 / 4.78 | 22.90 / 4.66 | 23.40 / 4.80 |
| `connectivity` | 99.06 | 99.07 | 99.24 | 99.08 | 99.07 |
| `ai_pairwise` | 95.57 | 95.67 | **96.77** | 95.69 | 95.89 |
| `over_connection` (lower) | 15.74 | 15.80 | 15.70 | **15.57** | 15.69 |
| levels with a measure worse (> 0.05) | - | 7 | 9 | 14 | 5 |

- `RECT_ON_DICE_LEVELS` (the rectangle in place of DICE's extent): worse sp_008 `borders` 91.43 -> 90.32, `areas_same`
  1.20 -> 1.06; sp_004_b `over_connection` 17.62 -> 18.17, `portals` 1.23 -> 1.15; sp_003_b `portals` 6.32 -> 5.97,
  `outline` 96.84 -> 96.52; sp_02 `nodes_near` -0.08; sp_03 `borders` -0.11; sp_007 / sp_008_b `over_connection`
  +0.14 / +0.07. Better: sp_008_b `portals` 6.49 -> 11.49, `extra_ground` 23.28 -> 22.78, sp_004_b `ai_pairwise`
  88.3 -> 89.7. The differences are within half a pitch of DICE's sides.
- `terrain_fill`: sp_003_b `ai_pairwise` 74.4 -> 93.4, `connectivity` 95.28 -> 98.44, `over_connection` 37.02 ->
  36.38. Worse: sp_03 `ai_pairwise` 98.1 -> 97.4, `connectivity` -0.07; sp_005 `connectivity` -0.13; ons_mp_004
  -0.16; ons_mp_002 -0.07; sp_005_b `ai_pairwise` -0.3; `extra_ground` sp_008_b +0.64, sp_008 +0.40 (terrain filled
  under objects DICE also leaves out); small node-measure dips on sp_02 / sp_004_b / sp_008.
- `mesh_corners`: sp_007 `over_connection` 17.47 -> 15.99, sp_009 `ai_pairwise` 97.2 -> 100, sp_008_b
  `connectivity` 99.66 -> 99.81; but the node measures fall on most levels (sp_003_b `nodes_paired` 36.74 -> 35.28)
  and sp_007 `ai_pairwise` 94.4 -> 93.3: applied inside areas too (KYG-NODE-9). (One run aborted on sp_006 with a
  failed 0.67 GB allocation while other rounds held the memory; the last 7 levels were built again alone.)
- `lip_object_flat_all` (every object lip needs flat floor on both sides): every mean better but `ground` (-0.004);
  worse only sp_005_b `ai_pairwise` 98.5 -> 98.2, sp_006 `outline` 98.79 -> 98.68, sp_01 / sp_005
  `nodes_paired_candidate` -0.07 / -0.05, sp_02 `nodes_paired` -0.05; better sp_006 `ai_pairwise` 96.4 -> 98.2,
  sp_009 97.2 -> 100, ons_mp_004 `nodes_paired` 14.75 -> 15.54. Screen S1 (no object lips at all) showed why: sp_009
  collapsed (32.8 %, its aircraft sills) while four levels rose.

By XNAV-BLD-3's rule (a measure got worse on some level) none could go on alone; the numbers went to the user.

**The user's ruling (2026-11-27):** `lip_object_flat_all` **on** (`navwalk.LIP_OBJECT_FLAT_ALL`,
`WalkSettings::navkit`, the binding's default), its dips named exceptions in `spec/transforms/navkit.md` ("Parity
target"); `terrain_fill` off for now, a narrower one queued (fill only under the objects the sampler got stuck on:
sp_003_b's gain without sp_008 / sp_008_b's extra ground or sp_03's dip); `mesh_corners` and `RECT_ON_DICE_LEVELS`
off (`mesh_corners` at area boundaries only is the next test). navregress's reference `regression` re-recorded with
the new default (the old one kept as `regression_before_2026-11-27_nksdd.json`): its 15 rows equal the scratch
`lip_object_flat_all` run above in every value.

Files the ruling changed, and why:
- `navbuild_golden.json` (`test_navbuild_native.py --record --game`): the four golden builds, old -> new SHA-1
  (bytes) - sp_009 65e1f51d... (331,212) -> 5c3fc9f6... (333,728), sp_004_b 20f9b284... (8,046,096) -> ae128206...
  (7,926,916), sp_008_b 2774bb3f... (7,977,168) -> 99539f0d... (7,914,296), ons_mp_004 dc870cc8... (7,297,064) ->
  bf442008... (7,333,176): object lips with sloped sides no longer join ground. The recorded answers of the retired
  Python (e2a 331, e3c 456, e3h 29, stage 10) are unchanged: the 9 `walkable` cases among them now run with the
  rule they were recorded with (`retired_lips()`, `LIP_OBJECT_FLAT_ALL` off), as they test the port, not the rule.
- `navtools_golden.json` (`test_navtools_native.py --record --game --only measures`): the parity rows of sp_009 and
  sp_004_b, whose candidates are the new golden builds (sp_004_b `connectivity` 99.90 -> 99.86, within navregress's
  0.05 tolerance).
- `test_lips.py`: its "a 50 deg strip needs no flat floor (the old cap)" check is now the ruled behaviour (not
  crossed between 30-degree floors, crossed between flat ones), the old cap checked with the switch off.
- OPFOR's `work/navkit/parity/regression.json` (not in the repository): re-recorded, the old one kept dated.

Checks on the final tree (main e4da9408 merged): the quick tier 136 of 137 (the one failure `test_lips.py`'s old-cap
check, fixed above and passing alone); `test_navbuild_native.py --game` all passed (sp_009 as recorded);
`spec/trace.py --check` ok (1,506 rules).
egression.json` (not in the repository): re-recorded, the old one kept dated.

### 4. navregress and the harness

The old exact-match measures (`nodes_same`, `edges_same`, `costs`) left navregress's failing set (`INFO`: listed when
they change), as round NKS-deep E planned; the summary line prints `nodes_paired`. `harness/runs.rs`' `PARITY_HIGHER`
and `testharness/runs.py`'s fallback list equal navregress's `HIGHER` (19 measures).

### Left

- The exploration's walkable test (where KYG-PAR-4's ratios and cosines are read) - the edge speckle of
  `extra_ground`; DICE's graph gaps on open ground (sp_01's (-309, 100, -415), sp_03's areas without nodes).
- `mesh_corners` at area boundaries only; `terrain_fill` restricted to ground DICE's autoseed filters would reach;
  the sector-line / tile-line corner break on sp_003_b.
- The rulings: the four switches above.

## SPG2-rest5: reverse engineering for the specification (2026-11-27, branch `spg2r5`)

Static reading of `BFBC2Game.exe` (PC build 795745), read only. REA had no deep provider on this PC (only its
artifact-graph and .NET providers were available), so the reading was done with capstone over a scratch copy of the
executable, as in rounds SPG2-rest3 / rest4, plus a scratch map of every class's field descriptors (the registration
calls in the `ctr` section push each class's field table and type record: 1,378 tables). DICE's data was read through
the suite's readers. Nothing was run in the game, and nothing was written to `game\`.

| Item | Found | Rules |
|---|---|---|
| undergrowth settings (VEG-UG-5, 7) | the renderer reads `VisualTerrainSettings` (+0xDC): `UndergrowthDensityScale` (+0x4C) is the generator's density factor, `UndergrowthViewDistanceScale` (+0x48) the draw's, snapped down to whole cells, `UndergrowthPoplessDistanceRatio` (+0x24) the fade's start, `ViewDistanceScaleMax` / `ForceCellSideCount` / `MaxCellSideCount` the cell grid, `OverlapDensityScale` the per-cell capacity | VEG-UG-8 new |
| `WindScale`, second flag byte | `windParams` = (clock, `UndergrowthWindSpeed`, `WindScale`, 0) set per type at the draw 0xA0EE10; byte +0x35 read only by the loader and the two record copies | VEG-UG-9 new, VEG-UG-5 updated |
| particle evaluators (FX-PROC-5) | box, random, random XYZ, rotate vector, sphere, constant, colour interpolation, polynomial operator read; the operator's operands are c.x t + c.y t^2 + c.z t^3 + c.w, the reverse of `PolynomialData` | FX-PROC-6 new |
| inputs, `EfVelocity` | `EvaluatorInput` is `EmittableField`, the particle record's offsets; the update passes the age always and the dispatcher uses it only for `EfNormTime`; DICE's 40 `EfVelocity` processors are `TurbulanceData` without `Pre` (so `DefaultValues`); `EmitterParameter` 1 - 4 pick the particle's `Parameters` x - w | FX-PROC-7 new (verified) |
| processors' use of the value | the 0x80-byte particle record; spawn (position, direction x speed, size, rotation, rotation speed, animation) and update (turbulence, gravity, local force, air resistance, linear velocity, orientation, rotation, size) in 0xB2C1A0 | FX-PROC-8 new |
| loading roots (MENU-LOAD-1) | a screen's main vtable slot 4 (+0x10) is its root name: main menu "main_root", loading screen `GameModeData.LoaderName`, HUD `HudRootName` (via the level's `LevelDescription.GameMode`); DICE: loading_mp 8, loading_sp, loading_sp01 | MENU-LOAD-2 new (verified), MENU-LOAD-1 corrected |
| FSCommand handlers (MENU-RT-6) | slot 12 (+0x30): main menu 0x77BC10, HUD 0x7764F0, loading 0x753540 (all to the shared 0x95E680 but `startLoading`); the shared one 0x95E680. Of 133 movie FSCommands: main 28, HUD 29, shared 69, callback 4, none 4 | MENU-RT-7 new (verified) |
| event delay (MENU-EVT-4) | nothing writes the record's +0x01 / +0x02 / +0x08 but the pool's clear: the delayed pass never runs | MENU-EVT-5 new |
| criteria classes (PERS-AWD-5) | a met criterion is parked by `CriteriaType` (round, life, death streak) and re-armed by the reset (0x85F3E0 / 0x85F330); all-time and if-not-true never; the criterion objects' maker not found | PERS-AWD-6 new (observed; its counts probe-checked) |
| `<Code>_00` in `Final` | DICE (Dist, 2010): MP 380 of 380 codes, SP 70 of 70 have both `_00` and `_01` among `Final` | PERS-AWD-7 new (verified) |
| anim-tree node types (ATREE-BLK-1) | 0xF490A0 names types 1 - 26 (`AnimationNodeInfo` ... `RagdollNodeInfo`) | ATREE-NODE-1 new (verified) |
| blocks B and C (ATREE-BLK-2, 3) | B = (B + 16, n, 0, 0) + n 16-byte entries, C n records, n = the animation nodes; node +0x20 the index; B's first float the length (1/30 - 182.3 s) | ATREE-BLK-4 new (verified), ANIM-TREE-3 updated |

Probes and checks (game tier; counts, types and identifiers only, nothing of EA's kept):

- `spec/probes/menus.py`: the game modes' `LoaderName`, the three root slots' bytes and the screens' vtables, each
  FSCommand handler's answered commands against the movies' (54 checks, 6 new; 17 s).
- `spec/probes/resources.py`: node type names from the executable, counts per type, nodes in table order, block B's
  head and entries per animation node, the +0x20 index, the lengths (3.0 s).
- `spec/probes/effects_chain.py` and `core/tests/test_spec_probes.py::emitters`: the `EfVelocity` processors and the
  evaluators' `EmitterParameter`.
- `spec/probes/persistence.py` (new, 0.2 s): award codes, `Repeat`, `CriteriaType` and the `_00` / `_01` templates in
  DICE's start-up bundle; the grant's two name formats in the executable.

Rules went from 1,506 to 1,518 (by status: verified 899, implemented 143, observed 411, inferred 65); the census's
typeinfo records with a rule 801 -> 811.

### Playtests still pending (in-game tests are paused)

- `transforms/terrain_edit.md` section 5 (E5 / E7).
- ONS-CAP-3's patch.
- MAT-TEX-20's RenderDoc capture.
- BOT-GS-5.
- bot_ai.md's plan, including the kill probe.
- After OPFOR attaches the filled stats to the scoring record (PERS-SCORE-2, an OPFOR round): a round with kills and
  deaths whose save keeps them.
- New: a save with an award's `<Code>_00` / `_01` (PERS-AWD-5, 7); a burst of one menu event reaching the movie once
  per send (MENU-EVT-5); whether `-VisualTerrain.Undergrowth*` overrides change the undergrowth (VEG-UG-8, SET-CLI-1).

### What is left (SPG2-rest6)

- The undergrowth shaders' use of `windParams`; `UndergrowthMaxHeight`'s use.
- The mesh and sample-texture evaluators; the colour, transparency, alpha, texture-coordinate and clip-scale
  processors (the vertex fill); the mesh particles' update; the force vectors' set-up.
- What each FSCommand handler case does.
- Who makes the award criterion objects; the event 0x51.
- The anim-tree node records' fields; block C's words beside its pointers.
- OPFOR's fix for PERS-SCORE-2, as an OPFOR round.

## E4-rest2: the AI tools and placing on `ViewTool`, the last worker on `Task`, a Rush stage inserted between two (2026-11-27, branch `e4r2`)

What rounds E4 and E4-rest left (ROADMAP E4-rest2). Nothing was written to `game\`; the game never launched. Test
builds with `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=4`, build folder `rust-targets\bfbc2-tool-suite-e4r2`, the PC
shared with three other rounds' builds.

### 1. PR1b-0's leftovers: the AI workspace's nine `NavTool`s and placing onto `ViewTool`

- `bc2_editor::navtool::NavDrawTool` (`Tool::Nav`) holds what `NavState` held for drawing: the chosen `NavTool`, the
  clicks, polygon / box, the height range, the traversal kind, two-way, the note, who the edits are recorded as.
  `ViewTool::input` refreshes whether its clicks are live (the navigation drawn in the AI workspace with a project
  open: `nav_click`'s old condition) and closes a region's outline after Enter (`takes_enter` / `enter`, as the
  polygon tool); `click` takes the point the navigation's click took before (the picture's surface, else the
  navigation ground along the ray, else a marker's object: the views hand it over as `query::Hit::at`); a complete
  drawing is `Outcome::NavDrawing(edit, note)`, which the editor checks against the base (`nav::problems`), commits
  (`nav-add`) and draws (the clicks stay in the navigation's line layer, as before). `cancel` (Esc) drops the clicks,
  `left` (another tool) turns it off. The context menu's "Seed here" etc. add their point through `NavDrawTool::add`.
- `bc2_editor::placetool::PlaceTool` (`Tool::Place`) holds the object being placed (`BrowserState::placing` before):
  placing begins by choosing it (the tool before kept in `back`; a navigation tool chosen stays chosen), its input
  moves the object to the surface under the pointer, a click or the drag released over a view (`Pointer::dropped`)
  is `Outcome::Place` (`finish_placing`: `place`, or a model group's `duplicate`), its `draw` is the box and the
  yaw / sink read-out `tools_overlay` drew, Esc drops it; the tool before comes back either way. The wheel's turn
  and sink stay with the views' wheel.
- What changed for the user: a navigation tool and a gizmo tool were two independent choices; now a navigation
  tool is the views' tool, so choosing Select / Move / Rotate turns it off and the gizmo is not drawn while one is
  chosen. Choosing another tool while placing drops the object (before, it went on following the pointer).
- Interface tests, both themes: gr2 (`edit_build_install_restore_on_a_scratch_copy`: SP_009's route edge,
  traversal, link and note drawn by clicks, undo / redo, the side file, build, install, restore) and
  `ai_workspace_on_ons_mp_004`, e2b (the picker's placing with Shift / Alt wheel, the content browser's drag, the
  Objectives tile), `gameplay_tools`, `rush_stage_tools` pass unchanged but for the field paths
  (`tools.nav.note`, `tools.place.placing`). New `tests/view_tools.rs` (demo level, no game copy, CI): placing is
  the views' tool, Esc drops it and gives back the tool before, a click places and gives back Rotate; a seed by a
  click (one `nav-add`), a route edge's first point dropped by Esc ("Drawing cancelled"), the same tool again or
  Move turns the navigation tool off, a click with Select adds nothing.

Frames measured (headless 1500 x 900, demo level, median of 40 frames with the pointer moving over the ground; the
test fails past Select's frame + 16 ms):

| | dark | light |
|---|---:|---:|
| frame, Select | 1.03 ms | 0.85 ms |
| frame, placing (the object following the pointer) | 1.01 ms | 0.91 ms |
| frame, a navigation tool chosen (AI workspace) | 2.00 ms | 1.78 ms |
| a placing click to the committed `place` | 13.1 ms | 10.5 ms |
| a seed's click to the committed `nav-add` | 13.2 ms | 7.6 ms |

The budgets the editor tests hold, on this branch: `editor_core::stroke_budgets` (OP1's: a stroke's commit under 16
ms, undo under 100 ms) 400 strokes on 2048 x 2048, commit median 2.96 ms (p95 7.33, max 20.2), undo 20.8 ms, a
History jump of 40 15.8 ms, redo 15.6 ms. The views' tools are taken out of `ViewState` for each input call
(`std::mem::take`): `NavDrawTool::default` reads the PC's name once per process (`OnceLock`), not per frame.

### 2. The other workers onto `Task`

Of PR1b-0's 12 hand-made workers, rounds PR1b-2 and PR1b-1-rest had moved 11 (BENCHMARKS "PR1b-2", the install's run
in "PR1b-1-rest"). The last, a Package level's preparation in the editor (`prepare::Job::start_package`, a bare
`std::thread::spawn`), runs on `Task::spawn_value("Preparing <LV>")`: its lines still stream on the job's own channel;
a panic, which used to end the job with "stopped without saying why", is now the job's error line (the Task's
answer, read when the stream ends). What keeps bare threads: the four pipe readers (`prepare`'s stdout / stderr,
NavKit's two in `nav::start_navkit`), which read a child's output line by line while it runs, and the scoped
fan-outs inside a job (`catalog::read_first`, `package::read_entries`: one reader per bundle, joined before the job
answers).

### 3. A Rush stage inserted between two (GAME-RUSH-12)

`bc2_levelwrite::rushstage::insert_stage(w, schema, after, at, guid, ground)`: a copy of stage `after + 1`'s set
(a later stage's: its objects start out of play) moved to `at`; the connections between the two stages split - stage
`after` to the next goes to the copy, and from the copy (through `counterparts`) to the next; the next stage's to
stage `after` go from the copy, and from the next stage to the copy's counterparts; the next stage's links to the
shared logic copied, to the other stages not. `BaseCount` + 1. Checked by playing the level: the order `..., after,
new, next, ...`, every old stage's certain states as before, the copies as the next stage's objects, the next stage's
own waiting; where not, the counter of the stage before switches the object (up to 4 rounds), else refused.
`add_stage` and `insert_stage` share `copy_of`. Not copied: what the next stage's counter switches on for a stage
no one plays (MP_006's sixth forward base: copied, it was read as the last stage's and stayed behind when the stage
was taken out - one of the sweep's first failures).

`remove_stage` now also retargets the next stage's connections into the removed stage to the stage before's
counterparts (`handed_back`): the sweep's other first failure was MP_004, where stage 2's hold-back timer disables
stage 1's combat area; with a stage inserted and taken out, that connection had become the stage-1 counter's.

Editor: `GameplayKind::InsertStage(k)`, the Gameplay panel's "Insert after" on every stage but the last
(`PanelRequest::InsertAfter`), History label "Insert Rush stage <k + 2>". The `Stage` hint and "Remove stage"'s
tooltip had a run of spaces where a line continuation was lost (fixed).

Proofs:

- Synthetic Rush level (CI): a stage inserted after stages 1 and 2 (13 copies each), every wire split; taken out.
- Port Valdez Rush (`mp_012gr`): 3 insertions (712 copies, 113 wires, 5 without a counterpart); Squad Rush
  (`mp_012sr`) 1 (126 copies, 18 wires, 1).
- Game tier, every stock Rush and Squad Rush level (`every_rush_level_is_wired_stage_by_stage`, the module 2,186 s
  on the shared PC): 30 levels, 57 insertions (72 - 900 copies, 15 - 238 connections a level); each plays between
  the two, the other stages as before; taken out again, every instance, stage state and connection is back; 42
  connections had no counterpart to split (0 - 5 a level, left as they were). E4-rest's add / remove results
  unchanged; one state still differs with a stage taken out (`nam_mp_002r` stage 2: `CombatArea7`).
- Export (`port_valdez_with_a_stage_inserted_exports`, game tier): Port Valdez as BFBC2 Rush with a stage inserted
  after the first, `export::build` in 28.5 - 36.6 s, `export::check` holds it, the staged partition reads back byte
  for byte and plays five stages, the inserted second, `BaseCount` 5.
- Editor (headless, both themes, `a_rush_stage_through_the_interface`): after E4-rest's steps, "Insert after" on
  stage 1 and a click: one operation "Insert Rush stage 2", 4 stages, stage 1 and the later ones as before, the new
  stage's M-COMs new; undone. Screenshot `gameplay_insert_<theme>` with `BC2_EDITOR_SHOTS`.

### What is left

- In-game proof of every gameplay edit (in-game tests are paused, 2026-11-20): whether the game spawns at, captures
  and announces a copied flag and shows a fifth letter; an added M-COM arming and ending its stage; the rules'
  effect on a round; an added, inserted or shortened stage chain playing (bases, areas, hold-back timers,
  reinforcements, the round's end); the new texts on the HUD and the deploy screen; a spawner added to a flag.
- From E4-rest, unchanged: removing an added stage leaves the last counter the switches it gained (semantically the
  level's); `nam_mp_002r`'s hold-back area with stage 2 out; vehicle spawners shared between stages that a new
  stage's end does not stop where no counterpart is found.
- The 42 connections without a counterpart to split stay as they were (each the stage before's object acting on the
  next stage's: at worst a timer or message of the stage before reaching the next stage during the inserted one).

Checks on the final tree (main d6bad8f3 merged): `cargo clippy --all-targets -D warnings` on bc2-editor and
bc2-levelwrite, and bc2-cli's 19 tool groups alone, clean; bc2-editor's tests 58 passed (28 game-tier ignored) and
80 unit tests; the rush stage module with the game tier (5 of 5); gr2 and rush_stage_tools with the game copy; the
quick tier 137 of 137; `spec/trace.py --check` ok (1,519 rules, GAME-RUSH-12 unused on main).

## XD2: the shader translator's differential proof, widened (2026-11-27, branch `xd2`)

The user's forward emitter (ef104ca1, `hlsl.rs` `cfs_forward` / `has_non_nested_forward_jumps`) made 20 of the 8,320
distinct Xbox programs of `test_xenos_native.py --game` translate that used to fail with "jump to cf N is not a nested
forward jump". `test_xdiff.py --game-all` proved only Wake's 815 pixel shaders; none of the 20 is among them.

**The 20.** All vertex shaders, one shape (a skinned shader's if / else on a bone's values: CF 3 `(!p0) jmp 6`, CF 4
`(p0) exec`, CF 5 `(p0) jmp 11 / 12`, 5 or 6 `(!p0) exec` blocks). 4 in BF1943 (`guadal_canal` vs285, vs469;
`iwo_jima_s` vs255, vs520), 16 only in BFBC2's Xbox levels (`mp_001` vs44, 106, 213, 245, 276, 299, 358, 712;
`mp_002` vs8, 754; `mp_003` vs14, 153; `mp_004` vs305, 824; `mp_008` vs344, 421) - the task's "BF1943's databases"
holds for 4 of them. Three translate under fewer than 3 interfaces (iwo_jima_s vs255 and vs520 under 2, mp_002 vs8
under 1): the seeded random interfaces drop a TEXCOORD the shader writes ("VS writes TEXCOORDn, not in the
interface"), the same check the nested emitter makes. Exactly 20 programs' plain-interface translations carry the
`_cf` guard (a scan of the corpus).

**The proof.**
- `xenos_exec.py` runs vertex shaders: vfetch (the element's value by usage index, before the destination swizzle),
  oPos and o0-o15 exports (`Result.position`, `Result.outputs`). It records each jump's lanes (taken / not) and the CFs
  no lane reached, and marks lanes that read `c[k + a0]` outside the array of `c[k]` (`Result.out_of_range`: on both
  sides the value is the register file's layout, not the shader's; the comparison leaves those lanes out).
- `xdiff.py --vs [N ...]`: the translation wrapped (`main` renamed `xdiff_vs`, a wrapper's `main` takes a
  TEXCOORD16 with the pixel and the output slot and passes that output on, `nointerpolation`), each output of each
  vertex a pixel of its own; `--db <level>`, a resource name or `bfbc2:<level>` (OPFOR's unpacked Xbox copy). Vertex
  inputs: half the vertices in [-1, 1], half in [0, 20) - with (-1, 1) alone every bone index truncates to 0, every
  vertex takes the same side of the if / else (first measured: jump taken 0 of 512 or 512 of 512).
- The 20 on 512 vertices: all exact on oPos and every interpolator; the if's jump taken by 17 to 356 vertices and not
  by the rest; the else's jump taken by every vertex reaching it (it follows a `(p0)` block: not-taken is
  impossible); every CF reached; no lane out of range.
- The jumps in the 20 skip only blocks predicated off for the lanes taking them, so per-lane (the translation, the
  interpreter, Xenia) and per-wave branching agree; that is also why a jump bug could hide on them. Hence the
  hand-built if / else of plain execs (below), where the jumps alone decide.

**What was wrong.** `cfs_forward` ran a `cond_exec` / `cond_exec_end` (a bool constant's exec) unconditionally; the
nested emitter refuses it ("control flow cond_exec is not supported") and no part of the translator has bool
constants. Now refused as there. No corpus program has one: no recorded translation changed by it. Nothing else was
wrong: the 20 translations are the user's emitter's text.

**Re-recorded.** `xenos_golden_game.json` (`test_xenos_native.py --record --game`): 20 of 8,320 records changed,
exactly the 20 program keys above (each from three `E:jump to cf N is not a nested forward jump` to the new digests /
outcomes); `xenos_golden.json` unchanged. `test_xenos_native.py --game` then passes.

**Tests.**
- `rust/bc2-shader/tests/forward_jumps.rs` (new): a hand-built pixel shader (`if (r0.w > 0) r1 = r0 * r0 else r1 =
  r0 + r0` with overlapping jumps 1 -> 4, 3 -> 5) translated line for line with the guards; nested jumps keep the
  nested emitter; a bool-constant jump, a backward jump and a bool-constant `cond_exec` refused with the nested
  emitter's errors.
- `test_xdiff.py` quick: the same program as a pixel and a vertex shader against the interpreter, exact, both
  branches run (vertex: jump taken 194 / not 62 of 256), and a translation with the jump `1 -> 4` made `1 -> 5`
  caught (pixel 138 of 256 outputs differ, vertex 194 of 512). `--game`: the 20 by program key, exact, both sides
  of each if / else run.
- `--game-all`: every Wake pixel shader still exact (815 of 815; served and reference runs alike, 3,261 files).
- Not in a test (evidence for the README): Wake's 735 vertex shaders on 256 vertices, 728 exact; vs251 / vs710
  rounding on values near 1e4 (relative 1e-6), 5 fetch a vertex texture from fetch constant 22 with no sampler.
- `bfbc2 xdiff` takes `--vs` and no longer refuses another `--db` with `KeyError` (argspecs.rs regenerated;
  `argfmt_golden.json`, `abbrev_golden.json`, `cli_o5_shader_golden.json` re-recorded: only xdiff's lines changed).

Spec: XSH-CF-1 (verified).
## E7-TG3: Create Level's terrain, from a height map and from the generator (2026-11-27, branch `e7`)

ROADMAP rows E6-E7 (the Create Level part) and TG3, done together. Spec: `transforms/terrain_edit.md` TEDIT-NEW-1
(verified), TEDIT-NEW-2 (implemented), TEDIT-NEW-3 (verified); `transforms/validation.md` XVAL-TERR-4 (new);
`terrain_gen.md` TGEN-REQ-5 implemented. Project format 13.

**What it does.** Create Level's form ("From a height map", and "Generate" beside it with `bfbc2 terrain
generate`'s settings) makes a project on a **base level** (a multiplayer map with its own terrain: its mode layer,
objects, weather and its terrain's look, TGEN-BIOME-4) that opens like any project. The export
(`bc2_levelwrite::newterrain`) makes the terrain from the project's record, names it after the level
(`Terrains/<LV>/<LV>`: the level's `TerrainEntityData` reference, the base's terrain partitions `<T>.dbx` and
`<T>-MaterialMap.dbx` copied and renamed, the patch / undergrowth / cache / water entries cloned into each staged
database after the export's merges, the colour / occlusion map), stages it in the level's own terrain bundle with the
base bundle's other entries but none of the base terrain's, and registers that bundle without the base's terrain
bundles (TGEN-REQ-5). `mapexport check` reads it back (XVAL-TERR-4); the path-data step samples the stage.

**On DICE's data** (`rust/bc2-levelwrite/tests/new_terrain.rs`, game tier, release):

| Test | Result |
|---|---|
| `dice_heights_through_the_import`: every DICE main block of Dist's and Package's levels (bundles before 2013) as a height map with its own range, imported | 30 of 30 give DICE's stored samples, bounds and min / max tree (the envelope is built under the heights; the depth map is 255); MP_005 also through a 2,049 PNG file; 2.2 s |
| `new_level_exports_and_checks`, MP_005 as Conquest, generated (arid 1,024, seed 7, ring, a pad) | built in 26 - 29 s (the terrain and visual side 3.3 s); 19 terrain entries, 99 entries cloned into each database, 2 terrain partitions renamed; `export::check` (27 s) passes every rule; the terrain block names only the level's own bundle |
| the same with MP_005's own heights as a PNG (2,048) | built in 37 s; every rule passes; the staged main block has DICE's heights and bounds |
| the base's Conquest layer with no new terrain | every rule passes (the reference) |

The first check failed XVAL-REF-3 twice, and both were the export's: the base terrain's partition names
`Terrains/MP_005/MP_005-MaterialMap` (a partition MP_005's level bundle ships, not only its terrain resource), so the
export now copies every partition named after the terrain; and DICE's own terrain partition names editor partitions
that never ship (TEDIT-SRC-1: road splines, decal materials), which XVAL-REF-2 now lists as the base's own for the
renamed copy.

**Path data** (`new_level_path_data`, MP_005 prepared for NavKit, `bin\bfbc2.exe` built from the branch):

| Level | Built (sample + build) | Sectors / route nodes | Connectivity | Walkable 8 m cells in the playable box | Coverage | XVAL-PLACE-5 |
|---|---|---|---|---|---|---|
| generated (arid 1,024) | 48 - 82 s | 27 / 7,593 | 0.883 | 2,421 | 0.860 | 32 points off (the base's layer, laid out for a 2,048 m map, on 1,024 m of ground) |
| MP_005's heights imported | 57 - 65 s | 25 / 7,818 | 0.944 | 2,799 | 0.899 | 13 off |
| MP_005 itself sampled on the stage (DICE's terrain and name) | - | 25 / 7,854 | 0.944 | 2,799 | 0.899 | 13 off (the layer's, on the stage sampler) |

Connectivity: the share of route nodes on the main network; coverage: walkable cells (dry, under 40 degrees) a
main-network node reaches (15 m / 4 m, as XVAL-PLACE-5); the build checks: XVAL-AI-1 and the file parsed and written
back byte for byte.

**The canary (TGEN-OUT-5)** first found a bug: the stage sampler wrote only the staged main height field into its
tree, so a new terrain's water mesh (under the new name) was never seen and the build walked the sea floor (28 sectors
and 8,320 nodes against DICE's terrain's 25 and 7,854); `navkit sample --stage` now writes the staged water mesh too.
Then 7,818 against 7,854 nodes: the import's water is one quad over the square (TGEN-WATER-1) and DICE's MP_005 water
covers only the sea (1,902 vertices at 58.0 / 58.31 m), so the build's water rule (ground more than 1.5 m under water
left out) removed inland ground under 56.5 m. With DICE's water mesh in the copy's stage, the copy's path data
**equals** DICE's terrain's: 25 of 25 sectors, every node position, edge count and mesh.

| Run (release, this PC) | Time |
|---|---|
| `cargo test --release -p bc2-levelwrite --test levelwrite new_terrain:: -- --ignored` | 2 s + 178 s + 302 s |

### Left

- The editor's views show the base's terrain for a Create Level project until it is exported (the export's stage has
  the new one); the game-accurate view's `read_generated` could show it.
- The base's objects and gameplay stay where the base had them (TG5 / A3 place them on the new ground); sculpt and
  paint on a new terrain are refused until TG4.
- A height map's water is one quad (TGEN-WATER-1); shores and partial water bodies are not written.
- Bases: DICE's multiplayer maps with a terrain of their own; WW2 (Package-only, BF1943 terrains) not tried.
- E6 (vegetation) stays queued.
- **Unproven until a playtest** (`terrain_edit.md` section 5, step 7): the engine loading a level whose terrain
  bundle is its own and whose base's is not registered, the renamed terrain partitions and cloned entries, a
  height-map terrain and a generated one in game, the bots on them. Nothing was installed or launched.

## SPG3-rest: settings, input, scripting and the asset graph, statically (2026-11-27, branch `spg3r`)

What round SPG3 left (`spec/COVERAGE.md` section 2, its items 1, 2 and 4; item 3, the WW2 rebuild's step items
XDEP-STEP-3 / 5 / 6, is round OR7's). Static reading of `BFBC2Game.exe` (PC build 795745), read only: REA started,
but its Ghidra provider timed out (330 s) decompiling one function of the 19 MB executable, so the reading was done
with capstone over a scratch copy, as in rounds SPG2-rest3 to rest5. Event names were found from the hashes the
handlers compare (`h = h * 33 ^ c` from 5381) against the executable's strings, DICE's 186 event names and their
two-part combinations. DICE's data was read through the suite's readers; one `GameSettings.bin` the game wrote was
walked for its structure (names and sizes, no value). Nothing was run in the game, nothing written to `game\`.

| Item | Found | Rules |
|---|---|---|
| `GameSettings.bin` | written by the INI's save (0x833FD0 -> 0x815940 -> serializer 0x80DA10), read after the INI (0x815870, job completion 0x8345A0 -> 0x832270): 11 categories (`GstCount`), each a u32 count and per not-user-exposed setting {u32 type 1 - 4, u32 name length, name, value: binary as size + bytes, else as text}; the file the game wrote (242,795 bytes) walks to its last byte; its `GstBinary` holds the campaign save (`header`, `body`) | SET-BIN-1 new (verified) |
| bad values | int parse: sign and digits, trailing text ignored, fails without a digit or past 32 bits; float: `strtod`; a failed parse gives 0 (the INI reader's fallback), an empty value keeps the default, no range check on store; `DxVersion` 9 / 10 / 11 -> Dx9 / Dx10 / Dx11, anything else `SelectedRenderer_None` | SET-PARSE-1 new |
| `-<Object>.<Field>` | the argument list is read only by the finder (41 sites, each a fixed name) and the log line; the registry's `<Object>.<Field> <value>` text reader 0x5017C0 and adder 0x501AB0 are reached only through two thunks nothing names | SET-CLI-2 new (verified), SET-CLI-1 updated |
| the sliders | `controlsSliderMoved "<value>,<slider>"`: `mouse_sensitive` / `mouse_yaw_factor` / `mouse_pitch_factor` -> `Sensitivity0` / `1` / `2`, `joy_*` -> `3` - `5`; `controlInvertToggle`: `look` -> `Scheme0FlipY` + `Scheme1FlipY`, `fly` -> `Scheme2FlipY`, `wheel`, `accelerate` | INP-SET-2 new (verified) |
| setting / console entities | `ServerGameSettingEntity` on `ChangeSetting` sends message 0x13 (2 bits) to every connected player; the client flips `Scheme1FlipY`, `Scheme2FlipY`, `Scheme4FlipY` for `CGSInvertVerticalLookAxis`; `ServerConsoleCommandEntity`'s handler is `ret 4` | SCR-CMD-2 new (verified) |
| the 107 unused classes | factories: server 148 registrations, client 100, exact-class lookup; 45 of the 107 have none (no entity), 62 have one; their handlers' input events named (static models, pickups, event memory, sequence trigger, speed gate, continue, tactical objective, tests) | SCR-EVT-3 new (verified) |
| document server | six providers register through 0x4D06B0; the only server add 0x4D07D0 is named by nothing: no server, no pages, no socket | SCR-DEV-2 new (verified), SCR-DEV-1 updated |
| references in resources | mesh sets: LOD settings partitions, `<mesh>/<guid>` instances, shader names (the unshipped material partitions); Dx9 shader databases: 37,210 texture names; terrain visuals: height fields, undergrowth mesh sets, atlases; Flash movies: image partitions; voice-over waves: 41,393 of 41,463 named `<node>[_<n>]_wave` by the line banks | XDEP-RES-2 new (verified) |

Probes (game tier; counts, identifiers and code bytes only, nothing of EA's kept):

- `spec/probes/options.py` (0.6 s): the finder's call sites and their pushed names, the registry's text path and the
  document server's add reached by nothing (an E8 / E9 rel32 scan of the code sections and a stored-pointer search),
  the `.bin` loops and checks and `DxVersion`'s map as bytes, the sliders' and switches' pushed names, the category
  enumeration (25 checks, 9 new).
- `spec/probes/scripting.py` (12 s): the factory registrations per side, the data classes they make, the unused
  classes with and without one, each named event's hash in its handler, the console command handler's bytes (19
  checks, 6 new).
- `spec/probes/dependencies.py` (40 s, 22 s before: the 76 Dx9 shader databases are read whole): the names inside
  the eight name-bearing resource types by resolution, the voice-over waves the banks name (15 checks, 3 new).

Rules went from 1,524 to 1,532 (by status: verified 911, implemented 145, observed 412, inferred 64); the census's
typeinfo records with a rule 811 -> 816.

### Playtests still pending (in-game tests are paused)

- `transforms/terrain_edit.md` section 5 (E5 / E7); ONS-CAP-3's patch; MAT-TEX-20's RenderDoc capture; BOT-GS-5;
  bot_ai.md's plan, the kill probe; a save with kills after OPFOR's PERS-SCORE-2 fix; PERS-AWD-5 / 7; MENU-EVT-5;
  `-VisualTerrain.Undergrowth*` (now answered statically: SET-CLI-2 says it does nothing, a start would confirm it).
- New: a level with a game setting entity sent `ChangeSetting` (every player's look inverted, SCR-CMD-2); a
  `GameSettings.ini` with a word for a number (expected 0) and an unknown `DxVersion` (SET-PARSE-1).

### What is left (SPG3-rest2)

- What the renderer does with `SelectedRenderer_None`; a damaged `.bin` (a count past the end); what else writes
  `Scheme0Sensitivity`.
- The factories SCR-EVT-3 did not follow to a handler (lights, emitters, meshes, bangers, `WeaponCallback`); five
  unnamed event hashes; the events the unused classes raise; their fields.
- The 70 voice-over waves no bank names.

## SPG4-rest: every typeinfo record with its layout, the run-time behind SPG4's data side, the opaque insides (2026-11-27, branch `spg4r`)

What round SPG4 left (`spec/CENSUS.md` section 5, `spec/COVERAGE.md` section 2). Static reading of `BFBC2Game.exe`
(PC build 795745), read only, with capstone over a scratch copy (REA was not used: its Ghidra provider timed out on
the executable in the round before). DICE's data was read through the suite's readers (DICE's files only, dated
before 2013; counts, identifiers and code bytes only). Nothing was run in the game and nothing written to `game\`.
Two research forks read the run-time and the resources' insides in parallel; every finding they reported was
re-checked here by the probes before it became a rule.

| Item | Found | Rules |
|---|---|---|
| the typeinfo records | each of the 1,900 registered once by a static initialiser `push <args>; push <record>; mov ecx, <class info>; call <register>`: 0x500880 (961 data classes: member table, creator, parent), 0x500660 (400 run-time classes: parent), 0x43C390 (289 value types), 0x5008E0 (231 enumerations), 0x4FE260 (19 built-ins); a member is a 24-byte descriptor {name, flags 0x48 for arrays, type, element type, offset or value, 0}, a class's table its own members only; every one of the 7,087 members lies inside its record and past its parent's size | RTTI-REG-1, RTTI-MEM-1 new; ENG-TI-1 corrected (the table address) |
| the tables | every record's parent, size, own members with run-time offsets, or an enumeration's numbers (1,952 values; 20 enumerations not numbered in declaration order), by module | RTTI-CORE-1, RTTI-GSR-1 - 8, RTTI-GDLL-1, 2, and 15 module rules (RTTI-ENT-1 ... RTTI-UI-1) |
| DICE's DBX against the run time | 630 classes: the same 398, the DBX more 230, both ways 2 (`SubtitleWaveHashPair`, `UISharedConfigAsset`'s Vietnam members); 118 pipeline-only names: `MemoryReportGroup`, the mesh build's settings, the static models' pathfinding switches, the AI's sensing template, the sound nodes' `Name` | RTTI-DBX-1, RTTI-DBX-2 new |
| the factories | each factory's construct slot (server 4, client 6) to the constructor's last vtable, its `getType` to the class, the size slot against the record: 110 of 148 server and 75 of 100 client factories, 173 run-time classes | RTTI-FAC-1 new |
| the controller | `CharacterPhysicsData.States` matched by class (0xB08CE0), nine states with ids 0 - 8 (Havok's numbering), each update's transitions, the outside setters (parachute, scripted moves, network) | SOLD-PHYS-2 new |
| spawn on a squad mate | the request's checks (same team and squad, not `SquadNone`, not dying, anti-spam, combat area), the 13 offsets tried, the vehicle seat; `MaxPlayersPerSquad` / `SquadSpawnRadius` not on this path | SQD-PLAY-2 new |
| the kill cam | `ServerSettings.KillCam` (`Server.KillCam`), the spectate entities' test: dying, hardcore unless `UseInHardcoreMode`, the switch, the target's health | HUD-KILL-2 new; HUD-KILL-1 corrected |
| the campaign save | the `Save` event, `body` (version, 70, the entity's GUID, the persistent entries, subsystems, the player) and `header` (Julian-epoch time, level, save name, difficulty, 3) into `GameSettings.bin`'s `GstBinary` | CAMP-SAVE-1 new |
| the sound nodes | a factory per node data class (`SimpleTransformNode`'s), the enumerations' numbers (`MathOp`, `ConditionType`, `FaderType`, `LoopType`); the evaluation not traced | SGR-NODE-2 new |
| the occluder | the triangle block: one 192-bit part mask a triangle, padded to an even count (SPG4's "more records" were the padding); the job drops a triangle whose mask meets the instance's hidden parts | OCC-TRI-1 rewritten (verified), OCC-VTX-1 updated |
| the soot | the flag words pick `SootPerVertex` / `_Half` / `SootPerPixel` / `_Half`; a vertex's last word is (slot, box); the trailer's 60 words map slots to parts, one constant each at draw | SOOT-FLAG-1, SOOT-SLOT-1 new; SOOT-HDR-1, VTX-1, TAIL-1 updated |
| the impulse responses | two channel blocks a partition: a block scale 2^k - 1 and 1,024 (re, im) bins; the PC loader keeps the file as a blob, no reader found | IR-PART-1 new |

Probes (counts, identifiers and code bytes only):

- `spec/probes/typeinfo.py` (new; `--measure` 1.6 s): re-reads the executable into `spec/typeinfo.json` (546 KB) and
  writes the tables of `spec/engine/typeinfo/*.md`; `--check` (the quick tier's new `spec-typeinfo`, no game) fails
  when a table is not current or a record is in no group; the game tier compares the executable with the JSON and
  checks 22 counts (registrations, members, factories, the DBX comparison, the layout invariants).
- `spec/probes/runtime.py` (new, 14 s): 32 instruction / constant checks at the addresses the run-time rules cite and
  the DICE values those code paths read (spectate entities, team data, save entities, the controller's states).
- `spec/probes/resources.py` (4 s): 13 new counts for the occluder masks, the soot's flags and slots, the impulse
  responses' blocks.

The census: typeinfo records with a rule 816 -> 1,900 (of 1,900). Rules went from 1,532 to 1,571.

### Playtests still pending (in-game tests are paused)

- New: spawn on a mate prone on a ledge, in a full vehicle, outside the combat area, twice in a row (SQD-PLAY-2); a
  hardcore server with `Server.KillCam` on and off (HUD-KILL-2); a parachute closing into falling (SOLD-PHYS-2); a
  campaign save and its reload (CAMP-SAVE-1: the header's difficulty word, the body's version 70); a level edited to
  flip `ExcludeFromPathfinding` without rebuilding the path data (RTTI-DBX-2: expected no change).
- SPG3-rest's: a level with a game setting entity sending `ChangeSetting`; a `GameSettings.ini` with a word for a
  number and an unknown `DxVersion`; and the earlier rounds' list (`transforms/terrain_edit.md` section 5, ONS-CAP-3,
  MAT-TEX-20, BOT-GS-5, bot_ai.md's plan, PERS-SCORE-2, PERS-AWD-5 / 7, MENU-EVT-5).

### What is left (SPG4-rest2)

- The sound nodes' evaluation; the controller's Jump entry and InAir / Falling exits; `MaxPlayersPerSquad`'s and
  `SquadSpawnRadius`' readers and the a != b spawn form; the kill cam's client camera; the save body's objects and
  restore.
- The 22 component and 16 order factories and the client's bangers RTTI-FAC-1 did not resolve.
- The occluder's fourth vertex word's consumer; the soot's second mask; the impulse responses' reader on PC.
## OR6: OPFOR in Rust - the map export's install side (2026-11-27, branch `or6`, with Project OPFOR's `or6`)

Version 0.29.0 (OPFOR's `SUITE_REQUIRES` 0.29.0 to below 0.30.0).

### Inventory

| OPFOR / suite code | Lines | What it did | OR6 |
|---|---|---|---|
| `src\tools\mapedit\mapedit_install.py` | 457 -> 97 | plan / apply / revert / status of a map export: the stage's check, the shared patches (startup, overlay, awards, maps.ini, package.mft, bundleManifest), the level folder and record, the path data, the ledger writes through `console_common` / `opforcore.stages` | `bc2_install::mapedit` (feature `mapedit`, 1,160 lines with its docs; `stages.rs` 401); the script passes its options to `bfbc2 install mapedit` |
| `console_common`'s `check_install` / `install` / `check_revert` / `revert` / `installed` / `source_bundle` as the export install called them (`opforcore.stages.Stages`) | - | the ledger's whole-file stage operations | `bc2_install::stages` (on `bc2_gamefs::ledger`'s `install_keep` / `uninstall` / `uninstall_files`) - what IN1 builds on |
| `mapexport overlay / awards`' reading of the stages | ~60 | the texts and pictures from stage folders | `bc2_levelwrite::installpatch`, shared by `mapexport` and the install |
| the editor's `run_logged` (python + the script, its output through a log file: AU4-DS-13) | ~30 | the Install dialog | `export_ui::install_step`: the install in process on a worker |

**The crates.** The install's check and patches are `bc2_levelwrite`'s, behind a trait (`bc2_install::mapedit::Patches`,
implemented by `bc2_levelwrite::installpatch::LevelWrite`): with `bc2-levelwrite` as a dependency of `bc2-install`,
the Python module's source fingerprint (`bfbc2core.native.crates`, which follows optional dependencies too) took in
the export's crates, so every editor round would have made the module stale - the quick tier's "Rust core required"
entry caught it (its copy of the checkout leaves out `tests\`, which `bc2-levelwrite` includes recorded projects
from). The module's crates are as before.

AU1-G-N8 (installing an export needs OPFOR's Python): fixed - the editor's Install, Install now and Uninstall run
`bc2_install::mapedit` in process; `bfbc2 install mapedit` is the command line.

### Decided against the Python (spec XINST-MAP-5, -6, -7)

- **Each install keeps its own copy of its stage** (audit AU4-DS-12, XINST-MAP-5): the Python rebuilt every other
  installed export's description, texts and pictures from the stage folder its record named - the editor's live
  `work\mapedit\<lv>\stage`, which the next export of that project rewrites - so installing or reverting one level
  could change another's name or picture in the game. The install keeps `stage.json`, the description and the
  pictures in `Package\levels\<lv>\opfor-mapedit-stage\` and builds from them; an install from before OR6 falls back
  to its stage folder with a line saying so.
- **The record first, the record last** (XINST-MAP-6): the Python wrote the level's record after its bundles, so a
  kill between the folder and the record left a folder every later install refused ("not an exported level") and
  no revert could take out. The record is the folder's first write and a revert's last removal; a folder holding
  only the record's temporary files counts as an install's start.
- **DICE's package.mft keeps its last blank line** (XINST-MAP-7): DICE's file ends `\r\n\r\n` (REG-MFT-1); the
  Python's patch wrote one `\r\n`, a byte outside the levels' blocks. The WW2 install's text rules (`bc2_install::text`)
  are unchanged: the WW2 oracle holds them.
- The build's scratch (`work\mapedit\<lv>\shared`, `_revert`) is removed when a run goes through (XINST-STG-5; the
  Python left it).
- Kept as the Python had them: the record's form (`stage.json`'s fields, then `stage`, then `installed`;
  `json.dumps(indent=1)`, ASCII), the maps.ini marker line (it names `mapedit_install.py`: installs made by either are
  found), every printed line and refusal (the check's failure still reads "mapexport check failed").

### Proof

- **Parity** (`scratchpad\or6\parity.py`): the six shared files an export install writes, copied from OPFOR's
  `game\` with their old markers and migrated into the ledger by OPFOR's `migrate.py` (owners import, loading,
  loadset, minimap-edges, wake: the state an install meets on the author's copy). Two exports built fresh by
  `examples/e3d_matrix.rs` (Atacama's Rush as Conquest with its own name, pictures and a French text; Arica Harbor as
  a Vietnam Squad Deathmatch with its own name) and the Arica stage without a name of its own. The Python
  (`mapedit_install.py` of OPFOR 706b909, with this branch's `mapexport.exe`) and `bfbc2 install mapedit` each ran:
  plan, apply A, revert A, apply A, apply A again (a redo), apply plain N (a second export), apply named N (the level
  replaced), revert A (one out of two), apply plain N (no level with a name left: overlay and awards back as before
  the owner), revert all. **10 of 10 steps identical**: every game file (38 to 54) and level record, every ledger
  record but its transaction id and time (the Rust writes keys sorted; the values the same), every printed line
  (78 to 85 a step) but the Rust's one line on the kept copy, exit codes; the kept copies (2 to 8 files) the Rust's
  alone. After each full revert the copy is the migrated base, files and records.
- **Kills** (`scratchpad\or6\kill.py`, `BC2_INSTALL_CRASH_AT=<n>`, exit 9, then the same command again): from the state with export A installed, an apply of export N beside it - 18 write steps (the level folder, its record, 4 kept files, 4 bundles, the ledger commit's journal, 6 files and record) - and from the state with both, a revert of A - 17 steps (8 ledger steps, 9 removals, the record last): **35 of 35 kills finished identically** (files and ledger records those of an uninterrupted run; the ledger steps resumed by "an interrupted install was rolled back"). A long scratch path stopped the first parallel attempt: the bundle unpack refuses paths of 260 characters or more as FbrbTool (.NET) did - a real game folder is far shorter.
  On synthetic stages (`rust/bc2-levelwrite/tests/install_mapedit.rs`) every write step of an apply, an apply beside it and a
  revert (29 kills) is finished with the files and records of an uninterrupted run.
- **Tests**: `rust/bc2-levelwrite/tests/install_mapedit.rs` (5 tests: the text patches, an install beside another and every
  revert back byte for byte, the kept copy, the path data's owner, a kill at every step); OPFOR's
  `test_mapedit_install.py` (the wrapper's command lines, its refusals through the binary, a real export installed and
  reverted when the newest stage passes the check).
- **`game\`** (1,582 files) hashed before the round's first run and after its last: identical (`scratchpad\or6\game_before.json` / `game_after.json`).
- No WW2 rebuild: nothing the WW2 rebuild or install runs changed (`bc2_install::run`, `text`, `marks`, `levelbuild`
  and `bc2_gamefs` untouched; `bc2_install::stages` is new and used by the export install alone).

### Timing

Debug builds of `bfbc2.exe` / `mapexport.exe` (the twin runs this branch's `mapexport.exe`), one run each on a loaded PC: plan 35.5 s (Python) / 31.4 s (Rust), apply of the first export 33.5 / 35.3 s, of a second 45.1 / 42.8 s, revert of one of two 2.0 / 3.0 s, of the last 0.8 / 1.1 s. The stage's check (`mapexport check`'s rules, about 30 s, Rust in both) is most of an apply; the rest is the shared bundles' unpack and pack (Rust in both). The port is for the editor (no Python) and the fixes, not speed.

### What is left

- **AU5-ARC-3** (three implementations of the ledger format: `bc2_gamefs::ledger`, `bfbc2core.gameledger`,
  `opforcore.ledger`): none was replaced here. The export install stopped using OPFOR's Python ledger; the WW2
  install's `run.rs` keeps its own copies of the stage operations `bc2_install::stages` now has (moving it is a WW2
  rebuild slot's work); `bfbc2core.gameledger` serves NavKit's and stock-MP's Python and texqa; `opforcore.ledger`
  every other OPFOR stage.
- **IN1**: the doctor, the bug-report bundle, the game-folder setting (the editor still installs into
  `<opfor>\game`, building in `<opfor>\work\mapedit`), anonymised sharing, per-level slots for the shared patch
  (MM1-rest: one export disabled while the others stay). `bc2_install::mapedit::Installer` takes the game copy and the
  work folder as arguments, so a game-folder setting needs no change there.
- In game: an installed export has not been played (in-game tests paused).

## OR7: OPFOR in Rust - `rebuild_all` as a step graph (2026-11-27, branch `or7`, with Project OPFOR's `or7`)

The last item of the "OPFOR in Rust" row. OPFOR's WW2 rebuild (55 steps) no longer runs as a fixed list. It runs as a
step graph, in the suite: `bc2_install::rebuild` (new feature `rebuild`) and `bc2_native.rebuild_run` /
`rebuild_graph` / `rebuild_derive`.

| Project OPFOR (Python, until OR7) | here |
|---|---|
| `rebuild_all.run` / `run_changed` / `run_chains` / `run_step`, `JOBS`, `MIN_FREE`, `peak_memory`, `free_memory` | `rebuild::run::run` |
| `stepstamps.Tracker.begin` / `finish` / `why` / `stale_inputs`, `Hashes`, `scan`, `listing` | `rebuild::tracker` |
| (none: the fixed list) | `rebuild::graph`, `rebuild::derive` / `describe` |

OPFOR keeps the step list, the folders each step writes (`OUTPUTS`), the inputs no step writes (`EXTERNAL_INPUTS`,
still checked: exit 2) and the hook. It hands the runner a plan as JSON: each step's command line, log, scanned
folders and declared earlier steps, plus the interpreter and the environment. `stepstamps.py` keeps only the record
names and reads records.

**The graph** (spec XDEP-STEP-8). Step j waits for an earlier step i when:
- their records conflict: read-after-write, write-after-write, write-after-read, a listing above a write, or a probe
  of a write;
- OPFOR's declared graph says so: `rebuild_graph.json`, derived from a full rebuild's records by
  `rebuild_all --graph --write`, plus `READS`. `READS` covers the folders a step reads whole: `material_grid`'s
  `build_roots`, and `wake_install.our_files` in `wake_translate`, `emitter_atlas` and `fidelity_checks`, which a
  native call is handed inside a JSON string the hook cannot see. Each level's parts declare their own reads;
- either step is unknown to both: the list order.

Listings of `work\ww2` and the folders above it are no edge. The clone pass lists both, and counting them chained
every later step to it.

After a run, two steps that the new records order but that ran at once fail the run. Edges the declared graph does
not imply are reported.

**Running** (XDEP-STEP-9):
- a step starts when its earlier steps are done, at most `--jobs` (4) at once, the longest remaining path first;
- the steps' estimates must fit in a memory budget together. A step's estimate is its last measured peak (the commit
  of all its processes, through a job object; tracker 4 records it). The budget defaults to 70% of physical memory,
  capped at the memory free at the start less 1 GB;
- free physical memory must cover the step's estimate plus 1 GB;
- two steps whose scanned folders overlap never run at once.

With `--changed`, a step decides when it becomes ready, against the files as its earlier steps left them. The
send-backs of XDEP-STEP-7 are found before the run starts.

**XDEP-STEP-3 resolved for the graph; XDEP-STEP-10.** The main checkout's records (tracker 3) ordered the three level
chains one after another (write-after-read): Guadalcanal's chain "read" 527 of Iwo Jima's and 422 of Coral Sea's
files. These were not reads:
- `wake_install` saves its pickled indexes at the top of `work\ww2` and prunes them through the cache budget,
  handing it all of `work\ww2`;
- the hook stamped the first 50,000 files of that walk as read before it gave up and listed the folder. The chain's
  record held 33,019 inputs, among them 5,051 of `install_iwo_jima_s`, 1,106 of `_qa` and 271 of `menu_install`.

Now the budget's functions are not wrapped and an oversized folder keeps none of its stamps. The v4 records give no
edge between the chains, and they ran side by side.

**Found and fixed on the way.**
- The first `--changed` after a code change stopped at `wake_translate`: "its inputs from the clone pass were rewritten
  by a later step, which the pass before the run did not foresee". The pass had expected the clone pass to run. Once
  the steps before it reran and gave the same files, the clone pass was skipped, so `wake_translate` would have edited
  `wake_sky`'s databases. The pass now sends back to every writer whose recorded version the disk no longer holds,
  whether or not it expects that writer to run. New OPFOR test:
  `test_a_writer_a_rerun_leaves_unchanged_is_still_sent_back_to`.
- With the budget at 70% of physical memory, a run while other rounds were building took the PC to 15.2 GB in use. The
  default is now capped by the memory free at the start.
- `qa_checks`' step line lost `import time` in the rewrite; ruff caught it before a run.
- Two OPFOR tests (`test_qa_checks`, `test_ww2_fidelity_gate`) faked `subprocess.Popen`. The fidelity one ran the real
  `fidelity_checks.py` on the worktree's `work\ww2` once the fake no longer applied. Both now stand in for the runner
  (`rebuild_all.drive`).

**Proof.** All runs in OPFOR's `or7` worktree with its own `work\ww2`, seeded with the 14 declared inputs (599 files).
`game\` was read only, through a junction. The PC was shared with other rounds throughout: 4.4 - 9 GB in use by
others.

| Run | Steps | Steps' time | At once | Peaks together | In use (PC) | Oracle (7,945) |
|---|---|---|---|---|---|---|
| sequential before OR7 (WW2-ORC1, OR4, OR5) | 55 | 11.8 - 14 min | 1 (+2 chains) | - | - | 7,945 |
| seeded-empty, graph from main's tracker-3 records (chains in a row), `--jobs 3 --memory-gb 7` | 55 | 9.8 min | 3 | 6,510 MB | 8,154 -> 13,109 MB | 7,945 identical |
| `--changed` after the module was rebuilt (all code changed), budget 70% | 55 | 7.6 min | 4 | 10,721 MB | 8,975 -> 15,185 MB | 7,945 identical |
| seeded-empty again, final graph, defaults (budget capped by free memory) | 55 | 10.5 min | 4 | 5,718 MB | 9,046 -> 13,799 MB | 7,945 identical |
| `--changed --dry`, then `--changed`, nothing changed | 0 | 0.2 min | - | - | - | 7,945 identical |
| `install_ports\plans.json` touched (a byte added) | 7: `wake_translate` and the writers of the versions it and `grime` edit (XDEP-STEP-7) | 1.7 min | 3 | 4,735 MB | - | 7,945 identical |
| one leaf's code touched (`minimap_icons.py`, then put back) | 1 | 0.2 min | 1 | 77 MB | - | 7,945 identical |

- Step peaks (commit of all processes): a level chain 4,396 - 4,490 MB, `material_grid` 3,758 - 3,929 MB, the clone
  pass 1.8 GB, `bf1943_fx` 3.1 GB.
- The declared graph: 55 steps, 514 edges (62 after the reduction). The longest path is 6.2 min with the measured
  times; one after another, the same times add up to 15.1 min, since they were measured side by side on a busy PC.
- How far a run gets toward the 6.2 min depends on the memory free at its start. On this shared PC the budget let at
  most one level chain beside the small steps in run 2; the 7.6 min run had 4 GB more room.
- Install plan (`wake_install.py --plan`, 3 min 42 s): install oracle 21,149 of 21,149.
- `game\` sha256 before the first run and after the plan: 0 of 1,582 files changed.

**Tests.**
- bc2-install's rebuild module, 8 tests, on records built by hand and `cmd.exe` steps run for real: the conflicts and
  their kinds, the record keys and listing digests against `stepstamps`, a record from hook records and its
  decisions, side by side within a budget, a rerun that skips everything, a touched output, a failure, a derived
  graph.
- OPFOR: `test_rebuild_changed` (11, one new), `test_rebuild_tracker` (two rewritten for the graph),
  `test_ww2_build`'s declared-graph check, and the fidelity gate's place in the graph.

**Unproven until a playtest** (in-game tests paused): nothing is installed by this round, and the outputs are
byte-identical to the oracle.

**What is left.**
- The declared graph is only as good as the records and `READS`. A step that starts reading another step's folder
  through a native call's JSON string must be declared, or a run can start it too early. The check after the run
  only sees what the hook records.
- The plan's porting as a rebuild step (XDEP-STEP-5) is still proposed, not done.
- The graph's measured times were taken side by side on a busy PC. Re-derive them on an idle PC
  (`rebuild_all.py --graph --write` after a full rebuild).

Checks on the final tree (main bceb84f4 and 6d95c313 merged): cargo fmt; `cargo clippy --all-targets -D warnings` on bc2-install (feature `rebuild`) and bc2-py, and bc2-cli's 19 tool groups alone and together, clean; bc2-install's tests 41 passed (the rebuild module's 8 among them); the quick tier 134 of 137 on the first merge - the three failures were the new `OPFOR_REBUILD_TRACK` (now registered) and the spec / native-census checks reading OPFOR's main checkout, which does not have OR7's OPFOR side yet (against OPFOR's `or7`: `trace.py --check` ok, 1,574 rules, XDEP-STEP-8 - 10 unused on main; `native_census.py --check` 378 exports, 0 without a caller); OPFOR's `run_tests.py quick` 372 passed, 15 skipped; ruff and mypy clean on OPFOR's `ww2` and `tests`.


## TG4: vegetation, tiled layers and sculpt on a generated terrain (2026-11-27, branch `tg4`)

ROADMAP row TG4. Spec: `transforms/terrain_gen.md` TGEN-VEG-1 (implemented), TGEN-VEG-2 / -3 / -4 and TGEN-MAT-3 (new,
verified), TGEN-MAT-2 and TGEN-PLAN-1 updated; `transforms/terrain_edit.md` TEDIT-NEW-4 (new, verified), TEDIT-NEW-2
updated; `engine/vegetation.md`'s density question answered. Project format 14. Paused about an hour for OR7's
rebuild (the coordinator's hold); nothing was installed or launched, OPFOR's `game\` was only read.

### 1. DICE's vegetation, measured (TGEN-VEG-2)

`bc2_levelwrite::vegetation::dice_terrain_stats` reads DICE's 19 multiplayer terrains (the 30 of TGEN-BIOME-1 with
multiplayer levels; per terrain the level with the most forest and middlegrowth instances, the playable area the
union of all its multiplayer levels' combat areas) in 13.9 s, and `bc2_terragen::vegetation::measure` gives per kind
(forest, middlegrowth, rock, prop): density per hectare of dry ground inside the playable area and in bands of 0 -
50, 50 - 150, 150 - 300, 300 - 600 and 600+ m outside it, nearest-neighbour distances, Ripley's K ratio at 3 / 10 /
30 / 100 m, Clark and Evans, slope and height-over-water preferences, tilt (from the vertical and the normal), yaw,
scale and sink. The biome medians are kept as `vegetation_dice.rs` (`BiomeStats::dice`); the game tier measures them
again and holds them equal (`vegetation::dice_biome_statistics`). The table is in TGEN-VEG-2. Findings: DICE never
scales a member; trees and bushes stand upright with no favoured yaw, rocks are turned freely and sunk; densities fall
off steeply outside the playable area (snow's forests excepted); everything is clustered and prefers 20 - 30 degree
ground to flat ground.

Kinds: forest and middlegrowth groups' members; a static or rigid group's member is a rock by its blueprint's name
(rock, cliff, stone, boulder) and a prop when its blueprint lies under `vegetation/`, `objects/vegetation/` or a
terrain's folder. Man-made props, buildings and gameplay stay where the base has them (TG5 / A3).

### 2. The placement (TGEN-VEG-3)

`place_biome`: per kind and band, a Thomas cluster process (parents uniform on the band's dry ground, Gaussian
offspring), thinned by DICE's slope and height preferences, a hard-core gap of half DICE's 10th-percentile
nearest-neighbour distance, the type by the base's shares, the pose one of the type's own DICE instances' (yaw taken
out, vertical or normal frame, sink kept) at a uniform yaw. Fitting the Thomas formula to DICE's K ratios alone gave
the wrong clustering after the thinning (arid forest K10 1.4 too low at first; with each cluster enlarged by the
thinning's keep rate 5.2 against DICE's 2.3); calibrating the count and spread on the ground for six rounds (K10
excess and nearest-neighbour median, the best round kept) brings every kind within the test's bounds:

| Biome (1,024, seed 7) | Instances | Time | Kinds within the bounds |
|---|---:|---:|---|
| arid | 632 | 1.9 s | 4 of 4 (forest K10 4.2 against 2.3, inside the factor 3) |
| jungle | 5,224 | 3.2 s | 4 of 4 |
| snow | 3,949 | 2.8 s | 3 of 3 |
| temperate | 2,497 | 2.7 s | 3 of 3 (rock: DICE has one instance, not checked) |
| vietnam | 4,080 | 2.1 s | 3 of 3 |

Bounds: the playable area's density within 10 % (20 % under 100 instances), the nearest-neighbour median within a
factor 1.6 and the K10 excess within a factor 3 (kinds of 50 or more), the slope median within 12 degrees. The
per-kind numbers against DICE's medians are TGEN-VEG-3's table (`rust/bc2-terragen/tests/vegetation.rs`).

### 3. The writer: DICE's groups round-trip (TGEN-VEG-4)

The placement is written as round SG1's member edits (moves, then copies, then deletes). DICE's 17 model groups of
MP_005, MP_009CQ, MP_007, MP_004 and NAM_MP_003CQ (forest, middlegrowth, static, rigid; 15,699 members), every type's
members but its first added again at DICE's transforms and the originals deleted: group file, partition and physics
byte for byte, 17 of 17 (200 s: the member edits are applied one by one). The first version copied a type's first
member and failed on MP_005's static group: 17 `SpawnDatas` records there name another `MeshShaderSet` than their
type's first. So each added instance copies the DICE instance whose pose it took, and copies are made before deletes.

### 4. Tiled layers (TGEN-MAT-3)

`terrainpaint::layer_weights` reads a terrain's tiled layers back from its patch maps and atlas;
`newterrain::paint_tiled` measures each on the donor's own ground (coverage, slope and height preferences, paired
material) and lays it on the new terrain (seeded value noise plus the preferences, threshold by bisection to the
donor's coverage) through E5-rest's paint. MP_005 on an arid 1,024 terrain: Dirtfloor, DryGrass, Rubble, Trash over
307 cells, 47 tiles (105 slots left), no clone; Asphalt left out (no `Dunes_Asphalt_Seafloor_Destruction` to use or
clone); `Dirt` added to the material map for Dirtfloor's pair.

### 5. Sculpt and paint on the new terrain (TEDIT-NEW-4)

The editor opens a Create Level project on its new terrain (`newterrain_view`, in the loading task). The strokes keep
the base's file names, and the export reads and writes them on the new terrain's built resources: the new terrain is
built before the log is applied (`ExportSources::terrain`; the first run refused a stroke as 1,024 samples on a
2,048 height field). E7-TG3's refusal is lifted; a version-13 project's strokes, made on the base's view, are marked
in the 13 -> 14 migration and still refused.

### 6. Exports (game tier, release, MP_005 as Conquest, a generated arid 1,024 terrain)

| Test | Result |
|---|---|
| `vegetation::a_generated_level_with_vegetation_exports_and_checks` (vegetation, a 15 m hill and an Asphalt stroke made on the new terrain) | built in 38 - 46 s (terrain and tiled layers 7.8 s, placement 0.6 s, member edits 1.8 s); 111 forest and 137 middlegrowth instances in place of the base's 110 / 359 (MP_005 has no rock or prop types); `export::check` (35 - 41 s) passes every rule, the four groups' rules among them; the hill 15.00 m on the staged ground; forest inside 13.6 per ha against DICE's arid 14.8, middlegrowth 18.7 against 20.3 |
| `vegetation::a_generated_level_with_vegetation_has_path_data` (with `bin\bfbc2.exe` of main 9f61d758, the sampler unchanged) | with vegetation: built with path data in 79 s, 6,780 route nodes, connectivity 0.879, coverage 0.858; without: 60 s, 6,620 nodes, 0.883, 0.857 |
| `vegetation::dice_groups_round_trip_through_the_member_writer` | 17 of 17 groups byte for byte |
| `vegetation::dice_biome_statistics` | the 19 terrains measured again equal the kept table |
| `new_terrain::*`, `terrain_edits::*`, `terrain_layers::*`, `model_group_members::*` (E7-TG3's, E5's, E5-rest's, SG1's game tier) | 13 of 13 pass after the export's change (XVAL-PLACE-5's lines are the layers' own, as before) |

### Left

- **Unproven until a playtest:** the placed vegetation, rocks and props in game (look, collision, destruction of
  trees on new ground), the tiled layers' look, the sculpted and painted new terrain. Nothing was installed or
  launched.
- The editor's simplified and game-accurate views show the base's vegetation and objects where the base has them
  until the export; the game-accurate view draws the base's terrain for a Create Level project.
- Man-made props, buildings and gameplay stay where the base had them (TG5 / A3). Bases with no rock or prop types
  (MP_005) place none; no member type is added.
- Tiled layers only for donors whose layers are one kind each (MP_005, MP_006, MP_007); a layer with no shader for a
  combination is left out.
- The member edits are applied one by one (quadratic on large groups: 200 s for 15,699 members in the round-trip
  test, 1.8 s for MP_005's export); a bulk writer would be faster.
- The playable area of a generated level is the base's combat area, laid out for the base's map (TG5 places it).

## OR8: OPFOR in Rust, the leftovers - the WW2 effect porter and the sound porter (2026-11-27, branch `or8`, with Project OPFOR's `or8`)

What OR4 left of stage 4 ("OR4", "What is left"): `effectport` (the material grid's, the level builds' and the
vehicles' effects; its emitter template search) and the sound porter (Project OPFOR's `wake_level.port_sounds`).
Version **0.31.0** (removed: `effectport.EmitterIndex`, `templates_reference`, `REFERENCE`, `EMITTER_BATCH`,
`cache_dir`, `_read_many`, `port`'s `shared` / `plan`; `BFBC2_EFFECTPORT_REFERENCE`); OPFOR's `SUITE_REQUIRES` 0.31.0 -
0.32.0. Nothing was written to OPFOR's `game\`; the game never launched.

### Inventory

| Python until OR8 | now | |
|---|---|---|
| suite `effectport.py`: `walk`, `levels_of`, `level_copy`, `_effect_with_emitters`, `refs` | `bc2_convert::effectport` (`walk`, `levels_of`, `level_copy`, `effect_with_emitters`, `refs`) | ported |
| `EmitterIndex` (a pickle per game copy), `templates`, `templates_reference`, `_read_many`, `_read_parallel` | `effectport::EmitterIndex` (JSON, sorted keys, in the same `effectport` cache; `.pkl` left to the pruning), `templates` | ported, the reference scan retired |
| `port`, `port_many`, `check_defaults`, `Record.line` for the shown records | `effectport::port` / `port_many` (each effect's partitions and textures ported side by side), `check_defaults`, `Record::line` | ported |
| OPFOR `wake_level.port_sounds`, `sound_fills`, `fill_sound_instance`, `SOUND_FILLS`, `not_streaming`, `UNTEMPLATED` | `bc2_convert::soundport` (`port_sounds`, `walk`, `pool`, `fills`, `fill_instance`, `sound_fills`, `not_streaming`); waves converted side by side, 64 at a time | ported; `wake_level.port_sounds` a 6-line call of `effectport.port_sounds` with its donors |
| `audio.convert`'s `AudioError` for the report's `FAILED` lines | `wavport::convert_checked` (the reason a Wave does not convert; `convert` is it without the reason) | ported |
| `wake_level.convert_waves` (`wake_vo`'s waves) | stays | wake_vo is not part of this round |

The game data stays the caller's objects: `bc2_native.effectport_*` / `soundport_port` read a `bf1943import.Source`'s
`entries` once a call and ask its `data(key)`, and a `meshview.Index`'s (or a hiding view's, OPFOR's `WithoutOurs`)
`where` once a call and its `read(name)`; the emitter index reads the bundles itself. The dbxport rules and the
catalogue are the caller's (`dbxport._native_rules`, `_native_catalogue`), so OPFOR's table edits still apply.

### Proof

- **Twins side by side** (suite main's `effectport.py` and OPFOR main's `wake_level.py` loaded by path, against this
  worktree's; one process, the game copy read only):
  - effects: `port_many` of `bf1943_fx`'s 29 effects, the four material grids' 44 / 47 / 47 / 38, the three level
    builds' 2 / 3 / 2, `port` of the vehicles' two: **1,361 files, every report line and record (all nine fields)
    identical**; 50 `walk`s at Wake and Coral Sea and `levels_of` identical (59 of 59). Times about the Python's
    (1.2 - 3.2 s a set against 1.6 - 3.6 s: the index was on disk for both).
  - sounds: the five real `port_sounds` calls (`bf1943_fx`'s 33 roots, Wake's grid, Wake's level with `later`, the
    menu music and the stings through `WithoutOurs`): **566 files, the log lines and the returned lists identical**
    (bf1943_fx's 474 files 11.4 s against 14.9 s).
  - damaged: 20 effect cases (8 partitions cut at 0 - 200 bytes, 4 textures cut, 4 partitions with 40 bytes flipped,
    a dropped partition, a cut template, 300 cut emitter documents, a missing template) and 12 sound cases (6 waves
    cut to a third, the first donor and the last donor missing (twice each), 2 cut sound partitions): every outcome the Python's (the
    exception's type and message, or the same files and lines) but one: a missing template raises
    `MissingTemplate` naming it where the Python raised `DeletedEntry` (XWW2-FX-2).
- Suite: `bc2-convert`'s unit tests (effectport 5, soundport 6), `test_ports_native.py`'s emitter search against the
  rule written out (120 searches cold and from disk, the index read back, a damaged document, a changed bundle),
  `test_dbxport_rules.py`'s effect references. OPFOR: `test_ww2_sound_fills` re-derives the fills from DICE's
  conversions through the Rust.
- **WW2 rebuild** in OPFOR's `or8` worktree with its own `work\ww2` seeded with the 14 declared inputs (599 files;
  `game\`, the other `work\` folders and `research\ww2 models` junctions to the main checkout, read only),
  `OPFOR_SUITE_STRICT=1`, defaults: **all 55 steps in 16.0 min**, `compare_oracle` **7,945 of 7,945 identical**,
  `qa_checks` exit 0. Peaks: `bf1943_fx` 2.8 GB, `material_grid` 4.0 GB, a level chain 4.3 - 4.5 GB.
- **Install oracle**: `wake_install.py --plan` exit 0; `compare_oracle --install` **21,149 of 21,149 identical**.
- `game\`: its 1,582 files (11.6 GB) hashed (path, size, time, SHA-256) before and after: 0 changed.

### Decided

- XWW2-FX-2: a missing template is named (`MissingTemplate`), not the reader's `DeletedEntry` for an empty read.
- XWW2-SND-2: a wave that does not convert is reported with the Rust converter's reason, not the Python's
  `AudioError` text (no real wave fails; the cut waves above still convert, as the Python's did).
- Nothing else against DICE: every output and line is the Python's.

### What is left

- `wake_vo.waves` (`convert_waves` over `audio.convert_many`) still converts the voice-over in Python threads.
- The Type 95 silhouette, the DXT encoders' axes and the preview tools (OR4's list; OR14 has the first).

## SPG2-rest6: reverse engineering for the specification (2026-11-27, branch `spg2r6`)

Static reading of `BFBC2Game.exe` (PC build 795745), read only: capstone over a scratch copy of the executable (as in
rounds SPG2-rest3 - rest5; REA timed out on it before), with rest5's scratch map of the classes' field descriptors.
The undergrowth's vertex programs were read from DICE's Dx11 shader databases in memory with D3DCompiler's
disassembler; nothing of them is kept. DICE's data was read through the suite's readers. Nothing was run in the game,
and nothing was written to `game\`.

| Item | Found | Rules |
|---|---|---|
| `windParams` in the shaders (VEG-UG-9) | the 25 distinct Dx11 programs whose parameter block names `windParams` (register 9, 21 or 25; 780 programs in 74 databases, and 74 Dx9 / 74 Dx10 databases hold such programs) all add s = sin(((x + y + z) x 1000 + w.x) x w.y) x max(y, 0) x w.z to x and z in mesh space: `WindScale` metres of sway per metre of height, `UndergrowthWindSpeed` the angular frequency | VEG-UG-10 new (verified) |
| `UndergrowthMaxHeight` (VEG-UG-8) | the job 0xA0EB10 keeps each cell's lowest and highest instance base (0xA0EC12 - 0xA0EC44, the generator's 8th / 9th arguments 0xA0E7C6 - 0xA0E823) and writes the cell's box with top = highest + this (0xA0ED08 - 0xA0ED25); the draw's frustum test and distance (0xA1EB9A - 0xA1EC77); the generator never reads it | VEG-UG-11 new |
| mesh / sample-texture evaluators | mesh 0xB26830: `DistributionData`[trunc(r x n)] x v; sample texture 0xB265F0: one texel of the row baked in `GradientData`, scaled by the colour intensities; DICE: no mesh evaluators, 15 sample-texture ones, all 512 entries for 512 | FX-PROC-6 updated |
| the vertex fill (0xB27BF0 / 0xB280C0) | the batch 0xB292E0 with set-up 0xB26FB0 and per particle 0xB280C0, quads only: transparency x clamp(1.25 x depth) (a 0.8 m near fade), alpha levels, texture frame, size, the 33-entry clip-scale lookup, four 0x40-byte half-float vertices; 0xB27BF0 has no caller | FX-PROC-9 new |
| force vectors | 0xB2D7C6 - 0xB2D9BA: turbulence ±`Intensity` x dt x 30, gravity (0, -`Gravity` x dt, 0), local force -dt x `LocalForce`, air resistance v x max(-value x `DragFactor` x dt, -1) | FX-PROC-10 new |
| 0xB328C0 | the lifetime step of every emitter (0xB32730: `Lifetime` / `LifetimeFrameCount`, then removal); the mesh particles' own code is the spawn 0xB328D0 (`SpawnCompositeMeshParts`, physics bodies) | FX-PROC-11 new, FX-PROC-8 corrected (the update's sizes are the bounds') |
| FSCommand cases (MENU-RT-7) | each case of the four handlers: a message to the server (client 0x15713D8 +0x58, queue 0xC0A390), a job for the client's dispatcher, a level load (0x75DF60 / 0x956E10 / 0x75EAB0) or a UI manager field (0x19A4CA8) | MENU-RT-8 new |
| criterion objects | the listener's set-up 0x866DC0 -> per award 0x863DB0 -> the factory 0x8430E0 (0x24-byte objects, seven classes by `CriteriaType` and award ownership), armed by 0x861B90; only all-time criteria write the stats | PERS-AWD-8 new (verified) |
| event 0x51 | `StatEvent_Suicide` (the enum's 105 values; the kill handler 0x86DC90 pushes 0, 0xB, 2, 0xD, 1, 0xC, 3, 0xE, 0x51) | PERS-AWD-6 updated |
| node records | the 0x20-byte header (pairs, blend times, index, parent, weight parameter, active byte), the animation node (evaluator 0xF59F90), all 26 types' fields from their evaluators (state vtables 0x14AB8B4 - 0x14AB95C, builder 0xF69140), children as u16 indices | ATREE-NODE-2, 3, 4 new (verified) |
| blocks B and C | B: the cycle's distance (700 entries, 0.21 - 5.22) and angle (96, 45 - 90), read by 0xF492B0; C: five `PropertyCurveType` curves, each a mode word, a pointer and an interval count (0xF50720 -> 0xF59460 -> 0xF534C0 / 0xF53580, ramp 0xF507D0) | ATREE-BLK-5 new, ATREE-BLK-6 new (verified) |

Probes and checks (game tier; counts, shapes and identifiers only, nothing of EA's kept):

- `spec/probes/terrain.py`: the undergrowth programs per database and the sway's instruction shape (7.9 s);
  `core/tests/test_spec_probes.py::undergrowth_sway` checks it.
- `spec/probes/effects_chain.py`: the sample-texture gradients against `TextureDimensions`, the mesh evaluators, the
  clip-scale lookups' lengths, the fill processors by emittable type, the force fields by sign (54 s for the probe).
- `spec/probes/persistence.py`: the `StatEvent` enum and the kill handler's event numbers, the criterion factory's
  type tests and vtable stores (0.1 s).
- `spec/probes/resources.py`: 7 new checks on the node header, child references, animation flags, block C's curves
  and block B's cycle values over 1,926 trees (67 of 67 checks, 6.3 s).

Rules went from 1,587 to 1,599 (by status: verified 969, implemented 151, observed 417, inferred 62). `spec/rules.json`
is 977,029 bytes committed (LF), 23 KB under the repository's 1,000,000-byte limit.

### Playtests still pending (in-game tests are paused)

- `transforms/terrain_edit.md` section 5 (E5 / E7); ONS-CAP-3's patch; MAT-TEX-20's RenderDoc capture; BOT-GS-5;
  bot_ai.md's plan with the kill probe.
- After OPFOR's PERS-SCORE-2 fix: a round with kills whose save keeps them; a save with an award (PERS-AWD-5, 7); a
  burst of one menu event (MENU-EVT-5); `-VisualTerrain.Undergrowth*` overrides (VEG-UG-8, SET-CLI-1).
- New: a suicide and a team kill with the award log (PERS-AWD-6: the 0x51 test's +0xA8, the per-life reset); a
  settings dump for the default `UndergrowthWindSpeed` / `UndergrowthMaxHeight`, a `WindScale` 0 type standing still
  and a mesh taller than `UndergrowthMaxHeight` culled while its top is on screen (VEG-UG-10, 11); `LocalForce`'s sign
  on a DICE effect (FX-PROC-10).

### What is left (SPG2-rest7)

- The Dx9 undergrowth programs' sway (counted, not read).
- The particle shaders; other emittable types through the mesh spawn; the turbulence draws' axes; the caller's
  per-draw byte and vector; who sets the emitter parameters.
- The server message ids' names and the dispatcher's job numbers (MENU-RT-8).
- The criterion factory's callers 0x8617B0 / 0x8489A0; the 0x51 test's +0xA8.
- The anim-tree array elements of types 2, 3, 6, 7, 13, 23, the Selector's group order, the If node's expression
  record, block C's curve names, the header's pairs.
- OPFOR's fix for PERS-SCORE-2, as an OPFOR round.

## OR9: OPFOR in Rust - the Onslaught console port, stages 2 - 4 (2026-11-27, branch `or9`, with Project OPFOR's `or9`)

What OR5 left ("OR5", "What is left"): `console_port.py`'s stages 2 - 14 and the Napisal-level fixes, about 5,000
lines whose tools (DbxEdit, FbrbTool, stfs, the converters, culltree, aptgraft) are Rust already. This round writes
the plan for all of them (below) and ports its own share, stages 2 - 4. Version 0.32.0 (OPFOR's `SUITE_REQUIRES`
0.32.0 to below 0.33.0). Nothing was written to OPFOR's `game\`; the game never launched.

### The plan (OR9 - OR13)

| Round | Stages | OPFOR Python | Lines | What is hard |
|---|---|---|---|---|
| **OR9** (this) | 2 `console_linux_base`, 3 `console_mode`, 4 `console_loadset` | 310 + 324 + 294 | 928 | ElementTree's tree and `math.dist`; the closure's set order |
| OR10 | 5 `console_vehicles`, 6 `console_soldiers`, 7 `console_shaders` | 135 + 376 + 248 | 759 | the soldiers' renamed partitions (`opfor_guid('soldier')`); the shader merge through `shaderdb.merger` (Rust since OR3a) and `console_shaders\shared`, which stages 6, 7, 9 and the audit read |
| OR11 | 8 `console_markv`, 9 `console_input`, 10 `console_deps` | 389 + 314 + 249 | 952 | the Mark V's console conversion (mesh, Havok, textures: `bc2_convert`), the seat action maps, the in-memory reads of the stock bundles |
| OR12 | 11 `console_cull`, 12 `console_loading`, 13 `console_hud` | 142 + 223 + 237 | 602 | `culltree add` (Rust since O3), startup-00's per-level layers (M11), aptgraft's movie edits (Python API: OR14's area) |
| OR13 | 14 `console_icons`, `console_shader_audit`, `apply_ai_template`, `apply_combat_squads`, `apply_sector_offsets`; `console_port.py`'s driver, `console_common`'s leftovers | 403 + 509 + 340 + 215 + 267 | 1,734 | the icon's Pillow polygon fill (OR14's `type95_icon` silhouette: shared, OR14 first); the Napisal-level fixes run on Napisal's levels, which only the user's backups hold |

Each round proves its stages as OR9 did (below): the retired Python against the wrapper on the same scratch OPFOR
root, every work and game file byte for byte, on the levels the user's Xbox package and Linux server levels give,
plus damaged inputs. What stays Python at the end: the command lines, the install ledger's calls (OPFOR's
`opforcore.stages`, until OR15's one ledger) and `drop_scratch`.

### What moved

| OPFOR Python (until OR9) | Lines | now | Lines left |
|---|---|---|---|
| `console_linux_base.build` (the Linux base, the merge, `enum_cmds`, `slots`) | 310 | `bc2_install::onslaught::linux` (`bc2_native.onsport_linux`) | 92 (command line, `first_stage_source`, the install, revert) |
| `console_mode.plan` / `apply` / `drop_door_predestruction` / `mode_partition_guid` | 324 | `bc2_install::onslaught::mode` (`onsport_mode`) | 94 (command line, is-top and check before the build, the install, revert, scratch) |
| `console_loadset.plan` / `apply` / `closure_entries` / `cav_entries` / `manifest_without_mp2` | 294 | `bc2_install::onslaught::loadset` (`onsport_loadset`) | 83 (command line, the install, revert, scratch) |
| `console_common.find` / `unpack_cached` / `xml_of` / `tree_info` as the stages use them | - | `bc2_install::onslaught::port` | `console_common` keeps them for stages 5 - 14 |

Not ported, with reasons: `console_mode`'s explosive-objects branch (`EXPLOSIVES` was off: Nelson Bay crashed at load
with them; the Python never ran it) and `console_loadset.mp2_only` (a diagnostic nothing called). The stages' Python
twins are retired: OPFOR's three scripts are wrappers over `console_common.port_stage`.

### Decided (spec XCONS-ONS-2 - 4)

- **The cull slots** keep the Python's arithmetic: CPython's `math.dist` (its compensated `vector_norm`, now
  `bc2_pyrt::math`; a plain `sqrt` of the sum can be one unit off and pick another object at a tie), `round(x, 1)`
  as the position key with -0.0 equal to 0.0. The slots themselves are DICE's (the Linux server level's); the
  matching is OPFOR's own rule, so there is no DICE answer to decide it against.
- **The door distance** is Python's `** 2` and `** 0.5`: the C library's `pow`, not a multiply and `sqrt`.
- **The closure's order**: the Python visited the partitions in a set's order; the closure, and the XML files it
  writes on the way, are the same in any order, so the Rust visits in name order (no output depends on it).
- **The temporary folder** of stage 3 is named by the process id (Python's `mkdtemp` is random); it is deleted before
  the bundle is packed, and only the import line printed names it.
- XCONS-STG-1 said a level without a folder list "stops at stage 4 with the partitions to list"; no code did that:
  such a level takes the closure alone. The rule now says so.

### Proof

`scratchpad\or9\stages_parity.py`: one scratch OPFOR root (`C:\Programming\Projects\_rounds\or9\s\<level>`: the
Linux levels' paths pass Windows' 260 characters under the scratchpad) per side, run one after the other at the same
path so the paths inside the work files agree; `work\` holds copies of OPFOR main's stage 1 output, `con_level` and
the console mode partition; `game\` is junctions into OPFOR main's `game\` (read only) with the level's folder and
`package.mft` real copies; stage 2's input is the user's own stage 1 output (the legacy marker
`level-00.fbrb.opfor-linux-orig`; Nelson Bay, hand-merged in 2026-10, has none: its `.opfor-mode-orig`). The stages
install into the scratch game through OPFOR's ledger, as they do for real. The Python twins: OPFOR 0e4d0d3's three
scripts, loaded by path.

| Level (PC map) | Stage 2 input | Files compared (work and game) | Identical | Python / Rust, stages 2 - 4 |
|---|---|---|---|---|
| ONS_MP_002 (MP_002) | `.opfor-linux-orig` | 8,996 | **8,996** | 106 s / 97 s |
| ONS_MP_004 (MP_004) | `.opfor-linux-orig` | 9,184 | **9,184** | 101 s / 100 s |
| ONS_MP_005 (MP_005) | `.opfor-linux-orig` | 9,940 | **9,940** | 125 s / 118 s |
| ONS_MP_008 (MP_008) | `.opfor-mode-orig` | 15,017 | **15,017** | 91 s / 112 s |

The files are every work file of the three stages (the Linux unpack and its stamp, the control partitions and their
XML, `merge.cmd` / `merge.log` / `enum.cmd`, `ctl_ov`, the packed bundles, `console_mode\<lv>`, `console_loadset\`'s
mp2_common cache with the XML its closure wrote, the level's unpack) and the scratch game's real files (the level
bundle as each stage installed it, `package.mft`). The printed lines are the same but for stage 3's import line,
which names its temporary folder. Beyond the Python: on ONS_MP_002 the Rust's stage 2 and stage 3 bundles are byte for
byte the ones the user's own install made in October (`level-00.fbrb.opfor-mode-orig`, `.opfor-loadset-orig`).

**The manifest's edit** (stage 4 with `package.mft.opfor-loadset-orig`, the user's manifest from before a loadset
run, 146 mp2_common targets): ONS_MP_002 7,998 of 7,998, ONS_MP_008 13,901 of 13,901, ONS_MP_005 8,504 of 8,504
files identical, `package.mft` among them.

**Damaged inputs** (ONS_MP_002, `damage_case.sh`): the level bundle cut to half (stages 2, 3, 4) and to 99.5 %
(stage 3), one byte changed (stages 2, 3, 4), stage 1's level missing or cut to a third (stage 2), `package.mft`
empty or with LF line ends (stage 4), the console level missing, its cull tree missing, the console mode partition
missing (stage 3: the derived GUID): 14 of 14 cases end alike - the same files written, the same refusal with the
same command and message, or the same success. The one difference: a failing tool's "details:" line names the day's
tool log under the Python (its `dbx_run` logs) and "(no log file)" under the Rust (as OR5's in-process commands).
Neither side wrote to `game\`; OPFOR's `game\` was only read through junctions.

### What is left

- Stages 5 - 14, the shader audit, the Napisal-level fixes and the driver: OR10 - OR13 (the plan above).
- OR5's and OR9's in-process DbxEdit / FbrbTool commands write no tool log (a failure says "details: (no log
  file)"); the log writer is `bc2-cli`'s `pylog`: a shared place for it, then the stages pass the day's log.
- `bc2_pyrt::math` and `bc2_navbuild::pymath` both hold CPython's `vector_norm` (navbuild keeps its own so the
  navigation build does not link the Python-runtime emulation): one to go when navbuild may depend on bc2-pyrt.
- **Unproven until a playtest**: nothing new in game - the bundles are the Python's byte for byte (and ONS_MP_002's
  stage 2 and 3 bundles the user's installed ones), so the levels play as they did.

## SPLIT1: the specification's rule index split into shards (2026-11-27, branch `split1`)

`spec/rules.json`, written by `spec/trace.py --emit`, was 977,029 bytes as a committed blob (1,010,926 on disk with
CRLF), 23 KB under `tests/test_repo.py`'s 1,000,000-byte limit after SPG2-rest6; the next spec round would have
crossed it. Done the way this file was split on 2026-11-23: nothing lost, every rule's content unchanged.

| | before | after |
|---|---|---|
| index files | 1 (`spec/rules.json`) | 90 (`spec/rules/<area>/<file>.json`, one per spec markdown file with rules) |
| largest file | 977,029 bytes | 40,538 bytes (`spec/rules/transforms/validation.json`); all 90 together 994,730 |
| rules | 1,599: verified 969, implemented 151, observed 417, inferred 62 | the same |
| key order | id, title, file, status, variants, evidence (insertion) | sorted keys (`sort_keys=True`; `JSON_ORDER_KEPT` for `spec/trace.py` 2 -> 1) |

Layout: a shard sits at the markdown file's path under `spec/rules/` (`spec/engine/typeinfo/core.md` ->
`spec/rules/engine/typeinfo/core.json`), `{"about", "rules"}`, its rules in heading order. Per file rather than per
area: the largest area (engine) is already 385 KB of rules, the largest file 40 KB, so a shard reaches the limit only
if one markdown file grows about 25 times; a rule's diff lands in the shard named like its markdown file.

Proof:
- `python spec/trace.py --emit` then `--check`: `ok   1599 rules in 90 files: 969 verified, 151 implemented, 417
  observed, 62 inferred`, the same line as before the split.
- A one-off comparison (scratchpad `compare_union.py`, not committed): the shards' rules, concatenated in markdown-path
  order, equal the old `rules.json`'s list element for element (each rule compared as sorted-key JSON): 1,599 = 1,599,
  same order, ids unique, the same per-status counts, and every rule in the shard of its own `file`.
- `--check` fails a shard edited by hand (one line's indentation changed: "spec/rules/model/ai_orders.json is not what
  --emit writes") and a leftover file (`spec/rules/model/stray.json`: "is not a shard --emit writes"); a shard with
  CRLF line endings (a `core.autocrlf` checkout, as on this PC and CI's Windows runner) passes, and `--emit` leaves
  it untouched.
- `spec/probes/census.py --check` passes (it reads the markdown, not the index). CI runs `trace.py --check` through
  the quick tier's `spec-trace` entry (`.github/workflows/checks.yml`, `run_tests.py quick --no-game --ci-steps`),
  unchanged.

Guards against the next surprise: `trace.py --check` fails a shard over 800,000 bytes (80% of the limit: split the
markdown file); `tests/test_repo.py` prints a `warn` line, without failing, for every file of HEAD between 80% and
100% of the limit. On main today it warns of nine test fixtures (`granny_golden.json` 802,986 bytes,
`ports_golden.json` 817,885, `f0_meshdata_replay_game.json` 837,639, the five `f0_wave_replay_game_*.json` at about
901,500, `cli_o3_cull_golden_0.json` 831,948); they are fixed fixtures, not growing generated files.

Branches that re-emitted `spec/rules.json` (OR14, OR9): after merging main, `git rm spec/rules.json` (take main's
deletion) and run `python spec/trace.py --emit`; `--check` then confirms the shards.

After merging main with OR9 (da8e3d0a; its `spec/rules.json` 979,615 bytes, 1,602 rules): `git rm spec/rules.json`,
`--emit` (5 shards changed), `--check` ok with 1,602 rules (972 verified, 151 implemented, 417 observed, 62 inferred),
and the same comparison against main's `rules.json`: equal element for element, the largest shard still 40,538 bytes.

## OR14: OPFOR in Rust, the picture and menu leftovers (2026-11-27, branch `or14`, with Project OPFOR's `or14`)

Version **0.33.0** (OPFOR's `SUITE_REQUIRES` 0.33.0 to below 0.34.0). The four items of the ROADMAP row; nothing was
written to OPFOR's `game\` and the game never launched.

### 1. The Type 95 silhouette (Pillow's polygon fill)

`type95_icon.silhouette` (the WW2 rebuild's `type95_icon.py` / `weapon_icons.py`, and `console_icons.py`'s Onslaught
boats) is `bc2_convert::pictures::side` over `pictures::polygon` (Pillow 12.3's `ImageDraw.polygon` from its C
source: `(int)` vertices, `ROUND_UP` / `ROUND_DOWN` of single-precision crossings, the doubled last row, merged flat
edges), through `bc2_native.pictures_side_silhouette`. NumPy's float work as it runs on the oracle machine: `np.cross`,
`norm` as `(x^2 + y^2) + z^2`, `n @ light` as OpenBLAS's `dgemv` (two fused forms), the supersampled sums per sub-row.

- `rust/parity_silhouette.py`: **20,000 of 20,000** random polygons identical to Pillow; 300 seeded synthetic meshes:
  216 identical (5 refused by both), 84 differ only in the tie order (identical to the Python with
  `argsort(kind='stable')`); 85 game-mesh pictures (the Type 95 at 7 size / ss pairs, MRKV, JETS, 12 vehicles x 3
  sizes, 40 seeded meshes): 18 identical, 67 tie-order only, 0 other. **The Type 95 at its defaults, MRKV and JETS
  are identical to the default-argsort Python**: every picture OPFOR ships.
- Decided (XWW2-PIC-5): faces of equal depth sums are drawn in face order (a stable sort). NumPy's default `argsort`
  on AVX-512 is x86-simd-sort, unstable, its order of equal keys processor- and version-dependent; DICE drew its
  silhouettes by hand. `pictures_side_ties` bounds the pixels another order could change. A mesh of zero length along
  z or with non-finite positions, and `w`, `h` or `ss` of 0, are refused (the Python divided by zero).
- 7.1 s against 0.67 s for the 85 game pictures (1 - 4 ms a picture against 20 - 350 ms).

### 2. texwrite's principal axes (`numpy.linalg.eigh`)

`bc2_convert::eigh3` is LAPACK `dsyevd` (`UPLO='L'`) for 3 x 3 as NumPy 2.5.4 / scipy-openblas 0.3.34 computes it on
this SkylakeX-kernel machine: `dsytd2` (one reflector), `dsteqr`, `dorm2r` / `dlarf1f`; LAPACK's code unfused in
source order, the BLAS kernels' `y += a * b` fused (`dsymv`'s, `ddot`'s, `daxpy`'s tails, `dsyr2` as `daxpy`s,
`dger`, `dgemv_t`'s `m = 1` path), `dger`'s `alpha * y[j]` and `dsymv`'s `alpha * x[j]` rounded first. A Python
prototype matched 4,000 of 4,000 only with all three kernel families fused (unfused `dsymv` 832 misses, `ddot` 355,
`daxpy` 3,185). `texwrite.py`'s native path is one call (`TexWriteJob.encode()`); the reference keeps NumPy's eigh.

- `rust/parity_texwrite_axes.py`: **0 axes and 0 encoded blocks differ** on 3,000 random pictures (17,420 levels,
  279,092 blocks: DXT1, normals, 1-bit alpha, DXT5), 1,495 pictures from disk (111,346,730 blocks, each as two kinds),
  10,000,000 random covariances of texwrite's kind and 8,017 degenerate ones (zero, rank 1 / 2, repeated eigenvalues,
  2^-1070 - 2^1000, subnormal).
- Decided (XWW2-PIC-6): the oracle machine's bits are the rule (DICE's own encoder made DICE's textures); NumPy on a
  processor without FMA may differ in the last bits, the Rust does not.
- 1,000,000 axes: Rust 0.07 s (all cores) against NumPy 1.3 - 1.7 s; a 2048 x 2048 texture with 12 levels: 0.016 s
  (DXT1) / 0.027 s (DXT5) against 0.29 / 0.32 s.
- `bc2_levelwrite::awards` (the editor's award and loading-screen exports) drops its own Jacobi axis for `eigh3`.

### 3. One Pillow resize

`bc2_menu::pil::Rgba::resize_lanczos` (OR5's menu art) is a thin call of `bc2_convert::pictures::resample::resize`
(OR4's); OR5's copy (`coeffs`, `resample`, `lanczos`, `muldiv255`) is deleted, `bc2-menu` depends on `bc2-convert`.
The kept one sums in `i64` and clips where OR5's wrapped `i32` as Pillow's C does: no 8-bit picture can reach the
wrap (the Lanczos weights' absolute sum times 255 times 2^22 stays under 2^31). Proof: `rust/parity_menuart.py` (400
random cases each) **resize, cut-out, fit, layer, crop, flip, DXT5 decode / encode, untile: 3,600 of 3,600**
identical to Pillow / numpy and the retired `onslaught_art.py`; `pil.rs::pillow_answers`' recorded digests pass;
`resample.rs`' callers (`pictures::icons`, `loading`) unchanged (their tests and the WW2 rebuild below).

### 4. OPFOR's WW2 menu tools off aptgraft's and bc2loc's Python API

| OPFOR file | its aptgraft / bc2loc use | now |
|---|---|---|
| `ww2\type95_icon.py`, `onslaught\console_icons.py` | Movie.load, add_icon_frame, add_label_alias, label hiding | `bc2_native.AptMovie` (`graft::icon`) |
| `ww2\weapon_icons.py`, `duplicates.py`, `bf1943_hud_icons.py`, `qa_checks.py` | labels, places, geometry, add_label_alias | `AptMovie` |
| `ww2\wake_ui.py` | the constant-key swap | `AptMovie.rename_constants` (`graft::consts`) |
| `ww2\ww2_crosshairs.py` | crosshair_table / objects, deps, timeline_display, add_crosshair with image tiles | `AptMovie` (`graft::tiles`, `add_crosshair_tiles`) |
| `ww2\menu_install.py` | ~370 lines building code and characters on aptgraft's objects; the dump | `bc2_native.pacific_main_nav` / `main_root` (`bc2_menu::pacific`), `apt_dump_text` |
| `ww2\names.py`, `ww2_awards.py`, `wake_install.py`, `menu_install.py` | `bc2loc.LocFile`, `languages`, `load`, `khash` | `bc2_native.LocFile`, `loc_languages`, `loc_load` (`bc2_menu::loc`, moved out of `bfbc2 bc2loc`) |

`aptgraft.py`, `bc2loc.py` (and with them `aptcheck.py`, `menucommon.py`, `apt.py`) are now the Rust's test
references only (PYTHON_STATUS: twin, PR1c delete). The console readers' tests run on the Rust against answers recorded
from `stfs.py` / `xisolib.py` (`bfbc2-console-readers/tests/console_readers_answers.json`, its recorder), so PR1c can
delete those twins once `bc2-cli/src/readers/stfs.rs`'s `include_str!` usage text, `CLI_TOOLS['stfs']`,
`tests/test_cli_errors.py` and four spec citations stop naming them.

Proof:

- `rust/parity_menu_movies.py` (old: OPFOR 0e4d0d3 on main's aptgraft; new: the or14 worktrees; the real game copy
  read, every write in scratch): type95_icon **16 of 16** files, weapon_icons 5 of 5, duplicates 4 of 4, wake_ui 12 of
  12 (the crosshair movie, atlas and reported numbers among them), bf1943_hud_icons 22 of 22, qa_checks' crosshair
  check 2 of 2, console_icons (ons_mp_002, both bundles) 103 of 103; every printed line identical. Damaged inputs 32 of
  32 alike (22 refused by both with the same words); 500 random synthetic movies through add_icon_frame / add_label_alias
  and 500 random parts through split_image_part: all alike.
- `rust/parity_pacific_menu.py` (old: menu_install.py at 0e4d0d3 on this checkout's aptgraft.py and bc2loc.py): the
  menu step's movies (`work\ww2\menu_install\orig`, the ledger's bundle from before the stage) at the step's sizes and
  40 random label / logo sizes, 26 damaged movies (cut short, edited twice, a character of another type, a frame placed
  or unlabelled, the selector's labels renamed, the logo frame's character, setGameMod renamed / changed / moved,
  getGModStringExtensino missing) and the dumps: **120 of 120** alike (28 refused by both, every one an `AptError` with
  the Python's words); the whole step (`menu_install.build`: unpack, edits, atlas, DbxEdit, aptcheck, the dump diff,
  pack) old against new: **301 of 301 files identical** (the packed bundle among them), 28 of 28 printed lines; the
  texts: every language of the game copy (names.py's texts set, written and read back, every lookup), without a
  histogram, cut, magic and count damaged, random keys and texts: **121 of 121** alike. main_nav's edits: Python 72.6 s
  against Rust 16.3 s for the 84 cases; the step 64 s against 17 s.
- Two differences found and fixed on the Rust side: a movie cut short is an `AptError` with the reference's words
  (`... runs past the end of the file`, `bc2_formats::aptmodel::APT_ERROR`); `add_icon_frame` of an integer size 0
  keeps Python's `+0.0` for `-w / 2`.

### WW2 rebuild

- In OPFOR's `or14` worktree with its own `work\ww2` seeded with the 14 declared inputs (599 files; `game\`, the
  other `work\` folders and `research\ww2 models` junctions to the main checkout, read only), `OPFOR_SUITE_STRICT=1`,
  defaults: **all 55 steps in 17.1 min**, `compare_oracle` **7,945 of 7,945 identical** (every texture `texwrite`
  encodes, the icons, menus and texts these tools write among them), `qa_checks` exit 0. Peak: a level chain 4.3 GB;
  at most 4 steps at once.
- **Install oracle**: `wake_install.py --plan` exit 0; `compare_oracle --install` **21,149 of 21,149 identical**.
- `game\`: its 1,582 files (11.6 GB) hashed (path, size, time, SHA-256) before and after: 0 changed; OPFOR's main
  checkout untouched.
- `menu_install.py` is not a rebuild step: its proof is `parity_pacific_menu.py`'s whole-step run above.

### Tests

- Rust: `bc2-menu` (pil, api_tests: 4 synthetic movies), `bc2-convert` (eigh3's recorded NumPy bits, polygon,
  side), `bc2-formats`, `bc2-install`; clippy `-D warnings` on the changed crates and on each of `bc2-cli`'s 19 groups
  alone.
- Python: the suite's quick tier; OPFOR's quick tests (test_onslaught_icons, test_ww2_crosshairs against the game copy
  read only, test_rebuild_tracker); ruff and mypy on the changed Python of both repositories; `spec/trace.py --check`.
- **The rebuild tracker's side-by-side test** (coordinator's request): OPFOR's
  `test_independent_steps_run_side_by_side_and_a_failure_stops_the_run` failed on a busy PC (with about 3.5 GB free
  the runner's free-memory gate ran the two tiny steps one at a time). `bc2_install::rebuild::Plan` takes
  `free_gate` (default true; `rebuild_all.FREE_GATE`), the test runs with it off and a 64 GB budget; real rebuilds
  keep the gate. With 1.9 GB free (memory held by a helper): the old test fails ("at most 1 steps at once"), the new
  one passes.

### What is left

- PR1c: delete `aptgraft.py`, `bc2loc.py`, `aptcheck.py`, `menucommon.py`, `aptrefslib.py`, `apt.py`, `stfs.py`,
  `xisolib.py` with their tests moved to recorded answers (the console readers' are), and the four places above that
  still name `stfs.py`.
- The preview tools (`wake_preview`, `weapon_preview`) and the preview sheets (Pillow's text) stay Python.
- `attachments.py` still calls `np.linalg.eigh(c.T @ c)` (its own fit, not texwrite's).

## GM0: authoring new game modes and eras - research (2026-11-27, branch `gm0`)

Static only: a linear capstone sweep of a scratch copy of `BFBC2Game.exe` (PC build 795745; 6,575,079 instructions in
23 s, kept in the scratchpad and deleted after), targeted reading of the functions found, the extern names from the
getters' jump tables, and `spec/probes/gamemodes.py` over DICE's bundles (the Vietnam patch's startup bundle through
OPFOR's kept `startup-00.fbrb.opfor-orig`). Nothing was run; nothing was written to `game\`; battlefield-opfor and
OPFOR were read only.

| Question | Found | Rules |
|---|---|---|
| GameModeData's fields | 10 DICE partitions, 5 ModeIDs (Vietnam's reuse BFBC2's), 4 query ids; every field's readers (the loading / HUD roots, `extern.hudMovies`, `extern.gameModeID`, the catalogue, the stats category); the bundle refs have no reader the operand scan finds | GMD-DATA-1, GMD-READ-1 |
| where modes live | startup 6 + 4, `*_common` 9, main menus 10, eula 80, 15 Package mode levels; 168 deletion markers for a map's other modes | GMD-DATA-2 |
| code that switches on a mode | 26 ModeID compares (conquest 9, goldrush 2, squadrush 3, tdm 7, singleplayer 5) with what each decides; the mode-kind object at 0x15666C0 (tdm 2, squadrush 1, else 0); the query id's stats category (+ `singleplayer` 1), the "any" search's fixed four; the catalogue keyed by ModeID; SP / MP persistence templates only (`Ons_Template` shipped, never named) | GMD-SW-1 - 4, GMD-CAT-1, GMD-PERS-1, GMD-MM-1 |
| the game mod's readers | exactly 33 instructions on +0xC8C8C (2 dead, 5 stores, 20 `== 1`, 7 loads / 1 compare) and 8 on +0xC8C90, each named | ERA-MOD-1 |
| where the mod comes from | the loading screen (0x772DD0, the `_description` asset) and the HUD's start (0x775C00, LevelData) parse the level's `MatchmakingModID`; no profile setting exists (GAME-MODE-6 and OPFOR's first study corrected) | ERA-MOD-2 |
| a third era | only 0 / 1 in code; no third tag; `UISharedConfigAsset` has a fixed era pair (the Vietnam patch sets it: 2 + 26 items); not data-possible; a host layer over 0 / 1 with a 6-step patch set (native eras are the user's GM1 - GM3) | ERA-MOD-3, ERA-RES-1, ERA-RES-2, ERA-NEW-1, ERA-NEW-2 |
| licences | the setter's sign-in gate, `playerHasLicenseToNam`, level licences, `applyLicenses`, the five entitlement queries: described, not designed around | ERA-LIC-1, ERA-MM-1 |
| OPFOR / battlefield-opfor | OPFOR: Pacific levels keep Vietnam's HUD configuration and icon set; battlefield-opfor's 0x8FF330 detour returns the active era for every level | ERA-OPF-1, ERA-OPF-2 |

Probe and checks (game tier; counts, identifiers and addresses only, nothing of EA's kept): `spec/probes/gamemodes.py`
(10.6 s: the startup bundles, every bundle's table of contents for the mode partitions and markers, the exe's byte
search decoded with capstone); `core/tests/test_spec_probes.py::game_modes` checks its 19 answers.

Rules went from 1,606 to 1,630 (gamemodes.md 24: verified 10, observed 8, inferred 5, implemented 1); GAME-MODE-4, -5
and -6 and RTTI-DBX-1 corrected in place. `spec/trace.py --check`, `spec/probes/census.py --check` (CENSUS.md
rewritten for the new rules), the spec probes' test, the quick tier, ruff and mypy pass.

### Playtests still pending (in-game tests are paused)

- ERA-MOD-2 (the menu's era after a Vietnam match; a fresh start is BFBC2), ERA-OPF-1 (a Pacific level's victory
  stings and vignettes), ERA-OPF-2 (battlefield-opfor hosting another era's level), GMD-SW-2 (a new ModeID's kind 0
  on a squad layer), GMD-CAT-1 (Air Superiority's and Onslaught's menu names), GMD-NEW-2 (a mode reusing `conquest`).

### What is left (GM0)

- Queued as GM0-rest (beside the user's native-era rounds GM1 - GM3): the readers of the bundle refs and `KickIfLicenseMissing`, the mode-kind objects' virtual functions,
  the server's game-mod strings; OPFOR's era-aware HUD configuration and icon set; battlefield-opfor's parser fix.

## OR11: OPFOR in Rust - the Onslaught console port, stages 8 - 10 (2026-11-27, branch `or11`, with Project OPFOR's `or11`)

The plan's third round ("OR9", "The plan"): `console_port.py`'s stages 8 (`console_markv`, the Mark V), 9
(`console_input`, the seats' PC input) and 10 (`console_deps`, the referenced but unloaded PC partitions) in the
suite's Rust. Version 0.34.0 (OPFOR's `SUITE_REQUIRES` 0.34.0 to below 0.35.0). Nothing was written to OPFOR's
`game\`; the game never launched.

### What moved

| OPFOR Python (until OR11) | Lines | now | Lines left |
|---|---|---|---|
| `console_markv.build` / `install` / `restore_spawn` / `choose` / `console_layouts` / `pc_resource` / `MATERIALS` | 389 | `bc2_install::onslaught::markv` (`bc2_native.onsport_markv`) | 74 (command line, the ledger's check before the build, the level before stage 5, the install, revert, scratch) |
| `console_input.build` / `apply` / `role_inputs` / `seat_edits` / `seat_input` / `counterpart_xml` | 314 | `bc2_install::onslaught::input` (`onsport_input`; `onsport_role_inputs` / `onsport_seat_edits` for the shader audit) | 105 (command line, the audit's two wrappers, the install, revert, scratch) |
| `console_deps.build` / `apply` / `listing` / `refs_of` | 249 | `bc2_install::onslaught::deps` (`onsport_deps`) | 68 (command line, the install, revert, scratch) |

The tools under them were Rust already and are now called as libraries: the Havok conversion
(`bc2_convert::havok::convert`, `classes_of` over `hkclasses`: `convert.py havok`'s path), the mesh relayout
(`bc2_convert::meshconv`, `pair_by_kind` now public), the mesh set (`bc2_formats::meshdata`), the shader merge
(`bc2_formats::shaderdb::merge`, with a new `ArenaStore` for a crate without an arena of its own), DbxEdit and FbrbTool
in process. OR10's shared helpers in `onslaught::port` (`xml_of`, `drop_xml`, `write_manifest`, `manifest_lines`,
`under`) were not on main yet: private copies live in `onslaught/files.rs`, to be dropped when OR10 merges.

### Decided (spec XCONS-ONS-8 - 10)

- **Stock sources only.** Stage 8's search for the Mark V's missing textures (the user's PC bundles) and stage 10's
  partition sources skip a folder holding Project OPFOR's install record (`opfor-<name>-installed.json`: the WW2
  levels and `ww2_common`), the rule the suite's stock mesh index already uses. Those are BF1943 conversions, not
  BFBC2 data; the user's October install, on which the port was made and checked, had none.
- **A redo builds on the ledger's bundle.** Stage 8's shader databases and texture check, and stage 10's "loaded"
  set for the level's own Package bundle, read the bundle the install ledger hands the stage, not the game copy.
  The Python read the game copy, which on a redo already holds the stage's own output (stage 10 then found nothing
  missing and dropped its earlier copies).
- **The texture check per material.** The Python applied each material's renames to every texture name collected so
  far, so Main's `Humvee/HumveeWreck` went unchecked; the Rust checks each material's names with its own renames, as
  its cloned entry names them. The same textures on the user's level.
- **The ledger's check before stage 8's build**, as the Python's comment said and its code did not (it built first):
  a bundle changed outside the ledger is refused in the ledger's words instead of failing in the build's reader.
- Nothing decided for stage 9.

### Proof

`scratchpad\or11\parity.py`: one scratch OPFOR root per level (`C:\Programming\Projects\_rounds\or11\s\<level>`),
the retired Python (OPFOR 5f6fdac's scripts, a snapshot) and the Rust wrappers run one after the other at the same
path; `work\` holds copies of OPFOR main's `con_level` (and ONS_MP_002's `con_mp2`), the T-90 partition stage 9 reads
from `console_shaders\shared` and the mesh index; `game\` is junctions into OPFOR main's `game\` (read only) with the
level's folder, `package.mft` and `BFBC2Game.exe` real copies. The input is the Python's own output of the earlier
stages, as the user's install left it: ONS_MP_002 from its `.opfor-markv-orig` (stage 7's bundle, with the level before
stage 5 in the scratch ledger for the spawn), ONS_MP_005 from `.opfor-input-orig`, ONS_MP_004 and ONS_MP_008 from
`.opfor-deps-orig` (the Mark V is Valparaiso's alone, so stage 8 runs there only). The stages install into the
scratch game through OPFOR's ledger, as they do for real, with `--keep` so their work folders are compared.

Stages 8 - 10 chained, the scratch game as the user's October install (no WW2 levels):

| Level (PC map) | Stages | Files compared (work and game) | Identical | Python / Rust, per stage |
|---|---|---|---|---|
| ONS_MP_002 (MP_002) | 8, 9, 10 | 12,342 | **12,342** | 19.2 / 10.2 s, 8.8 / 8.5 s, 52.6 / 39.3 s |
| ONS_MP_004 (MP_004) | 9, 10 | 3,264 | **3,264** | 4.4 / 3.4 s, 52.2 / 42.5 s |
| ONS_MP_005 (MP_005) | 9, 10 | 4,035 | **4,035** | 11.3 / 8.7 s, 49.2 / 36.9 s |
| ONS_MP_008 (MP_008) | 9, 10 | 9,254 | **9,254** | 6.0 / 3.5 s, 67.4 / 46.0 s |

**With the WW2 levels installed** (the scratch game as OPFOR's `game\` is today: Coral Sea, Guadalcanal, Iwo Jima,
Wake Island and `ww2_common` junctioned in), stages 9 - 10: the Rust's files are its stock run's, every one (ONS_MP_004
3,264, ONS_MP_005 4,035, ONS_MP_008 9,254 of the same SHA-256); the retired Python's stage 10 copied the WW2 levels'
partitions into the Onslaught levels (the decision above, XCONS-ONS-10):

| Level | Python: files copied in (from WW2 levels) | Rust: files copied in | Python only / differ |
|---|---|---|---|
| ONS_MP_004 | 827 (coral_sea 519, guadal_canal 131, wake_island_s 20, iwo_jima_s 6) | 54 | 773 / 5 |
| ONS_MP_005 | 766 (coral_sea 526, guadal_canal 132, wake_island_s 11) | 0 | 768 / 2 |
| ONS_MP_008 | 837 (coral_sea 502, guadal_canal 122, iwo_jima_s 8, wake_island_s 2) | 108 | 729 / 5 |

Among them BF1943's Thompson and the WW2 builds of BFBC2's hand-held weapon animations (`at_mine`, `handgrenade`,
`knife`, `defibrillator` 1P sets: found nowhere on the stock install, which the levels loaded without), and two
shed Havok resources taken from a WW2 level's converted copy instead of sp_005's.

The files are every work file of the three stages (the boat's converted resources, unpacks with their XML, the
pre-stage-5 unpack, `spawn.cmd`, the merged shader databases, the packed bundles; stage 9's seat partitions and
imports; stage 10's unpack and copies) and the scratch game's real files (each level bundle as the stages installed
it, the ledger's blobs). The printed lines are the same.

**Damaged inputs** (`parity.py damaged`): 21 cases, each run by both sides on a fresh scratch root, end alike - the
same files written (or none), the same printed lines, the same exit code: ONS_MP_002 stage 8 with the level bundle cut
to half or one byte changed, the Mark V's LOD 0 mesh data missing or cut to 1 %, its Havok resource cut to 30 %, the
console level's manifest missing, `BFBC2Game.exe` cut to 5 % (no hkClass layouts); stage 10 with the bundle cut to half
or one byte changed; ONS_MP_004 stage 9 without the T-90 partition, with the bundle cut to half or one byte changed,
and stage 10 with the bundle cut to half. The error's last line: the same words, with the Rust's exception class
`ValueError` (or `OSError`) in front of the Python's own (`ValueError: KeyError: '...'`, `ValueError: ConvertError:
...`); a failing FbrbTool names the day's tool log under the Python and "(no log file)" under the in-process Rust
(OR9's open item). Stage 8's bundle cut or changed: the Python failed in its build's reader (a damaged bundle, a
shader database it does not read), the Rust is refused by the ledger before the build ("changed outside the install
ledger since vehicles:ons_mp_002 installed it"), the decision above; neither writes to `game\`.

`game\`'s registration files (`Package` and `Dist\win32`'s top-level files: `package.mft`, `bundleManifest`, the
overlays and their markers) and the four console levels' `level-00.fbrb` have the same SHA-256 and times before and
after the runs.

### What is left

- OR10's helpers: when OR10 merges, `onslaught/files.rs`'s copies of `xml_of`, `drop_xml`, `manifest_lines`,
  `write_manifest` and `under` go in favour of `onslaught::port`'s.
- The in-process tools still write no tool log (a failing stage 10 says "details: (no log file)", the Python named
  the day's FbrbTool log): OR9's open item.
- Stages 11 - 14, the shader audit, the Napisal-level fixes and the driver: OR12, OR13.
- **Unproven until a playtest**: nothing new in game on the stock install - the bundles are the Python's byte for
  byte. With the WW2 levels installed, a rerun of stages 8 / 10 now differs from what the Python would have built
  there (stock sources only), by design.

## GM1: native eras 2 and 3 - a decision for every reading site (2026-11-27, branch `gm1`)

The user's ruling (2026-11-27): OPFOR's eras are native engine eras (0 BFBC2, 1 Vietnam, 2 BF1943 / Pacific War,
3 BFBC1), not a mapping onto DICE's 0 / 1. Round GM1 decided each reader of the game mod, so that round GM2 can patch
with no guesswork.

**Method.** Static only:

- capstone over a scratch copy of `BFBC2Game.exe` (PC build 795745), function by function from GM0's 33 + 8 sites;
- the extern names from the getters' jump tables (HUD getter 0x777B30 added);
- the suite's APT dump of DICE's 22 Package main-menu movies and OPFOR's hudlayer_shared / mp_objectives dumps.

Nothing was run or built. Nothing was written to `game\`. battlefield-opfor and OPFOR were read only.

| Question | Found | Rules |
|---|---|---|
| tags and labels | `PACIFIC` (7) and `BC1` (3) fit the engine's fixed-length `VIETNAM` / `BC2` string builders, so each tag site needs a pointer only; labels `BFBC2143` and DICE's `BFBC2Beach` (four-frame sprites 466 / 477 have it) | ERA-NAT-1 |
| the parser | 0x8FF330: two case-insensitive compares, 49 bytes; a detour returning 0 - 3 from the level's tag (battlefield-opfor's returns the active era) | ERA-NAT-3 |
| the setter | today a non-zero mod without a session stores requested **1** and posts the sign-in: eras 2 / 3 would never switch offline and would later re-apply as Vietnam; a 15-byte cave keeps era 1's paths exactly; label `cmp ebp, 4; jae` plus a push thunk; redeem pop-up untouched | ERA-NAT-4, ERA-NAT-5 |
| the 41 instructions | each with function, callers, control, test, decision for eras 2 / 3 and patch range (45 ranges, all on instruction boundaries) | ERA-SITE-1 |
| named resources | menu config, HUD config, tips, icon set: four-entry name tables, era 1 = DICE's pointers; missing assets are not safe (the HUD start aborts on a failed icon set) | ERA-SITE-2, ERA-RISK-1 |
| the one-slot resources | the unlock grid's +0x134 / +0x138 are `UISharedConfigAsset`+0xB0 / +0x90 (constructor 0x946330); weapon maps built once by 0x9381F0; design: a typed `UISharedConfigAsset` clone per era returned by host getters at the slot reads only (swap and code-cave data rejected) | ERA-SITE-3 |
| tags | `extern.multiplayerMaps` and the two server-search cases: branch thunks (eras 0 / 3 take the `BC2` shape, 1 / 2 the `VIETNAM` one) and pointer thunks | ERA-SITE-4 |
| new-item tracker | tags 1 BFBC2-only, 2 Vietnam-only: era + 1 generalises; the tag's writer not found (open) | ERA-SITE-5 |
| left as they are | `playerHasLicenseToNam`, the burn sites, the collectible fallback, the copies, the dead levelList tag, the raw `extern.gameMod` | ERA-SITE-6 |
| the menus | main_nav's four-era frames; only sprite 471 tests 2 / 3 (both = origin); 32 main-menu and 17 HUD reads, every other one `== 1`; the contract for UI0 - UI7 | ERA-UI-1 - 3 |
| licences | 10 Vietnam check ranges kept and digest-checked, no patch overlapping; eras 2 / 3 ungated by data (the WW2 descriptions drop `nam`) | ERA-LIC-2, ERA-LIC-3 |
| risks | boolean sites, no array indexed by the mod, the server's +0xB8 / +0x118 strings, saved state, what battlefield-opfor and OPFOR must change | ERA-RISK-1 - 5 |

GM0's ERA-MOD-1 named the wrong function for 0x93D14F: it is in 0x93D120, reached from `extern.playerStats` through
0x944D50 (corrected in place).

**Probe and checks** (game tier; addresses and digests only, nothing of EA's kept). `spec/probes/gamemodes.py` gained
`PATCH_SITES` (45) and `KEEP_SITES` (10), each a sha256 prefix, a boundary check and the overlap check: 5.3 s for the
whole probe. `core/tests/test_spec_probes.py::game_modes` now checks 24 answers.

**Rules** went from 1,633 to 1,654: eras.md has 21 (8 verified, 4 observed, 9 inferred). These pass:
`spec/trace.py --check`, `spec/probes/census.py --check` (CENSUS.md rewritten), the game_modes step, the quick tier,
ruff and mypy.

### Playtests still pending (in-game tests are paused)

- ERA-NAT-3: levels of three eras in one session.
- ERA-NAT-4: switching to 2 / 3 offline, era 1 with and without `[patches] vietnam`.
- ERA-NAT-5: the labels reaching DICE's frames.
- ERA-SITE-2 / 3 / 4: each era's resources, era asset and level filter.
- ERA-UI-2: the main menu after a level of each era.
- ERA-RISK-3: the in-process server's level list and its +0xB8 string.

### What is left (GM1)

- For GM2, first: the new-item tracker's tag writer, and 0x9381F0's keys for the Vietnam weapon maps.
- For GM0-rest: the server's mod strings.
- Whether a lookup of a missing asset returns a null handle.
## OR12: OPFOR in Rust - the Onslaught console port, stages 11 - 13 (2026-11-27, branch `or12`, with Project OPFOR's `or12`)

OR12's share of the plan ("OR9", "The plan"): `console_cull`, `console_loading` and `console_hud`, 602 lines of OPFOR
Python, now `bc2_install::onslaught::{cull, loading, hud}` through `bc2_native.onsport_cull` / `onsport_loading` /
`onsport_hud` / `onsport_hud_movie`. Version **0.35.0** (OPFOR's `SUITE_REQUIRES` 0.35.0 to below 0.36.0; renumbered at merge, OR12 landed before OR10).
Nothing was written to OPFOR's `game\`; the game never launched.

### What moved

| OPFOR Python (until OR12) | Lines | now | Lines left |
|---|---|---|---|
| `console_cull.build` / `apply`'s copy, manifest sizes and pack (`run`, `sha`, `REF_DIRS`, `MISPLACED`) | 142 | `bc2_install::onslaught::cull` (`onsport_cull`, two calls: the build, then after the ledger's check the pack) | 92 (command line, the check, the install, revert, scratch) |
| `console_loading.console_setup` / `add_entry` / `retarget` / `build` up to the ledger | 223 | `bc2_install::onslaught::loading` (`onsport_loading`) | 106 (command line, the install, the per-level startup layers and their reverts, scratch) |
| `console_hud.build` / `unpatched` / `legacy_record` / `entry_record` / `patch_movie`, `PATCHES`, `LOADING_SCREEN` | 237 | `bc2_install::onslaught::hud` (`onsport_hud`, `onsport_hud_movie`) | 128 (command line, the check, each bundle's source and prior from the ledger, the install, then the kept originals and entry records, revert, scratch) |

The stages use only what OR9 put on main in `port.rs` (`find`, `read_text_utf8`, `write_text_utf8`, `StageOut`);
OR10's additions there are not needed; DbxEdit's `xml_of` is a private copy in `loading.rs` (it switches to a shared
one if OR10 lands one first). Cargo: the `onslaught` feature
is OR10's line plus `dep:bc2-menu` (the coordinator resolves that one-line conflict).

**Why `culltree add` stays a subprocess.** The cull tool's library is `bc2_levelwrite::culltool`; bc2-levelwrite
depends on bc2-install, so bc2-install cannot call it in process without a dependency cycle. The stage runs the
command line OPFOR passes (`console_common.cli_command('culltree')`, i.e. `bin\bfbc2.exe culltree add ...`), as the
Python did. Moving the culltool library below bc2-install (or the stage above bc2-levelwrite) would end that.

**The HUD's movie edits in process.** The Python ran `bfbc2 aptgraft onslaught-timer / mode-case / mode-alias` for each
of the 7 steps; the Rust calls the same library (`bc2_menu::graft::edit`, OR14's `AptMovie` model) and writes the movie
with `GRAFT_LAYOUT`, so the movies are the same bytes; each step's printed line is the one the command printed last.

### Decided (spec XCONS-ONS-11 - 13)

- **culltree's report** is read as UTF-8 (the Python's `text=True` read it in the ANSI code page): a difference only for
  a non-ASCII blueprint name, and DICE's names are ASCII (every name in the four levels' trees is).
- **A movie the Rust model cannot read** refuses with the model's words. `bfbc2 aptgraft` handed such a movie to the
  retired `aptgraft.py`; OR14 made OPFOR's movie tools `bc2_native.AptMovie`, and every movie DICE ships (the PC HUD,
  objectives, root, loading screens and the console HUD) reads in the model.
- **A failing HUD step** names its command (`aptgraft <command> ...`) and the model's message, not the binary's path,
  its "error:" line and its log's path (the damaged console HUD cases below).
- The entry record's JSON is `json.dumps(indent=1)`'s text, in the order the entries are patched (the Python's dict
  order).

### Proof

`scratchpad\or12\stages_parity.py` (+ `pair2.ps1`): one scratch OPFOR root (`C:\Programming\Projects\_rounds\or12\s\r`)
per side, run one after the other at the same path; `game\` is junctions into OPFOR main's `game\` (read only) with the
level's folder and `startup-00.fbrb` real copies taken from the user's own pre-stage files (`level-00` from
`.opfor-cull-orig`, `loader-00` from `.opfor-loading-orig`, `ingame-00` from `.opfor-hud-orig`, `startup-00` from the
earliest kept copy that holds the level's description) - the earlier stages' output, as the Python made it; `work\`
holds a copy of `con_level` and hard links of the console loader, the console HUD movie and (junctions) the Linux level
and `work\levels`. The Python twins: OPFOR a3fd230's three scripts, loaded by path. Steps, chained: cull, loading, hud
`--apply`; the three again (nothing to add / already applied / the HUD's redo on its entry record, built on the
ledger's prior and its kept originals); then the main menu's loading screen (`--loading-screen`). The stages install
into the scratch game through OPFOR's ledger, as they do for real.

| Level (PC map) | Files compared (work and game) | Identical | Printed lines | Python / Rust, the seven steps |
|---|---|---|---|---|
| ONS_MP_002 (MP_002) | 3,445 | **3,445** | same | 21 s / 21 s |
| ONS_MP_004 (MP_004) | 3,596 | **3,596** | same | 26 s / 25 s |
| ONS_MP_005 (MP_005) | 4,256 | **4,256** | same | 26 s / 27 s |
| ONS_MP_008 (MP_008) | 9,498 | **9,498** | same | 28 s / 24 s |

(ONS_MP_005's description is not in `startup-00.fbrb.opfor-loading-orig`: the level was imported after the first
loading install; its run starts from `.opfor-wake-orig`, the earliest kept copy that has it.) The time is the stage
tools' own (`culltree add`, the unpacks and packs); the movie edits are milliseconds either way.

Every work file (the unpacks, `add_report.txt`, `tree.dbx` / `level.dbx`, the console loader's unpack, the retargeted
partitions, the movie steps' `.res`, the kept originals `<name>.orig\`, the packed bundles) and the scratch game's real
files (`level-00`, `loader-00`, `ingame-00`, `startup-00` as installed, the entry records); the printed lines of every
step identical.

**Damaged inputs** (ONS_MP_002; the first 14 on the build from before `cargo fmt` reflowed one test): 19 cases end
alike - the same files written (2,216 to 3,451 files a case, all identical) and the same printed lines, the same refusal, or the same success:

- the level bundle cut to half / one byte changed (stage 11's unpack refuses), `loader-00` cut to half and `startup-00`
  cut to half (stage 12's unpack), the console loader missing (stage 12 falls back to the u3 extract and its unpack
  refuses, as the Python) or cut to half, `ingame-00` cut to half / one byte changed (stage 13's unpack);
- the console level's XML missing (stage 12 writes it first, as the Python), the console level naming no
  `LoadingMovie` (refused), no Linux reference level (`culltree` runs without `--ref`), no `work\levels` (without
  `--ref-dir`): all identical;
- the console HUD movie cut to half or missing: stage 13 refuses in the same step on both sides;
- the HUD's entry record exercised: a redo after the kept original was deleted or changed (the redo builds on the
  ledger's prior, so both succeed alike), and with a later stage's layer on `ingame-00` a `--plan` that reads the
  record: with the kept original intact it is put back first; deleted or changed, both refuse "not patched twice".

The two kinds of difference are the words of a failure, not files: a failing FbrbTool / DbxEdit command's "details:"
line names the day's tool log under the Python and "(no log file)" under the Rust (as OR9), and a failing HUD step
names `aptgraft <command> ...` and the model's message where the Python printed the binary's path, "error: AptError:
...", "what to do" and the log (spec XCONS-ONS-13). Neither side wrote to `game\`; OPFOR's `game\` was only read
through junctions: no file under it is dated 2026-11-27 (the registration files `package.mft` 2026-11-14, the
bundle manifests and `startup-00` untouched).

### What is left

- Stages 5 - 7 (OR10), stage 14, the shader audit, the Napisal-level fixes and the driver (OR13).
- `culltree add` in process: needs the cull tool's library below bc2-install (the cycle above).
- The private copies of OR10's `port.rs` helpers in `cull.rs` / `loading.rs` / `hud.rs`, once OR10 is on main.
- The in-process FbrbTool / DbxEdit commands still write no tool log ("details: (no log file)", as OR9).
- **Unproven until a playtest**: nothing new in game - the bundles are the Python's byte for byte.

## DC0: the decompiled database of BFBC2Game.exe (2026-11-27, branch `dc0`)

DC0 builds a Ghidra database of `BFBC2Game.exe`: PC build 795745, 32-bit x86, 19,322,816 bytes, SHA-1
`a48f7aea8bc0ec9087ea3efb91d5f41df4a3f6f9`. The database has:

- MSVC RTTI and Frostbite's reflection as classes;
- the spec's typeinfo records as types;
- the spec's and Project OPFOR's named addresses;
- every function decompiled to pseudo-C;
- an SQLite index and a lookup tool.

**The database is EA-derived and private.** It is in `C:\Programming\Projects\bfbc2-decomp-private\` (or
`BFBC2_DECOMP_DB`), outside every repo; `run_dc.py` refuses a folder inside a git checkout. Only `tools/decomp/` is
committed: the scripts, generators and lookup, with no decompiled code, strings or Ghidra output. Use is documented
in `tools/decomp/README.md`; how rules cite it is in `spec/README.md`, "The decompiled database".

### What was done

**Analysis.**
- Ghidra 12.1.4 headless, default analysers: x86 PE, MSVC RTTI, the decompiler's switch analysis, constant
  references, non-returning functions.
- 1,273 s at a 6 GB heap. The later passes ran at 9 GB, then 7 GB: free RAM fell to about 2.4 GB while the other
  rounds built.
- Result: 59,873 functions. Project and output total about 1.2 GB. C: stayed at 39 - 41 GB free.

**Classes** (`ApplyTypes.java`).

- MSVC RTTI gives only 282 vftables (Kynapse `Kaim::`, Apt, the C++ runtime): the engine is built without RTTI.
- Frostbite's reflection gives the rest:
  - the registration calls (RTTI-REG-1) map 1,986 class infos to their record names;
  - a byte scan finds 3,639 `getType` bodies, `mov eax, <class info>; ret` (RTTI-FAC-1);
  - the aligned data words pointing at them are vtable slots, and each vtable starts at the slot a constructor's
    code references: 1,609 vtables.
- Across all 1,891 vtables:
  - 25,343 slots read;
  - 4,801 functions made where a slot pointed at code with none;
  - 11,893 functions placed in their class, all as `__thiscall`. A function is placed when it is in one class's
    vtables, or in several of which one is the others' typeinfo ancestor (512), or when it stores only that class's
    vftables (constructors and destructors, the most derived when several).

**Types** (`gen_types.py` and `ApplyTypes.java`).

- `spec/typeinfo.json`'s records become 2,248 Ghidra types:
  - 2,017 structures and 231 enums;
  - the arrays as `Array<T>` (16 bytes) or `RefArray<T>` (20 bytes);
  - 8,405 members placed, 0 skipped.
- 1,335 types match a class of the same name, so the `this` of that class's functions has its layout: 9,672
  functions.

**Names** (`extract_names.py` and `ApplyNames.java`).

- 3,323 distinct in-image addresses from 5,333 citations:

  | Source | Citations |
  |---|---|
  | the spec's markdown, each with its rule id | 5,057 |
  | OPFOR's `research\BFBC2_DECOMP_RESEARCH.md` | 176 |
  | the hook tables (OPFOR's `src\dll\bc2.h`, battlefield-opfor's `bc2_795745` constants) | 53 |
  | the crash reader's `symbols_795745.txt` | 44 |
  | OPFOR's old `research\ghidra_project` | 3 |

  The old Ghidra project held no user names: 2,926 of its 2,928 symbols were imported RTTI and export labels,
  which the new analysis makes itself.
- 936 of the addresses have a name. A declared name beats words picked from a sentence; among declared names the
  spec wins. There are 7 conflicts, listed in the private `names.json`, for example 0x77BC10 `main_menu_command_dispatcher`
  against the hooks' `MainMenuDispatcher` and 0x951E00 `offline_host_request_hostMultiplayerLocal` against
  `ProcessStartRequests`.
- Where the addresses landed, at apply time:

  | Landed | Addresses |
  |---|---|
  | a function's entry (359 renamed, the rest already named by export or RTTI) | 1,089 |
  | inside a function | 1,091 |
  | data | 455 |
  | code in no function (160 were later in orphan functions) | 354 |
  | undefined bytes in code (43 later in discovered functions) | 129 |
  | undefined data | 137 |
  | inside an instruction | 68 |

  Every address carries a `[DC0]` comment with its rule ids.

**Function discovery** (`DiscoverFunctions.java`, then `ConfirmFunctions.java`). Two passes over what analysis left
outside functions in `.text`, skipping 0x0124F000 - 0x012BFFFF:

- 6,083 **orphan** functions, made at the first instruction of code that was disassembled but in no function;
- 27,369 **prologue** functions, made at 16-byte boundaries after padding or a return whose bytes start like an
  MSVC prologue. 27,371 were tried; 2 were rejected because their flow met a bad instruction.

Each function's `found` (`analysis` / `orphan` / `prologue`) is in the index.

The prologue functions, and every function in the skip range, are **doubtful**. A doubtful function is confirmed
only by an independent reference to its entry:
- a call or jump from a trusted function (repeated to a fixed point);
- disassembled code;
- a data reference, or an aligned data word holding its address;
- an export.

| Doubtful | Confirmed | Unconfirmed |
|---|---|---|
| prologue | 319 | 27,050 |
| skip range | 523 | 29 |

Unconfirmed functions keep their pseudo-C but are left out of `--callers` / `--callees` unless `--all` is passed.

**Hand check** of 50 random unconfirmed prologue functions (seed 42), disassembled by capstone:

- All 50 are well-formed MSVC code ending in `ret`: accessors, comparators, `memmove` wrappers through the IAT
  (0x1405510), EASTL allocator asserts, small loops.
- None is a string table, a jump table or float data misread as code: a false-positive rate of 0 / 50 (95 % upper
  bound about 6 %).
- They read as real functions that nothing references: dead template instantiations the linker kept. That reading
  is inferred; DC0 did not prove it.

**The skip range.** Its lower part, roughly 0x0124F000 - 0x0129FFFF, is obfuscated code (opaque `jo` / `jno` pairs,
`cpuid`, paired `xchg`) and reads as the executable's protection layer. DC0 decompiled it as it decompiles
everything, then analysed it no further. Under the copy-protection ruling no later round may. Its upper part
(0x012A0000 - 0x012BFFFF) reads as ordinary library code.

**Decompile** (`ExportDb.java`, 4 decompiler processes).

- 98,130 functions in two range exports:
  - 0x00400000 - 0x0124FFFF at 90 s per function: 89,640 functions in 700 s;
  - 0x01250000 - end at 4 s: 8,490 functions in 152 s. The skip range times the decompiler out.
- Results: 97,600 decompiled, 413 thunks, 113 timeouts (110 of them in the skip range), 4 failures.
- Output: 78 MB of pseudo-C in 64 KiB files, and a 76 MB index folder.

### Measure

| | Count | Share |
|---|---|---|
| functions | 98,130 | 64,678 analysis, 6,083 orphan, 27,369 prologue |
| trusted (not doubtful, or confirmed) | 71,051 | 72.4 % |
| named (not `FUN_`, by source) | 1,774 | 1.8 % (2.5 % of trusted) |
| in a class | 11,893 | 12.1 % (16.7 % of trusted) |
| `this` typed with a typeinfo layout | 9,672 | 9.9 % |
| signature set (not Ghidra's default) | 596 | 0.6 % |
| executable bytes | 16,793,600 | |
| bytes in functions | 12,934,098 | 77.0 % (analysis alone: 11,305,714, 67.3 %) |
| bytes in trusted functions | 11,829,497 | 70.4 % |

**Gaps** (RCX0's inputs; the private `index\gaps.jsonl` lists each one). All figures are for executable memory
outside every function.

| Gap | `.text` outside the skip range | skip range | `ctr` / `CONST` (static initialisers) | total |
|---|---|---|---|---|
| regions | 10,999 | 608 | 726 | 12,333 with code or 64+ undefined bytes; 101,951 in all |
| instructions | 202,140 | 12,992 | 16,146 | 231,278 (729,574 bytes) |
| undefined, non-padding bytes | 1,650,613 | 290,637 | 107,036 | 2,520,248 (10,011 regions of 64+) |
| padding bytes (`CC` / `90` / `00`) | | | | 556,591 |
| defined data bytes | | | | 53,191 |
| computed jumps unresolved | 1,182 | 268 | 0 | 1,450 of 2,584 (892 `jmp reg`, 556 `jmp [mem]`; 515 in no function) |
| computed calls unresolved | | | | 55,487 of 72,429 (virtual and function-pointer calls; 300,346 calls in all) |

Ghidra's error bookmarks: 515.

### For RCX0 (x64 recompilation)

- **Trust only the trusted set.** That is 71,051 functions and 70.4 % of executable bytes. The 27,050 unconfirmed
  prologue functions are real code that nothing references; lift them only if something reaches them at run time.
- **The 1,450 unresolved computed jumps** need switch recovery or run-time tables. Most are in `.text`: 892 are
  `jmp reg` (tail calls through registers, or switches Ghidra could not bound), 556 `jmp [mem]`.
- **The 55,487 unresolved computed calls** are mostly virtual calls. The 1,891 vtables (25,343 slots) give their
  target sets per class.
- **202,140 instructions in no function** in `.text`, and **1.65 MB of undefined non-padding bytes**. The largest
  regions are listed in `measure.json`, for example 0x00A74E21 - 0x00A78ACF (15.5 KB) and 0x00F4BF24 - 0x00F4DF3F.
  Each needs a decision: code reached at run time, or data.
- **The skip range stays out of scope** (copy protection).
- **The decompile failures**: 113 timeouts and 4 failures, listed in the index with their status.

### Spec addresses that look wrong (reported, not changed)

68 cited addresses lie inside an instruction:

- 41 are the operand bytes of an instruction (`push imm32` + 1 and the like), a citation style rather than an
  error. `spec/README.md` now asks for instruction starts.
- 3 are file offsets, named as such: MAT-TEX-6 and VEG-UG-3.
- The rest are range ends, scan bounds, or addresses in the skip range.

Three cite a function where the address is mid-function and mid-instruction:

- **ERA-LIC-1** (`spec/engine/gamemodes.md:359`): "the persistent-weapon checks 0x652380 / 0x6524C0". Both lie
  inside 0x652290 (385 bytes) and 0x652420 (248 bytes). Those two functions, both called from 0x662560, are the
  likely intent.
- **`spec/formats/animtree.md:165`**: "The run-time tree (0xF69140)". It lies inside an instruction of 0xF69030
  (2,030 bytes).

No rule was changed; the owning rounds should check these.

### Tests

- `tests/test_decomp_tools.py` (quick tier, `decomp-tools`; synthetic input only): names from spec text,
  precedence, the type plan, and the lookup on a small built index, including unconfirmed functions and `--all`.
- `ruff` and `mypy` are clean on `tools/`, which is now in `tests/test_lint.py` and `tests/test_types.py`.
- `spec/trace.py --check`: ok.

### What is left

- Names cover only 1.8 % of functions. Next steps: more of the reflection (the member tables' getters and setters,
  the factories' creators by class), `EASTL` / CRT signatures (Ghidra's Function ID found few), and the hook tables
  of the WW2 DLL.
- Signatures are mostly Ghidra's defaults: 596 set. Decompiler parameter ID was not run, being too slow for 98k
  functions.
- `RecoverClassesFromRTTIScript` was not run. DC0's own vtable pass covers the reflected classes; multiple
  inheritance (`vftable{for X}`) is only placed for RTTI classes.
- The 27,050 unconfirmed functions: a run-time trace (RCX0's harness) would settle which are reached.
## GM4: N native eras - every limit on the era count, and a host era table (2026-11-27, branch `gm4`)

GM1 settled four native eras (0 BFBC2, 1 Vietnam, 2 Pacific War, 3 BFBC1). GM4 asked what stops a fifth, and how GM2's
patch set can serve any number of eras from a host-owned table.

**Method.** Static only:

- capstone over a scratch copy of `BFBC2Game.exe` (PC build 795745);
- `bfbc2 apt dump` of DICE's 22 Package main-menu movies (OPFOR's `work/aptgraft` copy) and OPFOR's hudlayer_shared /
  mp_objectives dumps;
- `bfbc2 aptrefs` for extern ids and cases.

Nothing was run, launched or built. Nothing was written to `game\`. DC0's database answered only a name lookup: its
caller and pseudo-C queries failed while it re-exported, so callers came from a scan of the image for `call` / `jmp` /
pointer targets.

| Question | Found | Rules |
|---|---|---|
| the field | +0xC8C8C / +0xC8C90 are dwords (11 `mov dword` stores); no array indexed by the value; only two imm8 sites would carry N (the tracker bound, GM1's label test), so N <= 125 | ERA-N-1, ERA-N-4 |
| every limit | the parser (2 tags), the setter's labels (2), the gate, the 20 `== 1` sites, `UISharedConfigAsset`'s two slot pairs, the weapon maps and unlock grid (2 each), the tag builders (3 / 7 bytes), the menu tracker (4 pages), the launcher pipe (0 / 1), the online strings, the movies (2 - 4 frames) | ERA-N-1 |
| the new-item tracker (GM1's open question) | the key is a `UIMenuTrackerPage`: Awards 0, MenuUnlocks 1, MenuUnlocksVietnam 2, Store 3, Count 4. The writer is 0x9573F0 (pages 0 / 1 / 2 only); the loader 0x957140 reads pages 0 - 3 (`cmp eax, 4` at 0x9573D1); the saver 0x932CB0 writes the `GstMenuTracker` profile entries. GM1's "era + 1" would put era 2 on the Store page and era 3 on a page never read back; the rule is page k + 2 | ERA-N-2 |
| tag lengths | the builders allocate 4 / 8 and copy 3 / 7 by immediates; a 3- or 7-character tag needs a pointer only, any other length a builder cave (0x942266 - 0x942320) | ERA-N-5 |
| a third setter caller | UI message 0x38 comes from the launcher pipe `\\.\pipe\rome_snowroller`: `expansion` = `bfbc2vietnam` -> 1, else 0 (0x5B5108) | ERA-N-6 |
| dead ends | the tag helper 0x9201A0 (no caller), `extern.seenMenu` (a page parameter, no movie), `extern.fillColor` (constant orange), the `BFBC2` compare at 0x662409 (an entitlement group) | ERA-N-7 |
| the design | a host table, one row per era (tag, label, the four names, shared config, weapon map, tracker page, sign-in and redeem policy); GM2's thunks index it; online search left DICE's | ERA-N-8, ERA-N-9, ERA-N-13 |
| era 4's cost | on a table-built GM2: one row, N = 5 (tracker bound 7), the era's GM3-style data set and the menu frames; on a hard-coded GM2: about 15 sites change | ERA-N-10 |
| labels | the engine never parses a label back (the only references are the setter's pushes; `setGameMod` is ActionScript only); the movies match it as frame labels, in three `"BFBC2Nam"` compares and in the empty-label fallback; no string operation on it, so no prefix or length matters; a label is a key, renaming the visible name is art and text | ERA-N-11 |
| a fifth frame | 17 sprites carry era frames (main_nav 9, hudlayer_shared 4, sharedGraphics 2, weaponicon 2); only bgBar 466 / loggo_mc 477 have four; a missing label leaves the previous era's art. The selector 443 is a two-state toggle sending `"0"` / `"1"` | ERA-N-12 |

**Probe and checks** (game tier; addresses and digests only, nothing of EA's kept). `spec/probes/gamemodes.py` gained
`N_SITES`: 9 ranges with sha256 prefixes, the boundary check and the overlap check against the kept Vietnam ranges.
It also reads the tracker's page bound (4). The whole probe takes 7.3 s, and `core/tests/test_spec_probes.py::game_modes`
now checks 28 answers.

**Rules** went from 1,657 on main to 1,670: eras_n.md has 13 (2 verified, 6 observed, 5 inferred). eras.md's ERA-SITE-5
and ERA-SITE-4 carry the corrections. These pass: `spec/trace.py --check`, `spec/probes/census.py --check`
(CENSUS.md rewritten) and the game_modes step.

### Playtests still pending (in-game tests are paused)

- ERA-N-2: a page above 3 saved to GameSettings.bin and read back by the patched loader.
- ERA-N-11: APT's `gotoAndStop` with an unknown or numeric-looking label.
- ERA-N-12: a fifth era's frames in each of the four movies, and the load-time picks after a level.
- ERA-N-13: the in-process server's level list with an era tag in +0xB8.

### What is left (GM4)

- Whether battlefield-opfor's launcher uses the `rome_snowroller` pipe. Only OPFOR's source was searched.
- The Store page's writer on PC (none found), and the page-0 source 0x9484A0 as the award list (inferred).
- GM1's open questions stand: 0x9381F0's Vietnam map keys, the server strings' writers, a missing asset's handle.
