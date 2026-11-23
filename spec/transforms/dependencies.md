# Dependency graphs: the assets' and the WW2 rebuild's

Two graphs a tool computes rather than hard-codes. (a) The **asset graph**: what each partition, resource, shader
database, bundle and registry entry depends on, from the references the data holds and the names the engine finds
files by - the graph the export's closure (XEXP-CLO-1) walks and an editor's "what uses this" needs. (b) The **WW2
rebuild's step graph**: Project OPFOR BFBC2's build steps, what each reads and writes as its step records hold it, and
the order constraints that follow - the specification the queued "rebuild_all as a step graph" work implements.

**Sources.** Round SPG3 (2026-11-23): `spec/probes/dependencies.py` over DICE's bundles of the game copy (its
`checks()` hold the asset graph's counts) and, with `--steps`, over OPFOR's step records (`work/ww2/_rebuild/track`,
as the rebuild of 2026-11-22 / 23 left them; read only); OPFOR's `rebuild_all.py` and `stepstamps.py` (read only).

## (a) The asset graph

### XDEP-NODE-1: The nodes

Status: **inferred**

| Node | Identity | Defined in |
|---|---|---|
| partition | its entry name less `.dbx`, lower case (REF-FORM-4) | model/bundles.md PART-NAME-1 |
| resource | its entry name less `.res`, lower case, and its type (RES rules) | formats/res.md |
| shader database | a level's `Levels/<LV>/Shaders/<dxN>_*.res` and the `Shaders.dbx` collection | PART-NAME-3, formats/shaderdb.md |
| bundle | its path under `Dist` / `Package` (BNDL-LOC-1) | model/bundles.md |
| registry entry | a `package.mft` block and a `bundleManifest` line (REG-MFT-2, REG-BM-1) | model/registration.md |

A name is one node however many bundles carry a copy (each level bundle repeats shared partitions); which copy the
game loads is the precedence rules' business (PREC-ORD-1 - PREC-ORD-4), not the graph's.

[inferred: from REF-FORM-4, PART-NAME-1 - PART-NAME-3, BNDL-LOC-1, REG-MFT-2, REG-BM-1]

### XDEP-REF-1: Partition edges are the external references in the string table

Status: **verified**

An edge runs from a partition to every partition it names in an external reference (`<partition path>/<guid>`,
REF-FORM-2, REF-FORM-3). Every reference is a string of the partition's string table (DBX-STR-1), so a tool lists a
partition's edges from its table alone, without parsing the tree: the strings that match
`<path>/<8-4-4-4-12 hex guid>`. Over DICE's bundles (1,021 dated before 2013; 124,458 `.dbx` entries; 84,641
distinct partitions) this gives 266,618 distinct edges; 5,254 partitions name no other partition and 60,372 are named
by none (level partitions, blueprints only a level places, the assets of other eras). The most named is
`characters/skeletons/ske01` (6,596 partitions).

[test: core/tests/test_spec_probes.py::asset_graph]
[data: spec/probes/dependencies.py, 2026-11-23: 1,021 bundles, 124,458 .dbx entries, 84,641 partitions, 266,618 edges, 5,254 without an out-edge, 60,372 without an in-edge]

### XDEP-REF-2: Two edges in five point at no partition the game ships

Status: **verified**

113,597 of the 266,618 edges (43 %) name one of 7,652 partitions that is in no bundle - DICE's build stripped them
(REF-RES-3). By the target's top folder: `textures` 87,427 edges (texture partitions: the textures the engine uses are
resources found by name, XDEP-RES-1), `fx` 5,847, `emitters` 5,775, `objects` 5,674 (most under a `shaders/` folder:
the material partitions), `sound` 4,111, `animations` 2,062. A dependency tool must therefore treat an unresolved
reference as normal stock data - reported, never an error - and a closure must not try to copy what DICE never
shipped (XEXP-CLO-1: "listed, never filled").

[test: core/tests/test_spec_probes.py::asset_graph]
[data: spec/probes/dependencies.py, 2026-11-23: 113,597 dangling edges to 7,652 names; by top folder as above]

### XDEP-REF-3: The graph has eleven small cycles

Status: **verified**

The resolved graph is almost acyclic: 11 strongly connected components of more than one partition, the largest of
10 (the soldier skeleton `characters/skeletons/ske01` and the nine influence maps that name it and each other); the
others are weapon blueprints and their kit variants naming each other (`bu_sni_m95`, `_kobra`, `_scoped` ...), two
objective objects, the AA and plain BMD-3, two rock meshes, a level's ambient sounds and its big-world sound, and two
CAV partitions. A closure walk must keep a visited set; a topological order exists after collapsing these 11.

[test: core/tests/test_spec_probes.py::asset_graph]
[data: spec/probes/dependencies.py, 2026-11-23: 11 components, sizes 10, 3, 3, 3, 3, 3, 2, 2, 2, 2, 2]

### XDEP-RES-1: A resource belongs to a partition by name

Status: **verified**

Resources are not referenced; a partition owns the resources its name leads to (PART-NAME-2). Over DICE's 80,991
distinct `.res` names, tried in this order:

| Rule | Resources |
|---|---|
| the same name (`x_mesh.res` of `x_mesh.dbx`, `y_havok.res` of `y_havok.dbx`) | 16,363 |
| less `_wavedatasns_<n>` (a wave's streamed data, WAVE-SEG-2) | 479 |
| less `_lod<n>_data` (a mesh's LOD data) | 4,119 |
| less a last `_<word>` or `.<word>` (`y_havok_havok`, `.heightfield-000`) | 840 |
| none: voice-over waves (`sound/vo/...`: the language's VO tables name them, SND-VO rules) | 54,404 |
| none: textures (`ITexture`, found by name from shaders and meshes) | 3,064 |
| none: a level's shader databases (found by the level's name, PART-NAME-3) | 222 |
| none: other | 1,500 |

So a dependency tool adds resource edges by these name rules, and textures, shader databases and voice-over as
by-name lookups of their own.

[test: core/tests/test_spec_probes.py::asset_graph]
[data: spec/probes/dependencies.py, 2026-11-23: 80,991 resources, the counts per rule above]

### XDEP-REG-1: Bundle and registry edges

Status: **inferred**

Above the partitions: a level's registry block (`package.mft`) lists the bundles it loads in order (REG-MFT-4), and
each bundle needs its `bundleManifest` line (REG-BM-1, REG-NEW-1); a partition resolves through that stack
(REF-RES-2), so "partition P of level L depends on Q" means Q is in a bundle of L's stack before any marker hiding it
(BNDL-DEL-2). The suite's registry probe (`spec/probes/registry.py`) holds the stack's counts. A level's own
by-name files (its shader databases, emitter system, cull trees: PART-NAME-3) are edges from the level to its own
bundle.

[inferred: from REG-MFT-4, REG-BM-1, REG-NEW-1, REF-RES-2, BNDL-DEL-2, PART-NAME-3]

### XDEP-ALG-1: Computing a closure

Status: **inferred**

A tool computes what a level, an object or an edit needs as reachability over this graph: from the start
partitions, follow partition edges (XDEP-REF-1) through the level's bundle stack (XDEP-REG-1), stop at a partition a
shared bundle provides (the export's rule, XEXP-CLO-1), add each reached partition's resources by name (XDEP-RES-1),
record unresolved references without following them (XDEP-REF-2), and keep a visited set for the cycles
(XDEP-REF-3). The reverse graph answers "what uses this" (an editor's rename or delete: every in-edge must be
rewritten or the reference dangles; REF-DEL-1, REF-DEL-2 for lists the game reads unchecked). The string-table
listing makes the whole-game graph cheap: about 22 seconds for DICE's 124,458 entries.

[inferred: from XEXP-CLO-1, XDEP-REF-1 - XDEP-RES-1, REF-DEL-1, REF-DEL-2; the probe's run time on this PC]

## (b) The WW2 rebuild's step graph

### XDEP-STEP-1: Steps, records and what a record holds

Status: **implemented**

`rebuild_all.py` runs an ordered list of steps (`STEPS`: Wake's 43 builds, then per installed WW2 level its effects
and objects build, its chain - the rest of its build, its emitter atlas, its fidelity gate; 55 steps today), each a
command line under `src/tools/ww2`. Each step that succeeds leaves a record (`work/ww2/_rebuild/track/<key>.json`,
`stepstamps.Tracker.finish`): the files it read before writing them (`inputs`, sha1), the files it wrote under
`work\ww2` and elsewhere (`outputs`), the folders it listed (`lists`), the paths it only tested (`probes`), its code
(the scripts, modules and programs it ran) and the command line, tracker version and Python. `--changed` reruns a
step when its record is missing or its code, an input, a listing, a probe or an output differs, and sends the run back
to an earlier skipped step whose output a later step rewrote (`run_changed`, `Tracker.why`, `Tracker.stale_inputs`).
What a program other than Python reads beyond its arguments is not recorded (the scans of `step_roots` cover what it
writes).

[code: opfor:src/tools/ww2/stepstamps.py:1-38] [code: opfor:src/tools/ww2/stepstamps.py:191-262]
[code: opfor:src/tools/ww2/stepstamps.py:266-309] [code: opfor:src/tools/ww2/rebuild_all.py:97-126]
[code: opfor:src/tools/ww2/rebuild_all.py:413-470]

### XDEP-STEP-2: The measured graph

Status: **observed**

From the 55 records of the last rebuild: 10,029 outputs and 217,874 inputs (193,080 outside `work\ww2` - the game
copy, BF1943's files, the suite - and 2,157 inside it that no step writes). Joining each input to the last earlier
step that wrote it gives 339 read-after-write edges (22,600 files); 36 files are written by more than one step, in 9
write-after-write edges (41 files): `wake_level` -> `wake_ambience` (the level partition), `wake_shaders` ->
`wake_shaders --clone` -> `wake_translate` -> `wake_sky` (Wake's level and databases), four weapon builds ->
`grime.py --apply` (their textures), `rifle_grenades` -> `rg_projectiles`. The longest chain of edges is 14 steps; 19
steps read no earlier step's output (they read only the game copy, BF1943 and the suite) and can start at once.

[data: spec/probes/dependencies.py --steps, 2026-11-23, OPFOR's 55 step records of the rebuild of 2026-11-22 / 23: the counts above (they change with every rebuild and step change: the probe prints them, its checks() do not hold them)]

### XDEP-STEP-3: Three steps read an output a later step writes

Status: **observed**

Three steps read files that only a step after them writes, i.e. the previous rebuild's output:
`wake_translate.py Objects/ -Objects/Weapons` reads 33 of `material_grid.py`'s outputs
(`work/ww2/material_grid/effects/...`, `emitters/...`) and 2 of `bf1943_jp_bazooka.py`'s (the M18's textures), and
`material_grid.py` reads 2 of `wake_vo.py`'s (`vo/sound/vo/vo_en.dbx`, `vo_triggers.dbx`). On a clean `work\ww2` these
reads find nothing (or another run's files), so the result depends on what the folder held before - the same kind of
order fault the P0 audit found in `wake_shaders --clone`. The oracle passes because the outputs reach a fixed point
after one rebuild. A step-graph runner must either move these producers before their readers or record the edge as
an explicit two-pass dependency; until then, a rebuild from an empty `work\ww2` needs two passes of these steps.
(Unproven: whether the readers merely test for the files or use their content.)

[data: spec/probes/dependencies.py --steps, 2026-11-23: READ-BEFORE-WRITE 35 -> 36 (33 files), 35 -> 37 (2), 36 -> 38 (2)]
[code: opfor:src/tools/ww2/rebuild_all.py:97-117]

### XDEP-STEP-4: What a step-graph runner must keep

Status: **inferred**

A runner that schedules the rebuild from its records instead of the fixed list must keep:

1. **Read-after-write**: a step after every step whose recorded output it reads (XDEP-STEP-2);
2. **Write-after-write**: steps writing one file in their list order (the Wake level's five writers, the weapon
   textures before `grime.py --apply`), and readers of that file after the writer whose version they read;
3. **The read-before-write cases** resolved, not inherited (XDEP-STEP-3);
4. **The declared folders** (`OUTPUTS`) as a fallback for steps whose records miss non-Python reads, and the
   memory gate of the level chains (`JOBS`, `MIN_FREE`: a level build peaks at about 3 GB);
5. **External inputs** (the game copy, BF1943, the suite's code) as roots whose hashes rerun their readers, as
   `--changed` does now;
6. **A first run with no records** in the declared order, since edges come only from records.

With these the 19 independent steps and the level chains can run side by side; the 14-step chain bounds the time.

[inferred: from XDEP-STEP-1 - XDEP-STEP-3 and rebuild_all's OUTPUTS / JOBS / MIN_FREE]

## Open questions

- Which asset references the engine follows at load (a partition named but never loaded is no dependency at run
  time): the graph is the data's, not the loader's.
- Edges held outside the string table: resources that name partitions or other resources inside their own bytes
  (a mesh set's shader names, a shader database's texture names), and the VO tables' wave names.
- Whether the three read-before-write steps (XDEP-STEP-3) use the files' content.
- The stale records of steps renamed since (`ww2_level_build.py <level>` without its parts) are ignored by the probe;
  the runner should prune them.
