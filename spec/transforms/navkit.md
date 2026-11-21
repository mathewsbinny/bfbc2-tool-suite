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

### XNAV-INS-1: Install keeps DICE's original once, writes whole

Status: **verified**

Install refuses while the game runs or before a build. On the first install DICE's file is kept as
`pathdata.hpd.opfor-orig`, or, where the level had none, an `.opfor-new` marker is written so restore knows to
remove the file; later installs keep that first backup. The file is copied into the game copy atomically (a cut-short
copy once left a truncated file the AI loader read). Traversal annotations are installed only when made for that
exact file (its SHA-1); stale ones are removed.

[code: bfbc2-auto-navkit/navkit.py:751-785] [test: bfbc2-auto-navkit/tests/test_navkit_offline.py::main]

### XNAV-INS-2: Restore gives the level its original path data back

Status: **verified**

Restore puts DICE's kept original back (removing a stale no-original marker), or, for a level that had none, removes
the installed file and the marker; it removes the traversal annotations; with nothing installed it says so and
changes nothing; it refuses while the game runs.

[code: bfbc2-auto-navkit/navkit.py:788-807] [test: bfbc2-auto-navkit/tests/test_navkit_offline.py::main]

### XNAV-INS-3: DICE's original is found through every OPFOR backup

Status: **verified**

Where several OPFOR stages backed up a file, DICE's original is the earliest backup (NavKit's `.opfor-orig` first
among equals); a file an OPFOR stage installed where there was none is reported as OPFOR's, never taken for DICE's.

[code: bfbc2-auto-navkit/navkit.py:77-100] [test: bfbc2-auto-navkit/tests/test_navkit_offline.py::main]

## Open questions

- The levels where NavKit's coverage still falls short of DICE's (KYNAPSE_SPEC.md and the parity runs); the goal is
  a full content match, not byte identity.
- Offline sampling (`navsample_offline.py`) is not yet a `navkit.py` step.
- The game's own check that a level's path data matches its level (the runtime loads data built for another state of
  the level without an error: HPD-RT-2).
