# NavKit: path data built, edited, installed and restored

NavKit (`bfbc2-auto-navkit/navkit.py`, the generator in `rust/bc2-navbuild`) makes a level's Kynapse path data
(`pathdata.hpd`, `formats/hpd.md`) without Kynapse's generator. Its transformations:

- **build**: the level's collision samples (in-game `[dev] navsample` or offline from the level's Havok collision
  and height field, `formats/navsample.md`) and level objects in; `pathdata.hpd` out in NavKit's output folder;
- **apply**: a navigation side file's edits (walkability regions, seeds, unseeds, links, route edges, traversal
  annotations) on a NavKit build, or (graph edits only) on a copy of DICE's file;
- **install** / **restore**: the built file into the game copy's `Output\win32\ai\levels\<lv>\` and back.

The reference for what NavKit builds is DICE's own path data (`navregress`, `navparity`); the reference for its
code is the retired Python generator, recorded before deletion (`tests/navbuild_golden.json`).

## 1. Build

### XNAV-BLD-1: Deterministic, and the retired generator's bytes

Status: **verified**

A build with the build clock fixed is a function of its inputs: the grid walks, walkability rules, areas, tracing,
route graphs and whole sector stages give the answers the retired Python generator gave (recorded as SHA-1 per
answer on seeded random levels), and whole builds of recorded levels give the recorded path data's SHA-1; a changed
input is reported as such, not as a regression.

[test: bfbc2-auto-navkit/tests/test_navbuild_native.py::quick] [test: bfbc2-auto-navkit/tests/test_navbuild_native.py::game]
[test: bfbc2-auto-navkit/tests/test_navbuild_native.py::e3h]

### XNAV-BLD-2: The output reads back exactly and is structurally DICE's

Status: **verified**

Every file NavKit writes parses and writes back to the same bytes (the path-data writer and the runtime-mesh writer
are byte-exact, KYM-REC-1), in DICE's layout order (KYM-ORD-1); the Rust and the reference reader and writer agree
on every path data file of the game copy, its kept originals and 1,500 damaged copies.

[code: bfbc2-auto-navkit/navkit.py:886-888] [test: bfbc2-auto-navkit/tests/test_pathdata_native.py::main] [test: bfbc2-auto-navkit/tests/test_apply.py::main]

### XNAV-BLD-3: Builds may only get closer to DICE's

Status: **implemented**

`navregress` rebuilds a fixed set of levels with the current code and compares each with a saved reference run on
the parity measures against DICE's own path data (coverage of the AI's order positions, the main network, pairwise
routes, structure); it exits 1 when a measure got worse beyond its tolerance or a level failed to build.

[code: bfbc2-auto-navkit/tools/navregress.py:191-206] [code: bfbc2-auto-navkit/tools/navregress.py:268-270]
[doc: bfbc2-auto-navkit/README.md:76-77]

### XNAV-BLD-4: Walkability decides in the sample run's own precision, with no iteration order

Status: **verified**

Walkability (`bc2_navbuild::walk`: the run, the hand edits, the water and extent rules, the slope, clearance and
structure rules, the seeds, floods, ladders, unseeds and roof islands) computes heights and distances in `float64`
and compares a value the run stores as `float32` - a normal, a height in a ladder end's band, a `float32` object
triangle's bounds - against its limit rounded to `float32` (NumPy 2's rule for a Python number meeting a `float32`
array). For a sample file's normals (k / 127) the `float32` limits decide as `float64` ones would at every slope limit
the build uses (40, 55 and 15 degrees: 0 of 255 normal values differ), so only NavKit's own ladder bands can see the
rounding. Column indices round half to even; a surface lies on a sector's seed line when its x or z modulo 100 m,
rounded to 0.1 m, is 1.6 or 98.4. No answer depends on an iteration order: the anchors, seed lines and ladders done
are only tested for membership. A polygon region contains a point on its outline exactly when matplotlib's
`Path.contains_points` does (the crossing test with half-open edges, closed back to its first vertex; fewer than three
vertices contain nothing).

[code: rust/bc2-navbuild/src/walk.rs:105] [code: rust/bc2-navbuild/src/walk.rs:674]
[test: rust/bc2-navbuild/src/walk_tests.rs::polygon_crossings_as_matplotlib]
[test: rust/bc2-navbuild/src/walk_tests.rs::python_modulo_and_rounding]
[test: bfbc2-auto-navkit/tests/test_navedits.py::matplotlib_agreement]
[data: the 15 regression levels' 30 walkability stages give the retired Python's run, kept flags, gradients and stair flags bit for bit, and the same path data (rust/parity_nk4_walk.py, round NK4)]

### XNAV-MEAS-1: The graph measured over every DICE node and edge by nearest-node pairing

Status: **implemented**

`navdiff` pairs, per sector, every DICE route node with at most one NavKit node and every NavKit node with at most
one DICE node: all pairs within the tolerance in plan (half of KYG-NODE-7's DistPropagation by the DICE sector's
pitch: 0.425 m detailed, 1.675 m coarse, 7.675 m vehicle) and 1 m in height, taken nearest first (ties by DICE then
NavKit index). Over all DICE nodes and edges of the file (sectors only DICE has count as unpaired): `nodes_paired`,
the paired share of DICE's nodes; `nodes_paired_candidate`, of NavKit's; `edges_paired`, DICE's edges whose
partners hold the edge in the same direction; `costs_paired`, those whose cost word is within a tenth of DICE's
(both in sixteenths of a metre, KYG-EDGE-2); and `node_distance`, each DICE node's nearest NavKit node in plan
(within 1 m in height) as shares up to 0.05 / 0.1 / 0.25 / 0.5 / 1 / 2 / 4 m and beyond. The Rust core
(`bc2_navbuild::measures`) and the Python (`navdiff.paired_graph`) give the same counts. `navparity` reports them
beside the old exact-match measures (0.05 m: `nodes_same`, `edges_same`, `costs` over the matched edges only) and
`navregress` fails a level when one drops.

[code: bfbc2-auto-navkit/tools/navdiff.py:288-353] [code: rust/bc2-navbuild/src/measures.rs:454-555]
[test: bfbc2-auto-navkit/tests/test_navtools_native.py::quick]

## 2. Apply

### XNAV-APP-1: Region edits need a NavKit base; DICE's base takes graph edits only

Status: **verified**

`navkit apply` refuses without a side file, with a side file that does not load, with walkability (region) edits on
a DICE base, and with a DICE base on a level without DICE path data. On a NavKit base the walkability edits go into
the build; links and route edges go into the result's route graph afterwards (each reported applied or not, with
its reason); on a DICE base a copy of DICE's file takes the graph edits, for this PC only. Traversal annotations go
to `<level>.traversal.json` beside it, never into the route graph.

[code: bfbc2-auto-navkit/navkit.py:836-897] [test: bfbc2-auto-navkit/tests/test_navkit_offline.py::main] [test: bfbc2-auto-navkit/tests/test_navedits.py::main]

### XNAV-APP-2: What the edits guarantee

Status: **verified**

With no edits, apply gives the regression build's sector meshes and route graphs (only build times differ); an
unwalkable region leaves no walkable ground inside it; an unseed removes its island; a walkable region loses no
walkable ground inside it; the result reads back exactly or apply fails ("not usable").

[code: bfbc2-auto-navkit/navkit.py:884-888] [test: bfbc2-auto-navkit/tests/test_apply.py::main]

## 3. Install and restore

### XNAV-INS-1: Install keeps the file it replaces once, in the install ledger, and writes whole

Status: **verified**

Install refuses while the game runs (or while `tasklist` does not answer in 30 s) or before a build. It is one
journalled commit of the owner `navkit:<lv>` in the game copy's install ledger (XINST-GL-1): the file there before
NavKit is kept once by its sha256 (or recorded as none, so restore knows to remove the file); a later install keeps
that first prior. Another tool's install of the same file stays below NavKit's. The file is written beside its target,
flushed and moved over it. Traversal annotations are installed in the same commit only when made for that exact file
(its SHA-1); stale ones nobody installed are removed. NavKit's markers from before the ledger (`.opfor-orig`,
`.opfor-new`) are adopted into it first; another tool's are refused in words.

[code: bfbc2-auto-navkit/navkit.py:836-883] [code: rust/bc2-cli/src/navkit/driver.rs:153-226]
[test: bfbc2-auto-navkit/tests/test_navkit_offline.py::main] [test: core/tests/test_gameledger.py::cross]

### XNAV-INS-2: Restore gives the level the path data from before NavKit back

Status: **verified**

Restore takes `navkit:<lv>` out of the ledger: the file from before NavKit is written back, or, for a level that had
none, the installed file removed; when another tool installed over NavKit since, NavKit's layer is taken out from
under it (that tool's file stays, and its revert now gives back what was there before NavKit - audit AU4 DS-1). The
traversal annotations go with it. With nothing installed it says so and changes nothing; it refuses while the game
runs.

[code: bfbc2-auto-navkit/navkit.py:886-922] [code: rust/bc2-cli/src/navkit/driver.rs:228-281]
[test: bfbc2-auto-navkit/tests/test_navkit_offline.py::main] [test: core/tests/test_gameledger.py::cross]

### XNAV-INS-3: DICE's original is the bottom of the ledger's stack

Status: **verified**

DICE's original of a game file is what was there before any owner of the install ledger (the bottom layer's prior,
by its sha256); a file an install put where there was none is reported as installed, never taken for DICE's. A file
no ledger owner holds is answered by the markers of before the ledger (the earliest backup). The three readers (the
NavKit driver, its Rust twin, the map editor) answer alike.

[code: bfbc2-auto-navkit/navkit.py:77-108] [code: rust/bc2-navbuild/src/inputs.rs:168-234]
[code: rust/bc2-editor/src/navdata.rs:1479-1494]
[test: core/tests/test_gameledger.py::python_twin] [test: rust/bc2-gamefs/src/ledger_tests.rs::ds1_sequence_keeps_dices_file]

## Parity target

Set by the user, 2026-11-22. "100 % parity" with DICE's path data means a full content match, not byte identity
(XNAV-BLD-3 says builds only get closer; this says how close is done). Every NavKit round reports against it, on all
35 DICE files (`rust/bc2-formats/examples/kynapse_probe.rs`) and the regression levels (`navparity`, `navregress`):

1. **What bots experience,** on every level: `ground`, `connectivity`, `ai_coverage` and `ai_pairwise` at 99.5 % or
   more; `extra_ground` and `over_connection` under 1 %; `sectors_extra` 0.
2. **DICE's structure, rule by rule:** each generator rule of `kynapse_generation.md` that is decoded is reproduced
   exactly on DICE's files (the rule's own probe count), so `areas_same`, `portals`, the graph's measures and the
   D / F recall rise round by round (NKG1-NKG3) towards 100 %. The graph is judged over every DICE node and edge
   (round NKS-deep E, XNAV-MEAS-1): `nodes_paired` (DICE's nodes with a NavKit partner within half a propagation
   step), `nodes_paired_candidate` (the same pairs as a share of NavKit's nodes, so extra nodes earn nothing),
   `edges_paired` and `costs_paired` (DICE's edges held by the partners, and those with the cost within a tenth),
   with the nearest-node distance histogram; `nodes_near` (within 2 m) stays. The old exact-match measures
   (`nodes_same`, `edges_same` within 5 cm, `costs` of those few edges) are kept beside them for one round
   (NKS-deep E) so the change is visible, then dropped from the target.
3. **Listed exceptions only:** DICE data that describes a level as it no longer is (SP_03's 93 nodes under the PC
   ground, `navparity.stale_nodes`), where the current level wins; and any rule shown to be unrecoverable from
   DICE's output, named with its evidence. Nothing else may stay short of the target unexplained. Named by the user's
   ruling (2026-11-22, round NKG3): sp_02's `df_d_recall` 21.86 -> 21.53 % under DICE's terrain rule for D sides
   (KYG-DF-1): 43 of DICE's 12,932 sides, lost where the terrain's slope across the side is 0.60-0.77 and the centres'
   step 0.60-0.65 m, the band where DICE breaks only 17-34 % of sides and the height field's 1 m quantisation hides
   DICE's own slope measure; every other level's D precision and recall rose with it.
   Named by the user's ruling (2026-11-23, round NKS-deep B) with DICE's node placement on (KYG-NODE-7, KYG-NODE-8,
   KYG-EDGE-5; `nodes_near` 47.63 -> 56.21 %, `ai_pairwise` 92.67 -> 95.57 %, `connectivity` 98.79 -> 99.06 % on the
   15 levels): `costs` 95.29 -> 84.92 %, `nodes_same` 0.66 -> 0.63 % and `edges_same` 0.02 -> 0.01 %, measures of
   tiny exact-match samples (edges matched within 5 cm, a handful a level; nodes on DICE's exact positions, which the
   old cell-centre nodes met only on DICE's 4 % of cell-centre nodes); `over_connection` 15.31 -> 15.74 %, mostly the
   matched sample growing (sp_003_b's wrongly joined DICE group pairs 8,622 -> 7,629 and sp_03's 5,442 -> 4,753 while
   their matched DICE nodes grow 19,318 -> 29,999 and 21,654 -> 31,379), with one real rise, sp_007 10.87 -> 17.47 %
   (a 142-node DICE island joined to the main group by short detailed edges beside DICE's own island boundary; handed
   to round NKS-deep D); and the per-level dips `connectivity` sp_007 -0.21, ons_mp_002 -0.15, sp_004_b -0.10,
   `nodes_near` sp_009 41.08 -> 32.43 % (the old build held 2.6 x DICE's nodes there; NavKit's nodes near a DICE node
   23.0 -> 32.4 %) and sp_008_b 61.82 -> 59.16 %, `ai_pairwise` sp_008 96.2 -> 95.5 % (`rust/BENCHMARKS.md` "NKS-deep
   B").

Measured 2026-11-20 (15 regression levels, means): `ground` 98.22 %, `connectivity` 98.79 %, `ai_coverage`
99.16 %, `ai_pairwise` 92.67 %, `extra_ground` 8.31 %, `over_connection` 15.31 %, `sectors_extra` 1.33;
`areas_same` 5.18 %, `portals` 6.47 %, `nodes_same` 0.64 %, `nodes_near` 47.63 %, `edges_same` 0.02 %, D / F recall
18.8 / 33.5 %.

Measured 2026-11-22 after round NKG1 (the same levels, means): every measure above unchanged; the directed measures
navparity reports since NKG1, the graph as the runtime walks it (KYRT-PATH-3: a node's edges are all its copies';
edges into a border node with no owner copy dropped), `runtime_connectivity` 98.41 % (sp_003_b 90.01 % lowest) and
`strong_share` 99.69 % (DICE's own files 97.30 %). Structure (probe): table bounds the node box in 2,489 of 2,494
sectors (0 before), duplicate edges 0 (6,916), diagonal-neighbour links 5,922 of 14,904 entries (0 of 8,994; DICE
5,682 of 14,562).

Measured 2026-11-22 after round NKG2 (the same levels, means): `runtime_connectivity` 98.41 -> 98.80 % (sp_003_b
92.67 % lowest), `strong_share` 99.69 -> 100 % on every level, `nodes_same` 0.64 -> 0.66 %; `costs` 95.91 -> 95.29 %
(more edges matched, the new ones a sixteenth off: rust/BENCHMARKS.md "NKG2"); every other measure unchanged. Structure
(probe): border ids 125,145 (DICE 103,085), shared by 3 / 4 sectors 3,967 / 2,272 (DICE 4,799 / 3,553), edges out of
non-owner copies 0 (DICE 7,355). Named exceptions added by NKG2 (kynapse_generation.md): the lookup lists' tie order
(KYG-NODE-3) and the exact node placement (KYG-NODE-1) are not recoverable from DICE's output so far; DICE's detailed
sectors wait for a pitch-0.2 sampling run over DICE's list (KYG-DET-1).

Measured 2026-11-22 after round NKG3 (the same levels, means): DICE's terrain rule for D sides (KYG-DF-1, on by the
user's ruling): `df_d_precision` 50.53 -> 56.71 %, `df_d_recall` 18.84 -> 23.89 % (normal pitch 32.80 -> 40.60 %),
every other measure unchanged; sp_02's `df_d_recall` 21.86 -> 21.53 % is the named exception above
(rust/BENCHMARKS.md "NKG3"). Named exception added by NKG3: DICE's slope measure within 0.01 of
its threshold (the height field's quantisation hides it, KYG-DF-1).

## Open questions

- The levels where NavKit's coverage still falls short of DICE's (KYNAPSE_SPEC.md and the parity runs); the goal is
  a full content match, not byte identity, as the parity target above sets it.
- Offline sampling (`navsample_offline.py`) is not yet a `navkit.py` step.
- The game's own check that a level's path data matches its level (the runtime loads data built for another state of
  the level without an error: HPD-RT-2).
