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

[code: opfor:src/tools/ww2/stepstamps.py:1-38] [code: opfor:src/tools/ww2/stepstamps.py:208-279]
[code: opfor:src/tools/ww2/stepstamps.py:283-330] [code: opfor:src/tools/ww2/rebuild_all.py:105-134]
[code: opfor:src/tools/ww2/rebuild_all.py:421-477]

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

### XDEP-STEP-3: No step reads an output a later step writes

Status: **implemented**

Every step runs after each step whose output it reads. Before round WW2-ORD1 (OPFOR, 2026-11-23) three steps read
files only a later step wrote, so they read the previous rebuild's copy: `wake_translate.py Objects/ -Objects/Weapons`
read 33 of `material_grid.py`'s outputs and 2 of `bf1943_jp_bazooka.py`'s, and `material_grid.py` read 2 of
`wake_vo.py`'s. Both readers really use these files, through every OURS root at once. `wake_translate` reads the WW2
builds through `wake_install.our_files`, every root with the later build winning. `material_grid`'s `build_roots`
stops when a root is missing, and it follows the level's references into `wake_vo`'s VO partitions. On an empty
`work\ww2`, `material_grid` stopped. The fix was the order. `bf1943_jp_bazooka`, `wake_vo` and `anims` read no other
step's output; they now run first, then `material_grid`, then `wake_translate` (a test holds every OURS producer
before both readers). The one read left over was `install_index.pkl`. It is `wake_install`'s stock-bundle index,
keyed by the bundles' sizes and dates and rebuilt by whichever step finds it stale. It is declared a cache, with
`install_dbnames.pkl` and `install_templates.pkl`, and the tracker no longer records it.

The step records of a rebuild from a seeded-empty `work\ww2` (XDEP-STEP-5) give 0 reads of a later step's output. The
same rebuild settles in one pass: `--changed --dry` afterwards runs 0 of 55 steps. Getting there took four fixes that
an empty folder exposed:
- `wake_ambience` drew a preview into a folder it had not made yet.
- The tracker's hook stamped every file under a folder handed to the Rust core without its skips, so `material_grid`
  recorded rebuild_all's own step logs as inputs. Those logs are rewritten every run, so the step never settled. With
  a full `work\ww2` the folder is over the hook's 50,000-file limit and is listed, not stamped.
- A listed folder that the step itself, or a later step, fills at any depth looked changed.
- `qa_checks` failed when there was no install report.

[code: opfor:src/tools/ww2/rebuild_all.py:105-134] [code: opfor:src/tools/ww2/stepstamps_hook/sitecustomize.py:43-48]
[code: opfor:src/tools/ww2/stepstamps_hook/sitecustomize.py:66-72] [code: opfor:src/tools/ww2/stepstamps.py:303-311]
[test: opfor:src/tools/tests/test_ww2_build.py::test_ours_readers_after_every_ours_producer]
[test: opfor:src/tools/tests/test_rebuild_tracker.py::test_a_run_from_an_empty_work_settles]
[test: opfor:src/tools/tests/test_rebuild_tracker.py::test_the_install_indexes_are_not_tracked]
[data: spec/probes/dependencies.py --steps, 2026-11-23, before: READ-BEFORE-WRITE 35 -> 36 (33 files), 35 -> 37 (2), 36 -> 38 (2); after, on the 55 records of OPFOR's rebuild from a seeded-empty work\ww2 (round WW2-ORD1): 0 reads of a later step's output, 362 read-after-write edges (26,529 files), 7 write-after-write (26 files), longest chain 15 steps; rebuild_all --changed --dry afterwards: 0 of 55 steps would run]

### XDEP-STEP-4: What a step-graph runner must keep

Status: **inferred**

A runner that schedules the rebuild from its records instead of the fixed list must keep:

1. **Read-after-write**: a step after every step whose recorded output it reads (XDEP-STEP-2);
2. **Write-after-write**: steps writing one file in their list order (the Wake level's five writers, the weapon
   textures before `grime.py --apply`), and readers of that file after the writer whose version they read;
3. **No read-before-write**: none is left since round WW2-ORD1 (XDEP-STEP-3); a new one is resolved, not inherited;
4. **The declared folders** (`OUTPUTS`) as a fallback for steps whose records miss non-Python reads, and the
   memory gate of the level chains (`JOBS`, `MIN_FREE`: a level build peaks at about 3 GB);
5. **External inputs** (the game copy, BF1943, the suite's code, and the inputs inside `work\ww2` that no step writes,
   XDEP-STEP-5) as roots whose hashes rerun their readers, as `--changed` does now;
6. **A first run with no records** in the declared order, since edges come only from records.

With these the 19 independent steps and the level chains can run side by side; the 14-step chain bounds the time.

[inferred: from XDEP-STEP-1 - XDEP-STEP-3 and rebuild_all's OUTPUTS / JOBS / MIN_FREE]

### XDEP-STEP-5: Inputs inside work\ww2 that no step writes

Status: **implemented**

The rebuild reads three kinds of files under `work\ww2` that none of its steps produce. An empty `work\ww2` must be
seeded with them:
1. **Placed by hand**: `fonts\` (the Permanent Marker font and its licence, 2 files in the oracle, read by
   `menu_art`) and `weapon_icons\confirmed` (the user's sign-off of the weapon icons, which `weapon_icons.py` keeps
   across its rebuild).
2. **The install plan's staging**:
   - `wake_translate` reads `install_ports\plans.json` (wake_install's `MESH_PLANS`), and `emitter_atlas` reads
     `install_ports\objects`;
   - each level's build, atlas and checks read `install_ports_<lv>\` and
     `install_<lv>\level\objects\vegetation` (`tree_billboards`).

   These come from `wake_install.py --plan`, which reads the rebuild's outputs. The result is a rebuild -> install
   plan -> rebuild cycle, so from nothing, one pass cannot settle without a plan having run.
3. **Caches** keyed by their sources: `install_*.pkl` (untracked, XDEP-STEP-3), `material_grid*\effects_cache.pickle`
   and `grime_refs.json`.

Since round WW2-ORC1 (2026-11-23), `rebuild_all.EXTERNAL_INPUTS` declares (1) and (2), 14 paths, each with where it
comes from. A run checks them at start and stops (exit 2) on a missing one, naming its source; `--without-inputs`
runs anyway, for the first rebuild of an empty `work\ww2` (then the plan, then a rebuild). `--hash`, `--qa` and
`--dry` do not stop. The caches are left out of the hashes (`rebuild_all.CACHES`, XDEP-STEP-6). A rebuild from a
`work\ww2` seeded with only these inputs (599 files) gives the oracle, 7,945 of 7,945.

Not done (proposal, for the user): move the plan's porting into the rebuild as its own step, which would break the
cycle. Today the plan both ports (`install_ports*`, the vegetation staging) and builds the install staging. A step
`wake_install.py --ports` before `wake_translate` (and each level's chain) would write only the ports from the
rebuild's outputs. The staging would become a step's output, the cycle would become an order, and an empty
`work\ww2` would rebuild with only the hand-placed files. That needs proof that the ports read nothing the later
steps write (`wake_translate`, `emitter_atlas` and the level builds read them).

[data: OPFOR's 55 step records of the rebuild of 2026-11-22 / 23 joined with every step's outputs: inputs under work\ww2 that no step writes, by folder (install_ports, install_ports_<lv>, install_<lv>\level\objects\vegetation, the caches); a rebuild from a work\ww2 seeded with fonts\, weapon_icons\confirmed, install_ports\, install_ports_<lv>\ and install_<lv>\level\objects\vegetation only (599 files, round WW2-ORD1, 2026-11-23) ran all 55 steps in one pass]
[data: round WW2-ORC1, 2026-11-23: a rebuild from a work\ww2 seeded with the 14 declared inputs (599 files): 55 steps in 19.0 min, 7,945 files, --changed --dry afterwards 0 of 55; the same 7,945 digests as OPFOR's main checkout after its relics were moved]
[code: opfor:src/tools/ww2/rebuild_all.py:176-192] [code: opfor:src/tools/ww2/rebuild_all.py:605-614]
[test: opfor:src/tools/tests/test_ww2_rebuild_inputs.py::test_external_inputs_cover_the_hand_placed_files_and_the_plan_staging]
[test: opfor:src/tools/tests/test_ww2_rebuild_inputs.py::test_a_run_stops_on_a_missing_external_input_unless_told]

### XDEP-STEP-6: The rebuild oracle holds what the steps write

Status: **implemented**

Until round WW2-ORC1, OPFOR's rebuild oracle (`src\tools\tests\data\ww2_rebuild_oracle.json`, then 10,852 files)
hashed `work\ww2` as it was after a rebuild, not only what the steps write. A rebuild from a seeded-empty `work\ww2`
(XDEP-STEP-5) matched 7,921 files. Every difference came from the folder's history, none from the step order:
- **2,903 relics** that no step writes:
  - 2,798 `wake\shaders_hlsl` sources from manual `wake_translate --hlsl` runs (2026-11-07 - 11-15);
  - 78 `wake\textures_bf1943` textures;
  - 27 `guadal_canal` / `iwo_jima_s` `textures_bf1943\Tinted` textures.
- **18 `wake\textures_bf1943` textures kept by a skip**: `wake_shaders.convert_textures` kept a texture that was
  already there. OPFOR's folder held conversions from 2026-11-05 made by an older converter. All 18 are BF1943's
  console DXT5A (format 3) masks with flags 0, converted to L8 (10). Those broke TEX-FMT-2: the L8 rows of the
  smallest mips were unpadded (16 / 4 / 1 or 8 / 2 / 1 bytes, where DICE's own L8 textures give 16 / 8 / 4). They
  also set flag bit 1, which TEX-FLAG-1 keeps clear on a texture converted from a console copy.
- **4 `effects_cache.pickle`**: the unpickled content is the same, the pickle bytes are not.
- **6 emitter-atlas `manifest.json` / `report.txt`**: only the closure's file count differed (4,758 vs 4,736),
  because the relics were counted; the atlases were the same.

The oracle was regenerated by the user's ruling (2026-11-23, "regenerate it cleanly"):
- **The relics** were moved, never deleted, to `work\ww2_relics_2026-11-23\` with a manifest (path, sha1, size, date)
  by `archive_relics.py`. A relic is a file `hashes()` reads that a seeded-empty rebuild does not make.
- **The caches** are left out of the hashes (`CACHES`).
- **The 18 textures**: `convert_textures` converts every texture on every run and writes it when the file differs.
  grime.py's weathering of today's conversion is kept. Each of the 18 now ends 16 / 8 / 4 (8 / 4 / 4 for 64 x 32) with
  flags 0, as DICE's L8 textures and today's converter do. The bytes grow by 7 (17 for the 512 x 1024 atlas).
- **The new oracle**: 7,945 files, from OPFOR's main checkout after the move, identical to the seeded-empty rebuild.
  The old one is kept as `ww2_rebuild_oracle_2026-11-23_history.json`.

The install oracle followed (`ww2_install_oracle.json`, 21,197 -> 21,149 files; the old one is kept as
`_2026-11-23_relics`). `wake_install` stages every `.res` under its OURS roots, `wake\textures_bf1943` among them, so
the plan had staged the relics:
- 48 relic textures that no staged file names are gone (21 Wake, 13 Guadalcanal, 14 Iwo Jima);
- 17 of the 18 textures changed (the 18th was staged as today's already);
- `Textures/Sky/Sky_Beach_Generic_RGBA16f` is DICE's PC copy (mp_common's, flags 2) in all four levels, no longer a
  relic BF1943 conversion of 2026-11-07 (flags 0) that had overridden it (BFBC2 has the texture, so the pipeline
  takes DICE's);
- the 4 level bundles, 3 manifests and 4 reports changed with them.

Against the installed bundles of the game copy, the new level bundles differ only in these entries and in the Waves
an install before PR1b-1c holds. The plan left `game\` unchanged (sha256 of 1,582 files).

[data: round WW2-ORD1, 2026-11-23: compare_oracle on the seeded-empty rebuild's hashes, 7,921 of 10,852 identical, differ 28, missing 2,903, extra 0; each missing or differing file joined to the step records of OPFOR's rebuild of 2026-11-22 / 23 (no writer for the relics, the 18 textures and the pickles); the 18 textures' headers (format 10, flags 2 vs 0, smallest mips) against DICE's format-10 textures of the game copy's level bundles; the pickles unpickled and compared]
[data: round WW2-ORC1, 2026-11-23: the 18 textures' BF1943 sources (format 3, flags 0), the old files (flags 2, smallest levels 16 / 4 / 1 or 8 / 2 / 1) and today's conversions (flags 0, 16 / 8 / 4 or 8 / 4 / 4); archive_relics against the seeded-empty rebuild's 7,945 digests moved 2,903 files (2,798 wake\shaders_hlsl, 78 wake\textures_bf1943, 13 guadal_canal and 14 iwo_jima_s textures_bf1943), 12 emptied folders removed; main's rebuild_all --changed afterwards (30 of 55 steps) and a full rebuild each 7,945 of 7,945 against the seeded-empty rebuild, --changed --dry 0 of 55, a --changed run 0 steps and the same hashes; wake_install.py --plan: 21,117 of 21,197 against the old install oracle (differ 32, missing 48), the 48 missing names searched in every staged file of their level (none), the staged level bundles against the game copy's installed ones entry by entry, Sky_Beach_Generic_RGBA16f equal to mp_common's entry; game\ sha256 before and after the plan: 0 of 1,582 changed]
[code: opfor:src/tools/ww2/rebuild_all.py:165-169] [code: opfor:src/tools/ww2/wake_shaders.py:734-783]
[code: opfor:src/tools/ww2/archive_relics.py:37-58]
[test: opfor:src/tools/tests/test_ww2_rebuild_inputs.py::test_hashes_leave_out_the_caches_inside_outputs]
[test: opfor:src/tools/tests/test_ww2_rebuild_inputs.py::test_relics_are_the_files_a_seeded_rebuild_does_not_make]
[test: opfor:src/tools/tests/test_ww2_textures.py::test_convert_textures_redoes_a_conversion_the_converter_now_writes_otherwise]
[test: opfor:src/tools/tests/test_ww2_textures.py::test_l8_conversions_follow_dices_level_sizes_and_flags]

### XDEP-STEP-7: A step that edits another step's output in place depends on the version it read

Status: **implemented**

Some steps read a file an earlier step wrote and rewrite it in place. The Wake shader databases
(`wake\levels\wake_island_s\shaders\Dx9 / Dx10 / Dx11_Single.res`) are written by the full `wake_shaders` run and
rewritten in turn by the clone pass, `wake_translate` and `wake_sky`. Such a step's output depends on the version it
read, so rerunning it alone on a later version gives other bytes. The clone pass is deterministic: the seeded-empty
rebuild and a full rebuild give the same databases. Rerun on the databases as `wake_sky` left them, it is not
idempotent.

The tracker dropped these reads (a file the step also wrote is its output, not an input). When a `--changed` run was
sent back to the clone pass (`fidelity_checks` read databases a later step had rewritten), the clone pass ran alone
on the finished databases. Its writes changed 8 shader files and `translate_wiring.json`. Since tracker version 3
(round WW2-ORC1), a record keeps these reads as `edits`, with the version read. A step about to run whose edited file
on disk is not the version its earlier, skipped writer recorded sends the run back to that writer (here, the full
`wake_shaders` run).

A related case: a step must not write into a folder a later step clears. `rifle_grenades.py` wrote
`rifle_grenades\retexture_rifle_meshes.json` into `rg_projectiles.py`'s output folder, which `rg_projectiles` clears.
A full run never kept the file. A `--changed` run that reran `rg_projectiles` alone left `rifle_grenades`' record
expecting it, so it never settled. Nothing read the file (`wake_install` takes `RETEXTURE` from the module), and it is
no longer written.

[data: round WW2-ORC1, 2026-11-23: OPFOR's step records of the Wake shader databases - wake_shaders, the clone pass, wake_translate and wake_sky record them as outputs and none as an input, fidelity_checks reads wake_sky's / wake_translate's versions; a second --changed on main sent back to the clone pass ("its inputs from wake_shaders.py --clone were rewritten by a later step - back to 8") and left 9 files differing from the seeded-empty rebuild, which a full rebuild restored (7,945 of 7,945)]
[code: opfor:src/tools/ww2/stepstamps.py:248-260] [code: opfor:src/tools/ww2/stepstamps.py:331-338]
[test: opfor:src/tools/tests/test_rebuild_changed.py::test_a_step_editing_a_file_in_place_reruns_on_the_version_it_edited]

## Open questions

- Which asset references the engine follows at load (a partition named but never loaded is no dependency at run
  time): the graph is the data's, not the loader's.
- Edges held outside the string table: resources that name partitions or other resources inside their own bytes
  (a mesh set's shader names, a shader database's texture names), and the VO tables' wave names.
- Whether the rebuild should own the install plan's porting (XDEP-STEP-5: proposed, not done), or the plan's
  staging should stay a declared external input.
- The stale records of steps renamed since (`ww2_level_build.py <level>` without its parts) are ignored by the probe;
  the runner should prune them.
