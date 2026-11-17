# Rust core: baseline timings

Milestone E0 (`bfbc2-map-inspector/EDITOR_ARCHITECTURE.md` 3.8, "measured, not assumed"): the DBX reader in Python
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

## A stock level: MP_005 (all 4,572 `.dbx` under `work\levels\mp_005`, 31.4 MB; 2,567 binary)

| | Python | Rust via Python, 1 by 1 | Rust via Python, many (parallel) | Rust alone, 1 thread | Rust alone, 16 threads |
|---|---:|---:|---:|---:|---:|
| parse (trees kept) | 6.55 s (many: 4.16 s) | 4.84 s | **2.05 s** | 0.27 s | **0.15 s** |
| to_xml | 10.72 s | 1.60 s | **0.23 s** | 0.98 s | **0.19 s** |

## A WW2 level: Guadalcanal (`work\ww2\guadal_canal`, 77 files, 3.3 MB)

| | Python | Rust via Python, 1 by 1 | Rust via Python, many (parallel) | Rust alone, 1 thread | Rust alone, 16 threads |
|---|---:|---:|---:|---:|---:|
| parse (trees kept) | 0.39 s (many: 0.38 s) | 0.46 s | 0.35 s | 0.028 s | 0.018 s |
| to_xml | 1.15 s | 0.21 s | **0.06 s** | 0.087 s | 0.030 s |

## The whole corpus (48,120 distinct files of `work\levels`, `work\navkit\common`, `work\ww2`; 579 MB)

`--stream`: each result dropped at once (a check over the corpus; every tree at once does not fit in 16 GB of RAM as
Python objects). Python: one run each.

| | Python | Rust via Python, 1 by 1 | Rust via Python, many (parallel, 500 at a time) | Rust alone, 1 thread | Rust alone, 16 threads |
|---|---:|---:|---:|---:|---:|
| parse | 45.9 s (many: 58.3 s) | 22.1 s | 28.6 s | 3.16 s | **0.52 s** |
| to_xml | 111.5 s | 12.2 s | **3.9 s** | 11.9 s | **1.67 s** |

Before this milestone the suite's corpus checks ran the Python reader only: `core/tests/test_dbx.py` (full tier).
The new Rust corpus test (`tests/test_rust.py --corpus`: every file of `work\levels` and `work\navkit\common`
read from disk, parsed, written as XML and hashed against DbxEdit's fingerprints) takes 25 s.

## What the numbers say

- **XML: 5-9x faster on one thread, 29-47x on all cores** through Python (the Python string the XML becomes is
  part of the cost: Rust alone writes the corpus's XML in 1.7 s); the corpus parity test (all 48,120 files, both
  readers) takes 90 s on 15 processes.
- **Parsing into Python objects is bound by the objects, not the reading.** Rust reads MP_005 in 0.27 s on one
  thread; building the same trees as Python `Element` dataclasses takes the rest (each element is a Python object
  with a list of attribute tuples and a bytes value). One file at a time this saves little (6.5 -> 4.8 s); the gain
  comes with `parse_many`, which reads on all cores and builds the trees with the cycle collector paused: **3.2x**
  on MP_005. The collector, not the reader, was the largest single cost: keeping a level's trees made Python's
  cyclic GC walk them again and again (MP_005, measured separately: 6.7 s with it, 2.4 s without for Python; 4.0 s
  and 0.8 s for Rust). Pausing it per `parse` call does not help (the deferred passes run after it), so callers
  that load many files and keep them should use `parse_many`. When each tree is dropped at once (the corpus run),
  one file at a time is better (22 s against 29 s): the collector has little to walk, and `parse_many` holds
  500 Rust trees before building their Python copies.
- **Small levels gain little through Python** (Guadalcanal's 77 files: the per-call and object costs dominate);
  the Rust-only numbers are what the editor will see (E1: the scene in Rust, no Python objects).
- **Keeping a whole corpus of Rust trees at once is memory-bound:** 48,000 trees at once ran slower on 16 threads
  (18.5 s) than on one (10.7 s) while the PC paged; `--stream` (each tree dropped at once) is the corpus case.
  An arena or a borrowed tree (strings and values pointing into the file's bytes) would cut the allocations; a
  candidate for E1, measured first. *Done in E0b: see "Leaner DBX trees" below (the corpus held at once in 0.3 s
  and 0.7 GB).*

## Leaner DBX trees (2026-11-16, branch `rust-e0b`)

`dbx::DbxTree` (`parse_tree`, `parse_tree_all`): a read-only tree in four allocations per file - every element in
one vector in file order (28 bytes each, with the index where its subtree ends), every attribute in another, the
string table borrowed from the file's bytes (`Cow`: copied only when a byte is not UTF-8) and every numbers or
two-byte value a range of the file's bytes. `parse` (the owned, editable `DbxFile`) is now `parse_tree` then
`DbxTree::to_file`, and the Python bindings build their Python trees from the compact tree. Measured with
`cargo run --release -p bc2-formats --example dbx_mem -- [--distinct] [--threads N] <folders>` (a counting
allocator: allocations, and the bytes still held once every tree is built; the files themselves not counted).
PC busy with other work: times ±20 %, the allocation counts exact.

| every tree held at once | before: owned `DbxFile` | after: owned `DbxFile` (via the tree) | after: `DbxTree` |
|---|---:|---:|---:|
| MP_005 (2,567 binary files, 31.4 MB): allocations | 2,750,906 | 2,761,174 | **10,269** |
| MP_005: memory held | 273 MB (8.7x the files) | 238 MB | **35 MB (1.1x)** |
| MP_005: parse, 1 thread | 0.18 s | 0.21 s | **0.057 s** |
| MP_005: parse, 16 threads | 0.072 s | 0.073 s | **0.011 s** |
| corpus (48,119 distinct binary files, 579 MB): allocations | 53.9 million | 54.1 million | **192,477** |
| corpus: memory held | 5.33 GB (9.2x) | 4.68 GB (8.1x) | **0.69 GB (1.2x)** |
| corpus: parse, 1 thread | 4.5 s | 5.4 s | **1.3 s** |
| corpus: parse, 16 threads | 1.3 s | 3.2 s (paging) | **0.31 s** |
| corpus: drop every tree | 3.4 s | 3.5 s | **0.4 s** |

Through Python (`rust\bench_dbx.py`, MP_005, the module before and after): `parse_many` 1.25 -> 1.12 s,
`parse` one by one 3.30 -> 3.17 s, `to_xml_many` 0.14 -> 0.13 s. The Python objects, not the reading, are the
cost there.

- **The compact tree is 270x fewer allocations and 7.7x less memory**: the whole corpus fits in 0.7 GB beside its
  579 MB of files, where the owned trees took 5.3 GB and paged on this 16 GB PC; parsing is 3-3.5x faster on one
  thread and 4-6.5x on all cores, and dropping 48,000 trees takes 0.4 s instead of 3.4 s.
- **The owned tree costs about the same as before** (one reader for both: the owned parse is now the compact parse
  plus a copy, 0-20 % slower on one thread; it holds less, as the strings are copied once from the borrowed table).
  It stays the shape to edit and write (EDITOR_ARCHITECTURE.md 3.8, "Two DBX trees").
- During a parse the element and attribute vectors are sized from the file (one element per 6 bytes) and trimmed
  afterwards: 2.0 GB allocated in passing for the corpus, 0.7 GB kept.

## The level loader on `parse_many` (2026-11-16, branch `rust-e0b`)

`core/bfbc2core/level.py` read its partitions one `dbx.read` at a time. It now reads them in batches through
`dbx.parse_many` (`level.read_many`): the level partition with its static-group files, then every blueprint partition
the placed objects name, then the partitions their objects live in. The results are identical, checked entity by
entity (guid, type, category, transform, blueprint, blueprint type, destructible, team, AI, the whole element tree),
mesh by mesh, shape by shape, terrain and notes, against main's loader on SP_01, MP_005, MP_009GR and ONS_MP_004, with
and without geometry and with either backend; the snapshot test passes in both.

`level.load`, median of three alternating runs per loader, files in the system cache (PC busy with other work: ±20 %):

| | before | after |
|---|---:|---:|
| MP_005, `geometry=False` (116 partitions), Rust core | 0.27 s | **0.20 s** |
| SP_01, `geometry=False` (170 partitions), Rust core | 1.02 s | **0.79 s** |
| MP_005, `geometry=False`, Python reader | 0.32 s | 0.31 s |
| SP_01, `geometry=False`, Python reader | 1.24 s | 1.14 s |
| MP_005, whole load (collision, terrain, water), Rust core | 12.1 s | 11.5 s |
| SP_01, whole load, Rust core | 12.6 s | 12.2 s |

A level load reads only its own partitions and the blueprints it places (a few hundred files), so DBX is a small
part of it: the whole load is collision meshes, the height field and water (Python and NumPy). The batch helps the
DBX part by about a quarter with the Rust core; the rest of the load is where E1's Rust scene will save time.

## FBRB bundles (2026-11-16, branch `rust-e0b`)

`bc2_formats::fbrb`, ported from `core/bfbc2core/fbrb.py`: the same entries and the same bytes as the Python reader
on every bundle of the game copy and `work\ww2` (1,123 bundles, 833,461 entries, 28.6 GB of entry data;
`core/tests/test_fbrb_native.py --game`, 3 minutes on 6 processes). Bundles read from the system cache.

| | Python | Rust via Python, 1 by 1 | Rust via Python, `read_many` | Rust alone, 1 thread | Rust alone, 16 threads |
|---|---:|---:|---:|---:|---:|
| every entry of MP_005's 8 bundles (Dist and Package, 92.8 MB on disk, 309 MB of entry data) | 1.56 s | 0.58 s | **0.36 s** | 0.52 s | **0.29 s** |
| the entry list of every bundle of the game copy (1,086 bundles, 8.9 GB, 740,662 entries) | 1.96 s | 0.86 s | | 0.31 s | **0.04 s** |

- `python rust\bench_fbrb.py --extract | --entries <folders>`; `cargo run --release -p bc2-formats --example
  fbrb_bench -- [--headers-only] <folders>`.
- **Decompression is the cost, and a bundle is one gzip stream**, so one bundle cannot be split across cores:
  parallel reading is one bundle per core, and a level's load is bound by its largest bundle (MP_005's level-00,
  48 MB). The Rust inflater (zlib-rs) is about 2.7x faster than Python's zlib loop with its per-entry copies.
- **The entry lists through Python** are bound by building 740,000 Python `Entry` objects; Rust alone reads every
  header of the game in 0.04 s.
- The gzip reader is `flate2` on **zlib-rs**, a Rust port of zlib: `miniz_oxide` (flate2's default) does not refuse
  a back-reference before the start of the data ("invalid distance too far back"), so a damaged bundle zlib refuses
  read without an error (found by the damaged-bundle test, kept as a unit test).

## The level loader in Rust (2026-11-16, branch `rust-e1a`)

**Contended:** every figure below was taken while an OPFOR rebuild / WW2 install ran on the PC (it loads all cores),
so absolute times are noisy (±30 % or more); the ratios and the stage shares are what to read. Files in the system
cache; `level.load` medians of 3 runs, the scene alone best of 3.

- `python rust\bench_level.py [--runs N] <levels>`: `level.load` and each stage it calls, in three modes:
  **python** (everything in Python), **main** (main before E1a: the Rust DBX / FBRB readers, the rest in Python),
  **rust** (the Rust scene and readers: today's `level.load`).
- `cargo run --release -p bc2-scene --example scene_bench -- <OPFOR folder> <level> [runs]` (`RAYON_NUM_THREADS=1`
  for one thread): the scene alone, its walk and XML reads, and each reader.

### The profile (main, before the port)

| `level.load`, whole load | MP_005 | SP_01 |
|---|---:|---:|
| **total** | **6.08 s** | **6.35 s** |
| collision index (NavKit's `havok_index` + `mesh_aliases`: two globs and 17,500-18,200 XML files regex-scanned) | 5.54 s | 5.42 s |
| DBX partitions (`read_many`) | 0.095 s | 0.178 s |
| blueprints (`resolve_blueprints`) | 0.064 s | 0.055 s |
| terrain mesh | 0.056 s | 0.066 s |
| collision triangles (Havok packfiles) | 0.040 s | 0.046 s |
| teams, shapes | 0.044 s | 0.095 s |
| terrain read, water, the unpack check | 0.02 s | 0.02 s |

The collision index was 85-95 % of every load measured (MP_005 7.45 s, of which 5.69 s `mesh_aliases` and 1.08 s
`havok_index`, in the first, cProfile run); the terrain, water and Havok readers the plan named were 0.1-0.2 s
together. Port order: the index, then the Havok triangles, the terrain and the water, then the scene.

### Before and after

| `level.load` | python | main (before) | **rust (after)** | after vs before |
|---|---:|---:|---:|---:|
| MP_005, whole load | 9.97 s | 6.08 s | **0.59 s** | 10.3x |
| SP_01, whole load | 6.69 s | 6.35 s | **1.04 s** | 6.1x |
| MP_005, entities only (`geometry=False`) | 0.32 s | 0.23 s | **0.10 s** | 2.3x |
| SP_01, entities only | 0.71 s | 0.47 s | **0.30 s** | 1.6x |

In rust mode the whole load is the scene call (MP_005 0.36 s, SP_01 0.44 s), Python's own trees of the partition and
static groups for `Entity.element` (0.05 / 0.16 s), and building the `Entity` objects and the NumPy world-space meshes
(the rest, about 0.2-0.4 s). On the 18 levels of `core/tests/test_scene_native.py` the whole load went from 6-16 s
(the Python loader) to 0.5-1.5 s.

### The scene alone (Rust, no Python objects)

| | MP_005, 16 threads | MP_005, 1 thread | SP_01, 16 threads | SP_01, 1 thread |
|---|---:|---:|---:|---:|
| whole load | **0.30 s** | 1.42 s | **0.33 s** | 1.51 s |
| collision index (walk + XML) | 0.30 s | 1.39 s | 0.32 s | 1.39 s |
| of it: the walk (both file kinds, one pass) | 0.07 s | 0.29 s | 0.07 s | 0.30 s |
| of it: reading the XML (205 / 196 MB) | 0.23 s | 0.79 s | 0.25 s | 0.81 s |
| entities only (partition, blueprints, teams, shapes) | 0.017 s | 0.026 s | 0.034 s | 0.045 s |
| Havok triangles (136 / 172 packfiles, from memory, 1 thread) | 0.002 s | | 0.003 s | |
| terrain: parse / mesh (2048², stride 2) | 0.002 / 0.014 s | | 0.001 / 0.011 s | |
| water mesh | < 0.001 s | | < 0.001 s | |

- **The index is now the scene's whole cost, and it is I/O:** 17,589 XML files (205 MB) are read for the mesh
  aliases, of which only about 7 % have a `Mesh` field (measured on 4,000 of `mp_common`'s), so stopping early in each
  file would save little. Reading the aliases from the binary partitions, or caching the table by the files' sizes and
  times, would take it to the walk's 0.07 s (EDITOR_ARCHITECTURE.md 3.8, "The scene": open).
- **The readers are cheap in Rust:** Havok 15-20x, the terrain mesh 4-5x faster than NumPy on one thread; neither
  mattered for the load.
- Parity, not timing, set the design of the Python side: the world-space meshes stay NumPy's
  `t @ m[:3, :3] + m[3, :3]` (BLAS may fuse multiply-adds, so a Rust product could differ in the last bit).

## Shader databases and cull trees (2026-11-16, branch `rust-e1c`)

Profiled first (PC busy with other work: figures ±30 %). Tools: `cProfile` and stage timers on the two places the
suite spends time on these formats: the WW2 level build's shaders step (`wake_shaders.main`: base mp_005, every stock
level a donor, Dx9 / 10 / 11, replayed into a temp folder with Guadalcanal's needs list) and `culltree.py add --auto
--misplaced 2 --ref-dir <each work\levels folder>` (console_cull's run on Onslaught MP_005). Per-file timings:
`cargo run --release -p bc2-formats --example shaderdb_bench -- <file.res>...` (Rust alone).

**Shader databases: where the time was.** Reading and writing a database was never slow in Python: the records are
byte slices (35-80 ms to parse, 35-100 ms to write a 20-50 MB file). The whole game copy (276 databases, 6.1 GB):
read from the bundles 23 s, parse 9.7 s, write 13.1 s; OPFOR's work files (304, 7.4 GB): parse 10.8 s, write 21.2 s.

| one level's shaders-step merge (Guadalcanal, Dx9 / 10 / 11) | Python | Rust core |
|---|---:|---:|
| reading the 74 databases out of the level bundles (`fbrb.read`, already Rust) | 22.1 s | 20.6 s |
| parsing them | 8.6 s | 2.9 s |
| the merge (`Merger.add_donor`) | 7.5 s | **0.6 s** (`needed_test`, Python) |
| writing the three results | 0.36 s | 0.08 s |
| **total** | **40.5 s** | **25.9 s** |

| one database (mp_005 Dx10, 33 MB, 20,366 programs) | Python | Rust through Python | Rust alone |
|---|---:|---:|---:|
| parse | 52-67 ms | 20-24 ms | 1.5-2.3 ms (borrowed) |
| write | 62-104 ms | 13-20 ms | 4-13 ms |

- **The merge's time was an algorithm, not the format:** `add_donor` tested every donor entry and mesh-part name
  against every needed path (5 million calls of `needed` per level). `needed_test` looks each name's path prefixes
  up in a set: the same answers (tested on 3,006 names x 200 lists, and on the four WW2 merges), 12x faster, in
  Python. The merge itself was not ported: what remains of it is 0.6 s.
- **Through Python the parse is bound by the objects:** Rust reads a database in 2 ms; building its ~70,000 Python
  `bytes`, `str` and `Shader` objects takes the other 20. The writer writes straight into the new `bytes` object.
- **What was left was the bundle reads** (20 of 26 s): `wake_shaders` loaded each level's Dx9, Dx10 and Dx11
  databases with three `fbrb.read` calls, each decompressing the bundle's payload up to its entry.
  `shaderdb.load_levels(levels, dxs)` reads a level's databases in one call, several bundles at a time on all cores
  (`fbrb.read_many`), and gives what `load` gives per Dx (or its `ShaderDbError`). On it (OPFOR branch `rust-e1c`:
  `wake_shaders.merge_stock` / `add_bfbc2`, `wake_install.index_templates` and its Dx9 donor scan), with identical
  results: the merge **21-22 s -> 8-9 s per level** (all four WW2 levels, Dx9 / 10 / 11), the donor scan 6.9 ->
  3.3 s, the template index 13 -> 12 s (one database per bundle: only the parallel reads help). The price: a merge
  holds its three Dx at once, peak commit 1.2 -> 2.0 GB. `renderqa.truth_index` uses it too.
- **A WW2 level build** (16-22 min) spends about 40 s of its shaders step here; the rest of the step (2-3 min) is
  `wake_shaders.meshes` (DBX reads of BFBC2's partitions), and the clone / translate steps are HLSL compilation and
  Python logic, not database I/O. The install's "indexing the stock shader templates" reads every stock Dx9
  database once (cached): about 13 s in Python, of it 3 s parsing.

**Cull trees: where the time was.** The WW2 pipeline does not run `culltree.py` (BF1943's own trees are converted as
DBX); console_cull does, once per Onslaught level. Its `add` run took 200-220 s:

| `add --auto --misplaced 2`, 37 reference folders (Onslaught MP_005) | Python | Rust core |
|---|---:|---:|
| finding the reference levels: listing (glob) and reading 109,660 `.dbx` for `ExclusiveStaticCullTree` | 28 + 108 s | 12 s |
| one level partition: `Doc` / `Level` / `CullTree` (77,335 elements) | 213 / 537 / 349 ms | 100 / 218 / 179 ms |
| **the whole run** (subprocess, end to end; 87 reference levels and trees) | **190 s** | **61 s** |

- Ported: the reference search (`level_partitions`: the folder listing as glob gives it and the files searched on all
  cores) and `Doc`'s element reader (the same `El` objects, built from Rust). Not ported: the tree builder (`build`,
  `flatten`, `insert`, `encode`: 2-3 s of a run) and the writer (`CullTree.write`, `Doc.tobytes`: one tree and one
  level per run, under a second); the shape statistics (`misplaced`, `shape_for`, about 8 s) are Python geometry
  over the collected instances.
- `Doc` is 2x faster, not more: each of a level's 77,000 elements is a Python object with seven attributes set from
  Rust; Rust reads the stream in a few milliseconds.

## Not ported yet

- The DBX **writer** is in Rust (`dbx::write`, tested by round trips) but Python's `dbx.write` still writes the
  files the suite produces: the Python tools build Python trees, and converting them to Rust to write would cost
  more than it saves today. E1's `levelwrite` (Rust trees) is where the Rust writer takes over.
- **FBRB packing** stays with FbrbTool (`pack`, with DICE's padded layouts, `fbrb.manifest` and size2 rules):
  the Rust reader has only a small test writer (`fbrb::build`, as `fbrb.build`). E1's `levelwrite` is where packing
  moves to Rust.
