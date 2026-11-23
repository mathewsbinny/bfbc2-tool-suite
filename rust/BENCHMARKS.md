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
  times, would take it to the walk's 0.07 s (E1e below: the binary alone did not, the cache did).
- **The readers are cheap in Rust:** Havok 15-20x, the terrain mesh 4-5x faster than NumPy on one thread; neither
  mattered for the load.
- Parity, not timing, set the design of the Python side: the world-space meshes stay NumPy's
  `t @ m[:3, :3] + m[3, :3]` (BLAS may fuse multiply-adds, so a Rust product could differ in the last bit).

## The alias table, NavKit's collision readers, a WW2 level (2026-11-16, branch `rust-e1e`)

**Contended:** an OPFOR rebuild and sibling branches' builds ran during these runs (`bench_level.py` said so); ratios
and stage shares are what to read. Files in the system cache.

### The alias table

The E1a index read every converted XML file for the mesh aliases. Two ways out were measured (`scene_bench`, MP_005,
best of 5; SP_01 the same within 10 %):

| MP_005, collision index | 16 threads | 1 thread |
|---|---:|---:|
| the walk alone (packfiles, XML, `.dbx`) | 0.073-0.080 s | 0.40 s |
| reading the 17,589 XML files (205 MB) | 0.25-0.29 s | 2.8 s |
| reading the 19,575 `.dbx` files (87 MB) | 0.27-0.28 s | 5.7 s |
| index, E1a (XML) | 0.28-0.36 s | 1.8 s |
| index from the binary partitions (`dbx::field_ref`) | 0.31-0.36 s | 1.8 s |
| **index, cache warm** | **0.11 s** | **0.39 s** |
| index, cache cold (reads and writes it) | 0.36-0.42 s | 1.8 s |
| `havok_index` alone (no aliases) | 0.062-0.07 s | 0.37 s |

- **Reading the binary is not faster:** the cost is opening 17,600 files (about 0.2 ms each, the virus scanner
  included), not their bytes, so 87 MB instead of 205 MB changed nothing. It is kept for what it removes (the text of
  the converted XML is no longer read where a partition is binary) and because it gives the same table.
- **The cache is what makes it fast:** keyed by each XML file's and `.dbx`'s size and modification time, which the
  walk gets from the folder listing without opening a file, so a warm index is the walk plus four cache files
  (`aliases-<hash>.bin`). Any changed, new or removed file is read again; a damaged, foreign or other-version cache
  file is ignored and replaced; writes are atomic (a temp file renamed over the old one).
- **Parity:** the Python reference (NavKit's `mesh_aliases` on the XML), the Rust index without a cache, cold and
  warm: the same packfile index and the same alias table, same order, on all 37 prepared levels (`havok_index` alone
  too). One trap was found: MP_005's `emitter_sw_meshparticle_tow_01` is an empty *deleted* marker whose XML was left
  over with a `Mesh`; the index reads the XML wherever the binary cannot answer, as the reference does.

| `level.load` (rust mode, median of 3) | cache off | cache warm |
|---|---:|---:|
| MP_005, whole load (of it the scene) | 0.64 s (0.33) | **0.41 s** (0.16) |
| SP_01, whole load (of it the scene) | 0.98 s (0.35) | **0.84 s** (0.19) |

### NavKit's collision readers on the Rust core

`detailed`, `ladders`, `navsample_offline.level_geometry` and `navwater_mesh.object_triangles` read through
`bfbc2core.collision` (`havok_index`, `index`, `blueprint_triangles`). SP_009, run one after the other, e1a (NavKit's
Python collision) against e1e:

| SP_009 | before | after |
|---|---:|---:|
| `detailed.flagged_sectors` | 1.75 s | 0.07 s |
| `detailed.choose` | 1.01 s | 0.08 s |
| `ladders.extract` | 1.02 s | 0.07 s |
| `navsample_offline.level_geometry` | 9.62 s | 0.58 s |
| `navwater_mesh.object_triangles` | 1.71 s | 0.15 s |
| whole build (`navregress.build_level`, detailed zones included) | 97 s | 93 s |

The build itself is NavKit's own compute (the detailed sectors were about 1 s of it). Parity: every intermediate
(the four tools' results, pickled) and `pathdata.hpd` byte for byte the same before and after on SP_009, SP_004_B
(78 ladders), ONS_MP_004 and SP_008_B; the `.hpd` compared with each sector's build stamp (date and time, written
from the clock) set to zero, the only field that differs between two builds.

### A WW2 level: Guadalcanal

WW2 levels are Package-only bundles, which NavKit's prepare does not list, so it was prepared by hand into a temp
OPFOR-like folder (nothing under OPFOR's `work\`): FbrbTool `unpack` of the Package `guadal_canal\level-00.fbrb`
into `work\levels\guadal_canal\dist` and DbxEdit `xml` on it, `terrain-00.fbrb` into `terrain\`, an empty `pkg\` and
`game\`, NavKit's common data joined in by junctions; then `bench_level.py --opfor <temp folder> guadal_canal`.

| Guadalcanal (7,144 entities, 5,228 meshes) | python | main | rust, cache off | rust, cache warm |
|---|---:|---:|---:|---:|
| whole load | 14.47 s | 14.81 s | 0.91 s | **0.45 s** |
| entities only | 0.28 s | 0.20 s | 0.09 s | 0.08 s |

The Rust scene and the Python loader gave the same level entity by entity (`test_scene_native.compare`), and the
same collision index (592 packfiles, 953 aliases).
## The writers in Rust (2026-11-16, branch `rust-e1b`)

Contended: other agents were building and testing on the same 16-thread PC throughout, so every number is noisy
(the same run twice differed by up to 20 %). Best of 3 for the DBX writer, best of 2 for packing.

### The profile (before the port)

- **DBX writing is a small share of the pipelines.** The dbxport fair ports (`test_dbxport_fair.py`, all
  categories, under cProfile): 1,345 s, of which `dbx.write` 1.9 s in 2,353 calls (0.1 %), `dbx.parse` 15.8 s. The
  OPFOR WW2 installer (`wake_install.py`, not run: it writes into `game\`) calls `dbx.write` in nine places on the
  partitions it edits, a few thousand files at most: about 1 ms per file in Python (below).
- **Packing is the larger cost, and it is deflate.** The WW2 install packs each level's `level`, `terrain`,
  `ingame`, `loader` and `vo/en` bundles, `ww2_common`'s three and the shared bundles it patches, one FbrbTool
  process after another. A level bundle (186-341 MB) takes 17-25 s in FbrbTool, nearly all of it zlib on the
  payload; FbrbTool on all 1,086 bundles of the game copy took 2,281 s of process time.

### Before and after

| | before | after | |
|---|---|---|---|
| `dbx.write`, MP_005 (2,567 files, 31.4 MB) | 1.38 s (Python) | 0.30 s (Rust) | 4.6x |
| `dbx.write`, Guadalcanal (`work\ww2\guadal_canal`, 77 files, 3.3 MB) | 0.17 s | 0.03 s | 5.0x |
| `dbx.write`, all of `work\ww2` (15,179 files, 147 MB) | 7.22 s | 1.46 s | 5.0x |
| pack, the WW2 install's 19 bundles (4 levels x 4 + `ww2_common` x 3), one after another | 124.8 s (FbrbTool) | 107.7 s (Rust) | 1.16x |
| of it: a level bundle (Wake `level-00`, 340 MB zipped, 4,963 entries) | 24.9 s | 20.9 s | |
| of it: `ww2_common` `level-00` (302 MB, 17,426 entries) | 25.5 s | 22.9 s | |
| pack, a stored bundle (`streaming_sounds-00`, 592 MB, 4,744 entries) | 6.2-9.2 s | 7.7-8.7 s | (I/O) |
| pack, all 1,086 bundles of the game copy (process time, 6 at once) | 2,281 s | 2,476 s* | |

\* measured before the match comparison was made 8 bytes at a time (deflate 1.12 -> 0.85 s on `BFBC2Game.exe`,
19 MB; Python's C zlib takes 1.13 s on the same file).

- **The DBX writer is 4.6-5x faster**, and every byte the Python writer's. What is left is reading the Python
  tree: walking it (one `getattr` per field) costs about as much as the writing; the string table keeps each string
  object's id by its address, so a parsed tree's shared strings are read and hashed once.
- **Packing is bound by deflate, and deflate cannot be split.** The bundle must be FbrbTool's bytes, so the payload
  is one zlib stream compressed on one thread: the port is as fast as C zlib, about 15 % faster than FbrbTool
  overall (no process start, files read on all cores). The large gain is across bundles: `fbrb.pack_many` packs
  each on its own core, so the WW2 install's 19 bundles would take about the longest one's time (about 25 s)
  instead of the sum (about 2 minutes), once its scripts call `pack_many` instead of FbrbTool one at a time.

### Parity

- DBX: all 48,119 distinct binary corpus files (29.6 million elements), written back and after random edits:
  the Rust writer's bytes are the Python writer's; 41 edge trees (shapes, kinds, strings, nesting) give the Python
  writer's bytes or exception through `dbx.write`; DbxEdit's roundtrip leaves 3,000 Rust-written files unchanged.
- Packing: all 1,086 bundles of the game copy unpacked by FbrbTool and packed by both: byte-identical. Against the
  game's own files: 62 identical, 919 identical but for the gzip headers (DICE: xfl 0 / os 0x0b; .NET: xfl 4 /
  os 0), 105 also but for the payload of marker-only zipped bundles (DICE writes an empty 20-byte gzip stream,
  .NET's `GZipStream` none). Every deflate stream DICE wrote is zlib level 6.
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

- **Unpacking** stays with FbrbTool (`unpack`, writing the folder and `fbrb.manifest`); the Rust reader extracts
  every entry in memory (`fbrb::extract`), but writing FbrbTool's folders and manifests is not ported.
- **OPFOR's scripts** (`wake_install.py`, `ww2_level_install.py`, the onslaught tools) still run FbrbTool.exe for
  `pack`; switching them to `bfbc2core.fbrb.pack` / `pack_many` is a change in the OPFOR repository.
- zlib's fast levels 1-3 (`deflate_fast`) are not ported: nothing the suite writes uses them.
- The DBX **writer** is in Rust (`dbx::write`, tested by round trips) but Python's `dbx.write` still writes the
  files the suite produces: the Python tools build Python trees, and converting them to Rust to write would cost
  more than it saves today. E1's `levelwrite` (Rust trees) is where the Rust writer takes over.
- **FBRB packing** stays with FbrbTool (`pack`, with DICE's padded layouts, `fbrb.manifest` and size2 rules):
  the Rust reader has only a small test writer (`fbrb::build`, as `fbrb.build`). E1's `levelwrite` is where packing
  moves to Rust.

## The converter pipeline and NavKit (2026-11-16, branch `rust-e1d`)

**Contended:** the PC ran other builds the whole time (sibling ports' tests, OPFOR tools), so single figures vary by
30 % or more; the before / after pairs of one table were taken under the same load where possible. cProfile adds
about 1.5-2x to pure-Python code and nothing to native calls, so a profile's shares lean towards the Python parts.

### The profiles (before the port)

**A WW2 level build:** `ww2_level_build.py coral_sea` (OPFOR's, on the suite's main: the Rust DBX / FBRB readers),
under cProfile: 1,982 s (unprofiled, the last rebuild: 1,293 s). This run also rebuilt the DBX catalogue cache once
(a game-copy change had made it stale), which a normal build does not.

| rank | where the time went | s | share |
|---:|---|---:|---:|
| 1 | `fbrb.read` (in Rust: inflating bundles), of which **`meshview.Index.read` 790 s**: 2,070 BFBC2 partitions read one at a time (dbxport's templates 377 s, the sound porter's donors 314 s, the shader step 86 s); shader databases 74 s; the catalogue rebuild 53 s | 991 | 50 % |
| 2 | `dbx.parse` (Rust reader, Python trees): 90,952 partitions | 208 | 10 % |
| 3 | `dbxcatalogue.build` (the one-off cache rebuild) | 187 | 9 % |
| 4 | `pcbuild.compile_hlsl`: 2,444 HLSL compiles of the shader translation (the compiler's time) | 176 | 9 % |
| 5 | `wake_shaders.role_score` (OPFOR's donor scoring, 1.16 million calls) | 169 | 9 % |
| 6 | `shaderdb._parse` (shader databases) | 100 | 5 % |
| 7 | `dbxport._best` (template choice) | 46 | 2 % |
| 8 | `audio.encode_xas` (22 sounds) | 15 | 0.8 % |
| | texture, mesh and Havok conversion together (11 textures, 2 meshes, 11 packfiles) | < 4 | 0.2 % |

By step: fx 208 s (190 of it the catalogue), objects 11, level 186, shaders 144, clone 318, billboards 26,
translate 249, grid 816 (618 of it the sound porter, mostly its reads).

**A NavKit build:** `navregress.build_level` on sp_004_b (both pitches: 13 normal and 27 detailed sectors, 7.7
million surfaces at 0.2 m), main, under cProfile: 1,064 s.

| rank | where the time went | s | share |
|---:|---|---:|---:|
| 1 | `Level.neighbour` (with `connected`, `navwalk.step_fits`, `surfaces`): 142 million calls - from `build_graph`'s search between route-node groups 392 s, `components` 136 s, `trace_edges` 75 s, `line_walkable` 53 s | 656 | 62 % |
| 2 | `build_graph`'s own loops (the search's queue and dicts) | 80 | 8 % |
| 3 | `navwalk.walkable`'s step-rule floods: `roof_islands` / `islands_of` 61 s, `flood` 45 s | 106 | 10 % |
| 4 | `trace_edges`, `components` own time | 48 | 4 % |
| 5 | `narrow_lips`, `matched_flags`, `stair_slope`, `headroom` | 41 | 4 % |
| 6 | the collision index (`havok_index` glob: E1e's) | 5 | 0.5 % |

### What moved, and what it gave

**Textures** (`bc2_convert::texture`, `texture.convert`): every XenonTexture of BF1943's source (704, 193 MB):

| | Python | Rust via Python, one by one | Rust via Python, `texture_convert_many` |
|---|---:|---:|---:|
| convert all 704 | 48.1 s | **0.37 s** | **0.12 s** |

A level build converts few textures (0.7 s on coral_sea); Wake's steps convert the 704 several times over.

**The partition reads** (`meshview.Partitions`: a bundle's partitions read in one pass and kept, by path, size and
time, 768 MB at most): one partition of mp_common's level bundle cost 0.94 s, all 8,446 of them (36 MB) 2.42 s.
`ww2_level_build.py coral_sea` on main with the cache patched in (`meshview.Index.read` replaced at run time by the
branch's code, OPFOR's outputs compared and restored), against the last rebuild's log:

| step | before (rebuild log) | after | |
|---|---:|---:|---|
| fx | 24 s | 49 s | load (no reads to save) |
| objects | 11 s | 11 s | |
| level | 155 s | 122 s | |
| shaders | 82 s | 42 s | |
| clone | 93 s | 121 s | load |
| billboards | 2 s | 12 s | load |
| translate | 149 s | 199 s | load (HLSL compiles) |
| **grid** | **777 s** | **101 s** | the sound porter's reads |
| **whole build** | **1,293 s** | **667 s** | 1.9x |

The outputs are the same files (506 of them, sha1 before and after); `install_dbnames.pkl` and, in one run,
`terrain_rules.json` differ between any two runs of the build, with or without the cache (a pickled set and a JSON
dict whose order follows Python's string hashing: OPFOR's, reported) - both were put back as they were. Guadalcanal
and Iwo Jima's grid steps (800 s each in the last rebuild) do the same reads.

**NavKit** (`bc2_navbuild`, through `navbuild.Level.native()` and `navwalk.native_grid()`):
`test_navbuild_native.py --game` builds each level twice in one process, Python then Rust, the path data compared
byte for byte (the build clock fixed for both: every sector stores its build time):

| level | Python | Rust | | path data |
|---|---:|---:|---:|---|
| sp_009 (9 sectors) | 153 s | **19 s** | 8.1x | identical, 406,216 bytes |
| sp_004_b (40 sectors, 27 detailed) | 566 s | **186 s** | 3.0x | identical, 8,235,888 bytes |
| sp_008_b | 618 s | **201 s** | 3.1x | identical, 8,477,720 bytes |
| ons_mp_004 | 346 s | **90 s** | 3.8x | identical, 7,623,072 bytes |

Profiled again after the ports (sp_004_b, cProfile): 166 s instead of 1,064 s. What is left is Python around the
Rust calls and the walkability rules not ported: `walkable` 59 s (`narrow_lips` 15, `stair_slope` 14, `headroom`
7), `build_graph` 43 s (its nearest-cell `min` over an area's cells for each portal node: 28 s), `trace_edges`' own
loop 17 s, `build_areas` 16 s, the collision index's glob 2 s (E1e's). The graph search itself is 12 s in Rust.

The four levels are the quick / game tiers' two (sp_009, sp_004_b) and two more; NavKit's output is unchanged, so its
DICE-parity measures (`navregress`) are too.

**Not worth porting, measured:** mesh and Havok conversion (under 4 s of a level build); the HLSL compiles are the
compiler's time (running them on several threads is the gain there, in the shader translation); OPFOR's
`role_score` is OPFOR's Python.

## NavKit's walkability rules, areas and cell picking (2026-11-17, branch `rust-e2a`)

What E1d's profile left (above: `walkable` 59 s, `build_graph` 43 s, `build_areas` 16 s of sp_004_b's 166 s), moved
to `bc2_navbuild`; the Python stays the reference and the fallback (`BFBC2_PURE_PYTHON=1`, `navbuild.use_native`):

| Python | Rust | binding |
|---|---|---|
| `navwalk.headroom` (and so `low_headroom`) | `rules::Grid::headroom` | `NavGrid.headroom` |
| `navwalk.stair_slope` | `rules::Grid::stair_slope` | `NavGrid.stair_slope` |
| `navwalk.narrow_lips` | `rules::Grid::narrow_lips` | `NavGrid.narrow_lips` |
| `walkable`'s stair landings | `rules::Grid::landing` | `NavGrid.landing` |
| `navwalk.near_objects`, `on_objects` (after its NumPy box filter) | `rules::Grid::near_objects`, `on_objects` | `NavGrid.near_objects`, `on_objects` |
| `navbuild.build_areas` with `components` and `bisect` | `Level::build_areas` | `NavLevel.build_areas` |
| `build_graph`'s centre and portal cells (its `min` over an area's cells) | `pick::centre_cell`, `pick::nearest_cells` | `navkit_centre_cells`, `navkit_nearest_cells` |

`walkable` makes the run's `NavGrid` once (`navwalk.native_samples`) and hands it to every rule.

**Float parity.** E1d's rules (NumPy 2's promotion, `round` half to even) and three more, each found by measuring:

- `math.hypot` is CPython's own `vector_norm`, not the C library's `hypot`: ported (`pymath::hypot`; Dekker's exact
  product and the fused one gave the same answers on 300,000 pairs on this CPython 3.13).
- `x ** 2` on a Python float is the C library's `pow(|x|, 2.0)`, which differs from `x * x` on 1 value in 2,000
  (2 million random values): `pymath::sq` calls the same `pow`, kept from being folded into a multiply. A NumPy
  `float64`'s `** 2` is the same `pow` (300,000 values), so a float64 origin is taken as a Python float is.
- `float32 < python float` compares in `float32` under NumPy 2 (the Python float is cast): `narrow_lips`' flat test
  on the normals does so.

`tests\test_navbuild_native.py` (quick) checks those three on 150,000 values against `math`, then every rule, both
backends, on random runs (pitch 0.2 and 0.8, float32 and float64 normals, every option), `walkable` with every rule
on (stair, dynamic, exclude, footprint and forbidden objects), `build_areas` with and without bisection, and the
cell picking: 263,863 calls, no difference. Rust line coverage of `bc2-navbuild`: 99.5 %.

**Whole levels** (2026-11-17, a quiet PC; `tests\test_navbuild_native.py --game --levels
sp_009,sp_004_b,sp_008_b,ons_mp_004`, the build clock fixed for both backends): every level's path data byte for
byte the same. "E1d" is the table above (E1's Rust, the rules still Python, a busier PC):

| level | Python | E1d | Rust (E2a) | vs Python | path data |
|---|---:|---:|---:|---:|---|
| sp_009 | 85 s | 19 s | **9 s** | 9x | identical, 406,216 bytes |
| sp_004_b | 478 s | 186 s | **58 s** | 8x | identical, 8,235,888 bytes |
| sp_008_b | 542 s | 201 s | **46 s** | 12x | identical, 8,477,720 bytes |
| ons_mp_004 | 342 s | 90 s | **37 s** | 9x | identical, 7,623,072 bytes |

`rust\bench_navkit.py` alone, one build each: sp_009 8.1 s (Python 81.9 s), sp_004_b 47.2 s (Python 456.5 s).

Profiled again (sp_004_b, cProfile: 60 s with the profiler's overhead): `merge_detailed` 51 s of it, mostly
`trace_edges`' own loop 12 s (15 s with what it calls), the graph search 6 s (Rust), `trace_emit` 3.5 s,
`roof_islands` 5 s, `add_cover_nodes` 3.6 s, `walkable`'s Python around the rules 2.3 s, the samples' load 4 s,
`build_areas` 1.6 s and the cell picking 0.8 s (both Rust now); 21 million `dict.get` and 13 million `list.append`.
`trace_edges` / `trace_emit` are the next port if NavKit builds need to be faster still.

## Retired Python (2026-11-17, branch `retire-e1`; 2026-11-18, branches `retire-r1` - `r3`, `retire-f1a`)

The full test tier passed on round E1 (140 of 140, 2026-11-17), which under `EDITOR_ARCHITECTURE.md` 3.8 retires the
Python of the ported **logic** of round E1d. Retired (deleted; the module is now required for them, README.md "The
Rust core"):

| piece | Python removed | Rust |
|---|---|---|
| texture conversion | `texture.py`: `convert_python`, `_convert_dxt5a`, `_convert_565`, `_convert_argb8`, `_dxt5a_block`, `_rows4`, the backend switch (`use_native`, `backend`); 422 -> 253 lines. `tiled`, `packed`, `layout` stay: format knowledge `texqa.py`'s independent decoder and the tests use | `bc2_convert::texture` (`texture.convert`, `convert_many`) |
| NavKit's grid walks | `navbuild.py`: `neighbour`'s Python search (and its `prefer`), `components`' flood, `trace_edges`' per-side fallback, `line_walkable`'s walk and `_crosses_d`, `_search`, the dtype declines; `navwalk.py`: `walkable`'s flood, `islands_of`, `matched_flags`' loop, `native_grid`'s declines; 2,054 -> 1,934 and 1,062 -> 1,018 lines. `Level.connected` and `surfaces` stay (the stage code calls them), as do `step_fits` and `unseeds`' island drop (not ported) | `bc2_navbuild` (`NavLevel`, `navkit_matched_flags`) |

Kept, with their live Python-vs-Rust comparison: E2a's walkability rules, `build_areas` (and `bisect`), the cell
picking and `pymath` (quick and game tiers only so far), and every format reader and writer (for good).

**Before deleting, what the tools really feed the Rust** (no input may leave the Rust path for the Python one):

- Textures: every XenonTexture of BF1943's source (704) and of OPFOR's Onslaught builds (451) converted by Rust, 0
  refused, 0 fallbacks. Over 596,804 built textures (11 formats, every size 1..69 and the powers of two to 16,384
  plus odd ones, every mip count, 2D and cube; data exactly the layout's size), Rust reads past the data on 448: the
  Python raised on 224 and "converted" 224 - all RGBA16F (13), 1..16 x 513 or 513 x 1..16, 4 or more levels - into
  textures whose levels have the wrong sizes (short slices). Not ported: they are refused with a clearer
  `ConvertError` ("the layout reads past the end of the texture data (a level outside the console storage ...)";
  a Rust test covers them). The 1,362 quick-tier textures: Rust's bytes or messages equal the Python's on all.
- NavKit (every `NavLevel` method, `Level.native` / `native_grid` declines, `matched_flags`' dtype decline, the Python
  search and `prefer` neighbour, `line_walkable` on a non-`Level`, counted by wrapping them): sp_009 (32,623
  `line_walkable`, 249 searches, 626 side-neighbour calls ...) and sp_004_b (525,019 `line_walkable`, 37,385
  `neighbour`, 3,142 searches, 7,752 side-neighbour calls, 28 floods) - 0 declines, 0 errors, 0 Python paths. The
  quick tier's random levels: the only declines were the test's own (float16 gradients: 2; a surface outside the
  level: 1 error). The call sites now cast instead (flags to bool / uint8, gradients of another type to float64) and a
  surface outside the level raises `ValueError`; `test_detail_features.py`'s stand-in grid became a real `Level`.

**The new oracle: results recorded from the Python before it was deleted.**

- `bfbc2-console-convert/tests/texture_golden.json` (214 KB, one case per line): recorded by running the new test's
  cases through `texture.convert_python` itself - the 1,362 seeded quick-tier textures (output SHA-1 or the
  `ConvertError` message: 639 converted, 723 refused) and every BF1943 (704) and Onslaught (451) texture (input and
  output SHA-1); Rust's results equal the recording on all 2,517.
- `bfbc2-auto-navkit/tests/navbuild_golden.json` (17 KB): the SHA-1 of every walk's answers on 24 + 16 seeded random
  levels (neighbour on every surface, direction and rule; `build_areas`, `components`, `trace_edges`, 120
  `line_walkable` per level with and without D sides, the searches; `matched_flags`, `walkable`, `islands_of`), and
  of four levels' path data with the build clock fixed (2026-11-17 12:00): sp_009 406,216 bytes, sp_004_b 8,235,888,
  sp_008_b 8,477,720, ons_mp_004 7,623,072 (the sizes of the E2a table above), with the SHA-1 of each level's input
  files. Proven against the original code (`abe6cc7`, extracted aside): its all-Python walks give the recorded
  digests on all 40 random levels, as does its Rust; its Rust builds give the recorded path data on all four levels.
  Its all-Python builds (the retired reference itself, the same clock) give the recorded path data on sp_009 (189 s)
  and sp_004_b (954 s, a busy PC); sp_008_b and ons_mp_004 were proven Python = Rust in the E2a table above.

`tests/test_native_required.py` checks the required module (missing, stale, matching) and the download on a copy of
the suite; navregress stays the independent check against DICE's path data.

### Round R1: the rest of NavKit (2026-11-18, branch `retire-r1`)

The full test tier passed on 2026-11-17 23:54 (173 of 173, `ee25186`), which retires the Python of the logic ported in
rounds E2a, E3c, E3h and E3o's NavKit part. Retired (deleted; the module is required, `navbuild.require_native`,
`bfbc2core.native.require`):

| file | Python removed | lines |
|---|---|---:|
| `navbuild.py` | the switch (`_load_native`, `_native`, `_NATIVE_ERRORS`, `backend`, `use_native`) and every fallback: `build_areas`' tiles / components loop and `bisect`; `trace_edges`' rows; `trace_emit`'s runs with `bridge_union` and the old `clip_bridges`; `_edges_from_store` and stage's per-sector refusal paths; `build_graph`'s Python (centre / portal picks `_centre_cells` / `_nearest_cells`, `edge_pairs`, `connect_groups`, `add_cover_nodes`' loop, `gabriel_prune`'s loop, `add_order_nodes`, `repair_detours`) | 2,356 -> 1,548 |
| `navwalk.py` | the switch and the Python of `headroom`, `stair_slope`, `narrow_lips`, the landings, `near_objects`, `on_objects`, `near_characters`, the seed / anchor picks, the unseeds' walk, `roof_islands`' rule | 1,090 -> 759 |
| `navdiff.py` | `area_cells_reference` and the switch (`pieces`' loop stays: see below) | 431 -> 384 |
| `core/bfbc2core/navdata.py` | `use_native`, the areas loop over parsed meshes, `_cells_reference`, `_union_roots_reference` | 476 -> 398 |
| `core/bfbc2core/navpath.py` | `use_native`, `route_reference`, `norm_formula` (the estimate is the fused formula, `NORM = 'fma'`: NumPy's bits on the recording PC, now on every PC) | 262 -> 170 |
| `rust/parity_e3o_nav.py`, `rust/bench_navkit.py` | `load` (navdata against its reference), `--backend python` | 199 -> 122, 75 -> 69 |

1,695 lines of tool and core code removed (339 written in their place: casts, clear errors, docstrings).

**Kept, and why** (the ruling keeps format readers and writers for good):

- `kymesh.parse_mesh_reference` and `kymesh.use_native`: the runtime mesh record's only Python reader (a format
  reader); `test_e3o_native.py` still compares it with the Rust reader, and `parity_e3o_nav.py mesh` on any folder.
- `navsample.load`'s version 1 / 2 record loop and `navsample.use_native`: the sample files' only Python reader (a
  format reader); `test_navbuild_native.py` still compares it with the core's.
- `hpdtool` (the path-data container: reader and writer) and `kymesh.write_mesh`: format code, never ported.
- `navdiff.pieces`' loop (`_pieces_loop`, was `pieces_reference`): not a Rust port's twin - the live path for fewer
  than 48 segments and for numbers that are not finite (`pieces` in NumPy otherwise; both E3h, both Python).
- `navbuild.drop_one_way` and `restore_one_way` (dict rows: `ONE_WAY_PORTALS = 'restore'` and `trace` read them),
  `detail_pass` / `navdetail`, `Level.connected`, `step_fits`, `near_points`, `ladder_ends`, `objective_floor`,
  `closest_between`: not ported.

**Research switches removed** (the user's decision, a separate commit on `retire-r1`): `BRIDGE_UNION` (False was the
old `clip_bridges`, which left open outline ends - a superseded bug path), `DEDUP_GRAPH_NODES` (False: two route
nodes on one surface joined by zero-length edges - a superseded bug path) and `DETOUR_REPAIR` with
`DETOUR_TRIGGER` / `DETOUR_MAX` / `DETOUR_LINK_ALL` / `DETOUR_PASSES` (`repair_detours`, a rejected experiment).
The Rust core builds the exact bridge union and one node per surface only, as the defaults always did; no tool, CLI
flag or test set them. Numbers of another type are cast where the Python's gates
sent them to the reference (a NumPy integer or float32 pitch, origin, tile, window bound, `r`, `width`; normals of
another float type for the flat test); triangles that are not n x 3 x 3, a sector window with a step, a mesh the core
cannot read, a boundary or route graph that is not finite raise `ValueError`.

**What the fallbacks took on real data** (before deleting; expect 0): the whole-level builds below ran with no Python
left to fall to (a gate would now raise); `navdata.load`, `navdiff.area_cells` on every area and the route graph's
`PathGraph` on every path-data file of the game copy and of OPFOR's `work\navkit`, `work\navbuild`, `work\navgen`
(`r1_scan_nav.py`, read only): 264 files, 2,372,253 areas (the 34,050 sectors E3o read): **0** would have taken a fallback (983 s).

**The oracle: results recorded from the Python before it was deleted** (step 1, commit `bce738a`, from the Python
code itself; the Rust gave the same digests while recording):

- `bfbc2-auto-navkit/tests/navbuild_golden.json`, `retired`: a digest of every E2a / E3c / E3h comparison's
  answer - 331 + 456 + 39 (the rules on 14 random runs at pitch 0.2 / 0.8, float32 and float64 normals, every option;
  `walkable` with every rule; `build_areas` with and without bisection; 60 tracing chains, 36 sector graphs,
  `build_areas_many`, `near_characters`, roof islands with the rule's settings varied and their counters, cover nodes,
  Gabriel; 10 whole stages over a corner of four sectors; 12 sector graphs with ladders; 16 unseeds; 61 random
  outlines' area cells). `quick` (the E1d grid walks) and `levels` (sp_009, sp_004_b, sp_008_b, ons_mp_004's path
  data, the build clock fixed) as before.
- `tests/navdata_golden.json` (new): `quick` - `navdata.load` of 40 built files (all in Python: the reference mesh
  reader, the areas loop, `area_cells_reference`, `_union_roots_reference`), 300 union-finds, 10,000 routes on 400
  seeded graphs (`route_reference`; 25 per graph digested); `game` - every path-data file of the game copy and DICE's
  file of every prepared level, 35 (the prepared levels' DICE files are the game copy's own; 188,988 areas) files: the NavData, every area's cells (`area_cells_reference`, NumPy) and
  pieces, keyed by the file's SHA-1 (a changed file is reported, not compared), and bench_sweep's 400 routes on
  sp_009 / sp_004_b. Recorded in 1,284 s from the step-1 code (the Rust gave the same digests on all 35 while recording), peak commit 2.06 GB (the all-Python NavData of sp_005_b; the Rust check now: 1.19 GB, 415 s).

**Proof** (`retire-r1`): `test_navbuild_native.py` quick - the grid walks and all 826 recorded answers the same, no
Python left; `--game --levels sp_009,sp_004_b,sp_008_b,ons_mp_004` - sp_009 406,216 bytes 4 s, sp_004_b 8,235,888 bytes 18 s, sp_008_b 8,477,720 bytes 19 s, ons_mp_004 7,623,072 bytes 12 s: all four as recorded; `test_e3o_native.py` quick and the
recording's game checks (`navdata`, cells, pieces, routes) - all as recorded; every NavKit unit test; navregress on
two small levels (the game tier's) (sp_004_b ground 98.49, extra 7.23, connect 100.00, AI 99.4; sp_009 99.97 / 2.16 / 100.00 / 100.0: within tolerance of the last regression); `python run_tests.py quick -j 2` - 95 of 96 twice: first mypy (three annotations in the new code, fixed: `test_types.py` all passed), then the DBX writer's `work\ww2` corpus sample, which a WW2 rebuild started beside it was rewriting (it passed on the first run; nothing NavKit); the full tier's all-level regression (about 5 hours) not run this round.
`test_bridge_union.py` now builds its patterns as levels and checks the core's `trace_emit` outlines (522 of 600 with
bridges, 337 overlapping: all closed); the old `clip_bridges` comparison went with it.

### 2026-11-18 (branch `retire-r3`): the shader translator's HLSL emission

The full tier passed on 2026-11-17 (173 of 173, `ee25186`). Retired:

| piece | Python removed | Rust |
|---|---|---|
| Xenos -> HLSL translation | `xenos_hlsl.py`: `_Emitter` (declarations, operands, every ALU / fetch op, control flow, assembly), `translate_python`, `_f`, `HELPERS`, `FLT_MAX`, `INSTANCED`, `INSTANCE_PRELUDE`, `backend()`, the Python fallback of `translate` / `translate_many` (status 2, untouched-tree check); 783 -> 150 lines. Kept: `Interface`, `TranslateError`, `ENGINE_SIZE`, `SHADOW`, `PROBE_*`, `INSTANCE_SEMANTICS` (callers use them); new `require()`, `helpers()` | `bc2_shader::hlsl` (`translate`, new `translate_given`); bindings `xenos_tree_program` (any tree handed over), `xenos_helpers` |

**Kept, and why.** `xenos.py`'s decoder (`parse_shader_python`, its `use_native` / `BFBC2_PURE_PYTHON`) is a format
reader - the Xenos container and microcode format - and stays for good as the Rust decoder's live reference, as do
`disasm` and the opcode tables. `xenos_exec.py` (the differential test's interpreter) is not a port: it is the
independent check of the translation. In `bfbc2-console-readers/apt/apt.py` nothing is plain logic: the dump, the
instruction records and the constants are the APT reader (round E3n ported the reader; it keeps its Python and its
switch). `bfbc2-menu-tools/aptgraft.py`'s `assemble_reference` (with `_ins_len_reference`, `_place_reference`,
`_emit_reference`), `Movie._stream_reference` and `Movie.write_reference` (`BFBC2_APTGRAFT_REFERENCE=1`) are Python
references of a faster Python algorithm, not of a Rust port: untouched.

**What the Rust had to take first.** The Python translator translated every tree the Rust did not: a tree whose
microcode was read (`xdiff.py --bisect`: its interpreter reads it), the Python decoder's (`use_native(False)`),
copies, pickles and trees built by hand (`test_xdiff`'s, `test_asset_accuracy`'s, OPFOR's
`test_ww2_terrain_patches`'), and status 2 (an interface value of another type, an empty material value, a sampler fix
with other format fields, a non-ASCII engine-named constant). Ported: `xenos_tree_program` reads a tree attribute by
attribute as the Python read it (instance `__dict__` first, then `getattr`; a vector operand's `swizzle`, a scalar
one's `components`; an attribute read only on some paths may be missing), converting only the instructions the CFs
run and computing `cube` over the tree's own `instructions` (`translate_given`). What a program cannot hold - a
literal not four 32-bit floats, an unknown operation name, a number of another type - and status 2 raise
`ValueError` saying what (the interface's field named), never another backend.

**Fallbacks on real data before deleting** (the old code, `translate_python` calls counted): OPFOR's
`wake_translate.translate_db` over Wake's Dx10 and Dx11 databases (the rebuild's default filters, terrain strips and
patches; compiles skipped): 3,576 translation jobs, 0 Python fallbacks, 0 status 2; the new code gives the same
3,576 outcomes, and the same as the retired Python run alone (`BFBC2_PURE_PYTHON=1` on the old suite: every job
handed to `translate_python`). The game corpus: 0 status 2 over 24,960 translations; the quick tier: 0 outside the
tests' own deliberate cases.

**The oracle, recorded from `translate_python` itself before it was deleted** (`test_xenos_native.py --record`, the
Python tree translated by the old code's Python):

- `bfbc2-shader-tools/tests/xenos_golden.json` (197 KB): the 1,500 built programs under three interfaces (4,428
  translations: 1,473 texts' SHA-1, 2,955 `TranslateError` messages), 1,000 programs of random microcode (220
  translated), the dispatch's 60 `translate_many` jobs.
- `bfbc2-shader-tools/tests/xenos_golden_game.json` (496 KB): all 8,320 distinct programs of BF1943's and BFBC2's
  Xbox databases, three interfaces each (22,863 texts, 2,097 `TranslateError` messages in plain).

Every tree kind is checked against them: fresh (the program kept in Rust), materialized (microcode read, handed
over) and the Python decoder's (handed over): all 4,428 + 220 + 60 quick and 8,320 x 3 game outcomes as recorded.
Also: a hand-built tree (plain classes, a CF instruction outside `instructions`), an edited materialized tree, the
declines. `test_xdiff --game` (7 shaders exact against the interpreter), `xdiff --bisect`, `test_renderqa --game`
and `test_asset_accuracy` (its emitter probes now translate one-instruction trees) pass.

**Speed of the handed-over path** (`bench_xenos.py`, Wake, busy PC, one run): a Python-decoded tree handed over and
translated 0.75 ms a program, the hand-over alone 0.24 ms; the retired `translate_python` measured 0.76 ms in the
same conditions, the kept program 0.40 ms. Callers' trees are kept programs (OPFOR: 3,576 of 3,576).

### The converters (2026-11-18, branch `retire-r2`)

The full test tier passed on main `ee25186` (173 of 173, 2026-11-17 23:54), which retires the Python of the
converters' ported **logic** (rounds E3a, E3g, E3i, E3o). Under the ruling a format reader or writer keeps its Python
for good; conversion logic with a Rust twin loses it. Retired (deleted; the module is required for them,
`require()` at the tools' entry points):

| piece | Python removed | kept, and why | Rust |
|---|---|---|---|
| EA-XAS codec | `audio.py`: `xas_encode_frames_python`, `xas_decode_frames_python`, `encode_xas_python`, the dispatch (`_with_xas`, `_load_native`, `backend`, `use_native`, `_encode_native`'s fallback, `BFBC2_PURE_PYTHON`); 695 -> 576 lines | the block framing (`_xas_stream`, `xas_block_samples`), the Wave / SPS readers and writers, `COEFS` (the format's table; `test_audio.py`'s own SNR search uses it) - never ported, or format | `bc2_convert::audio` (`xas_encode_many`, `xas_decode_frames`) |
| mesh vertex loop | `mesh.py`: `convert`'s per-vertex Python loop and its dispatch (`_load_native`, `backend`, `use_native`); 532 -> 514 | `element()` (the per-value rule: format knowledge, and OPFOR's `wake_audit` checks meshes with it), `parse` / `write` / `elements` / `console_layouts` / `decode` (the format), `relayout` and the region / window planning in `convert` (never ported) | `bc2_convert::mesh` (`mesh_vertices`) |
| Havok walk | `havok.py`: `Packfile` (the walk and its per-byte record of widths and owners), `ignorable`, `verify_reference`, the dispatch (`_load_native`, `backend`, `use_native`) and the fallbacks of `convert_packfile` / `verify`; 526 -> 293 | `classes(exe)` (reads the game's hkClass initializers: the Rust takes its layouts from it), the wrapper's conversion (never ported), the type constants | `bc2_convert::havok` (`havok_convert_packfile`, `havok_verify_packfile`) |
| Granny conversion | `granny.py`: `_to_pc_python`, `_marshal`, `_swap`, `_WORDS`, `_HALVES`, `to_pc` / `to_pc_many`'s fallbacks; 1,223 -> 1,161 | the reader (`Granny`, `root`, `members`, `size`, ...: the format's reference, still compared with the Rust reader both ways, `use_native` / `backend()` / `BFBC2_PURE_PYTHON` now for the reader only), `bone_table` / `renumber` (DICE's table format; `test_granny_port.py`'s units), `add_tracks`, `shift`, curves (never ported) | `bc2_convert::granny` (`granny_to_pc`, `granny_to_pc_many`) |

Format reader or logic, decided per piece: Granny's reader parses a file format into its object tree - it stays,
and its Rust twin is still checked against it on every Granny resource of the Xbox copy, the game copy and BF1943.
`to_pc` (swapping each section and object by its type, the CRC, DICE's table renumbered) is conversion: retired.
Havok's `Packfile` reads nothing out - it walks the packfile only to reverse each value in place (and to record what
it touched for `verify`): conversion, retired; `classes(exe)` and the wrapper stay. In mesh.py the readers and
writers stay and only the per-vertex loop goes; `element()` stays because it is the format's per-value rule in a
callable form (an OPFOR audit tool uses it). The EA-XAS encoder (a search for the least-error coefficients) and
decoder are the codec, ported logic: retired; the framing around them never had a Rust twin.

The binding changed with it: `havok_convert_packfile` and `havok_verify_packfile` raise `ValueError` with a refusal's
message (they returned `None` for refusals too, so the Python walk could say why) and return `None` only where the
walk stops without one; and the walk's class-name check now refuses a fixup into another section before it looks at
the class names, as the reference's `or` did (103 seeded refusals had a vaguer message otherwise).

**What the tools feed the Rust** (each dispatcher wrapped and counted on the reference's inputs, before deleting):

- Sounds: every Wave of BF1943's source (2,750, streamed ones with their waveDataSns entries; 2,810 encode calls) and
  every in-memory Wave of BFBC2's unpacked Xbox bundles (1,680; 1,847 calls): **0 declined**, the PC Wave the same
  bytes both ways on all 4,430. Quick tier: 2 of 1,652 encode calls declined - the test's own pcm beyond int16 and
  float32 pcm, which the NumPy encoder took (widened to int64, floats truncated): now a `ValueError` saying what is
  wrong (and the wrong shapes' unpacking errors became messages naming `(samples, channels)`).
- Meshes: every BF1943 mesh set port (273: 534 `mesh.convert` calls, 1,570 vertex runs) and every Onslaught console
  MeshData with layouts (239: 723 runs): **0 declined**, every input already `bytes`. Quick: 8 of the 400 seeded
  meshes name the unknown format 0x01, where the Python loop raised `KeyError: 1` - now `ConvertError` ("vertex
  element format 0x01 -> 0x01 is not converted"); no mesh the Python converted is refused.
- Physics: every BF1943 physics port's walk (191 of 202; havokport refuses 11 before it) and every Onslaught console
  physics file (594): **0 stops**, the same bytes. Quick: of 3,600 seeded packfiles, **7** the Python "converted" while
  its walk claimed bytes far past the data (to byte 18,160 - 65,024 of 656 - 992-byte files: its short slices; each
  checked with the retired code) - now `ConvertError`; 1,216 refusals whose Python exception was not its
  `ConvertError` (`struct.error` 1,120, `IndexError` 85, `ValueError` 11) are now `ConvertError`s saying the packfile
  reads past its data, and 40 the Python refused only after reading past the data (a width clash further on) say so
  too.
- verify: verify-havok's 400 Onslaught files, all answered by the Rust record, the same (exact, reasons, offsets).
  Quick: 2,003 seeded comparisons - the record gave `None` only where the reference raised (`ConvertError` 928,
  `IndexError` 28 on a PC copy longer than the console file): now the walk's own refusal (`verify_packfile` asks
  `convert_packfile` first, the reference's order), else "the PC file is longer"; 4 the reference refused only after
  reading past the data say so.
- Granny: every Xbox Granny resource of mp_common / sp_common (2,660) and BF1943's (1,495 conversions, the soldier
  animations also with the bone map): 4,155 converted by both, the same bytes, **0 declined**. Quick: 2,193
  conversions of built and damaged resources - none the Python converted is refused; 965 of the 1,454 refusals now
  carry the Rust core's message (`where: why` as a `GrannyError`, where the Python raised `struct.error`, `IndexError`,
  `TypeError` or a `RecursionError`, or the same `GrannyError` worded otherwise).

**The new oracle: results recorded from the Python before it was deleted** (each by the retired code itself through a
one-off recorder: the tests' own cases with the reference swapped in), each file with a `"now"` section listing,
case by case, the answers the Rust core gives otherwise on purpose (the refusals above; `--record` rewrites it from
the current code, which refuses to touch the reference rows):

| file | cases (reference rows) | now |
|---|---|---|
| `bfbc2-console-convert/tests/audio_golden.json` | quick 1,657 (encodes, decodes of the encoded frames, random frames, `encode_xas` streams at 6 rates); `bf1943` 2,750 and `xbox` 1,680 Waves (input and output SHA-1) | 4 |
| `bfbc2-bf1943-import/tests/ports_golden.json` | `mesh-quick` 401, `havok-quick` 3,600; `bf1943-mesh` 273 and `bf1943-havok` 202 ports (bench_ports' output digests); `onslaught-mesh` 239 and `onslaught-havok` 594 (input and output SHA-1) | 8 + 1,263 |
| `tests/havok_verify_golden.json` | quick 2,003 (the (offset, reason) list's digest); `onslaught` 400 (exact, reasons, offsets) | 32 |
| `bfbc2-anim-tools/tests/granny_golden.json` | quick 2,193; `xbox` 2,660 and `bf1943` 1,495 conversions (input and output SHA-1) | 965 |

The game rows have no `"now"` entries: on the game's files every answer is the reference's.

**Proof** (2026-11-18, below-normal priority, the module rebuilt from this branch's sources): quick tier
`python run_tests.py quick -j 2` 95 of 96 (mypy: a loop variable reused in `test_ports_native.py`, renamed; mypy, ruff and that test then pass); `test_audio_native.py --game --every 10` (275 + 168 Waves as recorded,
convert_many = convert), `--game` all 4,430 in the `--record` run (0 "now" entries from game files);
`test_granny_native.py --game` (2,660 + 1,495 conversions as recorded, 2,660 + 4,534 + 756 resources read the same
by both readers) and `test_granny_port.py --game` (2,654 Xbox -> PC equal their PC twins, 31 BF1943 animations equal
BFBC2's); `test_ports_native.py --game` (every BF1943 mesh, physics and effect port identical with the indexes and the
reference searches, the mesh / physics outputs as recorded, Onslaught 594 + 239 as recorded, emitter index 18,343);
`test_havok.py` (400 of 400 shared files equivalent, 594 of 594 converted); `test_e3o_native.py --game` (all passed,
verify-havok's 400 as recorded); `cargo clippy --all-targets -D warnings`, `cargo test -p bc2-convert`.

### Round F1a: the core format readers and writers (2026-11-18, branch `retire-f1a`)

The user's ruling of 2026-11-18 (`EDITOR_ARCHITECTURE.md` 3.8: "port everything, confirm it's a perfect port, then
retire the Python" - format readers and writers included, superseding "format readers stay for good" above) retires
the Python of the core formats, which had run on Rust through two full tiers (140 of 140 and 173 of 173,
2026-11-17). Recorded first (commit `fae35c1`), proven, then deleted; the module is required for them
(`bfbc2core.native.require`: `NativeMissing` saying how to get it, before any input is read):

| piece | Python removed | kept | lines |
|---|---|---|---:|
| `core/bfbc2core/dbx.py` | `_py_parse`, `_py_to_xml`, `_py_write`, `_strings`, `_escape`, `_damaged`, the switch (`_load_native`, `backend`, `use_native`) and `write`'s fallback to the Python writer | the API (`parse`, `to_xml`, `parse_many`, `to_xml_many`, `read`, `write`, `Element`, `DbxFile`, `DeletedEntry`, `is_binary`, `numbers`, `floats`, `value`, `put_varint`); `_import_native` and `PURE_PYTHON_ENV` (the readers that still have a Python twin find the module through them); `value()`'s .NET number text (not ported) | 529 -> 312 |
| `core/bfbc2core/fbrb.py` | `_header`, `_cstring`, `_py_read`, `_damaged`, the switch, the cache's "no core" answers, `_tool_pack` / `_tool_unpack` (FbrbTool.exe without the Rust core, and `unpack`'s retry of a refused bundle with it: the in-process engine is FbrbTool's, the same error) | the API (`entries`, `names`, `read`, `read_many`, `read_uncached`, the cache controls, `pack`, `pack_many`, `unpack`, `unpack_in_process`, `fbrbtool`, `build`) | 398 -> 208 |
| `core/bfbc2core/terrain.py` | `_py_parse`, the switch | `Heightfield`, `parse`, `read`, `tail`, `find`, `HEADER` | 161 -> 124 |
| `core/bfbc2core/water.py` | `_py_parse_mesh`, `_py_parse_visual`, `shaders_of`'s Python, the switch | the API | 223 -> 136 |
| `core/bfbc2core/collision.py` | `_navkit`, `_py_triangles`, the Python `index` / `havok_index` / `triangles_many` through NavKit's functions, the switch | the API and the mesh-alias cache | 186 -> 128 |
| `core/bfbc2core/level.py` | `load`'s Python loader (the entity and static-group loop, `collision.index` / `triangles_many` / `heightfield_of` / `water.read` calls), `resolve_blueprints`, `assign_teams`, `shapes_of`, `category_of`, `transform_of`, `blueprint_of`, `CATEGORIES`, `OBJECT_CATEGORIES`, `TEAMS`, `SHAPE_FIELDS`, the switch | `load` (the Rust scene), `Level`, `Entity`, `Mesh`, `Shape`, `read_many`, `terrain_name`, `heightfield_of`, `water_of`, `terrain_from_samples` (no caller used the removed names) | 660 -> 381 |
| `bfbc2-culltree/culltree.py` | `Doc`'s element loop, the switch (`_import_native`, `_load_native`, `backend`, `use_native`), `level_partitions`' glob / read when the core is off | `Doc` (its string table stays Python: no Rust twin), the rest of the tool; `level_partitions` still lists by glob a folder whose names are not valid Unicode and reads a file the core could not, for its error | 1,472 -> 1,409 |
| `bfbc2-auto-navkit/tools/levelobjects.py` | `havok_index`'s walk, `mesh_aliases`' XML scan, `triangles` over `hkpack.py` (each now `bfbc2core.collision`'s; `main` takes the index and aliases in one walk) | `hkpack.py` (`navsample_offline.py` reads packfiles with it: a format reader of a later round) | 272 -> 228 |

1,230 lines of Python deleted, 252 written (docstrings, `_rust()`, clear errors). Not in this round: `shaderdb.py`'s
reader / writer (its queries have `*_reference` twins), `xenos`, Granny, APT, `kymesh`, `navsample`, `mesh`, `audio`'s
formats, `dbxbin` (with E4a's retirement).

**What the Rust answers otherwise, on purpose** (no real input; each counted in its test):

- `dbx.write` (`rust/bc2-py/src/writers.rs`): the walk keeps its own stack (any depth: the Python wrote 700 deep and
  raised `RecursionError` at 3,000); children of any iterable (a generator) and raw bytes as `bytearray.extend` takes
  them (a memoryview, a list of ints) are now written, as the Python wrote them. Refused with the reason: an empty tag
  (the Python wrote tag 0, which reads back as an end mark), a 2-character string taken as an attribute pair (the
  Python unpacked `'ab'` as `('a', 'b')`), a tag / attribute / text that is not a `str` (`TypeError`; the Python's was
  an `AttributeError` from `.encode`), a kind outside 0-15 (`ValueError` for -1 too). `DELIBERATE` in
  `test_dbx_write_native.py` lists the five edge trees that differ from the recording.
- `culltree.Doc` (`bc2_formats::culltree::elements`, `culltree_elements` raising `ValueError`): a stream cut inside an
  element raises `CullTreeError` ("truncated file (it ends inside the element at byte N)") where the Python raised
  `IndexError` (42 of the quick tier's 2,146 cases; 77 more where both raised `CullTreeError`, worded otherwise); a
  varint past 64 bits raises `CullTreeError` ("... longer than 64 bits (not a cull tree or level partition)") where the
  Python read a big integer (113 cases, all the test's own inserted long varints; 50 more the Python refused later).

**What the fallbacks took on real data** (before deleting; expect 0): `dbx.parse` / `to_xml`, `fbrb`'s reader,
`terrain`, `water`, `collision` and `level.load` had no fallback while the module was there (a Python path only with
`BFBC2_PURE_PYTHON` or without the module). `dbx.write`'s fallback to the Python writer: 0 of the corpus's 48,120
trees as read and after seeded edits (the game run below writes every one with no Python left). `culltree.Doc`'s
fallback to the Python loop: 0 of the 1,269 trees and level partitions (522 MB) of the game copy's level bundles and
work\levels / ww2 / console_cull, and 0 in console_cull's add run (every reference level read); in the quick tier 282
of 2,146 cases, all the test's own damage (above). `fbrb.unpack`'s retry with FbrbTool.exe: none needed since E3k (one engine).

**The oracle: the Python's answers, recorded before it was deleted** (`core/tests/record_f1a.py` in `fae35c1`,
deleted with the Python; each answer from the Python code itself - `dbx._py_parse` / `_py_to_xml` / `_py_write`,
`fbrb._header` / `_py_read`, `use_native(False)` for terrain, water, collision and level, NavKit's `levelobjects` over
`hkpack.py`, `culltree`'s loop - with the Rust's digest of the same case beside it: every case the same but the
3,000-deep tree; digests as `core/tests/goldens.py` makes them; a file, bundle or level changed since is counted, not
compared):

| file | size | cases |
|---|---:|---|
| `core/tests/dbx_golden.json` | 99 KB | 10 built, 4,254 damaged, 500 random files (XML and tree); the writer's built partition, 41 edge trees, 2,000 random trees |
| `core/tests/dbx_golden_corpus_0.json` - `_3.json` | 4 x 0.5 MB | 139,842 corpus files (work\levels, work\navkit\common, work\ww2), 48,120 distinct, 579 MB: XML, tree, written as read and after seeded edits, keyed by SHA-1 (Python 1,086 s, 3 processes) |
| `core/tests/fbrb_golden.json` | 162 KB | 19 built, 1,742 damaged, 600 random bundles; the messages and `read_many`; the cache test's built bundles (12 read patterns); 1,123 game and work\ww2 bundles, 833,461 entries, 28.6 GB (entries, zipped flag, payload start, every entry's SHA-1) |
| `core/tests/geometry_golden.json` | 64 KB | 1,808 built and damaged height fields, water files and packfiles; the collision index of built folders; 960 damaged copies of three MP_005 packfiles; 186 game bundles (527 height fields, 53 water meshes, 52 visuals and shader tables, 4,128 packfiles); the collision index of 37 prepared levels |
| `core/tests/scene_golden.json` | 3 KB | SP_01, MP_005, MP_009GR, ONS_MP_004, entities only and whole: entities, elements, shapes, meshes, blueprints without collision, notes, height field, terrain mesh, water (Python 0.6-2.1 s and 12.7-87.2 s a load; Rust 0.2-0.5 s and 0.7-1.25 s) |
| `bfbc2-culltree/tests/culltree_golden.json` | 218 KB | 2,146 built, damaged and random streams; add on a built level; 1,269 trees and level partitions of the game copy's level bundles and work\levels / ww2 / console_cull; console_cull's `add --auto --misplaced 2` on Onslaught MP_005 against every work\levels folder (Python 173 s, Rust 48 s, the same tree, level and 241-line report) |
| `bfbc2-auto-navkit/tests/collision_golden.json` | 1 KB | NavKit's `detailed`, `ladders`, offline sampler geometry and boat mesh triangles on SP_009 and SP_004_B (Python 30 / 27 s, Rust 1 s) |

**Proof** (`retire-f1a`, below-normal priority, the module rebuilt from this branch, no Python left): the game
checks against the recordings - `test_dbx_native.py --game` (48,120 distinct corpus files as recorded, 218 s; XML =
DbxEdit's on 31,404 fingerprinted), `test_dbx_write_native.py --game` (the 48,120 written as read and edited as
recorded, 292 s; the five `DELIBERATE` edges), `test_fbrb_native.py --game` (1,123 bundles, 833,461 entries, 28.6 GB as
recorded, 49 s), `test_geometry_native.py --game` (186 bundles, the index of 37 levels, as recorded, 21 + 75 s),
`test_culltree_native.py --game` (1,269 trees and partitions, 522 MB, the same; the work\levels listing = glob; the
add run's tree, level and 241-line report as recorded, 56 s), `test_fbrb_cache.py --game` (324 bundles, 20,850 reads;
the stored bundles), `test_scene_native.py` (4 levels x 2 as recorded; 0.2-0.5 s / 0.7-1.25 s a load) and
`test_collision_backends.py` (SP_009, SP_004_B as recorded); `test_native_required.py` (the core formats stop with
`NativeMissing` without the module, run under `BFBC2_PURE_PYTHON=1` with it); `python run_tests.py quick -j 2`
98 of 98 (249 s; a first run had 97: the corpus recording as one 2 MB file broke the repository's 1 MB rule - now
four shards); `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, the workspace's tests (in the
quick tier's `test_rust.py`). OPFOR's callers (read only) use only the kept API (`dbx.parse` / `read` / `write` /
`to_xml`, `fbrb.entries` / `read` / `pack_many`, `culltree.py` run by `console_cull.py` from the suite): none break.

## Retired C# tools (2026-11-18, branch `retire-r4`)

After the full tier (173 of 173, 2026-11-17 23:54) the C# command lines whose commands were ported and proven
identical (E3b: DbxEdit; E3k: FbrbTool) were retired as tools, as their Python twins were above
(`EDITOR_ARCHITECTURE.md` 3.8). The Rust core is required for their commands: `bfbc2core.dbxedit` / `fbrbtool`
(`run`, `run_many`, `run_cmd`, new `main`); `fbrb.pack` / `pack_many` / `unpack` lost their FbrbTool.exe fallbacks
here and in round F1a (above), merged. By hand:
`python bfbc2-dbx-tools\dbxedit.py` / `python bfbc2-fbrb-tool\fbrbtool.py` (the exes' command lines, messages and
exit codes).

| removed | lines |
|---|---|
| `bfbc2-fbrb-tool/FbrbTool.cs`, its `build.bat` | 464 + 8 |
| `DbxEdit.cs`'s command line (`static class Program`: `Main`, the command dispatch, usage); `build.bat` builds `DbxEditGUI.exe` only | 280 -> 7 (header) |
| `DbxTests.cs`' exe groups (`CommandLine`, `MakeGolden`, `Golden`, `Battery`) and `run_tests.bat`'s exe copy / `--golden` | 150 -> 6 |
| `rust/bench_dbxedit.py`, `rust/bench_fbrbtool.py` (the exe against in process; their results stay in E3b / E3k) | 253 + 78 |
| the switches and fallbacks: `BFBC2_DBXEDIT_EXE`, `BFBC2_FBRBTOOL_EXE`, `use_exe`, `EXE`, `_exe` (dbxedit, fbrbtool), `fbrb.fbrbtool` / `_tool_pack` / `_tool_unpack`, `pipeline.FBRB` / `DBX` / `run`, NavKit's and `level.heightfield_of`'s exe look-ups, `validate`'s exe branch | about 150 |
| run_tests' steps "FbrbTool build (when stale)"; "DbxEdit build" builds DbxEditGUI only | |

**Kept, and why.** DbxEditGUI (the window) stays until the egui map editor replaces it, built from DbxEditGUI.cs
and DbxEdit.cs; DbxEdit.cs (the DBX classes, `ToolLog`) is also LevelDiff's library, so only its command-line class
went (trivially safe: nothing referenced it; the GUI, LevelDiff and DbxTests build and pass). DbxTests.cs stays as
the GUI's and the C# classes' tests (85 checks, the window included; `--corpus` for the full tier).
`bfbc2core.paths.find_tool` stays (DbxEditGUI, LevelDiff). `fingerprints_795745.txt` stays: it is DbxEdit's own
recording (the XML and edit fingerprints `test_dbx.py`, `test_rust.py --corpus` and `test_dbxedit.py --game` check).

**The new oracle: the exes' answers, recorded from the exes before they went** (`core/tests/csharp_golden.py`;
built from `f10522d`'s sources, the same bytes as the main checkout's builds; texts with the test folder as `<dir>`,
the log as `<log>`, temporary names as `.tmp-N`; digests are the first 16 hex digits of a SHA-256):

- `dbxedit_golden.json` (24 KB): DbxTests.cs' 82 unit cases as command lines - exit code, stdout, stderr, every
  file's SHA-1.
- `dbxedit_golden_game_0.json / _1.json` (1.4 MB): a seeded 2,000-file sample of OPFOR's work\levels, work\navkit\common and
  work\ww2 (139,842 files; 1,592 distinct), each through xml, roundtrip, import --like / without a reference /
  edited (the edits seeded by the file's SHA-1), check and, on every 4th file by SHA-1, the edit battery: 12,464
  runs, keyed by the input's SHA-1 (591 s).
- `fbrbtool_golden.json` (660 KB): test_fbrbtool's 228 command lines (run in a folder 64 characters long, as the
  test does: FbrbTool's path-length rules depend on it), the 600 seeded damaged bundles, what .NET 4's GZipStream
  answered for the 3,000 seeded gzip streams (a C# probe), test_fbrb_unpack_native's 78 built / odd / damaged bundles
  (exit code, files left), test_fbrb_pack_native's 25 built and refused folders, test_fbrb_native's 2 built bundles.
- `fbrbtool_golden_game.json` (250 KB): every bundle of the game copy up to 300 MiB (1,081 of 1,086; the 5 larger
  ones, 307-565 MiB, were left out for memory) and the 40 bundles of the logged invocations: unpack (the folder's
  digest), list, and pack of the unpacked folder (the bundle's SHA-1); 693 distinct bundles by SHA-1.

The tests now check the in-process commands against these: `test_dbxedit.py` (quick: the 82 cases as recorded and as
DbxTests.cs expects; `--game`: the recorded sample whose files are still there unchanged - at least a quarter must
be - and every edit fingerprint), `test_fbrbtool.py` (quick: 228 lines, 100 damaged, 500 streams; `--game`: 600,
3,000, a 40-bundle sample and the logged invocations' bundles), `test_fbrb_unpack_native.py` / `test_fbrb_pack_native.py`
/ `test_fbrb_native.py` (built inputs; the game copy by SHA-1: unpacked and packed again in Rust, the result the
exe's), `test_fbrb.py` (round trips in process). A bundle rebuilt since the recording is counted as not recorded and
the sweeps fail when more than half are.

**Proof** (`retire-r4` merged with main after F1a / E4f, the module rebuilt from it, below-normal priority, one test at
a time): `python run_tests.py quick -j 2` 99 of 99 (221 s); `test_dbxedit.py --game` (160 of 160 edit fingerprints;
1,592 of 1,592 recorded corpus files, 12,458 runs as recorded, 75 s); `test_fbrbtool.py --game` (228 lines, 600
damaged, 3,000 gzip streams, 40 game bundles with 30,177 entries and 40 logged invocations' bundles as recorded);
`test_fbrb_unpack_native.py --game` (1,075 of 1,075 bundles up to 300 MiB of payload identical to FbrbTool.exe's
files, 226 s of unpacking); `test_fbrb_pack_native.py --game` (1,081 of 1,086 bundles, 6.94 GB, unpacked and packed
again in Rust byte-identical to the bundles FbrbTool.exe packed - the game's own file 59, but for the gzip headers 917,
and the empty payload stream 105; the 5 over 300 MiB not recorded); `test_fbrb.py --game 12`; DbxTests.cs 85 of 85;
`cargo fmt`, clippy `-D warnings`, 292 cargo tests. OPFOR (callers switched in its own commit): 373 tests, lint and
types included, and opforcore's 71.

## E3a: the XAS audio encoder (2026-11-17, branch `rust-e3a`)

**Provisional timings:** OPFOR's WW2 rebuild (`rebuild_all.py`, a `ww2_level_build.py` under it) ran the whole
time, and every figure below was taken at below-normal priority beside it on the 16-thread PC. The ratios hold
within a run; the absolute seconds will be lower on a quiet PC.

**What moved.** `bfbc2-console-convert/audio.py`'s EA-XAS v1 codec, the top cost of OPFOR's `bf1943_fx` step in the
round 0 profile (`audio.encode_xas`: 158 s of 262 s; 16 s of a level build): `xas_encode_frames` and
`xas_decode_frames` into `bc2_convert::audio` (`encode`, `encode_all`, `decode_frames`), bound as `xas_encode`,
`xas_encode_many` and `xas_decode_frames`. `encode_xas`, `decode_xas` and `xas_encode_frames` / `xas_decode_frames`
dispatch to it (`use_native`, `backend()`, `BFBC2_PURE_PYTHON`); the NumPy code stays as `xas_encode_frames_python` /
`xas_decode_frames_python`, the fallback and the reference. A pcm the port does not take (not 2-D, not integer, or
beyond int16) goes to the reference. The block framing (DICE's blocks, the SPS writer) stays in Python: slicing
bytes, nothing to gain. New: `encode_xas_many` (every sound's frames in one call, on all cores) and
`convert_many` (whole Waves side by side: ffmpeg's EA-XMA decodes in threads, the encoder outside the interpreter).

**Float parity.** The reference is NumPy on int64 with three float steps; each is exact in integers:

- the shift: `ceil(log2(max(r, 1) / 7.0))` of an integer residual is the least `m` with `r <= 7 * 2^m`. Measured, not
  assumed: NumPy's formula equals the integer rule for every `r` below 2^26 (the encoder's are below 2^25), 67
  million values (`test_audio_native.py`, quick tier). The reason it can hold: `r / 7` is never within float64
  rounding of a power of two unless it is one, and `log2` of a power of two is exact.
- the nibble: `rint((target - pred) / step)` divides an integer below 2^53 by a power of two, an exact quotient;
  NumPy's `rint` rounds half to even, so the port shifts and rounds half to even in integers (`rint_shift`; a Rust
  test compares it with `round_ties_even` on 200,000 pairs).
- the error: summed in float64, but of integers (30 squares below 2^32): exact, a `u64` here. `np.clip` and `>>`
  (floor) are the integer clamp and arithmetic shift; nothing overflows int64.

**Algorithm.** Every group of 32 samples is encoded on its own (its first two samples stored, its prediction chain
within it), so the reference's search can be done group by group, and its choice reproduced with less work. The
reference tries 12 candidates (4 coefficient sets x the shift the set's residual needs and the next finer / coarser)
to the end and keeps the least squared error, the first in its order on a tie. The port tries the same candidates:

- a candidate whose shift equals the one before it (the shift clipped at 0 or 15) is the same candidate again,
  later in the reference's order: never chosen, skipped;
- the likely winner first: on the BF1943 sample below, 93 % of groups take the set with the least residual (rank 0:
  1,711,011 of 1,847,084), 69 % at its own shift and 31 % the finer one, 22 groups a coarser one. So: the
  least-residual set (its shift, then finer), the other sets by residual, every coarser shift last;
- a candidate stops at the sample where its running error passes the best so far, or equals it while coming later
  in the reference's order (errors only grow).

The tie rule makes the order free: proptests compare the search with a transliteration of the reference (every
candidate to the end, NumPy's float steps) on 9,000 random, smooth and near-constant (tie-heavy) groups.

| BF1943 sample, encode only, Rust on one thread (`examples/xas_bench.rs`) | s |
|---|---:|
| the reference's search, every candidate to the end | 3.19 |
| skipping duplicates, stopping at the best error, the reference's order | 1.93 |
| **+ the likely winner first** (the library) | **1.34** |

**Threads.** A sound's frames (one per channel and 128 samples) are split over rayon's pool (64 frames at least per
task, so short sounds stay on one thread); `encode_all` runs many sounds at once as well. The Python binding releases
the interpreter (`py.detach`) for all of it.

**Timings** (the sample: every 10th Wave of Battlefield 1943's source, 275 sounds, 279 segments, 59.1 million samples,
mostly 48 kHz mono; decoded once to PCM, then encoded from memory; best of three, the Python once):

| encode (`encode_xas`, through Python) | s | vs Python |
|---|---:|---:|
| Python (NumPy) | 20.45 | |
| Rust, one thread (`RAYON_NUM_THREADS=1`) | 1.52 | 13x |
| Rust, all threads, one sound at a time | 0.318 | 64x |
| Rust, `encode_xas_many` (all at once) | **0.236** | **87x** |

| decode (`decode_xas` of the same 279 streams) | s |
|---|---:|
| Python | 1.20 |
| Rust | 0.140 (9x) |

**The whole conversion** (`audio.convert` of the same 275 Waves: read, ffmpeg's EA-XMA decode, encode, write; the
same bytes each way):

| | s |
|---|---:|
| Python encoder, one by one | 34.4 |
| Rust encoder, one by one | 13.4 |
| `convert_many`, 4 threads | 4.8 |
| `convert_many`, 8 threads | **3.4** (10x) |

Now ffmpeg is the cost: one process per XMA stream, about 45 ms each with its encode. In the full parity run below, the encoder's
share of all of Battlefield 1943's 2,750 Waves fell from 336 s (Python) to 4.4 s (Rust, one sound at a time).
OPFOR's `bf1943_fx` (158 s of encoding in the round 0 profile) should lose about 155 s, and a level build its 16 s;
OPFOR's callers (`wake_level.port_sounds`, `wake_vo.waves`) convert one Wave at a time and would gain `convert_many`'s
factor of 3 - 4 more on the decodes if they batched (not changed here: OPFOR is read only to this round).

**Parity** (`bfbc2-console-convert/tests/test_audio_native.py`):

- quick (no game data, 9 s): the shift rule (67 million residuals); 768 seeded signals (silence, full-scale noise,
  clipping extremes, constant +32767 / -32768, tones, clipped tones, tone and noise, quiet noise, square, impulses,
  random walk) at 16 lengths (0, 1, 2, 31, 32, 33, 127, 128, 129, 255, 256, 257, 1,000, 4,096, 4,113, 20,000) and 1,
  2, 3 and 6 channels, 4.4 million samples: every frame byte the same, their decodes the same; other integer dtypes
  and Fortran order; 5,000 random frames decoded (every coefficient-set field, including the 12 no encoder writes,
  and every shift); 108 `encode_xas` streams at 6 rates against the reference's framing (lengths at, around and
  between DICE's block sizes); `encode_xas_many`; the dispatcher.
- the whole corpus, once, 2026-11-17 (`--game`, the reference whole on every sound): **all 2,750 Battlefield 1943
  Waves** (in memory and streamed with their `waveDataSns` entries, 823 million samples) and **all 1,680 in-memory
  BFBC2 Xbox Waves** (216 million samples) converted by `audio.convert` with each backend: **4,430 of 4,430 PC Waves
  byte for byte the same**; `convert_many` = `convert` on 81 of them. Peak memory 4.1 GB: the reference's int64
  arrays on the longest music (about 70 bytes a sample).
- the game tier (`--game --every 10`, 68 s, peak 1.5 GB): every 10th of each, 275 + 168 Waves; the reference takes a
  sound longer than 2^20 samples a channel in pieces of whole frames (the same frames: the groups are independent;
  the quick tier checks it), so the full tier (`--game`, every Wave, ~8 minutes) fits in 2.5 GB.
- Rust: 11 unit and property tests in `bc2-convert` (`audio_tests.rs`).

## The rest of NavKit's generator (2026-11-17, branch `rust-e3c`)

What E2a's profile left (sp_004_b, cProfile: 60 s, `merge_detailed` 51 s of it), moved to `bc2_navbuild` in the order
of its cost; each keeps its Python as the reference and the fallback (`BFBC2_PURE_PYTHON=1`, `navbuild.use_native`,
which also switches `navsample`):

| Python | Rust | binding |
|---|---|---|
| `navbuild.trace_edges` (whole: kinds, sides, the symmetric portals, one-way candidates) | `trace::trace_edges` | `NavLevel.trace_edges`; `trace_edges_many` (every sector of a stage, in parallel) |
| `trace_emit` with `bridge_union` (runs, 45-degree bridges) | `trace::trace_emit` | `NavLevel.trace_emit` |
| `navwalk.roof_islands` after its islands | `graph::roof_drop` | `navkit_roof_drop` |
| `add_cover_nodes` (`add` still called back: one node per surface) | `graph::cover_nodes` | `NavLevel.cover_nodes` |
| `gabriel_prune` (pairs tested in parallel, a spatial grid) | `graph::gabriel_prune` | `navkit_gabriel` |
| `build_graph`'s walkable pairs (`edge_pairs`, nodes in parallel) | `route::edge_pairs` | `NavLevel.route_edges` |
| `build_graph`'s group joining (`connect_groups`: search, string pulling, waypoints) | `route::connect_groups`, `window::WindowSearch` | `NavLevel.connect_groups` |
| `stage`'s `build_areas` per sector, the areas' lowest / highest ground | `Level::build_areas` in parallel | `NavLevel.build_areas_many` |
| `walkable`'s seed and anchor picks (1 million `round` calls on sp_004_b) | `rules::Grid::column_pick` | `NavGrid.column_picks` |
| `navwalk.near_characters` | `rules::Grid::near_characters` | `NavGrid.near_characters` |
| `navsample.load`'s version 1 / 2 records (sp_004_b's normal runs) | `navsample::parse_runs` | `navkit_navsample_parse` |

Two Python-only changes in `stage` / `merge_detailed`, the same values: `np.unique` of the kept surfaces' columns
became `sorted_unique` (the column array is sorted already: 2 s of hashing), and the detailed pass's seed points are
picked with NumPy instead of a loop over every kept surface.

**More float parity.** `math.dist` is CPython's `vector_norm` too (`pymath::dist3`, `dist2` on cells); float `//`
(`build_graph`'s 15 m buckets) is CPython's `float_floor_div`, not `floor(x / w)` (`pymath::floordiv`; `1.0 // 0.1`
is 9); `np.hypot` is the C library's (Rust's `f64::hypot`: checked on 150,000 values, `navkit_float_ops`); an
array's `** 2` is NumPy's square, a multiply (checked); `f64::midpoint` is not `(a + b) / 2` near the float range's
ends, so the reference's expression is kept.

**The group joining.** Ported as it was, it ran slower than the Python loop around the Rust search (3.6 s against
1.8 s on sp_009): (1) a root walk without path halving let union chains grow within a pass - the reference's
halving `find` is used everywhere now; (2) a memo of neighbours in a hash map cost more than computing them. What
made it fast, the answers unchanged: the window's surfaces get dense local numbers (row by row they are contiguous
ranges of the level's), so the walks' visited marks, back links and the cell map are arrays; every kept surface's
neighbours on its own column are found once, in parallel, as local numbers; a target is looked up as the walk
reaches it instead of building the set of other groups' surfaces for each group; and a group whose walk found nothing
is not walked again while its members are the same and no other group's node stands on a surface that walk reached
(the same walk would enter no new cells). sp_009: 0.27 s, its Python twin 1.7 s; sp_004_b: 1.6 s (the search alone
was 7.3 s in E2a's profile).

**Parity.** `tests\test_navbuild_native.py` (quick): the E3c pieces against their Python twin with the Rust side
**strict** (the fallback switched off, so a refusal fails the test instead of running the Python): 60 tracing chains
(`trace_edges` alone and batched, `drop_one_way`, the detail pass, `trace_emit`; random and ramped levels with thin
second layers and decks, cut windows, small tiles: 870 portals, 130 border portals, 6,395 outline runs, 81 one-way
candidates, 40 detail features), 36 whole `build_graph`s (cover nodes, Gabriel, groups joined: hundreds of waypoint
nodes, order nodes), 24 `roof_islands` (the rule's settings varied, its counts too), 24 `add_cover_nodes`, 40
`gabriel_prune`, 16 `build_areas_many`, 16 `near_characters`, 12 sample files (versions 1 and 2, cut-off runs, a
record cut short): 456 calls, 0 differences; the recorded grid walks (which include `trace_edges` and the roof rule)
unchanged. Rust: `e3c_tests.rs` (34 tests in the crate; a property test runs the window search against the generic
one, twice, cells compared). Whole levels, the build clock fixed, against `navbuild_golden.json`: **sp_009, sp_004_b,
sp_008_b and ons_mp_004 byte for byte as recorded** on the Rust backend, and sp_009 and sp_004_b also on the Python
backend (E2a's and E3c's Python reference, driven through the test's `game()` with `use_native(False)`).

**Whole levels** (a rebuild of OPFOR's running beside: provisional; below-normal priority):

| level | E2a | E3c | | path data |
|---|---:|---:|---:|---|
| sp_009 | 9 s | **3 s** | 3x | as recorded, 406,216 bytes |
| sp_004_b | 58 s | **17 s** | 3.4x | as recorded, 8,235,888 bytes |
| sp_008_b | 46 s | **20 s** | 2.3x | as recorded, 8,477,720 bytes |
| ons_mp_004 | 37 s | **14 s** | 2.6x | as recorded, 7,623,072 bytes |

`rust\bench_navkit.py` alone: sp_009 3.3 s (Python backend 14.9 s; E2a 8.1 s), sp_004_b 15.3 / 15.9 s (Python backend
88.8 s; E2a 47.2 s).

Profiled again (sp_004_b, cProfile: 17.5 s, E2a 60 s): `connect_groups` 1.6 s (Rust), `walkable`'s own NumPy over 7.7
million surfaces 1.4 s, `stage`'s own glue 0.9 s (the per-sector dicts of every cell: `area_of`, `build_graph`'s
surface -> cell map), `build_graph`'s own 0.9 s, the cover nodes 0.9 s (Rust; sequential: each area's nodes join the
next area's), `trace_edges_many` 0.8 s (mostly making its Python dicts), the islands and floods 1.4 s (E1d's Rust),
`build_areas_many` 0.5 s, the cell picking 0.6 s, `trace_emit` 0.4 s, the roof rule 0.4 s, `navdetail` 0.9 s
(Python), the mesh writer's `_area_body` 0.3 s. Not ported, by size: `walkable`'s NumPy glue, `navdetail`'s
`broken_sides` / `detail_features`, `stage`'s dicts (a Rust `info` per sector would remove most of them and the
conversions of cells between Python and Rust), `add_order_nodes`, `ladder_ends`, `kymesh`'s writer.

## E3d: HLSL compiles on all cores, dbxport's ranking from an index (2026-11-17, branch `rust-e3d`)

**Provisional timings:** OPFOR's WW2 builds (`ww2_level_build.py coral_sea`, `ww2_level_install.py`) and other
agents' tests ran beside every figure below, at below-normal priority on the 16-thread PC. Ratios hold within a run.

The two targets of OPFOR's round 0 profile (`project-opfor-bfbc2\research\RUST_PORT_PLAN.md`): `pcbuild.compile_hlsl`
(2,444 compiles, 147 s in a WW2 level build; 3,652, 365 s in `wake_translate`) and `dbxport._best` (3,510 calls,
76 s in a level build). **Neither moved to Rust, by measurement:** the compiles are Microsoft's compiler's time, and
ctypes already lets go of the interpreter for each `D3DCompile` call, so plain Python threads run it on every core (a
Rust driver could not compile faster); and `_best`'s cost was an algorithm, a full scan per call, which an index
takes down to under a second per level port - nothing left worth a port.

### HLSL: a compile cache and batches (`bfbc2-shader-tools/pcbuild.py`)

**What was measured.** The compiler is `d3dcompiler_47.dll` through ctypes, one `D3DCompile` per call in the process
(no fxc processes). It is deterministic and thread-safe: every shader below compiled the same bytes twice in a row and
on 2 - 16 threads. The corpus: the 2,810 shaders `wake_translate.py --hlsl` last wrote (OPFOR's
`work\ww2\wake\shaders_hlsl`, copied out; vs_5_0 / ps_5_0) - only **790 distinct sources**: 72 % of what the step
compiled was a repeat, which a cache answers without the compiler.

| 2,810 wake_translate shaders (790 distinct) | s | vs before |
|---|---:|---:|
| `compile_hlsl` one at a time, every job compiled (before) | 192.1 | |
| `compile_hlsl` one at a time, the memory cache (the drop-in: repeats not compiled) | 61.5 | 3.1x |
| `compile_many`, 16 threads, cold (no disk cache) | **7.4** | **26x** |
| `compile_hlsl` one at a time, warm disk cache (a rebuild: nothing compiled) | **6.0** | **32x** |

Threads alone, on 120 distinct shaders: 6.0 s one at a time; 2 threads 3.6 s, 4 threads 1.7 s, 8 threads 1.1 s,
16 threads 0.8 s (7.5x), every result the same bytes. Wake's 815 translated pixel shaders (`--game` below): 73.3 s
one at a time, 8.5 s by `compile_many`.

**What changed.** `compile_hlsl` (same signature) keys each compile by SHA-256 over the source, entry, profile,
defines (in order, as the strings `D3DCompile` sees), name, flags and the compiler DLL's own SHA-256, and looks in
memory (the process; 256 MB at most), then for a compile of the same job under way (it waits for it), then in the disk
cache `%LOCALAPPDATA%\bfbc2-tool-suite\cache\hlsl\<compiler>\` (`BFBC2_HLSL_CACHE`: a folder, or `0`;
`pcbuild.use_cache`). An entry holds a version tag, its key and its code's SHA-256; anything else is a miss, compiled
and written again; entries are written to a per-process, per-thread temporary file and renamed. New:
`compile_many(jobs, errors='raise' | 'return')` (all cores, input order, a repeated job compiled once),
`prefetch(jobs)` (start them, return at once), `compile_hlsl_uncached` (the plain compile), `CompileJob`,
`stats`, `workers()` (`BFBC2_HLSL_WORKERS`). The suite's only compile loop, `xdiff.py`'s, translates every shader
first and prefetches their compiles, so they run on all cores beside its interpreter.

**For OPFOR** (read only to this round): nothing to change for the cache. Every OPFOR compile goes through
`P.compile_hlsl` (`wake_translate.translate_db` line 991 - also behind `wake_sky` and
`ww2_char_shaders` -, `wake_water.py` 164, `minimap_edges.py` 71), so repeats inside a process stop being
compiled (the measurement above: 192 -> 62 s) and a rebuild with unchanged sources compiles nothing (-> 6 s), across
processes too. For all cores on a cold run, `translate_db` needs one change: its loop computes `vsrc` / `psrc` and
uses the compiled code on the next line (registers, binding tables, database appends in order), so the compiles must
be started before the loop - a first pass over the same programs that stops after building `vsrc` / `psrc` (the code
up to line 990, without the `--hlsl` file writes) and calls `P.prefetch([(vsrc, 'main', vs_prof), (psrc, 'main',
ps_prof), ...])`; the unchanged loop's `compile_hlsl` calls then take the results or wait for them. Expected: the
cold `wake_translate` close to the 26x row above (its other work, ~15 s translating plus ~29 s of bundle reads, then
dominates).

### dbxport: the template ranking from an index (`bfbc2-bf1943-import/dbxport.py`)

**The cost.** `Templates.choose` ranked each BF1943 instance against every BFBC2 candidate of its type (`_best`: a
score per candidate - 4 per shared input-action value, 2 for the same reference-array slot, 1 for the same ordinal -
the first top score wins). OPFOR's level port (`wake_level`) ranks a level's ~2,600 - 3,200 instances against all
157 BFBC2 level partitions: 320,948 candidates, 139,586 of the largest type, and the calls of a type scan them all.
In the fair-port corpus the pools are small (152 calls, under 0.01 s in `--quick`).

**The index** (`_Ranking`, built once per type and per donor list; `add()` drops them): posting lists from each input
value and each slot to the candidates holding it, and the first candidate of each ordinal. Only a candidate sharing
an input value or the slot can score 2 or more; any other scores at most 1. So the scores of the posting-list hits,
the top one (earliest on a tie) if any; else the first candidate of the role's ordinal (score 1); else the first
candidate (0) - the scan's choice and score. The donor list (`donors=`) is also built once instead of per call.
`_best` stays as the reference (`BFBC2_DBXPORT_REFERENCE=1` ranks with it).

| BF1943 level partition ported against a third of BFBC2's levels (53 partitions, 108,240 candidates; `--part 0/3`) | calls | the scan's ranking, s | whole port with the index, s |
|---|---:|---:|---:|
| Wake Island | 2,623 | 27.9 | 0.6 |
| Coral Sea | 980 | 8.2 | 0.3 |
| Guadalcanal | 3,214 | 25.2 | 0.8 |
| Iwo Jima | 2,357 | 25.4 | 0.6 |

With all 157 (OPFOR's pool) the scan's share is about three times these - the profile's 76 s per level build; the
index's whole port stays around a second. No Rust: what remains is the port itself.

**Parity.** Every ranking checked against `_best` (the same candidate object and score):

- the WW2 level ports, every slice of BFBC2's 157 level partitions (`--part 0/3`, then `1/6`, `2/6`, `4/6`, `5/6`:
  each partition in one pool; all 157 at once take 3.5 GB): **45,758 of 45,758 calls** the reference's, and each of
  the 20 ports byte for byte the same as the port with the scan;
- the fair-port corpus (`test_dbxport_fair.py`, every partition both games ship): 2,324 of 2,353
  partitions ported, all its checks passed, **1,835 of 1,835 rankings** the reference's (the rest of its choices are
  counterparts and defaults, which rank nothing);
- quick: 20,000 seeded random rankings (lists of 1 - 300 candidates from few input values, slots and ordinals: ties
  everywhere, every score 0 - 14 reached), and `Templates.choose` with and without donors, before and after `add()`,
  against `REFERENCE_RANKING`.

### Tests

`bfbc2-shader-tools/tests/test_pcbuild_batch.py` (quick: 48 made-up shaders - determinism, `compile_many` order,
errors and repeats, `prefetch` joins, the disk cache written whole, damaged / cut / foreign entries missed and
rewritten, keys apart for every input, `BFBC2_HLSL_CACHE`; `--game`: Wake's 815 translated pixel shaders, 815 of 815
the same bytes, peak 1.6 GB) and `bfbc2-bf1943-import/tests/test_dbxport_ranking.py` (quick: the random rankings and
`Templates.choose`; `--game [--part K/N]`: the four level ports, slice 0/3 in the game tier, peak 1.5 GB).

## E3b: DbxEdit's commands in process (2026-11-17, branch `rust-e3b`)

Every call of DbxEdit started .NET and parsed its file afresh: one WW2 day (`dbxedit-2026-11-17.log`, up to 13:05)
logged 1,979 launches - 1,089 `edit`, 313 `import`, 263 `xml`, 185 `check`, 129 `roundtrip` - and its `File.Replace`
over a file just written stopped two rebuilds. `bc2_formats::dbxedit` is a port of `bfbc2-dbx-tools/DbxEdit.cs`'s
commands, run in process through `bfbc2core.dbxedit` (README.md, "DbxEdit in process"):

| DbxEdit.cs | port |
|---|---|
| `Program`: `xml` (files and folders), `roundtrip`, `edit` (`replace`, `replaceprefix`, `drop`, `dropfield`, `partguid`, `set`, `setfield`, `additem`, `copyfield`, `graft`; `@file`), `import` (`--like`, `<xml>.dbx` beside it, `--types`; exit code 3 for guesses), `check` (`--slots-from`, `--bundle`), `fixslots`; the usage and option errors; `ToolLog.Fail` | `dbxedit::run` / `run_many`: stdout, stderr, exit code, log lines |
| `DbxFile` (the string area kept as stored until a string changes, the header word, container flag bytes, trailing bytes), `Varint` (32-bit, wrapping shifts), `AtomicFile` | `EditFile`, `read_varint` / `write_varint`, `write_atomic` (`atomic::replace`) |
| `XmlExport.Convert` / `ValueText` / `Number` (the walk; the "R" formatting is `dbx`'s, proven on the corpus in E0) | `convert`, `value_text`, `number` |
| `XmlImport` (its own XML reader, `Align`, `KindOf`, `Encode`, `FourBytes`) and `TypeBook` | `parse_xml`, `import`, `TypeBook` |
| `Editor`, `LevelCheck`, `SlotFixer`, `PlacedObject` | `editor`, `level_check`, `fix_slots` |
| .NET Framework 4: `float` / `double` / `int` / `byte` parsing, `OrdinalIgnoreCase`, `ToUpper/LowerInvariant`, `Path.ChangeExtension` / `GetFileName`, `File.ReadAllText` / `ReadAllLines`' byte order marks, `Directory.GetFiles`' order and its `*.dbx` matching, the exceptions and `ToolLog.Explain`'s words | `dbxedit::net`, `Fault` |

**Proof** (`core/tests/test_dbxedit.py`; DbxEdit.exe built from this branch's `DbxEdit.cs` is the reference):

- **DbxEdit's unit tests' cases** (`DbxTests.cs`: format, XML export and import, the XML reader, every edit command,
  graft / copyfield, level check, cull slots, command line) as 82 command lines on the same built files, plus usage,
  option, missing-file and damaged-file cases (each file cut at 4 places through `roundtrip`, `xml`, `edit`,
  `check`): each run both ways in two copies of a folder - **82 of 82 identical**: exit code, stdout, stderr (the
  unexpected-error lines too: cause, what to do, log path; only the temporary file's `<pid>-<n>` suffix differs)
  and every file written. Quick tier, 5.5 s.
- **DbxEdit's edit fingerprints** (`fingerprints_795745.txt`: 160 edit batteries on 20 level partitions, each the
  output file's SHA-1, exit code and the SHA-1 of the printed text): **160 of 160** in process.
- **The corpus** (`--game`; a seeded sample of `work\levels`, `work\navkit\common`, `work\ww2`): 2,990 files (10
  deleted by the WW2 rebuild running beside the test), **20,252 command runs** both ways - `xml`, `roundtrip`,
  `import --like` (the XML back), `import` without a reference (guesses, exit code 3), `import` of an edited XML
  (DbxTests' `Mutate`: decimals halved, text suffixed, bools flipped, an instance removed and another copied under a
  new guid), `check`, and on every 4th file DbxTests' edit battery (`drop`, `replace`, `replaceprefix`,
  `dropfield`, `partguid`, `set`, `setfield`, `additem`) - **0 differences** in exit code, stdout, stderr or bytes.
  An earlier run of 2,000 files (13,560 runs) before the import optimisations below: also 0. 439 s for 3,000.
- **The day's invocations replayed** (`python rust\bench_dbxedit.py replay <log> --before 13:05:00`): of the 1,979,
  the 253 whose inputs still existed (815 used temp folders since deleted; most WW2 steps work on scratch files
  such as `_work\src.dbx` / `cmd.txt` that each step overwrites or deletes) - 160 `edit`, 90 `xml`, 3 `roundtrip`
  - run both ways with outputs redirected to a temp folder (inputs only read; an `xml` input copied first): **253
  of 253 identical**. The day's imports could not be replayed: their XML inputs are deleted after use.
- `cargo test -p bc2-formats`: unit tests of each part; `tests/dbxedit_robustness.rs`: any bytes, damaged built
  files and any text through every command give a result or a fault, never a panic; built files round-trip and
  import back byte for byte, their XML `dbx::to_xml`'s.
- The corpus's string tables: 113,778 binary files, none with invalid UTF-8 (where .NET Framework's replacement of
  bad bytes and Rust's could differ once a table is rewritten).

**Time** (`python rust\bench_dbxedit.py commands`; the PC busy with a WW2 rebuild, best / median of 15; the exe's
time includes its start):

| command | file | DbxEdit.exe | in process | faster (median) |
|---|---|---:|---:|---:|
| xml | 18 KB (`TOW2.dbx`) | 38.3 / 40.5 ms | 2.11 / 2.63 ms | 15x |
| edit (setfield) | 18 KB | 56.9 / 60.6 ms | 1.70 / 1.94 ms | 31x |
| import --like | 18 KB | 65.7 / 71.2 ms | 3.07 / 3.42 ms | 21x |
| check | 18 KB | 43.7 / 45.1 ms | 0.70 / 0.91 ms | 49x |
| roundtrip | 18 KB | 36.6 / 39.1 ms | 0.64 / 0.74 ms | 53x |
| xml | 1,282 KB (`MP_005.dbx`) | 81.5 / 85.0 ms | 47.2 / 48.9 ms | 2x |
| edit (setfield) | 1,282 KB | 84.4 / 91.4 ms | 26.8 / 30.1 ms | 3x |
| import --like | 1,282 KB | 538 / 594 ms | 238 / 283 ms | 2x |
| check | 1,282 KB | 125 / 142 ms | 51.6 / 67.2 ms | 2x |
| roundtrip | 1,282 KB | 80.8 / 87.5 ms | 19.6 / 21.8 ms | 4x |

**The day's replay** (253 invocations): DbxEdit.exe one by one 26.8 s, in process one by one 7.3 s (4x), in
process as one batch (`dbxedit.run_many`, all cores) **1.12 s (24x)**, the batch's files the one-by-one run's on
253 of 253. Most of a WW2 step's DbxEdit time was .NET starting (35-55 ms a call): the files are small. Batching
needs callers that hand over many command lines at once (the 1P animation steps apply one `cmd.txt` to one
`src.dbx` for each of dozens of outputs: one `run_many`).

Where a large import's time goes (`cargo run --release -p bc2-formats --example dbxedit_bench -- MP_005.dbx`, before
-> after this round's optimisations): 259 -> 157 ms, of which reading the 4.5 MB of XML 33 ms, the types learned
9 ms, the reference copied 8 ms, every value's text (DbxEdit compares the XML's text with the old value's) 34 ms.
The optimisations: a message's element path is made only for a message (it was a string per element), XML and
attribute text normalised without copies when it has no line breaks or tabs, the type book's keys built in one
buffer, and `Align`'s queues made only when the reference child after the last match is not the XML child's (true
for an unedited file: no queue at all) - each exact (argued in `import.rs`; the 20,252 corpus runs were made after).

**Callers switched.** In the suite: NavKit's unpack (`xml` of the unpacked folder), `bfbc2-stockmp-ai`'s
validation (`check`, `roundtrip`, `xml`, `import --like`) and its tests run in process through `dbxedit.run_cmd`.
`BFBC2_DBXEDIT_EXE=1` (or no Rust core) runs the exe. OPFOR's callers are not switched (another repository); each is
a one-line change of the same kind (the report of round E3b lists them).

**Known limits.** .NET Framework's `NumberToDouble` (what DbxEdit's float and double parsing uses) can be one unit
off in the last place on numbers of more than 15 significant digits, and it keeps 50 digits at most; the port parses
correctly rounded (and keeps 50 digits). None of the corpus's runs met a difference. A damaged file fails at the
step DbxEdit's fails, with its exit code; the message names the same .NET exception, but a few of .NET's own
details (an `IOException`'s wording other than file-not-found / access / sharing) are worded by Rust. Nesting past
256 levels is refused (DbxEdit's recursion would end the process). The exe stays the reference this round.

## E3e: the bundle cache (2026-11-17, branch `rust-e3e`)

**Provisional timings:** other agents' builds and tests ran on the PC throughout (a WW2 level build among them);
every run below was at below-normal priority, the before and after runs close together in time.

**The cost.** Project OPFOR's round 0 profile of a WW2 level build: `fbrb_read` 726 times, 132 s, mostly the same
stock bundles (`shaderdb.resolve` 415 reads, `meshview` 104, `effectport` 111, `material_grid` 59; `wake_translate`
another 30 at about 1 s each). A zipped payload is one gzip stream, and `read_payload` decompressed it from its start
up to the last byte the entries asked for need, on every call.

**Measured first.** The game copy: 1,086 bundles, 1,075 zipped (7.4 GB on disk, 21.7 GB decompressed; 95 payloads of
64 - 256 MiB, 10 larger, up to 688 MiB), 11 stored (1.5 GB). Two of the profile's callers were logged read only
(OPFOR's `material_grid.wake_names`, and `detail_layers.index_levels` for Dx9, 10 and 11: `fbrb.read` wrapped, every
write refused):

| | reads | distinct bundles | decompressed by the reads | each bundle once, whole | reads ending past half the payload |
|---|---:|---:|---:|---:|---:|
| `material_grid.wake_names` | 71 | 23 | 11.0 GiB | 3.35 GiB | 48 (68 %) |
| `index_levels` x 3 (the shader databases of 74 levels) | 222 | 74 | 10.5 GiB | 5.53 GiB | 146 (66 %) |

The reads go deep (the shader databases sit near the ends of the level bundles; mp_common is read 9 times, to 490
MiB of 561), so decompressing a re-read bundle once in full costs little more than one read; and a prefix kept in
memory cannot hold them: the payloads are 80 - 560 MiB each.

**The replay** (`rust\bench_fbrb_replay.json`; `python rust\bench_fbrb_cache.py`): the level build's reads rebuilt in
one process to the profile's counts - the two captures (the 222 shader database reads cycled to 415; the grid's 71),
104 single resources and 111 batches of 1 - 12 partitions chosen (seeded) from the bundles the grid reads - then
`wake_translate`'s 30 texture headers: 731 reads of 87 bundles, 68.5 GiB decompressed by the reader before E3e (8.1
GiB distinct). Its "before" (146 s of reading) is close to the profile's 132 s. Each run is a new process; every
run gives the same 731 per-read SHA-1s.

Two runs, each before and after back to back: run A with an in-process budget of 256 MiB, run B with the final
default of 128 MiB (the layer does nothing on this pattern; the difference between the runs is the PC's load).

| replay (731 reads) | reading, A | reading, B | whole run, B | decompressed | disk cache | peak, B |
|---|---:|---:|---:|---:|---:|---:|
| before (`fbrb.read_uncached`: the reader before E3e) | 146.1 s | 156.5 s | 160.6 s | 68.4 GiB | | 123 MB |
| memory only (the disk cache off) | 150.6 s (whole run) | | | 68.4 GiB | | |
| **cold**: one process, an empty disk cache | **51.9 s** (2.8x) | **58.3 s** (2.7x) | 62.3 s | 12.6 - 12.9 GiB | 8.09 GiB written | 338 MB |
| **warm**: a new process, the cache cold left | **5.6 s** (26x) | **7.8 s** (20x) | 11.9 s | 0.01 GiB | 716 of 731 reads from it | 189 MB |
| 6 processes in turn (a rebuild's steps), before | 143.6 s | | | | | |
| 6 processes in turn, cold | 51.7 s | | | 12.8 GiB | 8.09 GiB written | |
| 6 processes in turn, warm | 5.9 s | | | 0.03 GiB | | |

("whole run" adds the benchmark's own SHA-1 of the 8.6 GB of entry data, about 4 s in every run; run A's cold peak
was 492 MB with the larger budget. Warm is what a rebuild sees once the cache holds its bundles: the cold cost is paid
once per bundle while it stays unchanged. The warm reads are now mostly copying: 8.6 GB of entry bytes from the
cache files, through the system cache, into Python.)

One read of mp_common's last entry (561 MiB payload): 1.30 s before, every time. With the cache: 1.31 s the first
time (a marker is left), 1.65 s the second (the whole payload decompressed and written, 588 MB, flushed to disk), 16
ms the third, then under 1 ms; in a new process 9 ms (its header), then under 1 ms. A stored bundle
(streaming_sounds, 565 MiB): its last entry 70 ms before (streamed from the start out of the system cache; 291 ms
cold), 3 ms after.

**Design, by the numbers above.**

- **Stored payloads** are read where each entry is (a seek; an entry past the end of the file is refused before its
  room is taken) instead of streamed from the start.
- **Headers**: the last 64 bundles' are kept (mp_common's: 9 ms to read and parse, 15,861 entries).
- **In the process** (`BFBC2_FBRB_MEMORY_MB`, default 128): the payload decompressed so far with its inflater
  (`Session`): a later read inside it is a copy, one past it continues. It serves small bundles and repeated shallow
  reads; on the profile's pattern it does nothing (150.6 s: the payloads are larger than any sane budget), which is
  why the disk cache exists.
- **On disk, for every process** (`%LOCALAPPDATA%\bfbc2-tool-suite\cache\fbrb`; `BFBC2_FBRB_CACHE` names another folder
  or `0`; `BFBC2_FBRB_CACHE_GB` the cap, default 16; `BFBC2_FBRB_CACHE_MIN_MB` the least compressed payload, default
  8): a bundle's first read leaves a marker; its second read, by any process, decompresses the whole payload once,
  writes it beside its final name and moves it there (`atomic::replace`), and every read after that is a seek into
  it. One-off reads (a viewer opening one entry, a sweep of the game copy) pay nothing; a re-read bundle costs one
  extra decompression, once while it stays unchanged (cold above: 12.6 GiB decompressed for 8.1 GiB of bundles). A
  process that finds another writer's fresh temporary file for the bundle reads it without writing (4 processes at
  once on 3 bundles: 5 - 9 writes, was 12). Beyond the cap the least recently used payloads go; temporary files of
  dead writers after an hour, markers after 30 days.
- **Not done:** an index of inflate restart points (zran: a 32 KiB window every few MiB, about 1 % of the payload on
  disk, any entry a seek plus a few MiB of inflating) needs zlib's `inflatePrime` to start inside a byte; neither
  flate2 nor zlib-rs's safe API has it, and the workspace denies `unsafe`. Worth it if the disk use (8 GB for the
  replay's bundles) ever matters. A memory map of the cache files (one copy less) would need `unsafe` too.

**The same results.** Every layer is keyed by the bundle's canonical path, length, modification and creation times,
taken from the open file the read uses; a cache file repeats the key in its header and is used only when the key and
its length check. The reference's errors come from reading 1 MiB of compressed payload at a time: a `Session` feeds
the same chunks, keeps only what whole chunks gave, and remembers a gzip error with the bytes before its chunk, so a
read that would reach that chunk gets the error and one that would not gets its bytes. Only a stream read to its end
with its checksum and length checked is written to disk: every read of it is then a copy (or `PayloadEnds`). A
stream that fails or ends early is never written; its reads go through the session.

**Parity:**

- Rust (`fbrb/cache_tests.rs`, 11 tests; `read.rs` `stored_reads_seek`): a bundle of several 1 MiB chunks, 12 read
  patterns (single entries early to late and back, the empty entry, a deleted marker, a missing name, everything
  forwards and backwards) twice in order through memory sessions (unbounded, 1 MiB, none) and the disk cache (with
  and without memory), a second cache as another process, reads side by side on 16 threads, `extract`; 27 damaged
  bundles (deflate data changed at 7 places x 3 masks, the trailer's checksum and length, cut at 5 places including
  chunk ends, junk after the member) through 4 setups, every result the uncached reader's, more than 50 of them
  errors; a bundle rewritten in place (same length, new time); damaged and foreign cache files ignored; the cap;
  stored reads against `stream_payload` for overlapping, unordered and empty entries in a file cut 5 ways.
- Python, quick (`core/tests/test_fbrb_cache.py`, 18 s): built bundles (4 chunks zipped; stored) through 5 setups
  against `read_uncached` and the Python reader; 25 damaged bundles (147 of 300 reads refused, the same errors); a
  bundle rewritten in place; 4 processes at once on one disk cache (the same bytes, one payload per bundle, no
  temporary file left); `read_many` with 36 reads of one bundle side by side; 3 game bundles.
- Python, game (`--game`, 73 s): every 3rd zipped bundle of the game copy by size, **324 bundles, 61,796 entries
  (5.12 GB) in 20,850 reads** of one to five entries in a random order (each bundle through the disk cache, or
  every other one under 200 MiB through memory only: 11,668 disk hits, 8,658 memory hits, 142 payloads written),
  every entry's SHA-1 the reader's without the cache (read in offset order); **all 11 stored bundles, 2,044
  entries** a few at a time in a random order, the Python reader's bytes.
- `core/tests/test_fbrb_native.py` (quick and game) compares the cached reader with the Python reader as before.
- The replay: the same 731 SHA-1s in all runs above.

**The test runner** sets `BFBC2_FBRB_CACHE=0` for its tests unless it is set (the game tier sweeps every bundle and
would fill the user's cache); the cache's own tests use folders of their own.


## E3f: the Xbox 360 shader translator in Rust (2026-11-17, branch `rust-e3f`)

**Provisional timings:** OPFOR's WW2 rebuild and other agents' builds and tests ran beside every figure below (the
benchmarks at below-normal priority on the 16-thread PC; the same run's rows vary by up to 1.5x between runs). Ratios
hold within a run.

**The profile.** OPFOR's round 0 profile gave the translator ~15 s of `wake_translate` (xenos `_decode_alu` 7.4 s
cumulative, `_alu_operand` 3.3 s, 4.8 M `_bits` calls), then hidden behind 365 s of HLSL compiles; E3d took the
compiles to ~7 s cold / ~6 s warm, so the translator became one of the step's largest costs. Measured on Wake
Island's 1,550 programs (the plain interface: each program's own interpolators and inputs), the Python spends
**80 % decoding** (`parse_shader`: 1.8 - 2.3 ms a program; mostly building ~600 attribute objects per program, 4.8 M
`_bits` calls per Wake build) and 20 % emitting HLSL (`translate`: 0.44 ms).

**The crate** (`rust/bc2-shader`; a crate of its own rather than a `bc2-convert` module: a decoder, an IR and an
emitter that the queued Dx9 port and the BF1943 weapons' and soldiers' translations build on, with no file format of
`bc2-formats`' and no tie to the texture / audio converters):

- `xenos::parse(blob, is_pixel) -> Shader`: every field `xenos.py` decodes under its Python name and type (an integer
  where Python keeps `_bits`' integer, a `bool` where it makes one, dict insertion order kept for the literals); it
  fails where `xenos.py` raises (`XenosError::Format` with the same message, `Malformed` where Python's own
  `struct.error` / `IndexError` / `ValueError` would end it) and never panics.
- `hlsl::translate(&Shader, &Interface) -> String`: `xenos_hlsl._Emitter` statement for statement;
  `TranslateError::Translate` with the Python's message, `Unsupported` where the Python would stop on another
  exception (an empty material value, a sampler fix with other format fields than `{}`, a non-ASCII name).
  `translate_all` and `translate_blobs` (decode and translate) run many on all cores. `pyrepr::float_repr` is
  CPython's `repr(float)`, which the text writes every number with.
- `bc2-py` (`xenos.rs`): `xenos_parse` / `xenos_parse_many` give a `XenosProgram` (the decoded program, kept in Rust)
  and the tree without its microcode, every record made through the caller's `Obj` class (`object.__new__` and its
  `__dict__`, no Python call per record); `xenos_microcode` the rest; `xenos_translate` / `xenos_translate_many`
  (interpreter released) read the `Interface` attributes and hand anything of another type than the Python reads
  back (status 2: the Python runs).
- `xenos.py`: `parse_shader` returns a `_NativeShader` (an `Obj` subclass; its private fields in `__slots__`, so its
  `__dict__`, `repr` and `to_dict` are the Python tree's): the container, tables and header at once, the microcode's
  attributes (`cf`, `instructions`, `cf_slots`, `unused_slots`, `unused_nonzero_slots`, `code_words`) built on their
  first read - a build's translation never builds them. `xenos_hlsl.translate` translates in Rust while the microcode
  is unread (a tree whose microcode was read may have been edited: it goes to the Python), else in Python; hand-made
  `Obj` trees (the tests') always in Python. Copies and pickles are plain `Obj` trees. New: `parse_many`,
  `translate_many` (`errors='raise' | 'return'`), `parse_shader_python` / `translate_python` (the reference),
  `use_native`, `backend()`. `xenos_exec.py` (the differential test's interpreter) stays Python: numpy already
  vectorises it over the pixels, and it runs only in tests.

**The float trap.** The first parity run differed in 7 of 4,431 translations: numbers of 17 significant digits.
Rust's shortest `{:e}` breaks an exact tie upwards, CPython's repr (David Gay's shortest mode) to the even digit -
and an `f32` widened to `f64` often ends in exactly such a tie (15.2146759033203125: Python `15.214675903320312`,
Rust `...313`). `float_repr` takes the length from `{:e}` and the digits from `{:.Ne}` (exact, ties to even) when they
read back as the value: 86,366 doubles (random bits, random `f32`s, every power of two and both neighbours) all
Python's repr.

**Parity** (`bfbc2-shader-tools/tests/test_xenos_native.py`):

| Corpus | Programs | Result |
|---|---:|---|
| BF1943 Xbox shader databases (the 5 levels', the UI's, the menu scene's) | 7,296 | |
| BF1943 post-process / system shader containers (raw blobs) | 106 | |
| BFBC2 Xbox level databases (OPFOR's `work\xbox`, 20 levels) | 55,903 | |
| **all, distinct** | **8,320** (63,305 with copies) | **8,320 trees identical** (every attribute, value, type and attribute order; the CFs' lists hold the very instruction objects); each under 3 interfaces (the plain one, two seeded random ones: material values of every shape and special float, register values, engine-named constants good and bad, sampler fixes including the `{{}}` escapes, sampler / fetch registers, instancing, bones as world, the shadow atlas, NaN tracing, probes): **22,863 HLSL texts byte-identical, 2,097 `TranslateError`s with the same message** |
| quick: built programs (seeded: pixel / vertex, constant tables, literals, every ALU and fetch opcode, exports, predication, jumps) | 1,500 | 1,476 trees identical, 24 refused alike; 1,473 translations identical, 2,955 errors alike |
| quick: damaged blobs (bit flips, overwritten container words, cuts, junk) | 3,000 | decoded alike or the same exception and message (733 decoded; `XenosError`, `ValueError`, `struct.error`) |
| quick: whole microcode of random words | 1,000 | 220 decoded and translated alike, 780 refused alike |

The differential test (`test_xdiff.py --game`: ps317, 36, 120, 202, 476, 745, 502 against the interpreter) passes on
both backends; `xdiff.py` now translates before its interpreter reads the microcode, so with the Rust core it
measures the Rust translation (the one the builds use). It reads its one database from its level's bundles now, not
through `bf1943import.Source` (all 550 MB of BF1943's bundles: the test went past 1 GB).

**Timings** (`rust/bench_xenos.py`, Wake Island's 1,550 programs, 200 at a time, best of 3; ms per program):

| | s | ms each | vs Python |
|---|---:|---:|---:|
| decode: Python (`parse_shader_python`) | 2.721 | 1.755 | |
| decode: Rust tree, microcode on first read (`parse_shader`) | 0.178 | 0.115 | 15.3x |
| decode: `parse_many` (all cores) | 0.084 | 0.054 | 32.5x |
| decode: Rust tree with its microcode built (`sh.instructions` read) | 0.541 | 0.349 | 5.0x |
| translate: Python (`translate_python`) | 0.720 | 0.465 | |
| translate: Rust, one at a time (`translate`) | 0.280 | 0.181 | 2.6x |
| translate: `translate_many` (all cores) | 0.065 | 0.042 | 11.1x |
| **decode + translate: Python** | 2.901 | 1.871 | |
| **decode + translate: Rust, one at a time** (what callers get unchanged) | 0.464 | 0.299 | **6.3x** |
| **decode + translate: `parse_many` + `translate_many`** | 0.200 | 0.129 | **14.5x** |

Without Python (`cargo run --release -p bc2-shader --example xenos_bench -- <corpus>`, the same programs): decoding
65 us, translating 134 us a program on one thread (20.5 MB of HLSL for 1,543 programs); all cores 0.055 s and
0.035 s for the 1,550. A first version wrote the text as a list of lines joined at the end (184 us); one growing string
took a quarter off. What is left of a translation's time is the text itself (13 KB a program, a `format!` a line);
the Python side's is building the header and constant table (~50 us of the 115 us tree).

**For OPFOR** (read only to this round): nothing has to change for the speed-up of the rows "one at a time" -
`wake_translate.translate_db`, `wake_water`, `wake_terrain` and `ww2_char_shaders` call `xenos.parse_shader` and
`X.translate` as before, read only the header and constant table of the trees, and `copy.copy` their `Interface`
(read attribute by attribute, so a copy with other fetch registers works). From the round 0 profile's ~15 s of
translator in `wake_translate`, expect ~2.5 s. For all cores, `translate_db` needs the first pass E3d's report
asked for: walk the same programs up to the `X.translate` line (line ~969), collecting `(vsh, viface)` and
`(ps, iface)` jobs and the `ck` keys instead of translating, then
`texts = X.translate_many(jobs, errors='return')` (a `TranslateError` in its job's place, as the loop's `except`
expects), apply the same text fix-ups (`pc_patch_position`, `pc_strip_position`, the wheel decals, the cutout and
debug replacements) and `P.prefetch` the compiles; the unchanged loop then takes `vsrc` / `psrc` from a dict by `ck`
and its `compile_hlsl` calls find the compiles done. `xenos.parse_many` can decode a database's programs at once for
`vs_parsed` (`[(w.vs[i].code, False) ...]`). Expected: the translator ~1 s, the compiles as E3d measured.

**What remains:** `disasm` (xdiff and the tools' text dumps; no build reads it), `xenos_exec.py` (tests only), and
the text writing itself, if a later profile shows it.

## E3g: the BF1943 model, physics and effect ports (2026-11-17, branch `rust-e3g`)

**Provisional timings:** OPFOR's WW2 rebuilds and other agents' tests ran beside every figure below (below-normal
priority, 16-thread PC, the machine near its memory limit). Ratios hold within a run.

`rust\bench_ports.py` ports, as Project OPFOR's WW2 steps port them, **every** mesh set (273: Rigid / Composite /
Skinned / Tree; `bf1943_objects`' three mesh donors), **every** HavokPhysicsData (202; its two Havok templates and the
hkUFloat8 table) and **every** effect (157 EffectAssets through `effectport.port_many` on Wake's emitter copies) of
BF1943's source, each output's SHA-256 recorded - the bulk a new WW2 map ports (round E1's "under 4 s of a level build"
was one level's handful of objects).

### The profiles (before)

| bulk port | s (cProfile) | where the time went |
|---|---:|---|
| 273 mesh sets | 74.2 | `meshport.Porter.pc_layout` 46.9 s cumulative for 1,578 calls: each call sorted BFBC2's 26,846 part names again per donor and recomputed every part's signature (1.3 M `signature` calls, 81 M `str.lower`, 85 M `startswith`); `mesh.convert`'s vertex loop 20.8 s (9.4 M `element` calls, 2.5 M one-value NumPy arrays for the packed normals) |
| 202 physics files | 15.3 | `havok.Packfile`'s field walk 12.7 s: 5.0 M `swap` calls (8.4 s own time), 4.9 M `_value`; `havokport`'s own restructure / subparts 1.1 s |
| 157 effects | 35.9 | `effectport.templates`' scan of BFBC2's 18,343 emitter documents 17.2 s (`fbrb_read`, one bundle after another); `port()` rebuilding BFBC2's partition-name set per effect 5.7 s (26 M `endswith`); `dbxport.port` 5.3 s; parsing the same partitions' types again 2.8 s |
| `test_havokport.py` (game tier, seed 348 s) | 381.6 | `material_slots`: `meshview.Index.read` of 991 object physics files one at a time, 1,039 `fbrb_read` calls of 0.36 s each = **370.9 s** (a resource's read inflates its bundle up to it); the conversions themselves 1.2 s, MOPP checks 4.7 s |

### What changed

- **`meshport.Porter.pc_layout`** (algorithm): BFBC2's part names sorted once, every part's signature computed once,
  each donor's standard parts listed once, and the last resort (the layout keeping the most elements) remembered per
  (signature, donors); the texture loop takes the mesh folder's run of the sorted source names by bisection instead
  of sorting all 14,027 per port. `pc_layout_reference` keeps the old scan (`REFERENCE`, `BFBC2_MESHPORT_REFERENCE=1`).
- **`mesh.convert`'s vertex loop in Rust** (`bc2-convert::mesh::convert_vertices`, `bc2_native.mesh_vertices`): the
  same byte swaps by value width; the 11:11:10 normal and the colour bytes decoded to halves with one rounding from the
  double, ties to even (`f64_to_f16`, NumPy's `npy_double_to_half`). Those formats hold only 2,048 + 1,024 + 256
  distinct values: the quick test checks every one. Any vertex the reference would not convert (an unknown format, a
  read past the data, an element past the PC vertex) returns `None` and the Python loop runs.
- **`havok.Packfile.convert` in Rust** (`bc2-convert::havok::convert_packfile`, `bc2_native.havok_convert_packfile` on a
  `HavokClasses` made once per class dict): the reference's walk step for step, with its per-byte record of widths and
  owners (a value converted twice the same way is converted once; two widths are refused with the same message).
  Where the reference would read past the data (its short slices "convert" bytes that are not there), meet a missing
  table entry, or nest deeper than its recursion allows, the Rust stops and `Packfile` runs - `verify` stays Python.
- **`effectport.templates`** (algorithm + all cores + disk cache): `EmitterIndex` holds, per emitter document, its
  instance types and which of them its bytes name (the old scan's pre-test), read in the scan's own batches as far as a
  search goes, the bundles on all cores (`fbrb.read_many`) and the types by a new `bc2_native.dbx_instance_types_many`
  (no Python tree); a complete index goes to `%LOCALAPPDATA%\bfbc2-tool-suite\cache\effectport` keyed by the game
  copy's bundles (path, size, time) and the documents' places (`BFBC2_EFFECTPORT_CACHE`: a folder or `0`). The greedy
  choice is decided from the rows; a document whose row cannot decide (it does not parse, or holds a needed type its
  bytes do not name) is tested on its bytes as the scan tests it. `templates_reference` keeps the scan
  (`REFERENCE`, `BFBC2_EFFECTPORT_REFERENCE=1`). `port()` takes BFBC2's partition names once per index
  (`bc2_partitions`), `port_many` parses each partition's types once for all its effects.
- **`meshview.Index.read_many`**: many entries, each bundle read once, the bundles on all cores; `test_havokport`'s
  material slots read through it.

Not ported, by measurement: `havokport.restructure` / `shape_subparts_to_550` (0.5 + 0.6 s over all 202 files),
`mesh.console_layouts` (0.7 s, once per Porter), `dbxport.port` inside the effect port (5 s for 1,410 partitions: the
port's own rules, the size of dbxport - 1,865 lines - against a few seconds a level), BF1943's bundle reader
(`fbrb1943.Bundle.data`, another suite's).

### Before and after

| bulk port (all of BF1943's) | reference, s | E3g, s | |
|---|---:|---:|---:|
| 273 mesh sets (`meshport`) | 30.6 | **4.2** | 7.3x |
| 202 physics files (`havokport`) | 8.3 | **1.0** | 8.3x |
| 157 effects (`effectport.port_many`), cold: no index on disk | 28.3 | **11.1** | 2.5x |
| 157 effects, the index from disk | 28.3 | **6.7** | 4.2x |
| `test_havokport.py` (game tier) | 348 (seed) | **11** | 32x |
| `test_ports_native.py --game`: everything above both ways, the index rows, Onslaught | | 124 (reference 62, E3g 13) | |

What is left in the mesh run is mostly reading (the donors' partitions, BF1943's bundles); in the effect run
`dbxport.port` (5 s) and reading the chosen template partitions' bundles (2 s).

### Parity

Every output both ways, byte for byte (`bench_ports.py --record` on the reference before any change, `--compare` after;
`test_ports_native.py --game` runs both in one process):

- **273 of 273** mesh ports (every LOD's MeshData, the mesh set, textures, skeleton poses, the plan and notes; 3 refused
  the same way by both), **202 of 202** physics ports (11 refused by both with the same message), **713 of 713** effect
  outputs (the 712 files and the report);
- Project OPFOR's Onslaught console builds (`work\onslaught\build_*\con_*`, read only): **594 of 594** console physics files through
  `havok.convert` and **239 of 239** console MeshData (each part in its console database layout, the PC layout made of
  the same elements in PC formats; 300 more have no layout in the builds' databases) the same bytes both ways;
- EmitterIndex's rows against `dbx.parse` on all of BFBC2's emitter documents: **18,343 of 18,343** documents' types and pre-test as `dbx.parse` and `bytes.__contains__` give them;
- quick (`test_ports_native.py`, built inputs, 3 s): 400 seeded console meshes (strips, shared and windowed regions,
  re-ordered and dropped elements; 112 refused by both) and every value of the packed formats; 3,600 packfiles of random
  class layouts - built, cut short, bytes changed - the same bytes or the same error in all (800 converted: 793 by the
  Rust walk, the other 7 are reads past the data the reference "converts", left to it); 3,000 layout searches against
  `pc_layout_reference` (838 PortErrors); the emitter search on built bundles cold, from disk, after a bundle changed and
  with a damaged document, against `templates_reference`. `cargo test -p bc2-convert`: halves, packed values, swaps,
  refusals; packfiles with parents, pointers reached twice, arrays, a byte claimed as two widths.

### For Project OPFOR (read only to this round)

Nothing has to change for the gains: `bf1943_objects`, `bf1943_vehicles`, `bf1943_weapons`, `kitbags`, `soldiers`,
`wake_install` port meshes through `Porter.port`, and `bf1943_objects` / `bf1943_vehicles` / `wake_level.resources`
physics through `havokport.convert`. Batch APIs that would help:

- `bf1943_vehicles` calls `effectport.port` once per vehicle effect without `shared=`: each call searched the
  emitter templates again (17 s each before; now the in-process index answers after the first). `effectport.port_many`
  over the vehicles' effects would also parse shared partitions once.
- `wake_level.resources` reads a HavokAsset donor's physics resource with `ix.read` per pair (0.36 s each from a large
  bundle): `ix.read_many` over the pairs' templates first.
- `bf1943_fx`, `material_grid` and `ww2_level_build` each call `port_many` in their own process: the disk index makes
  the template search ~0.4 s in each after the first.

Peak working set of a bench run: 1.6 - 1.9 GB, nearly all of it BF1943's source (552 MB of bundles held whole by
`bf1943import.Source`), the game index and the dbx catalogue; the ports add little.

### Tests

`bfbc2-bf1943-import/tests/test_ports_native.py` (quick, 2 s of work: the built inputs above; `--game`, 124 s: the
bulk ports both ways, the index rows, the Onslaught corpus), `rust/bc2-convert` unit tests (`mesh_tests.rs`,
`havok_tests.rs`), `test_havokport.py` now 11 s (`ix.read_many`). Seeds in `tests\test_durations.json`.

## E3i: Granny skeletons and animations (2026-11-17, branch `rust-e3i`)

**Provisional timings:** other agents' work (an OPFOR WW2 rebuild part of the time) ran beside every figure below,
at below-normal priority on the 16-thread PC. Ratios hold within a run.

### The profile (main, before)

The slowest Granny test, `test_granny_port.py --game` (2,654 Xbox resources converted and compared with their PC
twins, then BF1943's 395 soldier animations against BFBC2's copies), under cProfile: 253 s, of which

| where | calls | s (profiled) |
|---|---:|---:|
| `meshview.Index.read` -> `fbrb.read` of one entry (the BF1943 part: one call per animation, each decompressing its bundle's gzip stream from the start) | 522 / 398 | 130 (zlib 112) |
| `Granny.root` -> `read` / `_value` (the object trees: 6.3 M `read`, 16.5 M `_value`, 8.5 M `size` calls - a type's size recomputed for every member of every object) | 5,380 | 55 |
| the test's `clean` (dropping `__` keys and Padding before comparing) | 54 M | 42 |
| `to_pc` (`_swap` 1.6 M, `_marshal` 0.7 M) | 3,049 | 11 |
| `Granny.__init__` (the relocation dict) | 9,149 | 4.7 |

That profile ran before the Rust core was built in this worktree, so `fbrb.read` was Python's; with it the same
test took 98.8 s in the last full tier and 119.6 s here. The corpora: 2,660 Xbox, 4,534 PC and 756 BF1943 Granny
resources, typically 30 KB; a tree is ~10,000 values (13.8 M in mp_common's 1,409).

### What changed, and why

- **Algorithms first.** The BF1943 part read each animation's PC copy with its own bundle read: a bundle is one
  gzip stream, so each read decompressed it from the start. `meshview.Index.read_many(names)` reads each bundle
  once for all its names (`fbrb.read_many`, bundles on all cores); the test uses it. `Granny.size` keeps each
  type's size (it was recomputed per member per object; members were already cached), and `root()` builds its tree
  with the cycle collector paused (`_gc_paused`: the collections the tree's own allocations trigger; a tree has no
  cycles).
- **Rust** (`bc2_formats::granny`, `bc2_convert::granny`, bindings `bc2-py/src/granny.rs`): the reader
  (`Granny::parse`: DICE's table, the strings, the sections and relocations; `members`, `size`, `type_name`;
  `root`: a `Value` tree - objects with `__at__` / `__tref__` / members in order / `__type__`, lists, integer and
  float lists, transforms, strings), DICE's `bone_table` / `renumber`, and `to_pc` (the swaps and marshalling, the
  CRC, a bone map renumbering the table), `to_pc_all` on all cores. The binding builds `root()`'s dicts directly
  (each table string and each type's `__tref__` tuple made once, number arrays as one list). `granny.py`:
  `Granny(b)` takes its tables from `granny_header` (sections, section offsets and relocations made on first use by
  `granny_index`), `root()` from `granny_root`, `to_pc` from `granny_to_pc`; new `to_pc_many` and `roots` (all
  cores, the interpreter released). Anything the Rust core refuses runs the Python code, which raises what it
  raises; a Granny written to (`put`, `to_bytes`) reads in Python from then on. `use_native` / `backend()` /
  `BFBC2_PURE_PYTHON` as for the other ports.
- Not ported: `add_tracks` (`_Graft`), `shift_position`, `set_orientations`, `curve_keys`, `sample` - one file at a
  time on trees already read, milliseconds each.

### Before and after

`python rust\bench_granny.py --runs 2 --reference <main's granny.py>`: the 2,660 Xbox Granny resources of
mp_common / sp_common (76.8 MB) in memory; the Python column is main's `granny.py`:

| | main (Python) s | Rust core s | x |
|---|---:|---:|---:|
| `Granny(b)` (the tables) | 0.78 | 0.094 | 8.3 |
| `Granny(b).root()`, each tree dropped before the next | 7.86 | 2.51 | 3.1 |
| `Granny(b).root()`, all 2,660 trees kept | 13.20 | 10.10 | 1.3 |
| `Granny(b).root()` of the converted PC files, all kept | 14.14 | 10.28 | 1.4 |
| `roots(blobs)` (Rust: read on all cores, built with the collector paused) | 13.96 | 6.98 | 2.0 |
| `to_pc(b)` one at a time | 6.89 | 0.145 | 47 |
| `to_pc_many(blobs)` | 7.24 | 0.045 | 161 |

Reading is bounded by making the Python objects: the Rust reader alone (`cargo run --release -p bc2-formats
--example granny_bench -- <work\xbox\mp_common>`) parses mp_common's 1,409 resources in 0.033 s and reads their
trees (13.8 M values) in 0.63 s; turning a tree into dicts, lists, tuples, ints and floats is the rest. With every
tree kept alive the cycle collector's passes between calls walk all of them (2.3 M tracked containers here), in
either column: callers that read many files should drop each tree when done, or use `roots`.

The tests (one at a time, wall clock with Python's start):

| test | before | after |
|---|---:|---:|
| `test_granny_port.py --game` ("Granny Xbox 360 -> PC on your Xbox files and BF1943") | 119.6 s | **19.2 s** (6.2x) |
| `test_granny.py` ("Granny skeletons and animations (read, shift)") | 9.2 s | 7.1 s |

### Parity

`bfbc2-anim-tools/tests/test_granny_native.py`: each resource read with the Rust core and with the Python code -
the tables (byte order, string table, sections, section offsets, relocations in order), `root()` (every value, its
Python type, every dict's key order, floats bit for bit) and `to_pc`'s bytes, or the same exception:

- **quick:** resources built in both byte orders with every member type (inline structs and their arrays,
  references, arrays of references, variants and variant arrays, strings by index, past the table and by pointer,
  transforms, 8 / 16 / 32-bit, real16 and real32, scalar arrays); the big-endian build's `to_pc` is the
  little-endian build byte for byte; with and without a bone map, and a map missing a bone; **2,184 damaged
  copies** (bytes changed, cut short; 316 whose damaged counts would keep the Python code looping for minutes
  skipped) the same both ways; `to_pc_many` / `roots` against one-by-one calls; a Granny written to (`put`) reads
  and writes as Python's.
- **game** (229 s, peak 0.7 GB): **2,660 of 2,660** Xbox resources of mp_common / sp_common (read, converted, the
  PC result read again), **4,534 of 4,534** PC Granny resources of the game copy (read), and **756 of 756** Granny
  resources of Battlefield 1943's 85 bundles (read; **1,495** conversions: each, and each soldier animation also
  with the bone map onto BFBC2's soldier) - the same both ways. Time inside that run (trees compared, so kept a
  moment): reads 16.8 + 18.9 + 6.2 s in Python against 7.7 + 11.5 + 2.5 s; conversions 5.9 + 3.2 s against 0.3 +
  0.1 s.
- `test_granny_port.py --game` and `test_granny.py` pass on the Rust core as before (2,654 Xbox -> PC equal to the
  PC twins; 31 of 395 BF1943 animations equal to BFBC2's copies; the shifts and `add_tracks`).
- `cargo test`: `granny`'s tables and halves; `to_pc` of a built resource; DICE's bone tables.

### For OPFOR (read only this round)

Every OPFOR caller gets the Rust core through `granny.Granny`, `root()` (`animations`, `bone_map`) and `to_pc`
without a change. Batch candidates: `wake_install.py`'s port loop (line ~799: `granny.to_pc` per BF1943 resource,
mixed with mesh ports) could collect its Granny resources and call `granny.to_pc_many` once; `soldiers.py` (465),
`wake_flags.py` (78, 97) and `bf1943import.Importer.convert` convert one file each, where `to_pc` now takes
~0.05 ms - a batch saves little there. Readers of several trees at once (`soldiers.py` 202-231, `weapons.py`,
`scopes.py`, `type38.py`, `winchester1897.py`, `weapon_preview.py`) can use `granny.roots`.

## E3h: the rest of NavKit (2026-11-17, branch `rust-e3h`)

E3c's profile left sp_004_b at about 17 s, spread thin: the group joining 1.6 s (Rust), `walkable`'s own NumPy 1.4 s,
`stage` / `build_graph`'s Python glue 0.9 s each, the cover nodes 0.9 s (sequential), `trace_edges_many` 0.8 s
(mostly its Python dicts), the floods and islands 1.4 s, `navdetail` 0.9 s. Profiled again at the start of the round
(sp_004_b, 16.8 s; cProfile 18.5 s): the same picture. What moved (each keeps its Python as the reference and the
fallback, `BFBC2_PURE_PYTHON=1` / `navbuild.use_native(False)`):

| Python | Rust | binding |
|---|---|---|
| `build_graph` whole: centre and portal nodes with `add`'s dedup (`round(v, 3)` as CPython: `pymath::round3`), border nodes, cover nodes, walkable pairs, Gabriel, ladder links, group joining, `add_order_nodes` | `sector::build_graph`; every sector of a stage at once, in parallel; the new nodes come back in the reference's order and take their ids in Python (`_graph_finish`) | `NavLevel.graph_many` |
| a stage's areas (`build_areas_many`'s cells), kept for the stage instead of sent back for each step | `NavAreas` (`build_areas_many(store=True)`, `reorder`) | `trace_edges_stored`, `trace_emit_many`, `graph_many` read it |
| `trace_edges`' rows as Python dicts for every area; `drop_one_way` | the rows kept in `NavAreas` (`trace_edges_stored(keep_edges=True)`), `drop_one_way` there | `NavAreas.drop_one_way`, `edges_of` / `set_edges` |
| `detail_pass` on every area (dicts of each area's cells) | only the areas with two side-by-side cells more than the detail step apart can have a broken side: `NavLevel.detail_candidates` (the others get no features, unread; `detail_pass(only=...)`) | |
| `stage`'s `area_of` dict of every cell, `border_cell`'s scan | answered by the core (`navbuild._AreaOf`: a dict only when iterated) | `NavAreas.area_at`, `first_cell` |
| `walkable`'s flood, `islands_of` (E1d's breadth-first walks) | a parallel lock-free union-find whose links point to lower surfaces (each component's root is its lowest surface, so the islands keep their numbering; the step rule is symmetric, exactly); a first flood = the components of its seeds; `flood_bfs` / `islands_bfs` stay and are compared in a property test | `NavLevel.flood`, `islands` |
| the ladders' re-floods (all flags sent each time) | continued from flags kept in the core | `NavLevel.flood_more` |
| `walkable`'s unseeds (a Python breadth-first walk with `step_fits`) | the island flooded on the floods' level | `NavLevel.with_walk` + `flood` |
| three `NavLevel`s per stage made from Python's arrays (flood, roof islands, `Level.native`) | one; the others share its arrays (`Level`'s arrays behind `Arc`) | `NavLevel.with_walk` |
| `walkable`'s sector-edge seeds (a loop over every ok surface) | NumPy | |

Outside the build (navregress's measures, `navdata.load` for the Studio and the apply test): `navdiff.area_cells` in the
core (`parity::area_cells`, `navkit_area_cells`: a file's areas at once, in parallel; the NumPy kept as
`area_cells_reference`), `ordercheck.PathData`'s areas made when read (navparity's islands read only the route graph),
`navdiff.pieces` in NumPy past 48 segments (`pieces_reference` kept), `navparity.ladder_ground` reads DICE's file only
when there is ladder ground.

**Not taken, measured.** `trace_edges`' side neighbours in parallel within a sector: a detailed sector of 281,000
cells went 0.22 -> 0.20 s (its time is the rows' maps, which are sequential) - reverted.

**Parity.** `tests\test_navbuild_native.py` (quick), section E3h, the Rust side strict and its calls counted: 10 whole
`stage()` runs on random levels over the corner of four sectors at pitch 0.8 and 0.2 (40 sectors, 3,903 areas, 3,017
route nodes, 196 D lines / F cells; meshes, areas, `area_of`, border nodes, graphs, ids, the build's counters) - every
E3h path ran each time; 12 sector graphs with ladder links; 16 `walkable` runs with unseeds (292 surfaces dropped);
`navdiff.area_cells` on 61 random outlines; `round(v, 3)` on 180,000 values bit for bit: 40 calls, 0 differences. The
E3c checks (456 calls) and the recorded grid walks unchanged. Rust: `e3h_tests.rs` (a property test of the union-find
floods and islands against the breadth-first walks, 160 random multi-layer levels; the sector graph), `parity.rs`;
40 tests in the crate, `tests\test_rust.py` passes. Whole levels against `navbuild_golden.json` (the build clock
fixed): **sp_009, sp_004_b, sp_008_b and ons_mp_004 byte for byte as recorded** on the Rust backend, sp_009 also on
the Python backend (16 s). The measures: `navdiff.area_cells` on every area of every DICE file and two NavKit builds
(170,422 areas, 23.6 million cells) the same items in order, heights bit for bit (Rust 11.9 s, NumPy 94.1 s);
`pieces` on 164,393 DICE areas the same sets; navparity's rows for sp_009 and sp_004_b identical to the old code's.

**Timings** (other agents' builds beside it; below-normal priority):

| | E3c | E3h | |
|---|---:|---:|---:|
| sp_009 (`bench_navkit.py`) | 3.3 s | **2.3-2.5 s** | |
| sp_004_b (`bench_navkit.py`; this session's start: 16.8 s) | 15.3-17 s | **11-12 s** | 1.4x |
| sp_004_b (`test_navbuild_native --game`) | 17 s | **9 s** | |
| sp_008_b | 20 s | **14 s** | |
| ons_mp_004 | 14 s | **9 s** | |
| `test_apply.py` (sp_004_b, two builds with edits) | 178 s | **34 s** | the unseed's island (96,336 cells) was a Python walk |
| Studio edit mode (`test_editing.py`) | 95 s | **29 s** | the apply in the background 14 s |
| navparity's measures, sp_004_b (`run_level`) | 23 s | **11.5 s** | |

**The profile now** (sp_004_b, cProfile 12.2 s): `graph_many` 0.78 s, `build_areas_many` 0.58 s, `trace_edges_stored`
0.56 s (each the largest detailed sector: 281,000 cells - the rows' maps are sequential), `stage`'s own 0.49 s,
`walkable`'s own 0.46 s, `detail_pass` 1.1 s on the 460 of 2,622 normal areas that have a step (`broken_sides` 0.7 s),
the roof rule 0.4 s, `stair_slope` 0.32 s, `matched_flags` 0.28 s, the samples' load 0.55 s, the mesh writer
(`_area_body`) 0.38 s, `finalize_graph` 0.44 s, `ladder_ends` 0.27 s.

**What remains:** `navdetail` (D lines, F cells) in Rust; the sequential per-sector maps of `trace_edges` and the
largest sector's graph; `walkable`'s NumPy glue and the samples' load; the kymesh writer and `finalize_graph`;
navparity's `kymesh.parse_mesh` (each file parsed several times, 6 s on sp_004_b) and `match_cells`.

`havok_tests.rs`), `test_havokport.py` now 11 s (`ix.read_many`). Seeds in `tests\test_durations.json`.

## E3m: the sweep (2026-11-17, branch `rust-e3m`)

**Provisional timings:** other agents' builds and tests ran beside every figure (below-normal priority, the machine
near its memory limit); ratios hold within a run. cProfile adds 1.5-4x to Python-heavy code: the profiled seconds
rank the work, the before / after tables below are unprofiled.

The tools no earlier round had profiled, on their real workloads: `rust\bench_sweep.py <workload> --profile <folder>`
(the game copy and OPFOR's work folders read only; outputs and caches in a temporary folder) and each tool's
game-tier test under cProfile. Not profiled: the tools other agents own this round (NavKit, Granny, stock-MP AI, FBRB,
renderqa / xdiff / disasm), and four folders that hold only a README (`bfbc2-ai-behavior-simulator`,
`bfbc2-bot-ai-debugger`, `bfbc2-tactical-map-authoring-tool`, deleted in round FM1 as planned editor workspaces:
`RETIRED.md`; `bfbc2-diff-workbench` is LevelDiff, C#: 2.4 s on MP_005).

### The ranked table

| # | tool | workload | s (profiled) | top cost (calls) | likely fix | expected gain |
|---:|---|---|---:|---|---|---|
| 1 | culltree | `add --auto --misplaced 2` on Onslaught MP_005 against every `work\levels` folder (console_cull's run), 2.4 GB | 128 | `ref_pairs`: 89 reference level partitions and 46 trees parsed every run, 93 s (`Level.__init__` 36 s, `CullTree.__init__` 31 s, 14 M `El.attr`, 1.4 M `El.child`); the listing (`culltree_dbx_files` + `culltree_contains`, 109,660 files) 24 s; `shape_for` 19 s (1,985 calls, leave-one-out) | cache (reference rows on disk), algorithm (shape memo) | **done**: 3x warm, memory 2.5 -> 0.2 GB |
| 2 | culltree | `verify` on the same | 99 | the same reading; `object_checks`' leave-one-out shapes | cache + memo | **done**: 5.8x warm |
| 3 | navdata (core) | `navdata.load` without its cache, DICE's path data of the 17 prepared levels that have one | 207 | NavKit's `navdiff.area_cells` 121 s (168,094 areas), `kymesh.parse_mesh` 38 s; navdata's own union-find loops 12 s | Rust (`area_cells` and the mesh parse are NavKit's, E3h's this round) | 10x+; every first open of a level in the Studio / harness |
| 4 | menu tools | `aptgraft roundtrip --pack`, 73 movies (menu, MP / SP HUD) | 164 | `write` 97 s (`_emit` 257 k, `_layout` 900, `reachable` 400 calls 31 s), the parse 30 s, `apt.Movie.instructions` 19 s | algorithm (reachable / layout recomputed per write), Rust (APT model) | 3-5x |
| 5 | menu tools | `aptrefs xref` (every movie set, the exe's table) | 47 | `apt.Movie.instructions`: 1,924,750 instructions decoded (all distinct: no repetition to cache), `decode` 16 s, `disasm` 26 s | Rust APT decoder (dump text and records) | 5-8x |
| 6 | harness | `harness run` sp_009 + sp_004_b, cold cache, no parity | 49 | `navdata.load` 18 s (as 3), `navpath.route` 8.7 s (581 routes, 532 k `norm`), NavKit's `ordercheck` 6.9 s | cache (warm in real use), Rust A* | 2x |
| 7 | menu tools | aptcheck's rules on every movie (128) | 37 | `apt.Movie.instructions` 27 s (as 5) | Rust APT decoder | 5-8x |
| 8 | harness | `import-log`: `ingame.sessions` of the game copy's 349 MB opfor.log | 26 | 21 M `re.search` (13 patterns on every one of 3.2 M lines) | algorithm (literal search) | **done**: 2-4x |
| 9 | texqa | the WW2 vehicles' 22 textures, cold | 19 | per texture 0.47 s of NumPy decode / PSNR / SSIM; BF1943's source opened 4 s | batch (all cores); Rust DXT decode | ~cores; a full WW2 run (thousands of textures) is minutes |
| 10 | console convert | `verify-havok`, 400 Onslaught files | 17 | `havok.Packfile`'s Python walk 14 s (4.4 M `swap`): verify needs its per-byte owners; **0 of 400** files are byte-identical, so converting in Rust first does not help | Rust walk record (owners, arrays, objects) exposed to `ignorable` | ~10x |
| 11 | navpath (core) | 400 routes on sp_009 / sp_004_b | 11.7 | `closest_between` 5.2 s (200 failed routes), `route` 2 s | algorithm (island pairs cached) | 2x |
| 12 | console convert | soot test (BF1943 sources) | 10 | BF1943 bundles read whole (`fbrb_read` 2.5 s, gzip 2.4 s, 7,390 `scandir`) | bf1943import's reader (another suite) | - |
| 13 | Navigation Studio | loading sp_009 (level, both path data cold, the scene) | 9.1 | spread: `scene_load` 0.8 s, `overlay.nearest_node` 1.1 s (2,017), `area_cells` | - | small |
| 14 | crash reader | `log_before_crash` on the 349 MB log, 3 lookups | 18 (unprofiled) | the whole log decoded (3.5 s) and split (1.9 s) per lookup | algorithm (windows around the crash lines) | **done**: 23x |
| 15 | Map Inspector | loading MP_005 + SP_01 (9.2 M triangles) | 7.2 | `viewer.add_box` 2.1 s (7,940), `add_triangles` 1.9 s (18,219) | batch the boxes | 1.5x |
| 16 | bc2loc | `dump` English + Russian | 7.3 | `apt.Movie.__init__` 3.7 s (300 movies' constants for key names) | cache | 2x |
| 17 | terrain (console) | the terrain test | 7.1 | BF1943 bundles (3.4 s), gzip | - | - |
| - | small | mesh viewer test 3.3, model tools 0.7 / 2.0 / 3.4, console readers 1.5, exeref 3.4, LevelDiff 2.4 | | reading | - | - |

### What changed

- **culltree** (`bfbc2-culltree/culltree.py`): `ref_rows` makes `ref_pairs`' choices (the same levels, order, notes and
  skips) and keeps each reference level's `collect()` rows - its slotted instances' leaves in the blueprint's own space,
  the object left out (None: only the target level's objects are ever compared by identity) - in
  `%LOCALAPPDATA%\bfbc2-tool-suite\cache\culltree`, one pickle per level file, valid while the level file, its tree file
  (size, modification time) and `culltree.py` itself (a digest) are unchanged; the tree path is recomputed from the
  cached reference each run, so a tree that appears or moves is found as before. `collect()` takes the rows
  (`RefRows`) beside (tree, level, label) pairs. Without the disk cache (`BFBC2_CULLTREE_CACHE=0`) each reference level
  is still read once and dropped at once: the trees and partitions are no longer all held. `shape_for` takes a memo:
  in `misplaced` and `object_checks` (leave-one-out) every level's instance group but the object's own is the same
  list each call, and its consensus and shape are computed once. `BFBC2_CULLTREE_REFERENCE=1` (`REFERENCE`): `ref_pairs`
  and no memo, as before. The listing of the `--ref-dir` folders (Rust, 10 s of the warm run) is not cached: a file's
  content decides whether it is a level partition, and only reading it tells.
- **harness `ingame.sessions`**: only the lines holding one of the patterns' literals are read by the patterns (the old
  loop, kept as `sessions_reference`); in a text without carriage returns the literals are found with `str.find` over
  the whole text and the lines between counted by their breaks (newlines with `str.count`, the rarer breaks
  `str.splitlines` knows by their offsets: the game log has 7 `\x1d`), else line by line.
- **crash reader `log_before_crash`**: the log read as bytes; from the last `crashreport.xml` back, a window from just
  after a newline 402 newlines earlier to the newline after the mark is decoded and split. A newline byte always ends
  a line and is never inside a UTF-8 sequence, and the mark is ASCII, so the window's lines are the whole text's.
  `log_before_crash_reference` reads the whole text (`BFBC2_CRASHREADER_REFERENCE=1`).
- **apt.py** (`bfbc2-console-readers`): the field formats compiled once, opcodes below 0x80 (no argument) returned
  at once, the per-call lambdas gone: 16% (the same dump lines and instruction records on all 128 movies, 1,924,750
  instructions, against the file at HEAD). The decoder is the next Rust candidate (items 4, 5, 7).
- Tried and dropped: `havok.verify` converting in Rust first and walking in Python only where the bytes differ -
  every one of the 400 shared files differs in ignorable bytes, so the Python walk still ran for all (item 10).

### Before and after (unprofiled)

| workload | before, s | E3m, s | |
|---|---:|---:|---:|
| culltree add, MP_005 against `work\levels` (`BFBC2_CULLTREE_REFERENCE=1` / cold cache / warm) | 52.5 (peak 2.54 GB) | 45.2 cold (0.66 GB), **17.4** warm (**0.20 GB**) | 3.0x |
| culltree verify, the same (test run, machine busy) | 106.9 | 64.3 no cache, 64.8 cold, **18.4** warm | 5.8x |
| culltree add in the same test run | 70.2 | 71.5 no cache, 70.3 cold, **34.9** warm | 2.0x |
| `ingame.sessions`, 349 MB opfor.log, 587 sessions | 5.2-9.6 | **2.1-2.9** | 2.5-4x |
| `log_before_crash`, the same log, 3 lookups | 18.2 | **0.8** | 23x |
| `apt.Movie.instructions`, 128 movies | 20.9 | 17.5 | 1.2x |

### Parity

- culltree (`tests\test_culltree_e3m.py`): `add` and `verify` through the command line with the reference, with no
  cache, a cold cache and a warm one - the same exit code, report and written files. Game: Onslaught MP_005 against
  every `work\levels` folder, add (241 report lines, the same tree and level bytes) and verify (134 lines) all four
  ways. Quick: built levels with the target's two huts on swapped slots (misplaced), reference levels with lamps and
  huts, one whose tree is missing, one whose tree is damaged (the note), a file that is no level; and a reference
  changed after the cache was made. `test_culltree_native.py`'s add run reads every reference both ways
  (`BFBC2_CULLTREE_CACHE=0`).
- `ingame.sessions`: the game copy's log, 587 sessions equal field for field to `sessions_reference`; quick: the built
  log as is, CR LF, a U+2028, no final newline, blank lines around, two matches a line, reversed, three times.
- `log_before_crash`: the game copy's log, three lookups (the last crash, one at the log's time, one an hour before);
  quick: 300 seeded logs (CR LF, lone CR, U+2028 / U+0085 / `\x0b` / `\x1c` breaks, bytes that are not UTF-8, timed and
  untimed crash lines, two marks a line, 0-900 lines), `when` and line counts varied.

### Next rounds, by expected gain

1. NavKit's `navdiff.area_cells` + `kymesh.parse_mesh` in Rust (item 3; NavKit's owner): every cold `navdata.load`.
2. The APT decoder in Rust (`apt.Movie.instructions`: dump lines and records; items 4, 5, 7), then aptgraft's writer
   (`reachable` / `_layout` recomputed per write, item 4).
3. texqa's per-texture analysis on all cores (item 9).
4. `havok.verify` from a Rust walk record (item 10).
5. navpath: `closest_between` per island pair cached, the A* heuristic from one vectorised norm (item 11, if the
   per-call `norm`'s bits can be matched).

### Tests

`bfbc2-culltree/tests/test_culltree_e3m.py` (quick 2.5 s; `--game` ~520 s: eight culltree runs),
`test_crashreader.py` (quick: 300 seeded logs; `--game`: the game log both ways), `test_harness.py` (quick: 12 log
variants; `--game`: the game log both ways). Seeds in `tests\test_durations.json`. Note for the coordinator: the
harness's game test (and `harness.py run`) keeps its cache in OPFOR's `work\harness\cache` (`harness.cache_dir`) -
this round's profile of that test wrote two navdata cache files there (2026-11-17 16:36); `bench_sweep.py`'s harness
workload redirects it to its temporary folder.

## E3j: the stock-MP AI layer (2026-11-17, branch `rust-e3j`)

**Provisional timings:** other agents' work ran on the PC throughout (a WW2 rebuild for part of it, and for a while
the PC was paging: under 400 MB free); every run was at below-normal priority, old and new close together in time
and in alternating order.

`bfbc2-stockmp-ai` builds the AI layer of a stock multiplayer level: generate (a configuration from the level's
objectives and the path data), build (author the level partition in each level-00 bundle, pack, stage the path data)
and validate (DbxEdit's check and round trips, the suite's reader, the packed bundle against the built files, the
navigation). Its game-tier test builds Atacama Conquest twice and validates once, after a writer check of
Atacama's 2,567 binary DBX files.

**The profile before** (cProfile of the game test, 91 s profiled):

| where | calls | time |
|---|---:|---:|
| `fbrb.pack` (Rust, one bundle at a time) | 4 | 19.1 s |
| `FbrbTool.exe unpack` (process launches: the stock bundles, then the packed ones in validation) | 6 | 16.6 s |
| `dbxbin.parse` / `write` (2,567 test files, the level partitions) | 2,601 / 2,583 | 10.8 / 11.0 s |
| `shutil.copytree` of the unpacked bundle into the build folder (2,359 + 566 files a build) | 4 | 7.4 s |
| validation's `check_packed` (an unpack into a folder, every file read and deleted) | 2 | 7.8 s |
| `rmtree` (the build and test folders) | 23 | 4.9 s |
| validation's `check_changes` (every leaf's value formatted as text: `_net_g` 300,512 times) | 2 | 4.1 s |
| validation's `check_external` (mp_common's 15,861 files walked once per bundle) | 2 | 2.6 s |
| DbxEdit in process (`check`, `roundtrip`, `xml`, `import`; already round E3b) | 10 | 3.0 s |
| NavKit: `ordercheck.PathData` / `hpdtool.parse` (the same two path-data files) | 3 / 9 | 2.7 / 1.0 s |

No `DbxEdit.exe` was started (round E3b switched validation); `FbrbTool.exe` was, for every unpack.

**What changed, and why:**

- **Unpack in process** (`bc2_formats::fbrb::unpack_path`, `bfbc2core.fbrb.unpack`): FbrbTool's `unpack`, the same
  files and `fbrb.manifest` (its `LayoutTokens` for padded stored payloads ported), the entries written on all
  cores. Only bundles every reader agrees on are taken (one plain gzip member for header and payload, nothing after
  the payload, plain ASCII names, no two entries on one file, no file where a folder is needed, paths under .NET
  Framework's 248 characters); anything else goes to FbrbTool.exe, the reference. Atacama's Dist bundle (2,359 files):
  FbrbTool.exe 3.8 - 5.5 s, in process 1.4 - 1.5 s.
- **The build folder unpacked, not copied** (`pipeline.build_folder`): creating thousands of small files is the cost
  (`shutil.copytree` 3.9 - 4.9 s for Atacama's Dist; a copy on 8 threads was no faster: 3.5 - 4.0 s), and the
  in-process unpack writes them on all cores from the bundle in 1.4 s - the same files as the copy of the cached
  unpack (which is that bundle's unpack, stamped with its hash). Without the Rust core, or for a bundle it leaves to
  FbrbTool, the copy runs as before.
- **The variants packed side by side** (`pipeline.pack_bundles`: `fbrb.pack_many`): every variant is authored
  first, then Dist and Package are packed together; the report and the log keep the variants' order.
- **The packed bundle checked in memory** (`validate.check_packed`): `fbrb.read` of every live entry, compared with
  the built files by FbrbTool's path rules (a later entry on the same file wins, as its write would) - no folder
  written, read back and deleted.
- **Validation reads less:** `check_changes` compares leaves by kind, text, raw bytes and attributes and formats a
  value as text only where those differ (the text is a function of them); `_partition_index` walks each folder once
  per validation (mp_common was walked per bundle); a level partition read by one check is not parsed again by the
  next (kept by SHA-256 while validation runs, two at a time).
- **The path data read once:** `sector_offsets` and `config.Snapper`'s `PathData` keep their results by the file's
  SHA-256 (a build reads each file twice, validation again; a few at most).
- **`dbxbin` in Python, faster:** one-byte varints read inline, varints below 2^14 written from a table, the open
  container's list kept at hand. 2,567 Atacama files: parse 3.71 -> 2.68 s, write 1.94 -> 0.88 s, every tree and
  byte the old code's (also after an edit that rewrites the string table). A Rust parser would still have to build
  the same 2.1 million Python elements, which is most of the cost; not ported.

**Parity.** The round's code against the old (`git show HEAD` copies, run in fresh work folders: generate, build,
validate, every file of the work folder compared - the unpacked stock bundles, the build folders, the packed and
staged outputs, `config.json`, `build_report.json` and `validation.json` with the work folder's path normalised):

| level | mode | spawners / orders | files compared | identical | old / new (s) |
|---|---|---:|---:|---:|---:|
| mp_005 (Atacama), two runs | Conquest | 16 / 6 | 5,867 x 2 (566 MB) | all | 95.1 / 29.8; 83.0 / 24.7 |
| mp_005gr | Rush | 32 / 16 | 3,338 | all | 21.9 / 14.1 |
| mp_002 (Valparaiso) | Rush | 40 / 20 | 4,645 | all | 30.4 / 22.1 |
| mp_004 | Rush | 24 / 12 | 5,311 | all | 32.4 / 17.1 |
| mp_008 | Rush | 20 / 10 | 3,387 | all | 29.4 / 13.8 |
| mp_008cq | Conquest | 8 / 4 | 2,436 | all | 11.7 / 7.5 |
| mp_005sr, mp_002sr, mp_004sdm, mp_008sdm | squad modes | - | 346 / 310 / 308 / 1,219 (the unpacked stock bundle) | all | the same error both ways |

(Every validation passed both ways. The squad modes stop in `generate` - "only conquest and rush" / "no multiplayer
kits known for FactionNeutral" - identically.) The unpack: `core/tests/test_fbrb_unpack_native.py` - 30 built
bundles (zipped, stored, every padded layout and token), 48 odd and damaged ones (all left to FbrbTool; the API then
gives FbrbTool's files or error), and `--game`: **1,073 bundles of the game copy, 361,171 files, identical** both
ways, none refused (1,059 with payloads up to 150 MiB: unpack time FbrbTool 507 s, Rust 118 s; then the 14 of 150 -
250 MiB: 58 s / 16 s; the 13 larger ones were not run under the PC's memory rule that day). `cargo test`: the unpack's own tests.

**Time** (the game test `stock-MP AI layer, Atacama Conquest built`, wall, old and new alternating):

| run | old | new |
|---|---:|---:|
| 1 | 66.6 s | 36.1 s |
| 2 | 89.4 s | 38.4 s |

One level's generate + build + validate, from the parity runs: Atacama 83 - 95 s -> 25 - 30 s; the Rush levels
22 - 32 s -> 14 - 22 s. A rebuild of a level whose stock bundles are already unpacked skips the first unpack either
way.

**What remains** (the profile after, 67 s profiled): packing (16.2 s for the two builds' four bundles: zlib's deflate
of the whole payload, byte for byte, although only the level partition and a few entries changed - reusing the
unchanged prefix's compressed stream would need the deflater's state at an entry boundary), the test's own
`dbxbin` writer check (9.4 s of parse), the in-process unpacks (8.6 s for 8, mostly file creation), DbxEdit's
`check` against mp_common (5.4 s for 10 runs) and folder deletion (4.4 s).

`havok_tests.rs`), `test_havokport.py` now 11 s (`ix.read_many`). Seeds in `tests\test_durations.json`.

## E3l: the render harness and the differential test (2026-11-17, branch `rust-e3l`)

**Provisional timings:** other agents' builds and tests ran beside every figure below (all runs at below-normal
priority on the 16-thread PC; the same command varies by up to 1.5x between runs). Ratios hold within a run.

### The profiles (before)

`xdiff.py` (the differential test: BF1943's Wake pixel shaders through `xenos_exec.py` and through the translation
drawn by `shaderview.exe` on WARP), 60 shaders under cProfile: 15.1 s, of which **13.2 s waiting for 61 shaderview
processes** (0.22 s each), 0.73 s compiling (E3d's prefetch, on its own threads), 0.34 s reading the database's
bundle, **0.32 s in the interpreter** (60 runs, 3,616 ALU instructions), 0.04 s translating (Rust, 120 calls),
0.04 s disassembling. Every Wake shader (815) took **168 s** cold and 173 s with a warm HLSL cache.

`renderqa.py` (GPU): `render` of 16 stock meshes with mp_002's database, 10.1 s under cProfile, **8.07 s in 16
shaderview processes** (0.50 s each); the game test's `game()` 4.8 s warm, 4.7 s of it in 6 shaderview processes
(0.78 s each); the first run in a checkout also builds the truth index (4.8 s, `load_levels` / `mesh_names`: cached in
`build\renderqa_truth.pkl` after). `patches` and `strips` draw a level in one job (one process), so the process cost
there is once per command.

Where a shaderview process's time goes (60 of xdiff's jobs, `sv_cost`): one process per job 3.3 s on WARP, all 60
frames in one process 0.66 s; on the GPU 20.4 s against 0.67 s - **creating the Direct3D device is ~55 ms on WARP and
~340 ms on the GPU, for a job of ~10 ms**. And a second cost: the same 60 jobs on one device took 1.21 s on files a
minute old but **4.59 s on a fresh copy of them** (0.76 s on the copy's second pass) - each newly written file is
virus-scanned when another process opens it, ~5 ms a file, and xdiff wrote a dozen inputs per shader.

### What changed

- **`shaderview -serve [-warp]`** (`native\shaderview.cpp`): one device for many jobs. Job file paths come on stdin;
  each job runs from a cleared context (`ClearState`) with its own state, and everything it made - textures (a
  leaked texture reference in `texture()` fixed), samplers, render states, its frames - is released when it ends,
  however it ends. Errors no longer `exit()`: `fail(code, ...)` throws, the one-job mode prints the message and
  returns the code (the same stderr and exit codes as before), the server prints `\x1d` `err <line>` and `\x1d`
  `done <code>`. The one-job command line is unchanged (OPFOR's `wake_preview.py` / `weapon_preview.py`).
- **`pack <file>`**: a job's inputs in one file, any file argument written `*<offset>:<size>` (a slice of it; `*` is
  in no Windows path). `xdiff.run_hlsl` writes `inputs.bin` (the vertex and pixel shaders, vertex, index and constant
  buffers and textures, in that order: byte for byte the old separate files, concatenated) beside `job.txt`.
- **`renderqa.run_job`** (same signature; thread-safe): jobs go to idle `-serve` processes, one per (exe, WARP or
  GPU, working folder), a new one when none is idle - so threads calling it at once each get their own. A server
  whose job ends with 3 / 4 (device failed or lost) is closed and the job falls back to WARP as before; a server that
  dies is closed and its job run alone; a shaderview without `-serve` (a job runs alone but a new server did not
  answer) is run once per job for the rest of the process. `BFBC2_SHADERVIEW_SERVE=0`: one process per job, the
  reference. `close_servers()` (also at exit).
- **`xdiff.py`**: `run_shader` (one shader, thread-safe), run on worker threads (`--workers`, `BFBC2_XDIFF_WORKERS`,
  default up to 4), results gathered and printed in order.
- **Not ported, by measurement:** `xenos_exec.py` (the interpreter) is 5.5 s of the 121 s of a one-worker run of all
  815 shaders (4.5 %; 815 runs) after the changes above, and runs beside the draws on the worker threads; a Rust port
  would also have to reproduce NumPy's float32 arithmetic op for op. The disassembler: 0.81 s for 815 (60,659
  `disasm_instruction` calls), 0.7 %.

### Before and after

| | before | after | |
|---|---:|---:|---:|
| xdiff, every Wake pixel shader (815), cold HLSL cache | 168.3 s | **21.2 s** | 7.9x |
| the same, warm HLSL cache | 173.5 s | 24.7 s (8 workers 21.1 s; 1 worker 46.2 s) | 7.0x |
| (the same with `-serve` and 4 workers, before the pack) | | 44.7 / 41.5 s | |
| shaderview, 60 xdiff jobs, WARP: one process each / `-serve` | 6.13 s | 0.62 s | 9.9x |
| shaderview, 60 xdiff jobs, GPU: one process each / `-serve` | 14.90 s | 0.78 s | 19x |
| renderqa `render`, 16 stock meshes (mp_002), under cProfile | 10.1 s (8.07 s in shaderview) | 7.9 s (4.98 s) | |
| renderqa `validate`, the same 16 meshes | 16.3 s | 8.9 s | 1.8x |
| renderqa `compare --truth auto`, the same 16 | 24.4 s | 14.8 s | 1.6x |
| renderqa `strips`, mp_002 (40 strips) / mp_005 (50) | 7.9 / 6.7 - 8.0 s | 4.3 / 3.5 - 5.0 s | |
| renderqa `patches`, mp_009cq (33, one job) | 9.9 s | 5.2 s | |
| `test_renderqa.py --game` (now also each mesh drawn in a process of its own) | 12.7 s | 14.1 s | |
| `test_xdiff.py --game` (now also served against one process per shader) | 4.4 s | 3.0 s | |

What a one-worker run of the 815 now spends (cProfile, `-serve`, before the pack): waiting for the draws 86 s
(mostly the scans of fresh inputs, which the pack then cut), Python's file opens 12.8 s (14,286), reading `out.raw`
8.4 s, the first pass (cases, translations, compile prefetch from the warm cache) 8.8 s, the interpreter 5.5 s.
renderqa's remaining shaderview time is the draws themselves: real meshes with block-compressed textures on the GPU,
~0.3 s a mesh (three views), and its texture files are written per mesh (fresh files again; a texture cache across
meshes is the next step if renderqa's mesh loops grow).

### Parity

- **xdiff, every Wake pixel shader:** before (`main` at 68bec4c, one process per shader, one at a time) against
  after (`-serve`, 4 workers), cold and warm: **815 of 815 exact** in every run; the printed lines identical; with
  `-serve` alone 11,032 of 11,032 files identical (every shader's inputs, draw, `ps.hlsl`, `ps.xenos.txt`,
  `results.json`); with the pack (4, 8 and 1 workers) **2,446 of 2,446** result files (`out.raw`, `ps.hlsl`,
  `ps.xenos.txt` per shader, `results.json`) identical and **815 of 815 `inputs.bin` the old input files' bytes**.
  `test_xdiff.py --game-all`: the same run served on threads against one process per shader, one at a time: 3,261 of
  3,261 files and the printed lines alike.
- **shaderview:** the 60 jobs through the old executable, the new one alone and `-serve`, WARP and GPU: 60 of 60
  outputs byte for byte and 60 of 60 stdouts alike each way.
- **renderqa**, before against after on real data (GPU): `render`, `validate`, `compare` (16 meshes each),
  `strips` (mp_002, mp_005), `patches` (mp_009cq): every `results.json`, picture, sheet and coverage PNG identical
  (the only difference: `summary.txt` names its own output folder). The game test draws each stock mesh served and
  in a process of its own: the HDR pictures, classes, sample counts and records equal to the bit.
- **quick** (`test_shaderview_serve.py`, WARP): 6 built jobs (2D and cube textures, sampler states, both constant
  buffers, depth and stencil frames with the game's render states, coverage classes, counting draws, two frames in a
  job, quoted paths with spaces) served against one process each: 10 of 10 output files, lines and codes alike; jobs
  with more state followed by jobs with less on one server, each as alone; 6 failures (a missing input, an unknown
  command, short render-state numbers, a bad sampler state, a non-texture as a texture resource, a missing job) with
  the reference's exit codes and messages and the next job on the same server alike; 8 threads x 36 jobs alike, a
  server per thread, all ended by `close_servers`; 400 jobs on one server: 5 -> 6 MB.

### Tests

`bfbc2-shader-tools/tests/test_shaderview_serve.py` (quick, ~17 s, most of it the one-process reference),
`test_xdiff.py --game` (served on threads against one process per shader), `test_xdiff.py --game-all` (new game
entry: every Wake pixel shader, ~170 s), `test_renderqa.py --game` (served against a process of its own). Seeds in
`tests\test_durations.json`; the renderqa game seed was 48.1 s from an older run (the truth index built cold).

## E3n: the menu tools' APT code (2026-11-17, branch `rust-e3n`)

**Provisional timings:** WW2 rebuilds and other agents' builds and tests ran beside every figure (below-normal
priority); ratios hold within a run, absolute seconds move by up to 2x between runs.

E3m's items 4, 5, 7 and 16: `aptgraft roundtrip --pack` (164 s profiled), `aptrefs xref` (47 s), aptcheck on every
movie (37 s), bc2loc's movie constants (3.7 s). Algorithm first, then Rust.

### What changed

- **aptgraft's writer** (`bfbc2-menu-tools/aptgraft.py`). Each write recomputed the reachable nodes, sized every
  code node by assembling it (`Code.size` = `len(assemble(...))`, pointers 0) and then assembled it again to write
  it, so every instruction was placed twice and emitted twice per write; `roundtrip --pack` makes 6 writes of three
  models and walks the reachable nodes 8 times per movie. Now:
  - `Code.size` is the placement alone (`_place`, no bytes made); `Movie.plan(layout, gap)` keeps each code node's
    placement made while sizing it, and the write emits from it (alignment depends only on the offset modulo 4, which
    the placement shares with the write; branch offsets and body sizes are relative), so each instruction is placed
    once and emitted once;
  - placement, emission and the parser's instruction loop dispatch on one 256-entry operand table (`_KIND`, made from
    the operand sets in the reference's order) with precompiled `struct` packers; a `Words` node is one `pack` call;
  - `roundtrip` finds the reachable nodes once per model and shares one `Plan` per layout among the writes of the
    unchanged model (`write(plan=...)`: the node offsets, the kept dead data and each node's bytes per byte order);
    `leftovers(nodes)` takes the same nodes;
  - `Code.refs` walks with an explicit stack (the same order: a body's references right after its definition's).
  `aptgraft.REFERENCE` (`BFBC2_APTGRAFT_REFERENCE=1`) runs the writer and parser as before (`write_reference`,
  `assemble_reference`, `_stream_reference`).
- **apt.py's decoder in Rust** (`bc2_formats::apt`, bound as `bc2_native.apt_constants` / `apt_decode` /
  `apt_decode_many`; `bfbc2-console-readers/apt/apt.py`): the constants of `Movie.__init__`, `dump()` and
  `instructions()` with `dump_lines`. The records come back as 15 column lists that `apt.py` zips into
  `(Ins, start, limit, depth)` in C (`tuple.__new__(Ins, fields)`, the cycle collector paused) - building one tuple
  per record in Rust cost twice as much; the dump's lines stay in Rust (`AptLines`) until `dump_lines` is read (the
  tools never read it). `%g` is ported (`format_g`: six significant digits, ties to even, as Python). The Rust
  refuses whatever the Python would not decode in bounds - a read past the end or at a negative offset (Python reads
  those from the end, or raises `struct.error` / `IndexError`, which the dump writes into its text for frame items),
  nesting deeper than 64 - and that movie is decoded by the Python code, so every answer, error included, is the
  reference's. A Movie with a `sink` always runs in Python. `apt.decoded(movies, chunk)` decodes many movies at once
  on all cores, a chunk (twice the cores) at a time so only a few movies' records are held; aptrefs' scan and
  `aptcheck check <several movies>` use it. `apt.use_native`, `apt.backend()`, `BFBC2_PURE_PYTHON=1`;
  `dump_python` / `instructions_python` are the reference.
- **bc2loc's movie constants** (E3m item 16): `Movie.__init__`'s constant list in Rust (`apt_constants`, 4x on the
  133 movies) was enough; no disk cache was added (bc2loc dump 3.0 -> 1.9 s for English + Russian).
- Not done: aptgraft's parser and model in Rust. The parse builds the Python model (2.7 M `Ins`, 0.7 M `Words`,
  0.4 M `Str` objects for a roundtrip) that the grafting code edits; a Rust parser would still create every object
  through the interpreter. It is now the largest cost of `roundtrip` (about a third), with `write`'s per-node loop.

### Before and after (unprofiled)

| workload | before, s | E3n, s | |
|---|---:|---:|---:|
| `apt.Movie.instructions()`, the 133 movies (2,913,639 instructions) | 10.9 | 4.7 one by one, **3.6** `decoded()` | 2.3-3x |
| `apt.Movie.dump()`, the same (3,125,206 lines) | 7.1 | **2.2** | 3.2x |
| `apt.Movie(...)` (constants), the same | 0.57 | 0.13 | 4.4x |
| aptrefs xref (every movie set, the exe's table) | 11.8 / 12.8 | **4.3** / 6.5 | 2-2.7x |
| aptcheck's rules on every movie (128) | 9.1 / 12.6 | **5.0** / 5.3 | 1.8-2.4x |
| bc2loc dump, English + Russian | 3.0 | 1.9 | 1.6x |
| `aptgraft roundtrip --pack`, 73 movies (50 distinct) | 48.3 | **26.6** | 1.8x |
| the 133 movies: every write of the unchanged model (1,999 writes with grafts) | 116.0 | 58.9 | 2.0x |
| the 133 movies: `roundtrip --pack` | 172.7 | 99.4 | 1.7x |

(The second figures of aptrefs / aptcheck are the game test's, run beside a WW2 rebuild.) The 128 movies' rules
spend what is left in aptcheck's own Python (`Code.__init__`, `findings`: 3.6 of 5 s), aptrefs in `movie_refs`.

### Parity

- `bfbc2-menu-tools/tests/test_apt_e3n.py` (quick, 8 s; `--game` ~20 min beside a WW2 rebuild): test_aptgraft.py's built movies (little- and big-endian,
  grafted, crosshair-added) and a movie of 3,000 seeded `EA_PushFloat` values (zeros, infinities, NaNs, denormals,
  ties) - the constants, dump and records both ways, equal values with equal types, single and batched; 600 damaged
  copies (truncated, code bytes changed, operands pointed inside / past the file, opcodes made branches, functions,
  lists, unknown): the same result or the same exception (46 raise in Python; 179 more the Rust refused and Python
  decoded); every write, `roundtrip --pack`, grafts and crosshair-add against `REFERENCE`, and 200 damaged movies
  written or refused as the reference.
- `--game`: the game copy's 128 distinct SwfMovies (Dist and Package), OPFOR's pristine menu bundle and the Xbox HUD
  copies of `work\xbox_dlc` - 133 distinct movies: 2,913,639 instructions and 3,125,206 dump lines the same both
  ways; every write of each unchanged model (8 ways), two of its sprites grafted into the next movie of its byte order and written three ways, and `roundtrip --pack` - 1,463 writes - byte-identical to `REFERENCE` (the same bytes, problems and notes; 575.8 s reference, 450.5 s E3n beside a WW2 rebuild, both with grafting's own Python); the parity script of this round (scratch, the same corpus): 1,999 writes and 133 roundtrips, 0 differ; aptrefs xref (text and JSON), aptcheck on the 128 movie-set movies (1,924,750 instructions:
  the same records, functions and findings) and bc2loc dump (English, Russian) byte-identical with either decoder.
- Rust: `cargo test -p bc2-formats apt` (the `%g` cases, the dump of a built movie, a read past the end refused) and
  the module's doc example.

### For OPFOR (not edited this round)

OPFOR's menu callers use aptgraft's model (`Movie.load`, the graft helpers, `to_res` once per movie: the parse and
one write, now with the faster placement and table) and run `aptcheck.py check <movie>` in a subprocess per movie
(`src\tools\ww2\menu_install.py`, `wake_ui.py`, `duplicates.py`, `weapon_icons.py`): one `aptcheck.py check a b c`
(now decoded together on all cores), or `aptcheck.Code` in process over `apt.decoded(movies)`, would save a Python
start and an import per movie. `qa_checks.py` / `ww2_crosshairs.py` read crosshair tables and `deps` through the
model (no decoder). bc2loc users (`names.py`, `ww2_awards.py`, `menu_install.py`) gain the faster constants when
they read movies.

## E3r: the rest of the BF1943 importer (2026-11-17, branch `rust-e3r`)

**Provisional timings:** a WW2 rebuild and other agents' tests ran beside the first half of the work; every figure is
below-normal priority on the 16-thread PC, with every process kept near 1 GB (`RAYON_NUM_THREADS=2`,
`BFBC2_FBRB_CACHE=0` as the test runner sets it). Ratios hold within a run.

`rust\bench_bf1943.py` (new) runs the importer's real workloads and records each output's SHA-256: `source` (every
entry of BF1943's copy), `import` (`Importer.convert` of every animation, delta animation, skeleton and aim
animation, the thumb-donor undeploys, `check()`), `fair` (`dbxport.port` of every partition both games ship, as
`test_dbxport_fair.py` ports it, and the 29 firings with DICE's counterpart), `parity` (`dbxparity.compare` and
`predict`), `atlas` (`emitteratlas.build` / `prove` / `same_but_uv` on the inputs of Project OPFOR's last
`emitter_atlas` run: 615 emitter partitions, 14 authoring atlases, rebuilt read only from its manifest by `capture` -
the build gives the atlases that run recorded).

### The profiles (before)

| workload | s | where the time and memory went |
|---|---:|---|
| `bf1943import.Source` open | 2.9 | every bundle read and decompressed; **1.24 GB kept** (508 MB raw bundles, 733 MB decompressed payloads; a stored bundle's payload a second copy of its raw bytes) - most of every BF1943 tool's footprint (E3g: 1.6 - 1.9 GB peaks) |
| Wake's emitter atlas, `build` | 52.7 (profiled; 33 - 36 plain) | `dedupe` 50.5 s: the probe search's `std()` of every 8 x 8 window at even offsets (1.8 million numpy calls, 28 s) and the pre-test of every grid offset of every kept cell (1.8 million `(pk == probe).all(-1).mean()`) |
| Wake's emitter atlas, `prove` | 9.2 | small numpy calls per image and level (`array_equal`, `corrcoef`, the crops), the source atlases decoded again |
| `test_dbxport_fair.py` | 74.6 (profiled, an early lazy Source) | the test's own loop: each partition's folder found by scanning all 14,027 names (2,353 times: 33 M `endswith`, 16 s); `guids_of` 58,835 times for 2,353 partitions (10.8 s of parsing); DICE's partitions read a bundle at a time (11 s); **`dbxport.port` 6.9 s** for 2,357 ports |
| `test_bf1943import.py --game` | 34.4 (profiled) | `check()`'s tree comparison: `clean()` copies of both trees, 13.9 million calls, 12.4 s; DICE's resources read one at a time (TRACK_FORMATS again for every undeploy: 82 reads, 4.2 s; the test's 24 undeploys 4.4 s); the deploys converted again for each stance |
| `dbxparity` (compare) | 23.0 (profiled) | DICE's partitions read a bundle at a time 11 s; `flatten` / `dbx.value` 6.7 s |
| `bench_ports.py effects` | 14.9 (profiled) | `dbxport.port` 7.1 s for 1,410 ports (5 ms each: `align` 3.7 s, `_gate` 0.9 s, parse / write); reading 3.5 s |

### What changed

- **`bf1943import.Source`, lazy** (`WholeSource` keeps the old class: the reference, `BF1943_SOURCE_WHOLE=1`). The open
  checks each bundle with `fbrb1943.Bundle` exactly as before (the same bundles taken, the same entries and order)
  and keeps only the entries; the verdict per bundle - keyed by its size and modification time and `fbrb1943.py`'s
  SHA-1 - goes to the new `bf1943source` cache (3.2 MB), so later opens read no payload (0.04 s). A stored payload is
  read where each entry lies. A compressed payload is one gzip stream: its first read decompresses it once and keeps
  **restart points** - every 1 MiB of output, the inflater's whole state (`zlib`'s `Decompress.copy()`: window and
  bit position) and how far into the stream it had read; the bytes between two points are the earlier state, copied,
  fed the stream up to the later point (the same input into the same state). An entry costs at most 1 MiB of
  inflating; decoded spans are kept, least recently used dropped, within `BF1943_SOURCE_MEMORY_MB` (default 128).
  (E3e's Rust cache could not do this: the safe Rust inflaters have no `inflatePrime`; Python's zlib copies the state
  whole. A stream that is not one plain gzip member is decompressed whole per read, as before.) A bundle changed on
  disk after the open is refused where a read needs the file. A first version kept whole payloads in a 384 MB LRU:
  the fair-port test thrashed between levels-common and the level bundles (70 decompressions, 21 s); with restart
  points 3.5 s.
- **`emitteratlas.dedupe`** (`dedupe_reference` keeps the scan, `BFBC2_EMITTERATLAS_REFERENCE=1`): the probe search
  is one `std()` over a sliding-window view (`_probe_at`) - exact, because a full window holds 64 (8-bit) or 1,024
  (BC5) values, a power of two, so the mean, the deviations, their squares and every partial sum are dyadic numbers a
  double holds exactly in any summation order, and `argmax` takes the first maximum as the scan's `>` did; the
  offset pre-test is one comparison over all offsets of a kept cell (`_matches`); the offsets that pass are checked
  with `same_image` in the scan's order. A core narrower than the grid or an empty probe runs the scan. `prove`
  compares each crop once for its three numbers.
- **`bf1943import.check`**: `same_clean(a, b)` answers `clean(a) == clean(b)` without the copies (the plain
  comparison first, in C: equal trees are equal cleaned; the walk only where it fails); `check(cleaned=)` takes the
  reference. `Importer.game_read` reads each game resource once; a deploy's and HandsIkPose's tracks are converted once
  per Importer; the BF1943 soldier's bone names once per `check`.
- **`dbxport.guids_of`** remembers each partition's guids (by length and BLAKE2b; 8,192 partitions, a new set
  returned each time) - OPFOR's `bf1943_vehicles` builds `ours` the same way.
- **`meshview.Index.read_many(names, whole_bundles=False)`** reads only the partitions named, the bundles on all
  cores (and what `Partitions` already holds); the fair-port test and `dbxparity` read DICE's partitions with it and
  list each folder's partitions once. `test_bf1943import.py` reads DICE's undeploys at once.

**Not ported, by measurement:** `dbxport.port` - 3 - 5 ms a partition (6.9 s of the fair-port test, 7.1 s of the bulk
effect port, about 1 s of a WW2 level port since E3d), spread over `align` / `align_struct`'s rules, `_gate`, the
reference resolver and the reports: 1,865 lines of rules whose every branch would need its own proof, for a few
seconds a level build. `dbxparity`'s `flatten` / `dbx.value` (a research tool, 6 s). `prove`'s per-image numpy calls
(7 s once a level). meshport / havokport / effectport: nothing new beyond E3g - their bench is unchanged but for the
Source.

### Before and after

Before: `main` at 2a59fcd extracted, with the lazy `Source` put in (WholeSource's 1.24 GB would have broken the 1 GB
limit; its bytes are proven the same below) and its `Importer` / `check` as they were.

| workload | before, s | after, s | peak, before -> after |
|---|---:|---:|---|
| `Source` open | 2.9 (every bundle) | **0.04** (the index; 2.9 the first time) | 1.24 GB kept -> the entries + 128 MB of spans at most |
| emitter atlas `build` (Wake, OPFOR's inputs) | 33.4 | **5.2** (6.4x) | 870 MB both |
| emitter atlas `prove` | 9.7 | 7.5 | |
| emitter atlas, all three | 44.0 | **13.4** (3.3x) | |
| `test_dbxport_fair.py` (game tier) | 37.8 (seed with WholeSource 31.3, mem 2.5 GB) | **26.8** | 1,126 -> 886 MB |
| bench `fair`: 2,382 ports | 20.8 | **14.9** | |
| bench `import`: 808 converts and `check()` | 15.7 (`check` 8.9) | **13.9** (`check` 6.4) | |
| `test_bf1943import.py --game`, profiled, without E3r's new proofs | 34.4 | **16.7** | ~2 GB (WholeSource) -> 690 MB |
| `test_bf1943import.py --game` with E3r's two proofs | | 23.9 | 856 MB |
| `dbxparity` compare | 17.1 | **11.3** | 798 -> 444 MB |
| `dbxparity --predict` | 20.7 | 21.0 | 1.18 GB both |
| `bench_ports.py` mesh / havok / effects | 8.7 / 1.6 / 7.6 | 9.4 / 1.6 / 7.6 (noise) | 907 MB both |

### Parity

Every output of every workload both ways (`bench_bf1943.py --record` in each checkout, compared):

- **17,219 of 17,219** `bench_bf1943` outputs identical: 14,027 entries, 804 converts (bytes and records) and 4
  thumb-donor converts, `check()`'s failures, 2,353 fair ports and 29 counterpart ports (bytes and report),
  `dbxparity`'s compare and predict results;
- **620 of 620** atlas outputs (both atlases - the ones OPFOR's run recorded -, 615 partitions, the report, the proof,
  the trees); and in one checkout with `BFBC2_EMITTERATLAS_REFERENCE=1` against without: 620 of 620;
- **1,188 of 1,188** `bench_ports.py` outputs (273 mesh, 202 physics, 713 effect);
- `dbxparity.py` and `--predict` reports (and the enums JSON) byte for byte; the fair-port test's every count;
- `Source` against `WholeSource`'s reading on the user's copy (game tier): the same 82 bundles and 14,027 names,
  **14,027 of 14,027 entries** the same bytes; in a random order through a 4 MB span budget, 14,027 of 14,027
  (12,230 span decodes);
- `check()`: **3,024 of 3,024** tree comparisons answered by `same_clean` as by the reference;
- quick: 6,000 seeded random tree pairs (`same_clean` against `clean() ==`: NaNs, `__` keys, `Padding`, mixed kinds);
  `Source` on built bundles (restart points 4 KiB apart, a two-member payload, a stored one, a refused one, the index
  used and refreshed, a bundle changed after the open refused); `dedupe` against `dedupe_reference` on 48 seeded
  synthetic atlas sets (8-bit and BC5, copies at grid offsets, flat regions, narrow cores): 48 of 48.

### Tests

`test_bf1943import.py` (quick: the fuzz and the built-bundle Source; `--game`: the Source on the whole copy and the
dual `check`), `test_emitteratlas.py` (quick: the dedupe cases), `test_dbxport_fair.py` (faster: folders listed once,
DICE's partitions at once). NEEDS: the import game test `mem=1` (was 2.5), the fair-port test `cpu=4, mem=1.5` (was
2.5); seeds in `tests\test_durations.json`. No Rust changed: `cargo fmt`, `clippy` and the tests passed in the quick
tier as they were.

## E4d: dbxport's partition port in Rust (2026-11-18, branch `rust-e4d`)

E3r measured `dbxport.port` at 3 - 5 ms a partition and left its 1,865 lines of rules in Python; the newer ruling
(port as much as possible, data kept on the Rust side, every port proven identical) re-examined it. **Moved:** the
whole port - `bc2_convert::dbxport` (about 4,300 lines with tests, as rustfmt lays them out; the binding
`bc2-py/src/dbxport.rs` 470): the template choice (`_roles`, `_inputs`, E3d's
ranking index), `_Port` (alignment, the reference resolver, stand-ins, guards, the converters: enum numbers, bool
into number, empty lists, gradient rows, numbers to items, one-to-array, renamed, nested, inline items), the passes
(`_part_link_names`, `_values`, `_conventions`, `_bones`, `rebuild_audio_connections`, `_gate`), `_text_empty_arrays`
and the writer. Python keeps a thin layer (`dbxport.port` -> `_port_rust`) with the same signature, `Report` and
errors; `port_python` is the reference and the fallback.

**Data on the Rust side.** `Templates` parses its partitions there (`bc2_native.DbxTemplates`, on all cores) and
builds the Python tables only when something asks for them (`candidates`, `by_guid`, `by_type`, `where`: the Python
port and `choose`; `types()` answers without them - effectport uses it). The catalogue's tables are copied once per
set of tables (`DbxCatalogue`: a fair test's `fair_catalogue` copies share them; 0.7 s for the real one), its class
defaults copied when a port first asks for one (only the types the partition holds are looked up: converting the
whole `defaults` table on every call was half of each port's time). `guids_of` reads guids without a Python tree
(`dbx_instance_guids`). Per call only `ours`, `stand_ins` and the bone tables cross; `resolve` is called back.

**The model.** An arena of nodes, each standing for a Python `dbx.Element`: where the reference returns the same
element the port keeps the same node, so in-place passes change it everywhere it is, as Python's do. Python's string
semantics are reproduced where the rules use them (`str.strip`'s whitespace, `$` before a final newline, `re.I`'s
folds onto ASCII, `\w`, `int.from_bytes`, `struct.pack`'s overflow). Where Python's answer depends on more than the
port reproduces - its own exceptions other than `PortError` / `SchemaError` (`IndexError`, `struct.error`, a
`ValueError` from a malformed bool), a `\w` past ASCII, a NaN packed, a value past 128 bits, alignment nested past
150 levels (Python's `RecursionError`) or copies past 2 million nodes plus 32 per input node (instances held inline
in each other many times over) - the Rust side says so and `port_python` ports (`dbxport.STATS`).

The module's data tables (`ONE_TO_ARRAY`, `RENAMED`, `NESTED`, `ENUM_NUMBERS`, `NUMBERS_TO_ITEMS`, `TYPE_AS`,
`DICE_MOVED_CLASSES`, `HINT_ACTIONS`, `HINT_TEXTS`) are read from the caller's process on every call (`DbxRules`,
remade only when they changed; see "The fix" below); template roles keep the tables of when their partition was
added. Fallbacks also when the caller changed one of the rules the port has built in (the converters, `ROLE_GUARDS`,
the patterns, the constants, the functions: compared on every call with their values at import), passes a catalogue
that is no `dbxcatalogue.Catalogue`, a duck-typed `BoneTables`,
or `donors` that is no list or tuple (the reference reads a generator once per instance). `BFBC2_DBXPORT_PYTHON=1`
ports in Python; `BFBC2_DBXPORT_DUAL=1` runs both on every call and raises `DualMismatch` on any difference in the
bytes, report lines, records or exception; `BFBC2_DBXPORT_STATS=1` prints the counts at exit.

### Before and after

**Provisional timings** (the machine was shared - the effect bench's baseline alone ran 10.0 s and 18.1 s): below-normal
priority, `RAYON_NUM_THREADS=4`, `BFBC2_FBRB_CACHE=0`; `main` (0bbafd7, a worktree with `main`'s module) and this
branch run back to back, twice (round 1 / round 2). `bench_bf1943.py` now times its `fair` workload's parts.

| workload | before, s | after, s |
|---|---:|---:|
| `fair`: `dbxport.port`, 2,382 calls | 5.9 / 4.2 | **2.4 / 2.1** |
| `fair`: `guids_of` (58,835 calls, 2,826 partitions parsed) | 2.0 / 1.6 | **0.4 / 0.4** |
| `fair`: `Templates` for 58 pools (mostly the bundles read) | 10.2 / 7.5 | 7.4 / 5.0 |
| `fair` whole | 23.3 / 16.8 | **14.6 / 11.2** |
| `parity`: `dbxparity.predict` (519 ports) | 12.9 / 13.6 | **8.2 / 7.7** |
| `bench_ports.py effects` (1,410 ports and the rest of the effect port) | 10.0 / 18.1 | 9.4 / 12.7 |
| WW2 level ports, `bench_dbxport.py` (53 BFBC2 level partitions, the 4 levels; Python then Rust in one process, each with fresh `Templates` and catalogue copy) | 13.3 / 12.7 (Wake 10.7 / 10.8, Coral 0.35 / 0.23, Guadalcanal 0.98 / 0.67, Iwo 0.84 / 0.49) | **2.5 / 3.8** (Wake 1.7 / 2.6, Coral 0.06 / 0.08, Guadalcanal 0.14 / 0.24, Iwo 0.10 / 0.18) |

The level ports' Python row is this branch's Python port, whose `Templates` builds its Python tables on first use:
Wake's port carries the parse of the 53 partitions and the ranking indexes; the Rust row's Wake the catalogue copy
(0.7 s) and its indexes. One port on the fair workload is 0.5 ms in the core (choose and align 0.2, the passes 0.07,
parse 0.05, write 0.04, the gate 0.03) and 0.2 - 0.3 ms in the binding (`ours`, the class defaults asked, the records
as tuples, the `Report`), against 2.5 - 4 ms in Python.

Peaks: `fair` 915 -> 1,022 MB (the catalogue's copy: 607,100 guids), `parity` 1,213 -> 1,214, `import` 705 -> 706,
effects 880 -> 900 MB. `import fair parity` in one process 1,216 -> 1,506 MB: `parity` loads the catalogue again while
`fair`'s copy is still held - it is let go at the next port once nothing else holds its tables.

### Parity

Every comparison is both ports on the same call (DUAL: bytes, report lines, every record's fields, or exception
type and message) and, for the benches, every output's SHA-256 against a record made by `main` (0bbafd7):

- `bench_bf1943.py import fair parity` in DUAL: **2,901 of 2,901 ports** the same both ways (2,353 fair ports,
  29 counterpart ports, `dbxparity.predict`'s 519) and **3,192 of 3,192** outputs as `main` recorded them; 0 left to
  Python;
- `bench_ports.py effects` in DUAL: **1,410 of 1,410 effect ports** the same, **713 of 713** outputs as recorded;
  0 left to Python;
- `test_dbxport_ranking.py --game`: the four WW2 level ports by the Rust port the same bytes as the Python port with
  the reference scan (2,623 + 980 + 3,214 + 2,357 rankings, each checked); `bench_dbxport.py`: bytes and reports
  identical both ways;
- `test_dbxport.py`, `test_dbxport_fair.py` (game, the Rust port: 2,353 ports, every check as before),
  `test_dbxport_rules.py` and `test_converter_edges.py` in DUAL (64 crafted ports, the errors among them);
- `test_dbxport_native.py`: seeded random partitions, pools, catalogues and options (the rules' fields among
  random ones; schema errors, strict failures): **20,000 of 20,000** the same (10,701 ported, 7,260 `SchemaError`,
  2,039 `PortError`), 0 left to Python; 600 in the quick tier. Two seeds first found inputs on which the *reference*
  runs out of memory (a float's bits read as a `MaxMainBoneIndex`, `range(4.6e18)`; exponential inline copies): the
  generator keeps bone indices small and one inline item per partition, and the guards above leave the second kind
  to Python within the node budget (checked: 1.2 s, bounded memory).

### Tests

`bfbc2-bf1943-import/tests/test_dbxport_native.py` (quick: 600 random ports both ways and the guards; `--game`: every
fair, counterpart and effect port both ways, none left to Python); `test_dbxport_ranking.py --game` compares the
Rust port with the scan's Python port; `rust/bench_dbxport.py` (new) times the level ports both ways. Rust:
`cargo fmt`, `clippy --all-targets -D warnings` and `cargo test` (bc2-convert: the arena, Python's string rules, the
tables, the helpers) clean.

### The fix (2026-11-18, branch `fix-e4d`)

A full WW2 rebuild against the hash oracle found 4 outputs the Rust port gave otherwise: Project OPFOR's
`bf1943_vehicles` ports the Zero and the Corsair with `ENUM_NUMBERS[('ChildRotationBodyData', 'RotationInput')][16]`
set to `EiaDynamicGadget2` (the planes' gear on the DLL's gear input, put in place around each port and restored), and
the Rust port wrote `EiaUndefined`, the table as `dbxport.py` defines it. **Cause:** the Rust side had the tables
built in, and the check meant to send a changed table to Python never fired: its snapshot was taken at the first port
and held the tables themselves, so a change made in place compared equal to itself. Every other entry of every
table was the same as Python's (`DbxRules.is_default`); no table was wrong, the changed one was not read.

**Fixed:** the data tables are a `Rules` value the Python module builds from its live tables (a deep copy compared on
each call; `DbxRules` remade when they changed) and the port, the template roles and the class-default lookup read
it; `Templates` records the tables of each partition's addition (the Rust roles are computed then; the lazy Python
tables are built under them, `_tables_as`), as the reference's eager tables were. The rest of the built-in rules is
snapshotted at import as values (`_FIXED`: patterns as text, `ROLE_GUARDS` as tuples). Also fixed: `dbxport._native()`
read `dbx._native`, which the retired Python reader (round F) now sets only on first use - a `Templates` made before
any parse in a process held no Rust side and ported in Python; it asks `dbx._rust()`.

**Proof:** `test_dbxport_tables.py` (quick): every number of both `ENUM_NUMBERS` tables (mapped and not, with and
without a template), every `HINT_ACTIONS` / `HINT_TEXTS` spelling, every `ONE_TO_ARRAY`, `RENAMED`, `NESTED`,
`NUMBERS_TO_ITEMS`, `TYPE_AS` and `DICE_MOVED_CLASSES` entry, both ways; then the same tables changed in place (the
gear input and back, entries added and removed, a table emptied, new tables), template roles under a change, and a
changed built-in rule left to Python: 96 ports, all the same. On `main` the gear case gives `DualMismatch`.
`test_dbxport_opfor.py` (game): Project OPFOR's `bf1943_vehicles`, `bf1943_weapons` and `bf1943_objects --wake`
builds run against this checkout (a temporary junction as `OPFOR_TOOLKIT`), their outputs in a temporary folder and
every write under OPFOR refused, every `dbxport.port` call both ways: **62 + 19 + 234 = 315 calls the same**, 0 left to
Python (about 40 s). With `--rust --out`, the vehicle build's 149 outputs by the Rust port alone are byte for byte the
ones in OPFOR's `work\ww2\bf1943_vehicles` (its run with `BFBC2_DBXPORT_PYTHON=1`, which matches the hash oracle).

### What remains

The port's own time is now 0.5 ms a partition (choose and align 0.2, the passes 0.07, parse and write 0.09; the
binding's argument conversion 0.1-0.2); the workloads' time is the reading around it (bundles, BF1943's
decompression). Not done this round: a `port_many` running many partitions on all cores (the callers port one at a
time with per-partition setup; the core is ready for it - its data is `Sync`, `resolve` would be called back under
the interpreter); retiring `port_python` (kept as the reference and fallback this round); the catalogue copy keeps
607,100 guids (about 100 MB of the fair test's 1.0 GB peak, against 915 MB before).

## E3o: navdata, kymesh, navpath, havok.verify, texqa (2026-11-17, branch `rust-e3o`)

E3m's next ranked items, in the order of measured gain. **Provisional timings:** WW2 rebuilds and other agents' work
ran beside every figure (below-normal priority); before and after were taken in the same session with
`rust\bench_sweep.py`, unprofiled, "before" on main (e581798).

### The profile (before)

`navdata_load` (DICE's path data of the 17 prepared levels, no cache), cProfile 109.5 s: `kymesh.parse_mesh` 45.7 s
(5,122 calls: every sector's mesh parsed twice, once for the areas and once for the cells; 17 M `_v3`, 32 M
`struct.unpack_from`; a neighbour link's owner found by `max(...)` over every area offset), `_load`'s own 15.7 s (each
area's outline lines and portal ends made with `np.asarray`, 4.1 M calls), `_cells` 50 s (`navdiff.area_cells_many`
16 s - its per-area `segments` and arrays; the Rust area cells themselves 2.9 s - then each area's cell dict back to
arrays: 384 k `np.array`, 504 k `tolist`), the route graph's two union-finds 6.4 s (3.7 M `find` / `wfind` calls).
`navpath_routes` (400 routes): `closest_between` 5.2 s for the 200 failed routes, the A* 2 s with one
`np.linalg.norm` per pushed node. `havok_verify`: `Packfile`'s Python walk, then the per-byte compare and `ignorable`
in Python. `texqa_ww2`: 22 textures one after another, 0.47 s of NumPy each.

### What changed

| | Python (kept as the reference and the fallback) | now |
|---|---|---|
| `kymesh.parse_mesh` | `parse_mesh_reference` | `bc2_formats::kymesh::parse` (`bc2_native.kymesh_parse`): nested tuples the Python makes its dataclasses from. Where the reference would raise (a short record, A records, an unknown portal target, a link pointer before every area) or a value is a NaN (`struct`'s float conversion of a NaN is the interpreter's to define), `None`, and the reference reads: its error stays the error. `kymesh.use_native` |
| `navdata._load`'s areas | the loop over the parsed meshes (`np.asarray` per line and portal end) | `navkit_navdata_areas`: the whole file's areas as flat arrays; each outline line, portal end and border end a view of one array (the same dtype, shape and values) |
| `navdata._cells` | `_cells_reference` (parse, `navdiff.area_cells_many`, dicts to arrays) | `navkit_mesh_cells`: every mesh parsed and every area's boundary (`navdiff.segments` in float64) sampled by `parity::area_cells` in parallel, the cells returned as flat int64 / float64 / int32 arrays in the reference's order; `None` (the reference) for a boundary that is not all finite |
| the route graph's union-finds | `_union_roots_reference` | `navkit_union_roots`: the same steps (path halving, `parent[root of a] = root of b`, the edges in order, then `find(k)` for every k in order), so the roots - which `Island.root` keeps - are the same |
| `navpath.route` | `route_reference` | `bc2_navbuild::astar` (`bc2_native.PathGraph`, one per `NavData`): the same pops (Python compares `(f, g, node)` tuples, a total order: the heap's layout cannot matter), relaxations and `expanded`. The estimate is `np.linalg.norm(pos[m] - pos[goal])`, whose last bits are the BLAS's `ddot`: on this PC (NumPy 2.5.4, OpenBLAS) `sqrt(fma(z, z, fma(y, y, x * x)))` - 200,000 random vectors, 0 differences; the plain sum differs on 10.7 % of them. `navpath.norm_formula()` checks the core's formulas against `np.linalg.norm` on 20,000 seeded vectors at the first route and uses the one that matches bit for bit; none: the reference searches. A graph with a NaN or an inconsistency: the reference. `navpath.use_native` |
| `navpath.closest_between` | `closest_between_reference` | kept per island pair and `NavData` (dropped with it) |
| `havok.verify` | `verify_reference` (`Packfile`, then `ignorable` per differing byte) | `bc2_convert::havok::verify_packfile` (`havok_verify_packfile`): E3g's walk records the arrays too (member, data start, count); every differing byte's reason from its owner, the arrays, the objects (`seen`), the last section's base and fixups - `ignorable`'s rules. Where the reference's answer is not this one's to give - `pc` longer than the console file (its `IndexError`), a bounding-volume read past the data, a byte inside two bounding-volume objects whose answers differ (the reference takes the first its set yields: hash-seed dependent) - the reference runs |
| `texqa.run` | one process (`--workers 1`) | `analyse` in worker processes (`--workers N`, `BFBC2_TEXQA_WORKERS`, default every CPU thread; by default at least three textures per worker, a worker taking 1-2 s to start); 2N textures prepared ahead (their sources read just before they are sent, so a large run does not hold every source at once); the rows taken in order; a source that cannot be read is its row in the main process, as before |

### Before and after

| workload | before (main) | E3o | |
|---|---:|---:|---:|
| `navdata_load`, 17 levels, 24,052,639 cells, no cache | 82.9 s | **14.1 s** | 5.9x |
| `navpath_routes`, 400 routes (200 across islands) | 10.4 s | **2.0 s** | 5.2x |
| `havok_verify` (`test_havok.py`: 400 Onslaught files verified, 594 converted) | 15.4 s | **3.1 s** | 5.0x |
| `texqa_ww2`, 22 textures, cold | 16.3 s | not re-timed (machine limits: a WW2 rebuild ran) | |

What `navdata_load` spends now (cProfile, before the areas moved to flat arrays): `hpdtool.parse` 5.4 s (every
sector's nodes and edges in Python), each area's island (a NumPy nearest node per area, 168 k), the NavArea objects.

### Parity

- `kymesh.parse_mesh`: **every sector of 264 path-data files** - the game copy's 35 (infantry and vehicle, every
  level) and 229 NavKit outputs in OPFOR's `work\navkit`, `work\navbuild`, `work\navgen` (read only): **34,050
  sectors, 2,372,253 areas, 0 different** (each Mesh hashed with every float's bits, every type and order:
  `rust\parity_e3o_nav.py mesh`). Quick: 600 seeded meshes written by `kymesh.write_mesh` (portals, border portals,
  D lines, outlines of 0-9 points, F cells, links; -0.0, denormals, infinities), each also cut short twice, with
  bytes changed twice, a NaN written and a huge count: 4,200 of 4,200 the same Mesh or the same error; all 600 built
  ones read by Rust.
- `navdata.load`: 18 of the game copy's 35 path-data files (through sp_005_b) the same NavData as main's navdata.py (`parity_e3o_nav.py load --main`, 0 different); the rest not run - the comparison went past 1 GB on sp_006 while the machine was limited to 1 GB per process; sp_009 / sp_004_b also against the in-module reference. Quick: 40 built path-data files (1-4 sectors, shared border ids, ladder-steep edges)
  the same NavData both ways; 300 random graphs the same union-find roots with and without skipped edges.
- `navpath.route`: bench_sweep's 400 routes on sp_009 / sp_004_b the same Route field for field (nodes, cost,
  length, `expanded`, the gap, the reason); quick: 400 seeded graphs (grid positions: equal estimates and costs,
  so ties; one-way edges; islands), 10,000 routes (3,457 found, 6,058 across islands) the same; a NaN cost runs the
  reference.
- `havok.verify`: verify-havok's 400 Onslaught files the same (exact, reasons, unexplained offsets) as `verify_reference`, all 400 answered by the record (verify itself 9.9 -> 0.2 s). Quick: 500 seeded packfiles with the classes `ignorable` knows (a storage subpart's
  `indices16`, bounding volumes alone and inside a collidable, a triangles subpart, two motions with and without a
  saved motion) and random ones, each against 4 PC copies with 0-40 bytes changed, cut by 3 bytes or longer, and an
  `indices16` array of 8 with its padding index changed: 2,003 of 2,003 the same (offset, reason) lists, every reason
  met; all 1,047 the reference answers answered by the record (the rest: reads past the data, the reference's).
- texqa: the WW2 vehicles' game run (1 worker against all) not run yet (machine limits). Quick: 9 built textures (one unreadable, one flipped, sources for six) give the same rows,
  report, log and cache file with 3 workers as with one.

### Tests

`tests\test_e3o_native.py` (quick ~10 s; `--game`: every sector of the game copy's path data, navdata.load of the
17 prepared levels, the 400 routes, verify-havok's Onslaught files and the WW2 vehicles' texqa both ways), in
`run_tests.py` with NEEDS, seeds in `tests\test_durations.json`; `rust\parity_e3o_nav.py` (`mesh`, `load [--main]`)
for any folder of path data. Rust: `kymesh.rs` (a built record, refusals), `astar.rs` (routes, norms).

### What remains

`navdata`: `hpdtool.parse` (nodes and edges in Python) and the per-area island loop. `navpath.route` in the harness
(`route_pairs`: same-island pairs) now runs the Rust search; `harness run` itself was not re-measured this round.

## E3k: FbrbTool's commands in process (2026-11-17, branch `rust-e3k`)

Every call of FbrbTool started .NET, read the whole bundle and its whole payload into memory: the logs of 16 and 17
November held 14,967 launches before this round's own tests ran (`unpack` 7,597, `pack` 7,358, `list` 12).
`bc2_formats::fbrb::tool` is a port of `bfbc2-fbrb-tool/FbrbTool.cs`'s four commands, run in process through
`bfbc2core.fbrbtool` (README.md, "FbrbTool in process"); round E3j's in-process unpack (`fbrb::unpack_path`,
`bfbc2core.fbrb.unpack`) now runs on the same engine.

| FbrbTool.cs | port |
|---|---|
| `Main`: `unpack`, `pack`, `list`, `big` (BIGF / BIG4 archives), the usage line, `ToolLog.Fail` / `Explain` (with its own words for a path too long) | `tool::run` / `run_many`: stdout, stderr, exit code, log lines |
| `Read`: `File.ReadAllBytes`, `Gunzip` (.NET Framework's `GZipStream` and `CopyTo`), the header with C#'s 32-bit wrapping arithmetic and array bounds, `new List<Entry>(count)`, `MemoryStream`'s argument checks | `read_bundle`, `parse_header`, `fbrb::netgzip` |
| `Unpack`, `LayoutTokens`, `Safe` (`Path.Combine`, `GetFullPath`, `IsPathRooted`), `Directory.CreateDirectory`, `File.Create` + `FileStream.Write`'s checks, `File.WriteAllText` | `unpack_bundle`, `write_entries`, `layout_tokens`, `safe`, `full_path` / `normalize`, `create_directory` |
| `Pack`: `File.ReadAllLines`, `int.Parse` / `uint.Parse` / `Convert.ToUInt32(s, 16)` and their exceptions, in FbrbTool's order (name, file, size2, size, flags); the bytes by E1b's packer | `pack` (`pack_rows`) |
| `List` | `list` |

**.NET's gzip reader.** FbrbTool does not read gzip as zlib or Python do. Decompression in .NET Framework 4 is the
managed `Inflater` (checked by reflection: `DeflateStream.inflater` is `System.IO.Compression.Inflater`), with its own
rules: one member, trailing bytes ignored, the header's flags and checksum not checked, an unfilled Huffman table slot
decoding as symbol 0, distance codes 30 and 31 taken, a distance before the start copying zeros, and - the one found by
measurement - **output lost at a cut**: `Inflate` copies the window out before each decoding step, not after the step
that runs out of input, and `Stream.CopyTo` stops at the first `Read` that returns nothing, so a stream cut short gives
the bytes decoded up to the last 8 KiB input chunk boundary (`DeflateStream`'s buffer) rather than all it could
decode. `fbrb::netgzip` ports `Inflater`, `InputBuffer`, `OutputWindow`, `HuffmanTree`, `GZipDecoder`,
`DeflateStream.Read` and `CopyTo` as the resumable state machine they are. zlib reads a stream first: a stream zlib
takes whole, its trailer complete and matching, is one the managed inflater decodes to the same bytes (zlib refuses
everything it refuses, and its oddities only show on data zlib refuses), so the port only runs on damaged streams.
Proof: a C# probe of `GZipStream.CopyTo` against the port (zlib first, and the port alone) on valid and damaged
streams of every block type, header field and damage: 10,500 during development (two seeds of 3,000 small, 1,500 up
to 600 KB), 500 in the quick test and 3,000 in the game tier - **0 differences** in bytes or exception.

**Proof of the commands** (`core/tests/test_fbrbtool.py`; FbrbTool.exe built from this branch's `FbrbTool.cs` is
the reference, each case run both ways on two copies of a folder; stdout, stderr, exit code and every file's SHA-1
compared, temporary files' `<pid>` suffix aside):

- **228 command lines**: built bundles (zipped, stored, DICE's padded layouts in every `hdr` / `plen` / `size2` combination),
  unpack over older files and other-case names, names with dots, spaces, `..` runs, device names, illegal characters,
  past 260 / 248 characters, two entries on one file, a file where a folder is needed; header fields out of range
  (offsets and sizes past the payload or negative as C# ints, name offsets outside the header, counts past the header,
  negative or huge, string-table lengths huge or negative); cut, flipped and junk-tailed bundles; every `pack` manifest
  error (flags, size2, size and `hdr=` values .NET refuses each its own way, missing files and folders, a folder as an
  entry, an unsafe name, an old manifest), the bundle into a missing folder, over a folder, over another case; `big`
  archives and their errors; the usage errors - **228 of 228 identical**. Quick tier, about 40 s (the exe's starts).
- **600 randomly damaged bundles** (cut, bits flipped, bytes replaced, junk appended) through `list` and `unpack`:
  **identical** (248 fail in both, 352 succeed in both).
- **Every bundle of the game copy unpacked** (round E3j's sweep, `test_fbrb_unpack_native.py --game`, on the merged
  engine): **1,086 of 1,086 bundles identical, 445,372 files** (1,075 with payloads up to 300 MiB, then the 11 larger
  one at a time). Its 48 odd and damaged bundles now ask for FbrbTool's result (12 unpacked identically, 36 refused
  with the same files left behind); they found the one case E3k's own cases had missed (two entries on one file in
  another case: `File.Create` keeps the existing name; a write now moves onto the name already on disk).
- **Game bundles through every command** (`test_fbrbtool.py --game`): 40 of up to 120 MB (30,177 entries) unpacked,
  listed and their unpacked folders packed again, on main after the merge: **identical**.
- **The logged invocations replayed** (16 and 17 November, outputs redirected to a temporary folder; inputs only
  read): 520 replayed both ways over three runs (the last: of 30,628 logged launches, 21,875 distinct, 10,998 with
  their inputs still there) - **identical** (nearly all `unpack`: the `pack` inputs were temporary folders since
  deleted).
- `cargo test -p bc2-formats`: the engine's unit tests, E3j's unpack tests on it, `netgzip`'s.

**Time** (`python rust\bench_fbrbtool.py --runs 5`, after the WW2 rebuild, below-normal priority; best / median of 5, the
exe's start included; `unpack` into a fresh folder, `pack` of the exe's unpack of the same bundle):

| command | bundle | MB | FbrbTool.exe | in process | faster (median) |
|---|---|---:|---:|---:|---:|
| list | `async\weapon\bu_shg_f2000-00` | 0.4 | 41 / 43 ms | 3 / 4 ms | 12x |
| unpack | | | 54 / 55 ms | 12 / 12 ms | 4.5x |
| pack | | | 71 / 77 ms | 27 / 29 ms | 2.7x |
| list | `levels\mp_007\ingame-00` | 1.7 | 53 / 56 ms | 6 / 7 ms | 8x |
| unpack | | | 58 / 59 ms | 9 / 9 ms | 6.3x |
| pack | | | 134 / 136 ms | 79 / 80 ms | 1.7x |
| list | `levels\mp_006sdm\level-00` (1,747 files) | 18.9 | 559 / 570 ms | 147 / 150 ms | 3.8x |
| unpack | | | 1,348 / 2,685 ms | 478 / 552 ms | 4.9x |
| pack | | | 2,777 / 2,823 ms | 2,784 / 2,793 ms | 1.0x |
| list | `streaming_sounds-00` (stored) | 592.5 | 258 / 293 ms | 5 / 6 ms | 48x |
| unpack | | | 4,077 / 4,352 ms | 1,469 / 1,672 ms | 2.6x |
| pack | | | 1,303 / 3,730 ms | 1,112 / 2,320 ms | 1.6x |

Over the game tier's 40 bundles (up to 120 MB): unpack 17.7 -> 7.3 s, list 4.0 -> 0.7 s, pack 13.1 -> 10.6 s; round
E3j's sweep of 1,086 bundles: unpack 560 -> 211 s. Where the time goes: a small call was mostly .NET starting (40 - 50
ms); `list` of a zipped bundle is the payload's inflate (FbrbTool decompresses all of it even to list, and so must the
port: it prints the payload's length and fails where a damaged payload fails); `list` of a stored bundle reads only
its header (the exe read 592 MB). `unpack` is file creation: entries are checked in order and written on all cores,
each beside its name and moved over it (on 1,747 files: 1,074 ms written one by one with the moves, 703 ms without them,
552 ms on all cores with them). `pack` of a zipped bundle is the deflate of the payload at zlib's level 6 in one stream
- FbrbTool's bytes need .NET's single stream, so it cannot be split across cores; the suite's port of zlib's deflate (E1b)
runs as fast as .NET's native zlib. The gains are in the calls: a launch per call is gone.

**Callers switched.** In the suite: NavKit's unpack (`navkit.Project.run`), `level.heightfield_of`'s terrain unpack and
`bfbc2-stockmp-ai`'s `pipeline.run` go through `fbrbtool.run_cmd`; `bfbc2core.fbrb.unpack` (E3j's) is the same engine.
`BFBC2_FBRBTOOL_EXE=1` (or no Rust core) runs the exe. Project OPFOR's callers are not switched (another repository).

**Known limits.** A write that fails part-way on all cores (a disk full, a file locked) reports the first failing
entry in order as FbrbTool would, but entries after it may already be written (FbrbTool would have stopped). .NET's
messages for I/O errors other than missing / denied / in use / too long are worded by Rust. Path rules are those of
this Windows (device names are plain files on Windows 11; both ways go through the same Win32 calls). A manifest whose
UTF-8 is invalid is decoded with Rust's replacement rules (.NET's may count replacement characters differently).
`pack`'s `hdr=` larger than any real value pads in memory as .NET would. The exe stays the reference this round.

## E4a: the stock-MP AI layer's logic in Rust (2026-11-18, branch `rust-e4a`)

**Provisional timings:** other agents' work ran on the PC throughout (a full WW2 rebuild for about half of the runs).
Every run was at below-normal priority, with old and new run one after the other and the order switched from level
to level. Ratios hold within a pair; single figures vary by up to about 30 %.

Round E3j moved `bfbc2-stockmp-ai`'s unpacks, packs and bundle checks into Rust. This round moves the tool's own
logic: the level model, the authoring, and validation's readings. It keeps the data on the Rust side: the build and
validation no longer build any element tree in Python.

**The profile before** (cProfile, Atacama `generate` + `build` + `validate` in a fresh work folder, 35.7 s profiled):

| where | time |
|---|---:|
| `fbrb.pack_many` (Dist + Package side by side, deflate-bound) | 10.4 s |
| `fbrb.unpack` (the cached stock unpack, then the two build folders) | 5.1 s |
| DbxEdit in process, 10 runs one after another (4 `check` against mp_common, round trips, xml, import) | 5.0 s |
| `dbxbin.parse` / `write` (17 / 8 calls: the level partitions, the donor SP_004_B, kits) | 2.4 / 1.3 s |
| `author.NumberKinds` (every element of the level and the donor walked in Python) + `Author.build` (its `Dbx.instance` scans every instance for each new one) | 1.2 + 0.8 s |
| validation's `coredbx.parse` of the stock and authored partitions into Python trees (12 readings) | 1.9 s |
| `_folder_index` (mp_common's 15,861 files walked, `relpath` per file) | 1.9 s |
| `check_packed` (bundle read, then every built file read and compared in Python) | 2.6 s |
| `check_changes` / `check_structure` / `check_external` beyond the parse | about 1 s |

**What moved, and why:**

- **`dbxbin` -> `bc2_formats::dbx::edit::EditDbx`.** This is the editable tree, kept on the Rust side: one arena of elements named by handles
  (`NodeId`). Each element keeps the file's string indexes, the string table byte for byte until a string is added, each
  container's flag byte and whatever follows the tree. It has the queries (`instances`, `of_type`, `instance`, `child`,
  `attr`, `walk`), the edits (`set_attr`, `set_text`, `set_bool`, `set_numbers`, `set_transform`, `set_ref`,
  `copy_from` / `clone_node` with `dbxbin.clone`'s interning order) and `write`. `round_trips` checks many files on
  all cores (`dbxbin.round_trips`). Wherever `dbxbin` would fail later (a string index outside the table, a value cut
  short, a varint past 32 bits), it refuses the file.
- **The level model and the authoring -> the new crate `bc2-stockmp`.** It has `model::Model` (`levelmodel.Model`, Rush
  stages included) and `author::author` (`NumberKinds`, `Author` step for step: the same edits in the same order, so
  new strings join the table in the same order). The configuration is read from its Python dictionary into `Config`,
  as the reference reads it. Python calls them through `levelmodel.model_of` and `author.author_native`. A level is
  about 400,000 elements, and `NumberKinds` walked all of them (and the donor's) in Python on every build.
- **Validation's readings -> `bc2_stockmp::checks` / `files`.** On the suite's reader (`DbxTree`, the tree `coredbx.parse`
  gives), Rust reads the facts behind `check_structure`, `check_changes` (only the differing leaves come back, with
  `shown()`'s value text), `check_external`'s reference set and `check_stages`' instance facts. `validate.py` still
  decides and words every result. `_folder_index` and `check_packed` walk and compare on all cores.
- **DbxEdit's read-only runs at once.** `validate()` runs every variant's `check` (stock and authored, against
  mp_common) and `roundtrip` together through `dbxedit.run_many` before the checks. They read only. xml / import
  write to the temporary folder, so they stay in order.
- **The rule that keeps the Python's answers.** Every Rust function returns `None` where the port leaves the input to the
  Python. That covers anything the Python raises (every `AuthorError`, a `KeyError` for an unknown soldier, a value
  `struct.pack` refuses), anything it formats itself (a Python float as text), and any configuration value of another
  shape. The caller then runs the Python, so the result and every error message are the reference's. The Python
  stays the reference and the fallback this round: `stockcore.py`, with `BFBC2_STOCKMP_PYTHON=1` or
  `BFBC2_PURE_PYTHON=1`.

**Parity.** ee25186's `bfbc2-stockmp-ai` (a copy run against the same core) against this branch's, `generate` +
`build` + `validate` in fresh work folders. Every file of the level's work folder was compared: `src\`, `build\`
(the bundle folders, the original and authored partitions, the SectorOffsets assets), `out\` (packed bundles, staged
path data), `config.json`, `build_report.json` and `validation.json`, with the work folder's path normalised.

| level | mode | files | MB | identical | old / new (s) |
|---|---|---:|---:|---|---:|
| mp_005 (Atacama) | Conquest | 5,867 | 539 | yes | 26.1 / 15.7 |
| mp_005gr | Rush | 3,338 | 284 | yes | 17.0 / 11.1 |
| mp_002 (Valparaiso) | Rush | 4,645 | 503 | yes | 19.3 / 14.3 |
| mp_004 | Rush | 5,311 | 509 | yes | 20.9 / 16.5 |
| mp_008 | Rush | 3,387 | 509 | yes | 16.0 / 12.6 |
| mp_008cq | Conquest | 2,436 | 232 | yes | 10.5 / 8.9 |
| mp_005sr, mp_002sr, mp_004sdm, mp_008sdm | squad modes (stop in generate, as before) | 346 / 310 / 308 / 1,219 | | yes | the same error |
| mp_001, mp_003, mp_006, mp_006cq, mp_007, mp_012gr (another world's path data*) | Rush / Conquest | 5,101 / 3,261 / 5,925 / 2,643 / 4,879 / 3,919 | 2,867 | yes | 25.8 / 18.1, 18.7 / 14.0, 31.7 / 22.4, 17.7 / 11.9, 24.8 / 18.1, 26.7 / 19.0 |
| mp_009cq (another world's path data*) | Conquest | 2,543 | 386 | yes (validation fails both ways, identically) | 25.0 / 12.7 |
| mp_009gr (another world's path data*) | Rush | 332 | 83 | yes (the same generate error) | 1.9 / 1.9 |

\* Levels without path data of their own were built with another level's (`--pathdata`). This is not a playable
layer, but it puts more of the stock levels' model, authoring and validation through both versions. In total:
18 levels and 55,770 files, all identical.

More proof:

- `tests/test_native_levels.py` (game tier, new): all **52 multiplayer level partitions** of the game copy (Dist and
  Package). The level model is equal field for field (Rush stages included) and the round trip is equal on all 52.
  **30 are authored** from a generated configuration: bytes and report equal Python's, none left to Python. Their
  validation readings with the Rust core equal those without (100 s).
- `test_stockmp.py` (quick): Conquest, Rush and minimal layers on the synthetic levels, bytes and reports equal. Five
  cases are left to the Python and give its answers: an authored level, an unknown soldier, a decimal set as text, a
  value of no known kind, a `None`. The game tier also checks that the build's authoring of Atacama equals the
  Python reference's.
- `test_validate.py` (quick): every check call on the good and broken synthetic builds runs both ways with equal
  results (12 runs plus the packed-bundle comparison).
- A run whose module had gone stale (the Rust sources changed under it, so `bfbc2core` refused it) ran pure Python on
  both sides. For six levels the outputs were identical except validation's DbxEdit lines (the old copy found no
  `DbxEdit.exe` beside it). So the Python fallback gives the old bytes too.
- One known difference in wording: when **several leaves of one stock instance** change, `check_changes` names them in the Rust
  reading's order. The Python's order is a set's, which varies from process to process. No passing build has this.

**Time** (whole runs from the parity table: old -> new): Atacama 26.1 -> 15.7 s, the Rush levels 16 - 21 s -> 11 - 17 s,
the six levels on other path data 18 - 32 s -> 12 - 22 s. Validation alone: 6 - 15 s -> 2.3 - 5.2 s. Atacama
profiled: **35.7 -> 20.5 s**. Peak working set 470 - 530 MB -> 350 - 430 MB (no Python trees of the partitions). The
game-tier test `stock-MP AI layer, Atacama Conquest built` ran 53 s with the Rust core and 66 s with
`BFBC2_STOCKMP_PYTHON=1`, both under the WW2 rebuild. Its writer check of Atacama's 2,567 files now runs on all cores,
with the Python reference on the 4 largest; before, the Python ran on all 2,567 (9.4 s of parse in E3j's profile).

| piece (Atacama, one variant) | Python | Rust |
|---|---:|---:|
| level model (`model_of`) | 0.15 - 0.23 s | 0.013 - 0.022 s |
| authoring (parse, writer check, model, `NumberKinds`, `Author`, write) | 0.63 - 0.82 s | 0.056 s |
| validation's readings + folder index + packed bundle (both variants) | 7.4 s (profiled) | 1.7 s (profiled) |
| DbxEdit's six read-only runs | about 4 s in order | 1.8 s at once |

**What remains** (the profile after, 20.5 s): packing **10.3 s** (zlib's deflate of each payload, byte for byte; unchanged,
see E3j), the four unpacks **3.9 s** (file creation), DbxEdit **2.8 s** (1.8 s for the six runs together, then
xml / import), `check_packed` 1.1 s (reading 2,900 built files), `generate` 1.1 s (NavKit's `ordercheck.PathData` in
NumPy and the placement). The Python of `config.generate` (placement on the path data) and `pipeline` (staging,
manifests) was not ported: they are small next to the above. The Python model, `Author` and the checks' Python
readings are kept as fallback and reference until a later round retires them.

## E4c: aptgraft's model, aptcheck and aptrefs in Rust (2026-11-18, branch `rust-e4c`)

**Provisional timings:** other agents' builds and tests (a WW2 rebuild for the first profiles) ran beside every
figure, below-normal priority; ratios hold within a run, absolute seconds move by up to 1.5x between runs.

The corpus of E3n: 133 distinct movies (the game copy's SwfMovies, Dist and Package; OPFOR's pristine menu bundle;
the BIGF movies of `work\xbox_dlc`), read into memory first. `python rust\bench_apt.py [--model native|python|both]
[--profile WORKLOAD]` times every row below.

### The profile (main, before)

`roundtrip --pack` of the 133 movies one by one (cProfile, 240 s profiled): the parse 71 s (three models per movie:
`wref` / `character` / `_fill_items` / `_stream`, 2.2 M `wref`, 2.3 M `words`), the writes 74 s (798 writes, `_emit`
21 s, `plan` / `_layout` 49 s), `reachable` 22 s, apt.py's dump for `dump_text` 54 s (the Rust decoder was off in that
profile: E3n's decoder takes most of that away), the rest leftovers and comparisons. `aptcheck check`: `Code.__init__`
and `findings` over 2.9 M records built in Python (3.6 s of 5 in E3n's figures); aptrefs: `movie_refs` over the same
records. Every Python object of the model is made for every movie whether the caller reads it or not.

### What changed

- **The model in Rust** (`rust/bc2-formats/src/aptmodel.rs`, bound as `bc2_native.AptModel`): `Movie._parse` ported in
  its own order (the fills run once, on first read; shared nodes shared as the cache keys make them; a pointer read as
  a number where Python would fail refuses), `reachable()` in the Python walk's order, the `keep` and `pack` layouts,
  `write` in either byte order with the dead-data pass, `leftovers`, and `roundtrip` with `dump_text` (apt.py's dump
  from E3n's decoder, the offsets taken out by a scanner of the same pattern, Python's `str.strip`). Nodes are
  numbered; instructions are an arena per code node (streams of indices, branch targets as an instruction or a
  stream's end).
- **`aptgraft.Movie(...)` is a `NativeMovie`** when the Rust core is built: no Python object is made for the data
  until a caller reaches it (a root, a `Ref`'s node, `reachable()`, `cache`). A node first appears as a shell of its
  class (`_WordsShell(Words)` ...: `isinstance` holds) and is filled from Rust the first time one of its attributes is
  read or set - then it is a plain `Words` / `Code` / `Str` / `Raw` and Python's. Before each write (and
  `reachable()`, `leftovers()`) the filled nodes and every new node they reach go back to Rust (`_sync`: words as
  numbers and node numbers, code as columns with each branch target found as `_place` records it, by identity);
  another movie's node put into this one is adopted as Python shares it. `new_at` is set after each write as the
  Python write leaves it. Where the Rust refuses - a movie its parse would raise on, a value a write cannot pack (a
  negative or 40-bit operand, a float, a `str` in a words node, a branch to nothing), a read that parses more of the
  file (`words` / `string` / `character` / `item`, `code` of an offset not read) - the movie becomes the Python model
  in place (`_python()`: every node filled, the class switched to `Movie`) and the Python code answers: the same bytes
  or the same exception. Callers keep the API: the grafting code, `add_crosshair`, the aliases and mode cases, OPFOR's
  menu callers (read only this round): `src\tools\ww2\menu_install.py`, `wake_ui.py`, `duplicates.py`,
  `weapon_icons.py`, `type95_icon.py`, `ww2_crosshairs.py`, `qa_checks.py`, `bf1943_hud_icons.py`, `wake_install.py`,
  `src\tools\onslaught\console_icons.py` (in process), `console_hud.py` (the command line), the test
  `src\tools\tests\test_ww2_crosshairs.py`.
- **`roundtrip` in Rust, many at once**: `roundtrip_many(specs)` reads a chunk of movies (twice the cores) and runs
  each whole roundtrip in Rust on all cores; `aptgraft roundtrip` uses it. A movie it refuses runs
  `roundtrip_python` (the reference) at its place.
- **aptcheck and aptrefs** (`rust/bc2-formats/src/aptscan.rs`): `Code`'s functions and `findings()`, and
  `movie_refs`, over the records where E3n's decoder makes them - no record reaches Python.
  `aptcheck.check_many(movies)` (used by `check`) and `aptrefs.refs_many(movies)` (used by `movie`, `xref`, `find`)
  decode and scan many movies on all cores; a movie the decoder refuses, or a push-byte argument `int()` would reject,
  runs the Python (`check_many_python` / `refs_many_python`). `aptcheck verify` keeps `Code` (it needs the records).
- References kept: the Python model (`aptgraft.use_native(False)`, `BFBC2_PURE_PYTHON=1`, `REFERENCE` /
  `BFBC2_APTGRAFT_REFERENCE=1`, `_py_movie`, `roundtrip_python`), `Code` / `movie_refs`.

### Before and after (unprofiled, same run)

| workload (133 movies) | Python model, s | Rust, s | |
|---|---:|---:|---:|
| `Movie(members)` (the parse) | 13.26 | **1.46** | 9x |
| every write of each unchanged model (keep, pack, other order, dead off: 532 writes) | 61.61 | **4.64** | 13x |
| `roundtrip(..., pack=True)` one by one | 125.17 | 15.24 | 8x |
| `aptgraft roundtrip --pack --quiet <folder>` (batched on all cores) | 113.23 | **3.24** | 35x |
| two sprites grafted into the next movie, placed, a label alias, three writes (`graft_all`) | 104.52 | **10.95** | 9.5x |
| aptcheck's rules (`check_many`) | 10.66 | **1.03** | 10x |
| aptrefs' scan (`refs_many`) | 11.30 | **0.90** | 12.5x |
| `aptrefs xref` (every movie set, the exe's table) | 8.64 | **1.09** | 8x |

(The Python column is the code as main has it: E3n's writer and the Rust decoder.)

### Parity

- `bfbc2-menu-tools/tests/test_apt_e4c.py` (quick): the object rules (one object per node; a shell until read; a shell
  written before it is read, a grown string, one new node in two places, another movie's node; a `str` field, a
  negative / 40-bit / float / bool operand, a branch to nothing, a read node's offset taken away - each the Python
  model's bytes or exception; the cache view; a read that parses more turns the movie into the Python model); test
  `aptgraft`'s built movies (both byte orders, crosshair movies, 3,000 pushed floats): every write, roundtrip --pack,
  three grafts, label alias / mode alias / mode case and their writes, crosshair-add - the same as the Python model;
  300 damaged copies written and round-tripped the same (221 refused by both); aptcheck / aptrefs on the built movies
  and 200 damaged copies the same as `Code` / `movie_refs`. `test_apt_e3n.py` (the Rust model now answers where it
  ran E3n's writer) and `test_aptgraft.py` pass unchanged.
- `--game`: the 133 movies, both models in one process: 3,593 writes (each unchanged model 8 ways; two sprites grafted into the next movie of its byte order and written three ways; a label alias, `add_mode_alias(onslaught, conquest)` and `add_mode_case(getMapDesc)` with three writes; crosshair-add between every pair of the 37 movies that have a crossHairs table, two ids each), `roundtrip --pack` of each (`roundtrip_python` / `roundtrip_many`), aptcheck and aptrefs - 0 differ (Python 1,109 s, Rust 352 s for the same work; most of the Rust figure is the grafting code's own Python, 1,332 crosshair-adds).
- Rust: `cargo test -p bc2-formats apt` (the model's write-back in both byte orders, packed, a grown string moved,
  refusals; the strip pattern; the scans) and the modules' doc examples.

### What remains

- Edits still run in Python over the nodes they reach: a graft or an alias fills the nodes it walks (a label alias
  fills every frame and item of every timeline) and each write sends every filled node back - `_sync` is two thirds of
  `graft_all`'s 11 s. Dirty tracking would need list proxies for `vals` / `ins` (callers mutate them in place); the
  grafting operations themselves (`Grafter`, `add_label_alias`, `add_crosshair`) ported to Rust would avoid the fills.
- A `NativeMovie` cannot be pickled or deep-copied (its handle); no caller does. An unfilled shell's
  `type(n) is Words` is false until it is read (`isinstance` holds).
- `aptcheck verify` / `lint` still build `Code` (the records in Python) for the two movies they compare.

## E4b: the shader database logic (2026-11-18, branch `rust-e4b`)

E1c ported `shaderdb.py`'s reader and writer; this round its logic. **Profiled first** (`cProfile`, the game copy's
databases, the disk bundle cache off; OPFOR's code copied as it is, OPFOR not imported or written):

| workload (before) | total | where the time went |
|---|---:|---|
| OPFOR's `wake_install.index_templates` (74 stock Dx9 databases, `wake_shaders.signatures` per entry) | 13.6 s | 4.7 s OPFOR's own loop (`dict(db.entries)[n]` once per entry: quadratic), 3.9 s `signatures` (221,347 `block_textures` 1.25 s, `block_constants` 0.45 s, 1.37 M `program_refs` 0.76 s), 2.4 s bundle reads, 0.9 s parsing |
| `load_levels` of every level, Dx9 / 10 / 11 (222 databases) | 11.7 s | **4.8 s `shaderdb_parse`: 22 ms a database, nearly all of it making ~70,000 Python objects** (Rust reads one in 2 ms), 4.0 s bundle reads, 1.0 s bundle headers |
| `wake_shaders.merge_stock` for Guadalcanal (base mp_005, 74 donors scanned) | 24.2 s | 8.3 s bundle reads, **6.3 s parsing, 3.3 s `coverage`** (228 calls, Python string joins), 2.2 s `add_donor` (1.1 s `part_mesh`, 0.7 s the needed-name test), 1.4 s headers |
| fix_layouts' pattern: `entry_textures(db, {name})` per name | 1.1 ms a call | each call builds `entry_programs()` and walks every entry for one name |
| `Merger(base)` + `clone_entry` + `finish` | 31 + 0.3 x n + 40 ms | record indexes and the program sort: Python dicts over bytes, small |

**What moved, and why.** The cost was Python objects and per-call rebuilding, not arithmetic, so the database stays on
the Rust side:

- `bc2_native.ShaderDbHandle` (`rust/bc2-py/src/shaderdb_handle.rs`): `ShaderDb.parse` keeps the file's `bytes` and the
  place of every record (no copy of the records); a section becomes the Python list `shaderdb_parse` made (the same
  objects) only when the attribute is first read (`ShaderDb.__getattr__`). Once every record section is made, the name
  sections are made too and the file is let go (no database held twice).
- The queries ask the handle: `entry_textures`, `entry_signatures` (new: OPFOR's `wake_shaders.signatures` as a suite
  function), `entry_layouts` (new), `coverage`, `ShaderDb.mesh_names`, `ShaderDb.check` (with an MD5 of its own),
  `Merger.add_donor`'s needed entries and mesh-part names (`select_needed`), and the writer. A section never made is
  read from the file, one made from the caller's list (records live, item by item). The entries' and groups' lookups
  (name -> entries, GUID -> group) and each parameter block's textures / constant names are kept between calls; each
  call checks the lists they came from by identity (the list object, its length, every item; a block by its object),
  so a list changed in place or replaced is read again. The pure logic is `bc2_formats::shaderdb::query`
  (`block_textures`, `block_constants`, `block_engine_textures`, `program_refs`, `texture_kind`, `part_mesh`, `lower`
  (Python's `str.lower` on latin-1), `Needed`, `Coverage`, `check`, `md5`, `swap_guid`).
- `block_textures`, `block_constants`, `block_engine_textures`, `texture_kind` read in Rust per call (a `bytes` block,
  `'<'` / `'>'`; a latin-1 name).
- Anything outside the plain case - an item of another type, a name that is not latin-1, an index past its section, a
  short record, a struct prefix other than `<` / `>` - makes the handle answer `None`, and the Python reference
  (`entry_textures_reference`, `check_reference`, `coverage_reference`, ...; kept) runs and answers or raises as before.

**Not moved, by measurement:** `Merger`'s record copying, `clone_entry`, `retexture`, `finish` and `entry_contents`
(the clone workload is 0.77 s both ways; a merge's `add_donor` after the selection is a few hundred records);
`ShaderDb.entry_programs` and `program_refs` (they hand out the caller's own list objects / are one `struct` call).

### Before and after (`rust\bench_shaderdb.py`: each run its own process, "before" = the module's handle and block
readers hidden so `shaderdb.py` runs as it did; disk bundle cache off; PC shared with other agents' builds)

| workload | before | E4b | peak commit before / E4b | result |
|---|---:|---:|---:|---|
| merge (OPFOR's `merge_stock`, Guadalcanal, Dx9 / 10 / 11 written) | 19.2 s | **9.9 s** | 2,104 / 1,799 MB | the same files and reports |
| load (every level x Dx9 / 10 / 11 + coverage) | 16.1 s | **7.8 s** | 1,974 / 1,836 MB | same |
| templates_api (index_templates on `entry_signatures` + `entry_layouts`) | 10.1 s | **7.1 s** | 1,060 / 970 MB | same - **and the same digest as OPFOR's own code below** |
| templates (OPFOR's `index_templates` as it is) | 21.1 s | 19.5 s | 1,195 / 1,159 MB | same |
| textures (per-name `entry_textures`, six levels) | 0.95 s | 0.69 s | 314 / 292 MB | same |
| clone (`Merger` + `clone_entry` + `finish` + write) | 0.77 s | 0.78 s | 384 / 377 MB | same bytes |

One database (mp_005 Dx9: 19 MB, 1,714 entries, 4,230 blocks, 20,366 programs), per call:

| | before | E4b |
|---|---:|---:|
| `ShaderDb.parse` | 15-17 ms | **2.2-2.9 ms** (every section made later: 29 ms, as before) |
| `entry_textures(db, {name})` | 0.161 ms | **0.021 ms** (0.028 ms on the first call after the entries changed) |
| `entry_textures(db)` (all) | 37 ms | 9.9 ms |
| `coverage(db, [mesh])` | 3.7 ms | 0.42 ms |
| `block_textures` + `block_constants` | 5.1 us | 1.7 us |
| `to_bytes` (sections never made) | 16.7 ms | 7.5 ms |
| `ShaderDb.check()` (Dx10, 20,366 programs) | 18.7 ms | 8.8 ms |

OPFOR's verbatim template index gains little because its own `dict(db.entries)[n]` per entry is quadratic (4.7 s of
the profile); on the suite's functions the same table takes 7.1 s instead of 21.1 s.

### Parity

- `bfbc2-shader-tools/tests/test_shaderdb_queries.py` (quick, 22 s): built databases (Dx9, Dx10, Xbox 360 v120 /
  v117; a repeated entry name, a latin-1 name, an entry without programs) and mp_008cq's three: every query (all
  entries, sets / frozensets / lists of names, 25-40 names one by one, coverage with a dict and with names not
  latin-1, `select_needed`, `check`, OPFOR's `signatures` code on the Rust block readers, the writer) against the
  references on a database read by the Python reader - never made, made, and after 7 kinds of change (an entry
  appended, a group replaced, a group's program list changed in place, two blocks swapped, the program and key lists
  replaced, a name not latin-1, a group naming a program past the end): 65,466 answers on mp_008cq, 1,046 on the
  built files, all the reference's; 18,000 reads of damaged / random blocks (NaN, -0, cut, long names); the handle
  answers itself (no decline) on fresh, made and changed files and declines on a name that is not latin-1; parsing
  and the queries make no section; pickles and deep copies hold plain lists.
- `--game`: **745 databases** (436 in the game copy's and OPFOR's Xbox bundles, 5 BF1943, 304 work files; 17.35 GB):
  **13,643,145 answers, all the reference's** (715 s, 2 workers).
- `test_shaderdb_native.py` (quick and `--game`): unchanged and passing on the handle - its corpus compares every
  section the handle makes with the Python reader's, the written bytes with the file, and the four WW2 merges
  byte for byte with the Python merge.
- The bench's digests: every workload's output the same before and after (the merged databases byte for byte).

### For Project OPFOR (not edited)

The suite's changes reach OPFOR without edits (parsing, `entry_textures`, `coverage`, the merge's selection, `check`,
the block readers). OPFOR would gain more by calling the new functions:

- `wake_shaders.signatures` -> `shaderdb.entry_signatures` (the same table: the bench's two template digests are
  equal); `wake_shaders.entry_layouts` -> `shaderdb.entry_layouts`.
- `wake_install.index_templates`: `dict(db.entries)[n]` inside the per-entry loop is quadratic (4.7 of 13.6 s);
  `entry_signatures` + `entry_layouts` give the table in 7.1 s instead of 21.1 s.
- `wake_install.fix_layouts` (`dict(base.entries)[n]` in the `bad` comprehension, `d.entry_programs()` rebuilt per
  candidate), `_fix_layouts_apply` and `graft` (`S.entry_textures(donor, {dn})` twice per entry: now 8x faster per
  call by itself), `ww2_level_build`'s clone steps and `S.entry_textures(db)` whole.
- `wake_install.level_db` keeps 7 databases and reloads: a reload's parse is now 2-3 ms instead of 15-20.
- `type38.donor_db` scans `d.entries` per prefix (makes the entry list; `coverage` would not).
- In the suite, `renderqa.prepare_strip` / the patch runs rebuild `{n.lower(): g ...}` and `entry_programs()` per
  strip: small next to `shaderview`, left as they are.

**Remaining:** the `Merger` itself (records, `clone_entry`, `finish`) in Rust would need the merged database kept on
the Rust side end to end (OPFOR mutates `base.entries`, `base.groups`, `base.programs` directly between merges);
`entry_contents` / `verify_merge` (the `merge` command only). A partly made database holds its file and its made lists
(the file is let go once every record section is made).


## E4f: the map and bot test harness and the crash-reader tools (2026-11-18, branch `rust-e4f`)

`bfbc2-automated-map-and-bot-test-harness` (the harness package, `crash-reader/crashreader.py`, `crash-reader/exeref.py`)
in a new crate, `rust/bc2-harness`. **Profiled first** (cProfile and wall clocks, the game copy's `opfor.log` (349 MB)
and `BFBC2Game.exe` (build 795745), read only; harness runs to temporary folders):

| workload (before) | total | where the time went |
|---|---:|---|
| `harness.py import-log` on the real log | 4.4 s | 2.4 s `open().read()` (UTF-8 decode of 349 MB), 2.0 s `ingame.sessions` (E3m's literal search, Python lines) |
| `crashreader.log_before_crash` (3 lookups) | 0.6 s | reading the file; E3m's windows already decode only the crash lines |
| `exeref.py` str / refs / dis (one command) | 0.4-1.2 s | interpreter + pefile + capstone set-up, the code walk (`on_boundary`'s capstone decode from each function start), numpy's call / jmp scan |
| `harness.py run` on sp_009 + sp_004_b (Rust core built) | 42 s | 23 s NavKit's `navparity` (not the harness's; cached after the first run), 9 s `ordercheck.on_area` (NavKit's tool), 2.6 s battery (`navpath.route`), 2.5 s hpdcheck, **2.0 s `reach_from` (the harness's BFS)**, **1.9 s `classify.Ground.cells` (a scan of every cell per point)**, 2.5 s level reading |

**What moved** (`rust/bc2-harness`, bindings `rust/bc2-py/src/harness.rs`, Python switch `crash-reader/harnesscore.py`):

- `oplog`: `ingame.sessions` (`harness_sessions`) and the file's (`harness_sessions_file`: read, decoded with Python's
  `errors='replace'` rule, split as `str.splitlines` splits on all cores, only lines holding a literal read with the
  reference's patterns); `crashreader.log_before_crash` (`crash_log_before`, the windows of E3m on the bytes).
- `pe`: the exe mapped as pefile 2024.8.26 maps a well-formed PE32 (refused otherwise: pefile maps it).
- `x86`: 32-bit decoding with **capstone 5.0's text and operands** on iced-x86 1.21 (pure Rust): capstone's Intel
  printer rules (`X86IntelInstPrinter.c`: the memory operand, immediates' signs and masks, `X86ImmSize.inc` and its
  missed F6 /1 entry; `X86_lockrep`: lock / rep / repne / repe / repz / bnd / notrack / xacquire / xrelease from the
  prefix bytes as capstone's decoder reads them; `checkPrefix`'s lockable list), capstone's names (ljmp / lcall, pushal,
  fcompi, the compare pseudo-ops, `nop r/m`), the x87 operands it leaves out, its branch-target arithmetic (16-bit
  displacements unsigned unless 0x67), and its refusals (x87 aliases, the hint-nop / prefetch / MPX / fence families,
  newer instructions). **Proof: every byte offset of BFBC2Game.exe's code, 16,786,165 starts, decoded identically**
  (size, mnemonic, op_str, immediates and memory operands; `x86_dump` against a capstone dump). The proved encodings
  are a table (`x86_verified.rs`: 2,730 instruction keys, 20,466 undecodable keys, from `x86_codes`); an encoding
  outside it is refused and capstone decodes it, so another exe never gets a different answer. On 3.4 M synthetic
  probes (every prefix set x opcode x ModRM) the core answered 877 k identically and refused the rest; 0 answered
  differently.
- `exe`: `crashreader.Image` (function starts, `around`, `call_before`, `name_of`) and `exeref.Exe` (`cstr`,
  `find_strings`, `on_boundary`, `refs`, `data_refs`, `window` / `annotate`) whole, over the mapped image kept on the
  Rust side (`ExeHandle`); each method refuses (`NotImplementedError`) where the reference would raise or an
  instruction is unproved, and the Python facade (`Image._native_call`) then runs the `*_reference` method.
- `graph`: the reachability pair bits (`harness_pair_bits`: a BFS per source node, on all cores) and the ground
  classes' cell columns (`HarnessCells`: a bucket index, the reference's float64 test, cell order).

**Parity** (`crash-reader/tests/test_crash_native.py`; `tests/test_harness.py --game`):

| what | result |
|---|---|
| x86, BFBC2Game.exe, every code offset (`x86_dump` vs capstone) | 16,786,165 of 16,786,165 identical, 0 refused |
| x86, every 61st offset (game tier) | 275,185 of 275,185, 0 refused |
| x86, 20,000 seeded byte strings (quick tier) | 13,210 answered, all capstone's; 6,790 refused |
| the mapped image | pefile's, byte for byte (BFBC2Game.exe 26,292,224 bytes; built exes) |
| built exes (test_exeref's + 2 random-code exes): every method at every code address | 420 + 5,013 + 5,199 answers the reference's |
| `exeref.py` command line, 24 commands on BFBC2Game.exe | 24 of 24 byte for byte as `BFBC2_HARNESS_PYTHON=1` |
| crash reader on 40 built reports at sampled code addresses | 40 of 40 byte for byte |
| opfor.log sessions: the real log | 587 of 587 sessions field for field (as `sessions_python` and `sessions_reference`) |
| sessions of 400 seeded logs (every line break, bad UTF-8, Unicode digits) | 400 of 400 (320 read by the core, 80 refused to Python) |
| `log_before_crash`: 300 seeded logs + the real log's 3 lookups | identical |
| pair bits (60 seeded graphs), cell columns (2,000) | identical |
| `harness.py run` sp_009 + sp_004_b, core vs `BFBC2_HARNESS_PYTHON=1` | every result and the report byte for byte |

### Before and after (`rust\bench_harness.py`, in process, best of 2; game tests' wall clocks; shared machine)

| workload | Python | E4f | |
|---|---:|---:|---:|
| import-log's sessions of opfor.log (349 MB, read included) | 3.17 s | **0.43 s** | x7.4 |
| crash reader: log lines before a crash (3 lookups) | 0.59 s | 0.47 s | x1.3 (the file read) |
| exeref: load, 3 string searches, their refs, 3 refs and windows | 1.06 s | **0.19 s** | x5.7 |
| crash reader: load, code view, call before and name at 60 addresses | 0.42 s | **0.05 s** | x7.7 |
| `exeref.py`, 24 commands as processes | 11.7 s | 5.5 s | (interpreter start-up included) |
| crash reader, 40 reports in process | 2.4 s | 1.0 s | |
| `harness.py run` sp_009 + sp_004_b (structure, reachability, battery, orders, offsets, level; caches warm) | 12 s | 9 s | |

**Not moved:** NavKit's tools the harness calls (`navparity` / `navdiff`, `ordercheck.on_area`, `hpdcheck`,
`navpath.route` - NavKit's and core's, outside this round), `levelinfo` (core's dbx / fbrb / level readers, F1a's),
`checks.join` and `random_points` (NumPy's generator: the same draws need NumPy), and `runs` (saved-run JSON, the
comparison and the report: milliseconds). cargo fmt / clippy -D warnings / tests clean; ruff and mypy clean.
## E4e: NavKit's tools around the build (2026-11-18, branch `rust-e4e`)

NavKit's driver (`navkit.py`) and the tools around the build: what is left after rounds E1d - E3o and R1 moved the
generator itself. **Provisional timings:** four other agents worked beside every figure (below-normal priority); each
before / after pair was taken in the same session, unprofiled unless it says so.

### The profile (before)

| workload | time | where it went |
|---|---:|---|
| `navvehicle.build_level` ONS_MP_002 (`test_vehicle_level.py`'s build) | 90 s (cProfile) | **`obstacle_distance` 85 s**: NumPy over bands of fixed layers, every offset of the 3.5 m cylinder shifted and compared per layer (24,454 `reduce` calls, 22.8 s of them in `np.any`) |
| `navkit.py prepare` sp_009, a fresh OPFOR folder (common bundles unpacked too) | 887 s (cProfile) | **`levelobjects.main` 603 s**: `blueprint_flags` 327 s and `mesh_aliases` 255 s each read every `.xml` of the level and of sp_common / mp_common (33,828 opens, 537 s of them `open` - new files are scanned on their first opening); the unpacks 281 s (DbxEdit's `xml` 229 s: 16,872 files converted and flushed one after another) |
| `navkit.py prepare` sp_004_b, common bundles already unpacked | 478 s (cProfile) | `levelobjects.main` 448 s (33,460 opens: 402 s) |
| the parity measures (`navparity.run_level`, navregress's second half), sp_004_b | 11.7 s (27 s cProfile) | `navdiff.diff` 5.9 s (`compare_sector`: cells matched in dicts, areas by shape as sets of pieces, nodes by position), `navcompare` 3.6 s (`ordercheck.PathData.areas`: every area's crack rule in Python), `ladder_ground` 1.1 s, `navdf` 0.4 s; `hpdtool.parse` 11 times for two files |
| `navwater_mesh.level_run` + `navdomains.level_world` ONS_MP_004 | 28 + 9 s (cProfile) | `World.tables`: 463 Python Dijkstras 7.1 s; the boat mesh's picture (matplotlib: 3,677 `plot` calls and its import) about 9 s; the rest spread thin |
| `navregress.build_level` sp_004_b / sp_009 | 22.6 / 6.4 s (cProfile) | spread thin since E3h: `walkable` 6.6 s, `detail_pass` 2.0 s (`navdetail`), `write_file` 1.4 s, `build_areas_many` / `graph_many` 1.6 s each |
| `navsample_offline.run_level` sp_009 at 0.8 m | 8.7 s (cProfile; 4.0 s plain) | collision loading 3.6 s, the capsule test 2.4 s, the terrain test 2.1 s |
| `navkit.py build` / `check` sp_009 (stand-in folder) | 2.9 s / 1.6 s | |

### What moved

| Python (kept: the reference and the fallback this round) | Rust | binding |
|---|---|---|
| `navvehicle.obstacle_distance` (`obstacle_distance_reference`) | `vehicle::obstacle_distance`: each candidate surface walks the offsets nearest first and stops at its first hit, columns in parallel; the plane, the rise and the band in `float32` as NumPy 2 computes them | `navkit_obstacle_distance` |
| `navdomains.dijkstra` behind `World.leg_dijkstra` / `tables` | `domains::Graph` (a domain's adjacency kept in the core, `Domain.graph()`), `dijkstra` (the same pops: `(d, node)` is a total order), `tables` (every transition point at once) | `DomainGraph`, `navkit_domain_graph` |
| `levelobjects.blueprint_flags` (its glob walk and reads; `mesh_aliases` / `havok_index` were done the same way on this branch, then main's round F1a moved them to `bfbc2core.collision`'s Rust index, which the merge keeps) | `levelscan::scan`: one walk in `glob`'s order, each `.xml` read once on all cores (`xmlinst.field_ref`'s matching, `open(..., errors='replace')`'s text); the dict made in Python from its records (`levelobjects.scan`) | `navkit_level_scan` |
| `navdiff.compare_sector` and the cells of `diff` | `measures::compare_sector` / `compare_many`: meshes parsed (E3o's reader) and sampled (E3h's `area_cells`) in the core, every sector in parallel; Python's `round` (half to even), `round(x, n)` through the exact integer it rounds to, `x ** 2` as `pymath::sq` | `navkit_diff` |
| `ordercheck.PathData.areas` / `on_area` (the harness and the stock-MP AI call it too) | `measures::order_areas` (the crack rule), `on_area` | `OrderAreas`, `navkit_order_areas` |
| `navdf.compare` with `ground_of` | `measures::df_features`, `df_compare`, `ground_of` | `navkit_df` (`navdf.compare_sectors`) |
| `ordercheck.PathData`'s route groups | E3o's `navkit_union_roots` on the node ids numbered as first met (the same roots) | |
| `navsample_offline.terrain_blocks` (`terrain_blocks_reference`) | `offline::terrain_blocks`: every surface's ring points on the height field, in parallel; `terrain_at`'s `float32` / `float64` arithmetic | `navkit_terrain_blocks` |
| DbxEdit's `xml` (a folder's files one at a time) | `dbxedit::cli::xml`: a folder's files on all cores, the messages and counts in the files' order | (in process, `bfbc2core.dbxedit`) |

Python only, the same values: `hpdtool.parse` reads each sector's nodes and edges with `iter_unpack` and checks the
blocks' length by arithmetic instead of writing every block again (a block cut short still raises from the record
loop, as before); `hpdtool.read` / `shared_reads()` parse a file once for all of `navparity.run_level`'s readers.

`bfbc2-auto-navkit/tools/navnative.py` is the switch: `BFBC2_NAVTOOLS_PYTHON=1` (these pieces only),
`BFBC2_PURE_PYTHON=1` or `navnative.use_native(False)` run the Python; where the core refuses an input (arrays that do
not fit, a cost that is negative or NaN, a value `round` cannot take, a mesh it does not read, a file it cannot read)
it returns `None` and the Python runs, raising what it raises.

### Before and after

| workload | before | after | |
|---|---:|---:|---:|
| ONS_MP_002 vehicle grid (`vehicle_grid`) | 81.8 s | **2.0 s** | 41x |
| ONS_MP_002 vehicle path data (`build_level`) | 57.6 s | **3.1 s** | 19x |
| ONS_MP_004 domains world and its tables | 4.9 - 6.9 s | **2.3 - 4.1 s** | |
| `navkit.py prepare` sp_004_b, fresh folder (common bundles unpacked and converted too) | 549.5 s | **56.7 s** | 9.7x |
| `navkit.py prepare` sp_009 after it (common bundles there) | 410.5 s | **9.6 s** | 43x |
| levelobjects' scan alone, sp_004_b's folders (warm) | 14.4 s | **3.8 s** | 3.8x |
| parity measures, 15 levels (`navparity.run_level` on every regression build) | 581.6 s | **158.7 s** | 3.7x |
| ... sp_004_b / sp_009 | 10.6 / 1.9 s | **3.2 / 0.4 s** | |
| ... sp_006 (the largest gain) | 100.4 s | **19.3 s** | 5.2x |
| `navregress.py --levels sp_004_b,sp_009` (the game tier's): per level, build + structure + measures | 30 / 5 s | **16 / 3 s** | |
| offline sampler, sp_009's 0.2 m pair over its detailed sectors (geometry loaded) | 29.2 s | **8.9 s** | 3.3x |
| offline sampler, sp_004_b at 0.8 m (geometry loaded) | 11.1 s | **3.4 s** | 3.3x |
| `test_vehicle_level.py` (ONS_MP_002's vehicle file and its checks) | 36.5 s (durations seed) | **3 s** | |

(The prepare figures without the parallel DbxEdit `xml`: 285.6 s and 45.6 s.)

### Parity

- **Recorded from the Python before the port** (`bfbc2-auto-navkit/tests/navtools_golden.json`, `test_navtools_native.py
  --game`, the build clock fixed): ONS_MP_002's vehicle measures (every per-surface array of `vehicle_grid`) and its
  vehicle path data (291,704 bytes); ONS_MP_004's boat mesh side file and report (less their build time), its domains
  world (transitions and tables) and 40 plans; navparity's rows for sp_009 and sp_004_b (built with the clock fixed);
  sp_009's offline samples at 0.8 m (both capsules). All **as recorded** after the port.
- **The measures on every level**: `navparity.run_level` on the 15 regression builds of OPFOR's `work\navkit\regress`
  (13 campaign levels, ONS_MP_002 / 004), Rust and Python: **15 of 15 rows identical** (every key, value and order).
- **`navkit.py prepare`** on a fresh stand-in OPFOR folder (the game copy linked, its own `work`), old code
  (`BFBC2_NAVTOOLS_PYTHON=1`) against new: sp_004_b then sp_009 - **51,256 files identical** (`work\levels`: 8,091;
  `work\navkit`: 43,165 - the common bundles' unpacked and converted files, `objects.json`, the triangle `.npy` files,
  `ladders.json`, `navkit.json`). With the parallel DbxEdit `xml` as well (a third fresh folder): the same 51,256 files
  identical to the old code's.
- **levelobjects' scan** on OPFOR's sp_009, ONS_MP_002 and sp_004_b folders (read only): `blueprint_flags`,
  `mesh_aliases`, `havok_index` the same items in the same order (624 / 910 / 581, 558 / 1,225 / 570, 572 / 853 / 537).
- **Quick** (`test_navtools_native.py`): 24 random multi-layer runs of `obstacle_distance` (pitch 0.2 / 0.8 / 1, both
  bands, with and without extra heights) bit for bit; 30 random graphs (equal costs, zero costs, unreachable nodes)
  tables and searches, 72 more searches with and without targets; a built folder tree (hidden folders, upper-case
  `.XML`, CRLF, invalid UTF-8, attributes in both orders, `_Havok_Havok.res`) for the scan; 40 random file pairs
  (areas kept, moved, raised, dropped, nodes moved, edges' costs changed, sectors only one side has; D lines and F
  cells) for `navdiff.diff`, `navdf.compare_sectors`, the route groups and `on_area` - all the same as the Python.
- **navregress** on the game tier's two levels: sp_004_b's and sp_009's rows (53 measures each) **identical** to the
  previous run's (`regression_last.json` of 01:04 this morning, main's code), the build time and the candidate's SHA-1
  (each sector stores its build time) aside; the builds themselves as recorded (`test_navbuild_native.py --game`).
- **The offline sampler**: sp_009's 0.2 m pair and sp_004_b's 0.8 m pair, Python's terrain test against the core's:
  the same files (SHA-1 of both capsules' runs).
- Game tier, all passing: `test_navtools_native.py --game`, `test_navbuild_native.py --game` (sp_009, sp_004_b),
  `navregress` (2 levels), `test_apply.py`, `test_vehicle_level.py`, `test_navwater_mesh.py --game`, `test_links.py`,
  `test_traversal.py`, `test_collision_backends.py`, `core/tests/test_navdata.py`, `tests/test_e3o_native.py --game`.
- DbxEdit: `core/tests/test_dbxedit.py` against `DbxEdit.exe` passes (its `xml` cases included).
- Rust: `vehicle`, `domains`, `levelscan` and `measures` unit tests; `cargo clippy --all-targets -D warnings`, `cargo fmt`.

### What remains

- The measures: `hpdtool.parse`'s Python objects (now once per file), `navdata.load` twice for `ladder_ground`,
  `ordercheck.PathData`'s per-edge loop and `nearest_group` (NumPy per point), `stale_nodes`' per-node loop.
- The build (sp_004_b ~10 s): `walkable`'s NumPy glue, `navdetail` (D lines, F cells: 1-2 s; its sets' iteration order
  reaches the output, so a port must reproduce it), `kymesh.write_mesh`, `finalize_graph`.
- The boat mesh: its picture (matplotlib, about half of `level_run`; the PNG is an output), `validate`'s sight test,
  `build_graph` and `grid_path` in Python.
- The offline sampler (`navsample_offline`): the capsule test against the triangles (`_line_tri_interval`, NumPy, now
  the largest part of a run) and the collision loading.
- `prepare`: the unpacks themselves (FbrbTool in process) and `levelobjects`' per-blueprint triangles (`hkpack`).


## F0: the file formats that had no Rust (2026-11-18, branch `retire-f0`)

The ruling of 2026-11-18 (`EDITOR_ARCHITECTURE.md` 3.8: port everything, prove it, then retire the Python) reaches
the format readers and writers. This round **ports**; each port keeps its Python as the reference and the fallback
(`*_reference`, `use_native(False)`, `BFBC2_PURE_PYTHON=1`) until it passes a later full tier. Every port answers
`None` where the reference raises, a value is a NaN, or a value is not one it reads exactly as `struct` does (a float
overflowing a 32-bit float, a sequence of another length, a `memoryview`): the Python then runs, so a result is always
the reference's and every error message stays its own.

| Python (reference) | Rust | bound as |
|---|---|---|
| `hpdtool.parse` / `write` (path data) | `bc2_formats::hpd` | `hpd_parse` (builds hpdtool's dataclasses: `__new__` + `__dict__`), `hpd_write` |
| `kymesh.write_mesh` | `bc2_formats::kymesh::write` | `kymesh_write` |
| `bc2loc.LocFile` (parse, `decode`, `to_bytes`), `khash` | `bc2_formats::loc` | `loc_parse`, `loc_histogram` / `LocTable`, `loc_decode`, `loc_write`, `loc_khash` |
| `menucommon.parse_bigf`, `apt.read_bigf` | `bc2_formats::bigf` | `bigf_parse` |
| `stfs.Package` (header, file table, `read`) | `bc2_formats::stfs` | `stfs_open`, `StfsReader` |
| `xiso.find_partition` / `walk` | `bc2_formats::xiso` | `xiso_find_partition`, `xiso_walk` (a real file opened by path only) |
| `audio.read_wave`, `write_wave_pc`, `read_sps`, `read_snr`, `write_sps` | `bc2_formats::wave` | `wave_read`, `wave_write_pc`, `sps_read`, `snr_read`, `sps_write` |
| `mesh.parse` / `write`, `meshset.parse` / `write` | `bc2_formats::meshdata` | `mesh_parse`, `mesh_write`, `meshset_parse`, `meshset_write` |
| `texture.tiled` / `packed` / `layout` | `bc2_convert::texture` (the converter's own) | `texture_tiled` (ints; NumPy arrays stay Python), `texture_packed`, `texture_layout` |
| `texture.levels` / `decode` (NumPy) | `bc2_convert::texcheck` | `texture_levels`, `texture_decode` |
| `havok.classes` (the exe's hkClass initializers) | `bc2_convert::hkclasses` | `havok_classes` |

### Parity (each both ways; quick and `--game` of the tests named)

| format | corpus | result |
|---|---|---|
| path data (`test_pathdata_native.py`) | 144 `pathdata.hpd` + kept originals of the game copy and `work\` (656 MB), 8,858 runtime meshes; 60 built files, 1,932 damaged, 600 changed objects | every object identical (types, attribute order, float bits), every file and mesh written byte for byte as read; 0 cases the reference answered left to it |
| localization (`test_bc2loc_native.py`) | 25 language files of every overlay layer and the pristine copy (59,177 texts), 200 built files + histograms, 2,750 damaged, 3,010 keys | every attribute, text and written byte identical (also after `set`); all texts decoded in Rust |
| BIGF | 128 menu / HUD movies, 200 built, 2,000 damaged | the same members or the same error |
| STFS / XISO (`test_console_readers_native.py`) | the Xbox disc image (693 files), the 3 Update packages (2,533 MB, every entry read), 4 built packages, 4 built discs, 1,500 damaged packages, 300 damaged discs | identical values, entries, bytes, listing order and file positions; damaged: the same values or the same error |
| Waves (`test_wave_native.py`) | 84,397 game Waves (2,750 BF1943, 35,813 BFBC2 Xbox, 45,834 BFBC2 PC) with every segment's stream: 260,951 comparisons; 253,191 damaged; 1,840 built; 600 changed | all identical; no case the reference answered left to it (outside NaN-rate damage: 3) |
| mesh headers (`test_meshdata_native.py`) | 11,319 game mesh resources (7,276 MeshData, 2,696 Rigid, 637 Composite, 553 Skinned, 157 Tree): 23,516 comparisons; 24,103 damaged; 2,175 built; 800 changed | all identical |
| textures (`test_texture_helpers_native.py`) | 4,578 game PC textures, 30,946 levels decoded (DXT1, DXT3, DXT5, BC5, L8); 200,000 tiled offsets, 20,000 packed, 5,000 layouts; 2,000 damaged | every pixel, dtype and shape identical |
| `havok.classes` (`test_havok_classes_native.py`) | BFBC2Game.exe (297 classes, 1,044 members), 100 built executables, 400 damaged | identical dicts in order, the same errors |

Found by the parity tests on the way: an `__init__` argument shadowed in `bc2loc`'s new path (an error message named
the file's NAME field instead of the caller's name: fixed before commit); STFS entries damaged to 16.7 M chained
blocks made the first port allocate 32 GB (the reference builds every block's bytes before cutting at the size). The
port now stops at the size (the same bytes) and reads runs of adjacent blocks at once.

### Timings (same process, warm; the machine shared with other agents' builds)

| workload | Python | Rust |
|---|---:|---:|
| `hpdtool.parse`, 144 files / 656 MB | 150.9 s | **5.5 s** |
| `hpdtool.write`, the same | 3.3 s | 1.7 s |
| `kymesh.write_mesh`, 8,858 meshes (the time is reading the Python objects) | 20.6 s | 11.7 s |
| `LocFile` read + every text decoded + `to_bytes`, 25 files | 0.22 s | 0.05 s |
| STFS: one 16 MB entry of an Update package (I/O bound) | 0.038 s | 0.024 s (the first port read block by block through a buffered reader: the whole packages took 50.8 s against Python's 6.2 s; now runs of adjacent blocks in one read) |
| XISO: the disc's 693 files (warm) | 2-6 ms | 1.2 ms |
| `read_wave` per call (small Waves) | 2.7 us | 1.0-1.3 us; `write_wave_pc` 1.45 -> 1.24 us. The `--game` loop over 84,397 Waves measured 0.79 s Python / 1.06 s Rust: cases the reference raises on run both (a refusal then the Python) |
| `mesh` / `meshset` parse, 11,319 resources | 0.28 s | 0.19 s |
| `texture.decode`, 30,946 levels | 126.8 s | **31.8 s** (most of it the comparison arrays' copies) |
| `havok.classes(BFBC2Game.exe)` | 0.06 s | 0.02 s |

### Not ported yet (what remains of the Python format code)

- ~~`bfbc2-anim-tools/granny.py`'s writing side~~, ~~`bfbc2-console-convert`'s `terrain.py`, `soot.py`,
  `mesh.relayout` / `decode` / `console_layouts`, `havok.convert_wrapper`, `texture.check`~~ and
  ~~`bfbc2-model-tools`' `skinmesh.py`, `texwrite.py`~~: round F2a (below; `granny.curve_keys` and the decimator not yet).
- `bfbc2-bf1943-import`: `mopp.py`, `havokport.py`, `meshport.py`'s writers, `emitteratlas.py`, `dbxcatalogue.py`.
- `bfbc2-menu-tools/menucommon.PEImage` (E4f's `bc2-harness::pe` covers the exe for the crash reader / exeref; not
  yet switched here), `core/bfbc2core/xmlinst.py`, `navtune.py` (JSON side files).


## G1: the map editor's shell and read-only map view (2026-11-18, branch `rust-g1`)

`rust/bc2-editor` (binary `bfbc2-editor`; README.md, "The map editor"). No Python is replaced (the Map Inspector
stays), so there is no before to compare with; recorded for later rounds: a whole headless run of the editor (start,
load the level with geometry through `bc2_scene`, upload it, lay out and draw 30 frames of the window at 1600 x 950,
save the PNG), **debug build**, the mesh-alias cache warm, on the development PC (GeForce RTX 4060 Laptop, Vulkan):

| level | entities | run | peak working set |
|---|---:|---:|---:|
| SP_01 | 13,251 | 4.4 s | 280 MB |
| MP_005 | 6,979 | 3.1 s | 273 MB |
| MP_009GR | 10,005 | 2.7 s | 242 MB |
| ONS_MP_004 | 7,100 | 3.1 s | 273 MB |

MP_005: 3,919,352 triangles drawn; its load alone (the status bar's "loaded in") 0.4 s. Collision is uploaded once
per blueprint and drawn instanced (one instance per placed entity, as `bc2_scene` keeps it), so moving an object
later changes one instance. The editor's tests (`tests/test_editor.py`): 14 unit tests, the outliner against the Map
Inspector on 4 levels, 4 offscreen render / click tests and the 4 levels drawn with their selection outlined, about
a minute in debug.

## F2b: the BF1943 importer's remaining Python, the menu tools' PE reader, xmlinst and navtune (2026-11-18, branch `rust-f2b`)

**Provisional timings:** two WW2 rebuilds ran beside parts of the work (every process under 1 GB then,
`RAYON_NUM_THREADS=2`); the rest at most 2 GB, 4 threads, below-normal priority on the 16-thread PC. Ratios hold
within a run.

Each port keeps its Python as the reference and the fallback; a Rust function answers `(result,)`, refuses (`None`:
the Python runs) where the reference would raise another exception or decide by a rule not reproduced, and raises the
reference's own error with its message. The importer's switch is `bfbc2-bf1943-import/importcore.py`
(`BFBC2_IMPORTER_PYTHON=1`); the core modules and the menu tools use the module unless `BFBC2_PURE_PYTHON=1`.

### Inventory and what moved

| Python (lines) | Rust | what the profile said |
|---|---|---|
| `mopp.py` (201) | `bc2-convert/src/mopp.rs`: `query_int`, `query`, `complete`, `ray_cast`, `ray_cast_brute` | `test_havokport`'s MOPP checks: `query_int` 4.4 s of 7.6 s (profiled) |
| `havokport.py` (755; E3g ported the packfile walk) | `havokport.rs`: `restructure`, the material table's swap, `shape_subparts_to_550` / `aabb_w`, `ufloat8_table`, `_wrapper`, `part_materials`, `part_run`, `with_part_materials`, `havok_materials`, `with_havok_materials`, `bodies`, `with_face_materials` | 0.5 + 0.6 s over BF1943's 202 files (E3g) |
| `meshport.py` (370) | `meshport.rs`: `Porter.pc_layout`'s search (`MeshLayouts`); the MeshData and mesh set writers it calls are F0's | 0.6 s of an 8 s bulk port; the rest is reading BF1943's bundles |
| `emitteratlas.py` (819) | `emitteratlas.rs`: `build` (read / write of the atlases, infos, cells, merge, dedupe, pack, compose, the re-pointed partitions), `prove`, `same_but_uv` | Wake: build 6.1 s, prove 6.8 s |
| `dbxcatalogue.py` (302) | `dbxcatalogue.rs`: `build`'s pass and `exe_identifiers` | 173 s: `dbx_parse` into Python trees 74 s, bundle reads 34 s, the walk 46 s |
| `menucommon.PEImage` (87) | `bc2-harness` `pe::raw`: the header, `strings`, `find_cstring`, `find_dword` | `strings` 0.2 s per run of bc2loc / aptrefs |
| `bfbc2core/xmlinst.py` (35) | `bc2-formats/src/xmlinst.rs` | NavKit's six scans over OPFOR's 791 level XML files: 40 s |
| `bfbc2core/navtune.py` (226) | `bc2-navbuild/src/navtune.rs`: `validate`, the side file's text, `summary` | milliseconds (side files are small); ported for the ruling |

Kept Python, not ported: `navtune.load`'s `json.load` (the standard library's C parser; its checks are `validate`'s),
`emitteratlas`'s dataclasses (built from the Rust result), meshport's `port` orchestration (calls into F0's writers,
texture and Granny conversion).

How NumPy's floats are matched where the reference decides on them (`emitteratlas.rs`, `havokport.rs`, `mopp.rs`):

- `aabb_w`: float32 arithmetic in the same order; `np.dot` / `.sum()` of the three lanes checked on 200,000 random
  vectors to be left-to-right float32;
- the probe search: exact integer variances; full 8 x 8 windows are exact in NumPy too; for a cut window two
  different windows' variances equal or within 1e-9 are refused;
- `np.corrcoef` on integer data: the exact rational correlation, refused within 1e-9 of `SAME`, `PASS` or a 4-place
  rounding boundary;
- Python's `sum` of floats (3.12's compensated sum) in the MOPP ray test, its `round`, `min` / `max`, floor division
  and slicing; a ray over NumPy scalars (summed plainly by Python) is refused.

### Parity

| what | inputs | identical | left to the Python |
|---|---|---:|---:|
| `test_havokport_native.py` quick | 2,000 MOPP codes x 5 queries, 1,500 4.6.1 packfiles, 1,500 PC resources x 11 functions | 28,675 of 28,675 | 1,334 (damaged inputs) |
| ... `--game` | 202 HavokPhysicsData x 2 templates x table / none (restructure and convert), 92 finished, 1,290 BFBC2 physics resources x 9 functions, 944 MOPP codes (9,440 boxes, 944 completeness checks, 9,440 rays) | 61,518 of 61,518 | 2,570 (11 restructures: velocity limits without a table; 962 rays over NumPy scalars on purpose; the rest seeded) |
| `test_dbxcatalogue_native.py --game` / `cmp` | the game copy: 84,739 partitions | every table in order: 631 classes, 9,657 paths, 1,615 text paths, 3,443 number paths, 432 prototypes, 607,100 guids, 84,739 primaries, 631 defaults, 505 seconds, 17,218 exe names; shared elements as the reference's | 0 |
| ... quick | 6 seeded games of built bundles (chunks past 400 names, PER_TYPE, TEXTS_CAP, deleted markers, problems) | 6 of 6 | 0 |
| `test_emitteratlas_native.py --game` | OPFOR's four emitter_atlas runs' inputs: Wake (615 emitters, 1,603 uses), Coral Sea (500, 1,298), Guadalcanal (588, 1,524), Iwo Jima (601, 1,576) | atlases, 2,304 partitions, 6,001 uses, cells, reports, 6,001 proof rows, 2,304 tree comparisons | 0 |
| ... quick | 40 seeded synthetic levels | 40 of 40 (build, prove, same_but_uv) | 0 |
| `bench_bf1943.py --compare` (source, import, fair, parity, atlas on Wake) | `BFBC2_IMPORTER_PYTHON=1` recorded, then the core | 17,839 of 17,839 | |
| `bench_ports.py mesh havok --compare` | 273 mesh sets, 202 physics files | 475 of 475 | |
| `test_ports_native.py` | 3,000 seeded layout searches against `pc_layout_reference` | 3,000 of 3,000 | |
| `test_pe_native.py` | 400 built executables; the game copy's 11 .exe / .dll (header, strings 1 - 4, 200 C strings, 200 dwords each) | 19,966 of 19,966 | |
| `test_xmlinst_native.py` | 3,000 seeded texts; OPFOR's 791 converted level XML files | 1,352,550 instances and 3,030,990 references | (the 3 non-ASCII files' word decisions: none needed) |
| `test_navtune_native.py` | 40,000 seeded checks, 2,000 side files, 5,997 summaries (no real side file exists yet) | all | |

The game tests that use these modules pass unchanged: `test_havokport.py` (23 s), `test_meshport.py`,
`test_ports_native.py --game`, `test_bf1943import.py --game`, `test_dbxport_fair.py`, `test_menu_tools.py --game`.

### Timings

| workload | Python, s | Rust core, s | |
|---|---:|---:|---:|
| `dbxcatalogue.build`, the game copy | 173 | **24** | 7x |
| emitter atlas, Wake: build / prove / same_but_uv | 6.1 / 6.8 / 0.6 | **1.2 / 3.0 / 0.2** | 3x |
| emitter atlas build, the WW2 levels (Coral Sea / Guadalcanal / Iwo Jima) | 6.7 / 6.2 / 7.6 | **1.6 / 1.5 / 1.7** | 4x |
| NavKit's six `xmlinst` scans, 791 level XML files | 40.3 | **1.8** | 22x |
| `bench_ports.py havok` (202 files) | 1.7 | **1.2** | |
| `PEImage.strings(2)`, BFBC2Game.exe | 0.21 | **0.17** | |
| `bench_ports.py mesh` (273 mesh sets) | 7.4 | 7.7 (noise: reading BF1943's bundles) | |

Peak working set: the atlas bench 1.43 GB (1.69 GB with the Python).

### Tests

Quick: `test_havokport_native.py`, `test_dbxcatalogue_native.py`, `test_emitteratlas_native.py`,
`bfbc2-menu-tools/tests/test_pe_native.py`, `core/tests/test_xmlinst_native.py`, `core/tests/test_navtune_native.py`;
game: the first five with `--game`. Unit tests in `mopp.rs`, `havokport.rs`, `pyfmt.rs`, `dbxcatalogue.rs`,
`emitteratlas.rs`, `meshport.rs`, `pe.rs` (`raw`), `xmlinst.rs`, `navtune.rs`. `rust/bench_bf1943.py capture` takes a
manifest folder (the WW2 levels' runs).


## G2: the user's decisions on G1 (2026-11-18, branch `rust-g2`)

`EDITOR_ARCHITECTURE.md` 3.10, "Decisions on G1's first version". No Python replaced; what was measured and proved:

**Preparing a level from the editor.** Opening a level NavKit has not prepared runs `rust/bc2-editor/prepare_level.py`
(NavKit's `prepare` and `bfbc2core.level.heightfield_of`, the Map Inspector's terrain unpack) as a child process; the
editor shows "Preparing MP_005 - Step n of 4" with a bar, the time and the tools' last line, then opens the map.
MP_005 on a **fresh stand-in OPFOR folder** in temp (the game copy linked read only by a junction, its own `work\`;
`tests/g2.rs` with `BC2_EDITOR_PREPARE=mp_005`), debug build, four other agents beside it:

| step | from | took |
|---|---:|---:|
| 1. the shared bundles (sp_common, mp_common: unpacked and converted, once per OPFOR folder) | 1.3 s | 143.6 s |
| 2. the level's bundles | 144.9 s | 9.1 s |
| 3. objects, collision and spawn points (`levelobjects`, ladders, the sampling box) | 154.0 s | 59.8 s |
| 4. the terrain unpack | 213.8 s | ~6 s |
| loaded and shown (6,979 entities, terrain, no notes) | | **225.1 s in all** |

A second level on the same folder skips step 1's work (E4e: sp_009 after sp_004_b, 9.6 s). The junction was removed
before the stand-in folder; nothing was written to the game copy or the real OPFOR `work\`.

**Faction labels** (`factions.rs`, `tests/factions.rs`, read only): MP_005 US / RU, MP_012GR and ONS_MP_005GR RU / US
(the Russians are team 1 there), MP_004SDM and SP_01 T1 / T2 (`FactionNeutral`: said in Properties),
NAM_MP_002CQ and NAM_MP_005R US / NVA (Vietnam's team 2 kits are DICE's `vietnam/vc_*` art), Wake Island, Coral Sea,
Guadalcanal and Iwo Jima US / JP - 11 of 11; the Package-only levels' partitions are read from their
`level-00.fbrb` in memory (`bc2_formats::fbrb::read`; the whole test, 11 levels, 7.2 s).

**The 3D view's tokens** (`design/project/README.md`, Colour): 22 new colour tokens (`unowned`, 9 `view-*`, 12
`scene-*`; 50 in all), each reaching the style, the dock or the palette in both themes. On rendered frames
(`tests/g2.rs`, both themes): every `view-*` and `scene-*` colour found where it is drawn; overlay markers against the
terrain around them, median to median / darkest tenth to brightest tenth: team-1 4.00 / 3.95, team-2 4.01 / 3.95,
team-neutral 5.63 / 5.54, unowned 4.02 / 3.97 (Dark); 3.97 / 3.92, 3.99 / 3.92, 5.69 / 5.52, 4.10 / 3.99 (Light).
Overlays are lit evenly (0.88 - 1.0) by the instance colour's alpha in `view.wgsl`; no cost measurable.

**Help** is generated from the action registry (13 actions, Help on F1 new), the views' 9 controls and the 4 workspaces; a
test fails if an action lacks a description. Headless screenshots of this round: MP_005 (Dark, Light),
NAM_MP_002CQ and Iwo Jima (Dark, Light; stand-in OPFOR folders in temp with their Package bundles unpacked), the
Help window, the preparing view and a failed preparation.

## E2: editing an existing map, first part (2026-11-18, branch `rust-e2edit`)

New crate `rust/bc2-levelwrite` (the map project and levelwrite) and the editor's Objects workspace (README.md, "The
map editor"). Like G1, nothing in Python is replaced: `author.py` / `dbxbin.py` (the stock-MP AI layer) and DbxEdit
stay; levelwrite is the editor's one mutation API and is proven against them.

### Proof: levelwrite on real levels (`cargo test -p bc2-levelwrite --test real_levels`)

Each base copied from OPFOR's work folder to a temp folder (OPFOR's files only read); a project log of 9 operations
(a move, a rotate, a duplicate without and one with event connections, a delete without and one with, a text, a
whole-number, a decimal and a bool field set) saved, loaded and applied:

| base | instances | apply (debug) | changes in the partition (field-level) | expected exactly |
|---|---:|---:|---:|---|
| MP_005 | 4,683 | 0.13 s | 12 | yes |
| SP_01 | 9,616 | 0.64 s | 71 | yes |
| Guadalcanal (WW2, `work\ww2`) | 3,292 | 0.12 s | 12 | yes |

"Expected": the moved and turned objects' `Transform`, the copies added, `LevelData.Objects` and
`EventConnections`, the deleted instances removed and every field elsewhere that referred to them (SP_01's deleted
gameplay entity is named by many), the four fields set - and nothing else (`diff::diff`, instance by instance,
values compared as bytes, never through XML). Each edited partition: parses (the DBX reader and `EditDbx`);
`EditDbx` writes it back byte for byte; `dbx::write` gives the same XML and is a fixed point; `bc2_scene` loads it
with each moved, turned and copied entity at the log's transform and the deleted ones gone. An untouched partition
writes back byte for byte.

References, byte for byte on all three levels:

| edit | reference | result |
|---|---|---|
| delete (no event connections) | DbxEdit `drop=<guid>` (`dbxedit::editor::drop`) | identical |
| delete (with event connections) | DbxEdit `drop` | identical but for `EventConnections`: levelwrite removes the deleted object's connections, DbxEdit leaves them pointing at `null` (by design) |
| set text, whole number, decimal | DbxEdit `setfield=` | identical |
| move, rotate, duplicate, set text / number / bool | the Python `dbxbin` reference (`tests/dbxbin_reference.py`: `set_transform`; `clone`, `set_attr('guid')`, keep `guid` / `type`, an `Objects` item and the connections cloned and `set_ref`, as `author.py` does) | identical |

No reference exists for a delete in `dbxbin` / `author.py` (DbxEdit is that reference) or for a duplicate in DbxEdit
(its `graft` copies from another file without a new guid).

Field types come from the level partition's own data by `dbxcatalogue`'s rules (`schema.rs`: shapes, `looks_whole`,
`is_whole`, texts); a 4-byte number the data does not say is whole or decimal is refused unless 0 (`author.py`'s rule;
DbxEdit's import guesses a decimal there). The project's tests: the JSON shape, save / load, undo, redo, redo after
undo, a new operation clearing the redo stack, history "go to", autosave and recovery, the base-changed report
(operations naming objects a changed base lacks: listed, not dropped), another format or a newer version refused.

### Proof: surface placement and the interface (`cargo test -p bc2-editor`)

- `query.rs` (unit): rays and the sink on the demo's terrain (a known height formula) and on a mesh (the house's
  roof): placed at sink 0, 0.5 m and 25 % of the object's height, straight down and along the normal, turned to
  the normal; vertex and edge snapping. `tests/real_level_editing.rs`: the same three sinks at 60 triangle centroids
  of MP_005's terrain (the object's lowest point the sink below the sample's plane).
- `gizmo.rs` (unit): handles found under the pointer, an arm drag of 7.3 m (7 m on a 1 m grid), a plane drag, a ring
  turn of a quarter (80 degrees on 40-degree steps), local axes.
- `edit.rs` (unit): the log drives the scene (move, duplicate with collision, delete, undo, redo, "go to", save,
  reopen).
- `tests/editing.rs` (the demo, headless, as a user): click to select; G; a drag of the X arm by about 5 m gives one
  `move` of exactly 5 m; Ctrl+Z and Ctrl+Y; E and a ring drag gives one `rotate` of a multiple of 15 degrees; Q and a
  box over the view selects the 5 objects; Ctrl+D one `duplicate` of 5 with the copies selected; Delete one `delete`
  of them; End with surface snapping sunk 0.5 m puts the house 0.5 m into the terrain, and a later surface-snapped move
  keeps 0.5 m; a layer; Ctrl+S removes the autosave; the reopened project gives the same scene. 8 operations.
- `tests/real_level_editing.rs`: an MP_005 project, a placed object dragged 10 m on a 10 m grid and put on the ground
  sunk 25 % of its height: the live partition holds the scene's transform, differs from the base only in that
  `Transform`, and "Check edits" replays the log to the same bytes.

### What remains of E2

The viewport modes (lit with the game's shaders, unlit, wireframe, collision, navmesh, cull tree), polygon
selection, editing static group members (they live in the `*_HavokAsset` group files), deep duplicates (sub-instances
an object owns, such as a flag's capture area, are shared by the copy today), several objects' fields edited at once
("Mixed"), and levelwrite's proof run over every prepared level rather than three. *All but static group members
(still read-only, queued) and the game's own shaders in the editor view (route A of R5 draws them in the Game-accurate
view) were done in round E2b, below.*

## E2b: context menus, edit modes, sink by mouse, the asset browser (2026-11-19, branch `rust-e2b`)

`EDITOR_ARCHITECTURE.md` 3.10, "Done in round E2b". No Python replaced.

**levelwrite's new operations** (`rust/bc2-levelwrite`): `place` (an added instance: a clone of a template - a plain
placement of the same blueprint, else any placed object - with a new UUID v5 guid, the blueprint in
`ReferencedObject` or another reference field (a vehicle spawner's `Vehicle`), the fields most of the level's
placements of it share, an `Objects` item, no event connections; with `owned`: the template's sub-instances copied
with it, only the connections among them), a deep duplicate (`duplicate` with `owned`: `LevelWriter::owned` is the
greatest set reachable from the object whose every referrer is inside it, `LevelData.Objects` or an event connection;
the copies' references among themselves remapped; every connection touching the set copied once, both ends
remapped), `hide` and `lock` (per object, the editor's organisation) and `MapProject::amend_last` (a gesture going
on: one operation). Unit test on a made-up owning level: the ring, the marker and a shared instance; replay through
`apply` gives the same bytes.

**Every prepared level** (`cargo test -p bc2-editor --test every_level`; OPFOR's `work\levels`: 37, and every level G3
lists from `game\Package`, read from its `level-00.fbrb` in memory: 45 - Vietnam, Pacific War, BFBC1, extra layouts;
nothing written): a scripted log of a move, a rotate, a duplicate, a deep duplicate of a flag (else the first object
of the first 400 that owns something), the same owner placed as a gameplay tile, two placements (the most and the
least placed blueprint), a delete, a decimal field, a hide and a lock. **82 of 82 levels**: the field-level diff is
exactly the expected set (the copies and placements added, the deleted removed, the moved, turned and set fields,
`Objects`, every `EventConnections` naming a copied or deleted object, every field that named the deleted one); the
result parses, writes back byte for byte and reads as a tree; the log replayed (also after a JSON round trip) gives
the same bytes; no deep copy names an original it copied. Deep copies of 0 (SP levels without an owner in the first
400 instances) to 33 sub-objects (ONS_MP_004). Debug build, 438 s in all (the SP levels' ownership search the most).

**The interface** (`cargo test -p bc2-editor --test e2b`, headless on the demo level with a real partition written by
the suite's DBX writer): the context menus' entries per workspace (Objects, Gameplay, AI, View; the Terrain and
Vegetation ground menus' brushes greyed with "Coming in the terrain round"); disabled entries with their reasons; a
right-click selects and opens the object menu, an entry clicked runs (Hide: one operation); the ground menu; "Place
object here..." picker (Enter takes the first match), Shift+wheel turns 15 degrees a notch, Alt+wheel sinks 0.1 m a
notch, a click places (one `place`, the crate's plain placement its template); a content tile dragged into the view
places the house; Objectives: the flag tile dragged in with team 2 gives one `place` with its 5 owned sub-instances,
the copy's `MapMarker` names its own marker, the outliner lists it, the Gameplay workspace selects it and its menu
offers Set team; Spawners: a spawn point with team 1; Alt+wheel over a selected crate: three notches one operation,
sink 0.30 m; after a pause Alt+Shift: 0.01 m a notch, a second operation; one undo takes back a whole burst; Esc closes
the popup; Ctrl+Shift+D on the flag copies it with its ring and marker (references remapped); the six view modes each
draw differently (the navmesh mode says when a level has no path data; the cull tree reads the level's slots); a
polygon of four clicks and Enter selects the two crates; two crates' Health shows Mixed, "40" sets both (one
operation); Set team 1 from the Gameplay workspace; "Check edits" replays the whole log to the same bytes; saved and
reopened, the same log.

**The asset browser on the game copy** (`cargo test -p bc2-editor --test asset_browser`, read only): MP_005 lists 675
placeable objects of BFBC2 (115 its own placements, the rest from its folders and 156 from the set's other bundles'
headers - the whole set listed in 5.2 s, debug); a blueprint of another level's bundle (`objects/buildings/airtower`,
`sp_007`) is resolved when placed in 0.07 s. Spawners on MP_005: player spawn points (41), the HQ spawn (with 6 owned),
AI spawners, a vehicle spawner per vehicle (ah64, mi28, cobra, humvee, m1a2, quadbike, t90 ...); Objectives: capture
points (with 13 owned), the combat area (44 owned area points), ticket counters. MP_005SR (Rush) adds M-COMs, arm /
disarm awards, life counters and mission objectives, and offers the set's other vehicles (a10, bmd3, f18 ...) as
spawner tiles from the catalogue.
## F2a: the console converter's rest, the model tools, Granny's writing side (2026-11-18, branch `rust-f2a`)

Round F2a ports the Python that remained of `bfbc2-console-convert`, `bfbc2-model-tools` and Granny's writer
(`EDITOR_ARCHITECTURE.md` 3.8: port everything, prove it, then retire the Python). Like F0 it **ports**: each Python
function stays as its reference and fallback (`*_reference`, the module's `use_native(False)`,
`BFBC2_PURE_PYTHON=1`) until a later full tier. The bindings (`rust/bc2-py/src/f2a.rs`) answer `None` where the
reference must run (it raises something other than its module's own error - a `struct.error`, a NumPy buffer too
small - or meets a value the port does not mirror), `(False, message)` where it raises its own (`TerrainError`,
`SootError`, `ConvertError`: the same message, a part name's `repr` included), and the value otherwise.

| Python (reference) | Rust (`bc2_convert`) | bound as |
|---|---|---|
| `terrain.heightfield` (main and sub-terrain blocks, both sources), `water_mesh`, `material_map`, `visual_terrain`, `visual_water`; `soot.soot_mesh` | `terrain` | `terrain_heightfield`, `terrain_convert(kind, x)` |
| `terrain.visual_terrain_parts`, `road_strips`, `visual_water_parts`, `material_names` (the PC readers) | `terrain` | `terrain_visual_terrain_parts`, `terrain_road_strips`, `terrain_visual_water_parts`, `terrain_material_names` |
| `havok.convert_wrapper` | `havok::convert_wrapper` | `havok_convert_wrapper` |
| `mesh.console_layouts` | `meshconv::console_layouts` (the reference's lazy pattern on the `regex` crate's leftmost-first matcher) | `mesh_console_layouts` |
| `mesh.decode` | `meshconv::decode` | `mesh_decode` (arrays as dtype, shape, bytes) |
| `mesh.relayout` (with an `edit`: the blocks and their columns from the core, the callback and the rows in NumPy) | `meshconv::relayout`, `relayout_blocks` | `mesh_relayout`, `mesh_relayout_blocks` |
| `texture.check` (the mip-chain error and the block counts; the correlation stays `np.corrcoef`) | `texcheck::check` | `texture_check` |
| `texwrite` `levels` / `_down`, `dxt1`, `dxt5`, `_color_blocks`, `_alpha_blocks`, `alpha1`, `alpha5`, `l8_down` (the axes stay `numpy.linalg.eigh`'s) | `texwrite` (blocks on all cores) | `texwrite_levels`, `TexWriteJob`, `texwrite_alpha`, `texwrite_l8_down` |
| `SkinnedLod.__init__`'s parts, `decode`, `_winding`, `_encode`, `to_bytes`, `bounds` | `skinmesh` | `skin_cut`, `skin_decode`, `skin_winding`, `skin_encode`, `skin_lay_out`, `skin_bounds` |
| `Granny.to_bytes` (the CRC), `add_tracks` / `_Graft`, `shift_position`, `set_orientations` | `granny_edit` | `granny_with_crc`, `granny_add_tracks`, `granny_shift_plan`, `granny_orientation_plan` (the values; `Granny.put` writes them) |

### The bits NumPy, LAPACK and CPython set

- **Halves**: NumPy casts a half's NaN to a float or double keeping its payload (`npy_halfbits_to_floatbits`, no
  quieting) and a float's NaN back the same way; the first port went through Rust's `f32 -> f64` cast (the processor
  quiets a signalling NaN), and the tests' random vertices caught it (`mesh::f16_to_f32`, `f16_to_f64`, `f32_to_f16`).
- **NumPy's sums**: a block's mean and covariance are summed texel by texel from the first (a reduction over a middle
  axis); `einsum('nki,ni->nk', d, axis)` adds `(x*a + y*b) + z*c` with the axis as `eigh` returns it (a strided view;
  with a contiguous axis NumPy adds `(x*a + z*c) + y*b` - measured, so the port follows the view); three channels'
  squared errors `(r + g) + b`; `texture.check`'s mean of absolute differences sums quarters exactly, one division.
- **CPython's `sum`** of floats (3.12+) is Neumaier's compensated sum: Granny's ramp weights compare a key's
  projection with the first and last keys', which lay 1e-16 apart on one cutscene curve (`sp08_ab3_jumpoverwagon`),
  so a plain sum re-quantized it differently; `granny_edit::py_sum` is `builtin_sum_impl`'s loop.
- **The eigensolver** stays NumPy's. A model of `dsyevd` for 3 x 3 (`dsytd2`, `dsteqr`, `dormtr` through OpenBLAS
  0.3.30's small kernels, the reference LAPACK 3.12 sources) matched the tridiagonal form only with OpenBLAS's `ddot`
  fused multiply-add, matched `dsteqr` exactly, and did not match `dormtr`'s last bits in any combination of fused /
  unfused updates: those depend on the kernels OpenBLAS picks for the processor (here Zen 4: the SkylakeX set). The
  core gives each block's covariance (exact sums of integer texels' products), NumPy solves them all at once (0.06 s
  for a 1024 x 1024 picture's 87,381 blocks), the core encodes. **`np.corrcoef`** (`texture.check`'s correlation) sums
  with BLAS too: kept in NumPy on the core's decoded levels.

### Parity (both ways; quick and `--game`)

| area (test) | inputs | result |
|---|---|---|
| terrain, soot, the physics wrapper (`bfbc2-console-convert/tests/test_console_f2a_native.py`) | built: 750 heightfields (main and sub-terrain, both sources), 1,200 water meshes, material maps, visual terrains and visual waters, 300 PC visual terrains, 300 soot meshes, 300 wrappers; game: every BF1943 and BFBC2 Xbox (OPFOR `work\xbox`) resource - 935 HavokPhysicsData wrappers, 436 SootMesh, 134 TerrainHeightfield, 14 each of the material maps, visual terrains, visual waters and water meshes - and the game copy's 34 PC material maps, visual terrains and visual waters through the PC readers; damaged: 3,179 built, 2,124 game | all identical (3,900 built and 1,887 game comparisons answered in Rust, none left to the reference); damaged: the same result or error, 22 left to the reference (a NaN float in a wrapper, reads past the end), 1 of them answered by it |
| `console_layouts`, `decode`, `relayout` | built: 200 shader databases, 600 meshes of 1 - 3 parts, Xbox meshes with the packed formats, relayouts plain and edited; game: BF1943's 7 console shader databases and 1,580 mesh parts decoded with their layouts, the game copy's 4,576 MeshData decoded part by part and 4,121 relaid into their own layouts, plain and with an edit (21,358 comparisons) | all identical, every real input answered in Rust |
| `texture.check` | 1,200 built (DXT1, normal, DXT5, L8, RGBA16F; with references, damaged); every PC texture of the game copy (4,578) alone and against itself | all identical (worst, correlation, blocks; floats by their bits), all answered in Rust |
| `texwrite` (`bfbc2-model-tools/tests/test_model_f2a_native.py`) | 4,080 built pictures (smooth, noise, flat, grey, two- and three-colour; 1 x 1 to 256 x 256; every kind with and without mips, 1-bit alpha; odd sizes and 3 / 4 / 5 channels for `dxt1` / `dxt5`; the alpha decoders, `l8_down`); 2,693 DXT1 / normal / DXT5 textures of the game copy decoded and encoded again | all identical, all answered in Rust; damaged and refused: the same result or error (57 left to the reference and answered by it: `l8_down` of an odd height, where NumPy's broadcast makes an empty or one-row level, and a float picture) |
| `skinmesh` | 300 built skinned LODs (DICE's first- and third-person layouts, depth-only parts sharing windows) read, decoded (every usage), bounded, written, then `add_to`, `replace`, `add_part` with lathes and written again; the game copy's 618 skinned LODs the same | all identical, all answered in Rust; damaged: 3 left to the reference (a NaN position's bounds), the same results |
| Granny writing (`bfbc2-anim-tools/tests/test_granny_f2a_native.py`) | 300 built CRCs; the WW2 rifles' 57 animations and, `--game`, every GrannyAnimation of the game copy (4,889): every movable position curve shifted (random vectors, with and without ramp, and to a rest), every quantized orientation curve rewritten, `to_bytes`, four tracks added from the Garand's undeploy (copies, a constant, an identity); the refusals (curves `shift_position` does not move, clashing specs); damaged copies (cut short in the game tier) | all identical (each edit's report and the resource's SHA-1 after it); every real input answered in Rust (4,889 edit sequences, 4,889 `add_tracks`); damaged: 9,778 identical (4,615 left to the reference, none of them answered by it); the refusals the same errors |

Found on the way (fixed before commit): `relayout`'s half columns were packed as if they held halves (they hold the
float32 values the reference keeps), so the core answered `None` for every real mesh with a half element and the
reference answered - the first tests passed on outcomes alone; they now count what the core answers
(`tests/f2a_parity.py`'s `watch` / `watched`). The curve binding refused a float format's `KnotsControls`
(D3I1K32fC32f: 94 curves left to the reference). The NaN payloads and CPython's sum above. And the Granny test kept a
copy of the resource after every edit and built `str()` of whole outcomes for its labels (27 GB of commit on the
cutscenes): a SHA-1 now, labels only for a mismatch, the game copy streamed file by file (its peak 850 MB).

### Timings (same process, warm, best of 3; `rust/bench_f2a.py`; other agents' builds beside it)

| workload | Python | Rust | |
|---|---:|---:|---:|
| `wake_island_s`: 4 terrain resources converted (heightfield 2048, material map, visual terrain, visual water) | 0.018 s | 0.011 s | 1.6x |
| `coral_sea`: 3 terrain resources converted | 0.014 s | 0.009 s | 1.5x |
| `console_layouts` of `levels/wake_island_s/shaders/single.res` (3 MB) | 0.201 s | 0.018 s | 11.4x |
| `texwrite.texture` 1024 x 1024 colour (3 channels, mips) | 0.404 s | 0.105 s | 3.8x |
| the same, normal map | 0.485 s | 0.101 s | 4.8x |
| the same, DXT5 (4 channels) | 0.778 s | 0.136 s | 5.7x |
| the same, colour with 1-bit alpha (4 channels) | 1.154 s | 0.126 s | 9.2x |
| `SkinnedLod`: an M1 Garand LOD read, every usage decoded, written | 0.004 s | 0.005 s | 0.8x (the crossings outweigh a few hundred vertices' work) |
| Granny: every track of the M1911's 3p undeploy shifted (ramp), `to_bytes` | 0.007 s | 0.003 s | 2.2x |
| `granny.add_tracks`: four tracks onto it | 0.033 s | 0.006 s | 5.7x |

From the `--game` tests (whole corpora, both ways in one process):

| workload | Python | Rust |
|---|---:|---:|
| `texwrite.texture` of 2,693 game textures decoded | 692.8 s | **170.5 s** (371 s before the blocks went on all cores; NumPy's `eigh` is about a third of what remains) |
| `texture.check` with a reference, 4,578 game textures | 291.8 s | 202.5 s (decoding, the Rust core's in both, dominates) |
| `mesh.decode`, BF1943's 1,580 console mesh parts | 9.83 s | **0.27 s** |
| `console_layouts`, BF1943's 7 console shader databases | 0.71 s | 0.08 s |
| `mesh.relayout`, 4,121 game meshes into their own layouts | 3.33 s | 2.08 s |
| terrain, soot and wrapper conversions, 1,577 console resources | 0.31 s | 0.09 s |
| `SkinnedLod` read, written, bounded, 618 game LODs | 0.69 s | 0.63 s |
| Granny `add_tracks` on 4,889 animations (with the reads) | 151.2 s | 25.8 s |
| Granny edits on 4,889 animations (with the reads; `Granny.put` in Python both ways) | 223.6 s | 175.9 s |

The profile behind the order: texture writing is this round's one workload that costs a WW2 build real time (OPFOR's
grime, minimap, loading and award pictures, the WW2 weapons' textures); the console decode and `console_layouts` the
next (the BF1943 importer's mesh ports, Wake's translation); terrain, soot, wrappers, Granny edits and the skinned LODs
are milliseconds per resource either way.

### What remains of this round's Python

- Kept as the reference and fallback until a full tier: every `*_reference` above.
- Not ported (glue, or not in this round): `texwrite.resource`, `wrapped`, `l8_bytes`, `atlas_levels` (header
  packing); `terrain.road_positions`, `quadtree_nodes`; `mesh.element`, `pair_by_kind`, `strip_to_list`,
  `texcoord_element` (Rust twins exist; Python callers still use them); `shapes.py` (`Geo.tangents`, which `_encode`
  calls) and `decimate.py`; `granny.curve_keys` / `sample` (the reader's side); `Granny.put` / `field` / `array_at`.
- Stays NumPy by design: `numpy.linalg.eigh` (texwrite's axes), `np.corrcoef` (`texture.check`), a relayout `edit`
  callback's arrays and the rows packed from them.

## R5: game-accurate rendering, phase 1 (2026-11-18, branch `rust-r5`)

`EDITOR_ARCHITECTURE.md` 3.11 (the study, the route chosen, the plan). New crate `rust/bc2-render`: the render harness
(`bfbc2-shader-tools/renderqa.py` + `native/shaderview.cpp`) in Rust - DICE's compiled Direct3D 11 shaders from the
level's `Dx11_Single.res`, their constants, textures, sampler states and render states, the weather's light, the
harness's tonemap - and the editor's **Game-accurate** switch (`bc2-editor/src/gameview.rs`). Nothing Python is
replaced; renderqa stays.

### Parity with the C++ harness

| what | inputs | result |
|---|---|---|
| `examples/harness_views.rs` against `renderqa.py render --together --size 512` | MP_005's `IND_Warehouse` + its material-mask mesh, renderqa's three views, `weather/r_mp005` (Default), the fixed sun | **identical**: 786,432 of 786,432 pixels in the three views (max difference 0) |
| `tests/warehouse.rs` | the same, the three-quarter view | 8 draws (7 + 1, renderqa's parts), 73,711 pixels covered, mean colour (63.76, 57.21, 48.77) - the harness picture's own numbers |

### Costs per frame (development PC: GeForce RTX 4060 Laptop, Direct3D 11 hardware device)

`examples/level_view.rs`, MP_005 at the harbour warehouses, 1280 x 720: 1,619 placed objects (`objects.json`) and
49 x 49 terrain patches, 5,221 draws; steady frames (the first makes the GPU objects: 216 ms of submit):

| build | draw list (constants per draw) | submit | GPU + read back (float RGBA, depth, stencil) | tonemap (CPU, rayon) | frame |
|---|---:|---:|---:|---:|---:|
| release | 30 ms | 33 ms | 8 ms | 7 ms | **~80 ms** |
| debug | 380 ms | 185 ms | 120 ms | 97 ms | ~780 ms |

In the editor (`tests/game_view.rs`, debug, 1600 x 950 window, the 3D view about 920 x 660): 3,915 objects (136
meshes) and the patches, 10,390 draws; loading the view 2.1 s (database, weather, every placed object's mesh set
prepared, terrain); the egui upload ~28 ms. Phase 3 (3.11) takes the copy and the CPU tonemap away (a shared texture,
the game's own tonemap on the GPU) and the per-frame constants (kept per draw, culling, LODs, instanced programs).

### Tests

`cargo test -p bc2-render`: 14 unit tests (DXBC reflection and discard scan, render states, cbuffer placement,
camera, tonemap curve and exposure, weather fields, the visual terrain grammar, files) and `tests/warehouse.rs`;
`cargo test -p bc2-editor --test game_view` (MP_005 headless: the switch, the frame drawn, back to the editor's view;
`BC2_EDITOR_SHOTS` saves both views in both themes). Both skip with a note without the prepared level or Direct3D 11.

## R5c: game-accurate rendering, phase 3a - interactive and editable (2026-11-19, branch `rust-r5c`)

`EDITOR_ARCHITECTURE.md` 3.11, "Phase 3a". New `bc2_render::live` (the draw list built once: planned constant
buffers, culling with a grid, the renderer's order kept), `d3d/live.rs` (materials resolved once, one constant ring
bound by offset, only changed bindings set, picking, the selection outline, the shared texture), `World::set_placed`
/ `sources` / `load_package`, `Files::bundles`; the editor's game view on it (`gameview.rs`: edits followed, picking
and the outline on the game frame, the frame shared with egui's Vulkan device). Development PC: GeForce RTX 4060
Laptop, Direct3D 11 hardware device; egui on Vulkan.

### Frame costs, 1600 x 950, before (R5b's per-frame draw list) and after

`examples/live_view.rs`, the median of 10 frames (release) or 3 (debug), the camera turning 0.05 degrees a frame so
every frame is built again. Before: build (constants per draw), submit, post, GPU + read back (float colour, depth,
stencil). After: build (camera values, culling, the blended order, the constant arena), upload (one `Map`), submit,
post, the GPU waited for, the frame copied into the shared texture. MP_005 at the warehouses; SP_01, NAM_MP_002CQ
and Wake Island from the south-west of the placed objects' median (`--view auto`).

| level | draws listed / drawn | build before → after (ms) | submit before → after | GPU (+ read back) before → after | share | **frame before → after** |
|---|---:|---:|---:|---:|---:|---:|
| MP_005, release | 10,417 / 3,093 | 54.7 → 3.2 | 64.1 → 1.0 (+0.5 upload) | 17.0 → 6.2 | 1.6 | **136.1 → 12.5** |
| SP_01, release | 7,839 / 2,565 | 36.8 → 2.0 | 32.9 → 0.8 (+0.2) | 13.5 → 1.0 | 0.2 | **83.3 → 4.3** |
| NAM_MP_002CQ (Vietnam), release | 4,577 / 1,823 | 17.6 → 1.0 | 16.9 → 0.5 (+0.1) | 15.5 → 1.1 | 0.2 | **50.0 → 2.9** |
| Wake Island (Pacific War), release | 10,395 / 4,063 | 43.3 → 4.2 | 44.0 → 1.2 (+0.6) | 14.5 → 7.0 | 0.4 | **101.9 → 13.4** |
| MP_005, debug | 10,417 / 3,093 | 517.5 → 11.7 | 168.9 → 11.7 (+0.5) | 115.9 → 4.7 | 1.8 | **802.4 → 30.5** |
| SP_01, debug | 7,839 / 2,565 | 428.3 → 8.9 | 151.8 → 7.0 (+0.2) | 127.8 → 8.4 | 3.6 | **708.1 → 28.6** |
| NAM_MP_002CQ, debug | 4,577 / 1,823 | 183.6 → 8.0 | 87.5 → 6.1 (+0.1) | 125.7 → 1.1 | 0.6 | **397.0 → 16.5** |
| Wake Island, debug | 10,395 / 4,063 | 562.7 → 13.2 | 158.6 → 11.3 (+0.5) | 112.0 → 9.3 | 3.2 | **833.5 → 37.6** |

The copy fallback's read back of the picture alone costs 1.5-5.7 ms more, then egui's upload (18.8 ms in the debug
editor). The first frame of a level makes its materials: 105-210 ms release, 0.4-1.5 s debug. In the editor (the
3D view about 920 x 660, `tests/game_view_live.rs` and `tests/game_view_package.rs`, debug): MP_005 21.5 ms shared
against 41.7 ms copied; NAM_MP_002CQ 15.6 ms; Wake Island 75-112 ms (a wide view: 7,507 of 10,395 draws). Peak
memory of the release example: 631-781 MB working set, 668-962 MB private.

Where the frame goes now: the GPU on wide views (MP_005, Wake Island), then the build. LODs and instancing (next
round) cut the GPU's work; the CPU submit is already about a millisecond.

### Proofs

| what | result |
|---|---|
| live list against R5b's per-frame list (colour bits, depth bits, draw class, post-processed RGBA8), culling off and on | **identical** on every view run: MP_005 warehouses and a second view at 1280 x 720 (921,600 of 921,600) and 1600 x 950 (1,520,000 of 1,520,000); SP_01, NAM_MP_002CQ, Wake Island at 1600 x 950 (1,520,000 each) |
| `--job`: the frame as a `shaderview` job (all 10,417 draws), drawn by the C++ harness, against the live frame (culled) | MP_005 warehouses **921,600 of 921,600**; the harbour from the east (`60,40,300`) 921,600 of 921,600; the west shore (`-90,60,300`, looking east) 921,584-921,600, see below |
| an edit replayed (`--edit`: an object moved, one turned a quarter, one removed, one duplicated, one added) against the level loaded again with the edits | **921,600 of 921,600** (and the post-processed picture identical); replayed in 10.5 ms, 4 instances planned again; the unedited frame differs in 39,307 pixels |
| the editor: the warehouse moved through `GameView::sync`, against `GameView::load` of the edited scene | 611,800 of 611,800 |
| picking (`--pick`): at the projected centres of 26 + 45 objects in two views | every answer confirmed by the selection mask (the picked object lies at the frame's depth under the pixel) |
| the editor: a click at the 3D view's centre | `objects/buildings/ind_warehouse` selected, its outline drawn (1,718 pixels orange) |
| the editor's window, shared texture against the copy fallback | 1,520,000 of 1,520,000 pixels identical (the costs line left out) |

**The west shore stays open (R5b's far horizon):** there, 0-16 far water pixels (blended, the same depth and alpha,
colour differing in the fourth decimal) change from run to run in all three paths alike: R5b's per-frame list, the
live list with culling on or off, and `shaderview` (R5b's list against `shaderview`: 921,584 in one run). The live
list adds no difference of its own: culling off and on give the same count in every run. The water reads the
scene's linear depth through its `GenerateMips` chain, the first suspect.

**Fixed after the merge (R5c fix, branch `rust-r5c-fix`): two devices' post-processed pictures differing.** The
editor test's edit check found the replayed and the freshly loaded view's RGBA8 pictures apart (once one 8-bit step
in the quick tier; on main, alone, 1,414,404 bytes by up to 57) while their HDR scenes were identical bit for bit.
Not the exposure: `psExposure` reads a fixed old exposure of 1, there is no history between frames. The cause was
`Device::buffer`'s cache, keyed by the data's address and length: the post passes' full-screen quads (192 bytes
each, one per pass size) were made from data dropped at once, so a later quad of another size could take a freed
address and be handed the cached buffer of the earlier size - a post pass drawing the wrong area, depending on the
allocator's history. The cache now keeps each `Arc` it was given (no other data can take the address while its
buffer is kept) and the quads are made directly, kept by size. `d3d::tests::a_dropped_datas_buffer_is_not_given_to_new_data`
reproduces the hazard (it fails on the old code, a new allocation taking the dropped data's address and its buffer)
and passes now. The test asserts the post-processed pictures bit for bit again: 5 runs alone and 3 with the quick
tier running (2 beside it, 1 inside it), all 0 bytes apart. The editor's view keeps no exposure state between
frames, so no reset is needed on load or after an edit.

### Package levels (round G3's levels, read from their bundles in place)

Opened through a stand-in OPFOR folder in temp (the game copy linked, the junction removed first; nothing written
to OPFOR's `work\`). Load (release): NAM_MP_002CQ 9.8 s (11 rounds, 84 MB from 5 bundles), Wake Island 25.0 s (12
rounds, 134 MB from 4 bundles). Missing per level:

- **NAM_MP_002CQ:** nothing reported (1,133 objects, 261 group members, 82 meshes, 12 strip and 2 water draws,
  terrain drawn).
- **Wake Island:** nothing reported missing (1,329 objects, 2,999 group members, 96 meshes, 40 strip draws, 1
  water draw, terrain drawn), but its tiled-mask terrain cells draw black (the converted terrain's patch stream is
  not what R5's reading gives; the terrain-patch donor rule), the sky is the fog colour (the sky pass is not drawn
  yet: every level) and the carriers stand on no water (the open sea is not in its `VisualWater`).

### Still placeholders

Unchanged from R5b (the capture postponed): the sun direction, shadow cascades, `EnvironmentMain` (sky, haze, fog),
the exposure and tonemap constants (`post::PLACEHOLDERS`), tree and wind constants, the terrain's patch stream.

### Tests

`cargo test -p bc2-render`: 20 unit tests (a plan filled gives `frame::cbuffer`'s bytes; boxes outside the
frustum; and R5's), `tests/live.rs` (MP_005: the live list against the old one with culling on and off, picking
at the centre checked by the mask, an edit against a fresh load), `tests/world.rs`, `tests/warehouse.rs`.
`cargo test -p bc2-editor --test game_view_live` (shared against copied, a click picks the warehouse, the outline,
an edit through `sync`); `--test game_view_package` (with `BC2_R5C_PACKAGE_OPFOR`: NAM_MP_002CQ and Wake Island
opened and drawn, their notes and costs).

## G3: the levels that live only in game\Package (2026-11-18, branch `rust-g3`)

`EDITOR_ARCHITECTURE.md` 3.10, "Done in round G3"; `rust/bc2-editor/src/package.rs`. No Python replaced: NavKit
never listed these levels. The editor lists every installed Package-only level (`package.mft` names it, every
bundle of its level stack is present) - 45 on OPFOR's game copy: Vietnam's 20 (`nam_mp_00{2,3,5,6,7}` x cq / r /
sdm / sr), the Pacific War's 4 (Wake Island, Coral Sea, Guadalcanal, Iwo Jima) and DICE's other 21 (the 8 Bad
Company 1 maps and 5 `MP_SP_*` layouts on `mp2_common`, 8 mode layouts on a Dist level such as MP_005GR) - and
prepares each in Rust from
its bundles, reading only what the scene opens, into `work\editor\levels\<level>` (not `work\levels`: its corpora
stay as they are - OPFOR's 37 folders the same before and after; all runs went to a stand-in OPFOR folder in temp
with the game copy linked read only, the junction removed first).

**Prepare and open** (debug build of the test, `tests/g3.rs` `real_package_levels`, a WW2 rebuild running beside it;
the editor's process stayed under 150 MB):

| | levels | prepare (s) | open (s) | copy (MB) | entities | collision meshes |
|---|---:|---:|---:|---:|---:|---:|
| Vietnam (`nam_*`) | 20 | 2.4 - 4.7 | 0.15 - 0.33 | 7.9 - 9.0 | 4,070 - 6,140 | 2,416 - 4,007 |
| Pacific War | 4 | 2.7 - 4.0 | 0.24 - 0.29 | 12.6 - 17.6 | 3,539 - 7,144 | 2,847 - 5,230 |
| Bad Company 1 maps | 8 | 2.9 - 3.7 | 0.28 - 0.34 | 18.1 - 19.9 | 11,158 - 12,679 | 8,969 - 9,217 |
| other mode layouts (8 on Dist levels, 5 `MP_SP_*`) | 13 | 2.9 - 3.7 | 0.21 - 0.34 | 9.5 - 19.0 | 4,011 - 12,307 | 2,342 - 9,968 |
| all | 45 | 151 in all | | 585 in all | | |

Each opened with terrain (1024 or 2048 samples a side) and water, team labels from its data (US / NVA on Vietnam's,
US / JP on the Pacific War's; T1 / T2 on the squad deathmatch layouts that name no faction) and no notes. A whole
unpack of the same bundles, by comparison, writes 1.3 - 2.0 GB per stack (below). Vietnam's terrain is in
`nam_common`'s terrain bundle (each level's own terrain bundle holds only `*deleted*` markers of the others' terrains);
`*deleted*` markers hid 1 name a later bundle had on 9 levels (`nam_mp_006*`, `nam_mp_007*`, MP_009SR), none of them
read by the scene.

**The independent read** (`tests/package_levels_check.py`): every bundle of each level's stack unpacked whole by the
suite's FbrbTool and converted by DbxEdit `xml`, as NavKit prepares a Dist level (NavKit's own current unpacks of
`mp_common` and the Dist base levels read in place), the terrain bundles unpacked too, then loaded by the suite's level
loader (`bfbc2core.level`'s `scene_load`) and compared with what the editor opened from its copy: the entity GUIDs in
order, the entities with a blueprint type, every collision mesh (entity and blueprint, in order), the blueprints
without collision, the collision triangles, the terrain's size and the water's triangles. **45 of 45 levels the
same** (Vietnam and the others in 41 levels, scratch peak 1.32 GB; the Pacific War's 4, peak 1.97 GB; the scratch
deleted). Guadalcanal's 7,144 entities are E1e's hand-prepared count.

**Projects on Package levels** ("New map from an existing map"; `BC2_EDITOR_G3_PROJECTS`): on NAM_MP_002CQ, Wake
Island and Coral Sea a project made through the editor's own flow has its base's profile (Vietnam, WW2), and a move
plus a delete apply through levelwrite to the partition with nothing else changed (the moved object's `Transform`;
the deleted instance, its `LevelData.Objects` entry and, on Wake, its `EventConnections`); "Check edits" replays both
cleanly.

**Choose mode** (the user's start-screen design; `src/catalog.rs`, `src/app/modes.rs`): four cards from the levels'
data (BFBC1 8 maps, BFBC2 51, Vietnam 20, WW2 4 on OPFOR's copy), each with the game's own loading screen
(`awards/images/loadingscreens/`: `bc1_oasis`, `mp_005`, `nam_mp_002` - DICE's - and `wake_island_s` - OPFOR's
converted; a `RES` header, then a 1280 x 720 DXT1 `ITexture`, decoded by `bc2-convert`'s `texcheck` and halved to
640 x 360), 82 level thumbnails (`awards/images/levels/<level>.res`, the picture in the top left of a 512 x 256 texture,
its white margin cut) and 83 game modes (the descriptions' `GameMode`). Read from four bundles at once in 4.8 s on the
first start (debug build), 0.11 s from `work\editor\art` (about 1.1 MB of PNGs and `art.json`) after;
`card_art_on_the_real_game_copy` (`BC2_EDITOR_G3_ART=1`) checks every set's card on the real copy,
`mode_cards_fall_back_to_the_tile` the plain tile where a picture is missing or not a texture.

## G4: the start flow - Open project, Edit existing level, Create level (2026-11-19, branch `rust-g4`)

`EDITOR_ARCHITECTURE.md` 3.10, "Done in round G4". No Python replaced; no game data written (the catalog's
`work\editor\art` cache only, as G3).

**Multiplayer or campaign** (`catalog::LevelKind`, from each description's `GameMode`; `cargo test -p bc2-editor
--test g4 real_counts` with `BC2_EDITOR_G4_REAL=1`, read only, OPFOR's game copy):

| set | levels | multiplayer (Edit existing level) | campaign (Create level only) | not read |
|---|---:|---:|---:|---:|
| BFBC1 | 8 | 8 | 0 | 0 |
| BFBC2 | 51 | 38 | 13 (`sp_01` - `sp_04`, `sp_003_b` - `sp_009`) | 0 |
| Vietnam | 20 | 20 | 0 | 0 |
| WW2 / Pacific War | 4 | 4 | 0 | 0 |
| all | 83 | 70 | 13 | 0 |

The `MP_SP_*` layouts and the Onslaught levels name multiplayer modes, so they are multiplayer.

**Through the interface** (`cargo test -p bc2-editor --test g4`, headless, clicking the buttons the start screen
drew; a made-up game copy in temp with four Package levels and their descriptions - WW2 Conquest, Vietnam Rush,
BFBC2 Conquest, BFBC2 campaign):
- `edit_existing_level`: Edit existing level, BFBC2 lists the multiplayer level and not the campaign one; Back; WW2,
  Open prepares and opens the WW2 level read-only.
- `create_and_open_projects`: Create level, From existing levels lists all four (the campaign level too); its New
  level, a name and folder, Create level opens the new project, its file records `"kind": "campaign"`. The next start
  (the saved state): Open project lists it under recent and in the projects folder; an autosave differing from the file
  brings the recovery offer; the saved project opens and the autosave is gone.
- `create_from_a_height_map`: the form refuses a missing file ("does not read"), a 300 x 300 PNG (the sizes it could
  be) and a RAW whose length is not the size chosen ("choose 1024"); a 1025 x 1025 16-bit PNG with WW2, 1024 m,
  -10 to 250 m and water at 2.5 m saves `Atoll.mapedit.json` recording exactly these, the sample range 0 - 40,960 and
  the height map copied beside it byte for byte; reopened, it shows its page (no level loaded).
- `top_bar_fits`: a project named "Operation Long Project Name North Ridge" on a WW2 level at 1500 px: the badge and
  name end at x = 910, the buttons start at 913 (before G4 the name and badge ran under Save).
- Unit tests: `heightmap` (sizes, PNG / RAW / 8-bit / not-a-PNG checks, the settings, a saved project), `catalog`
  (kinds from the modes), `bc2-levelwrite` (`base.kind` and `terrain_import` round trip; a project without them
  writes the same keys as before).

**Screenshots** (`BC2_EDITOR_G4_SHOTS=<folder>`, `screenshots`): the start, Open project, the mode cards, WW2's and
BFBC2's maps, the New project dialog, Create level, From existing levels, a campaign base's dialog, the height-map form
(ready and refused) and the saved project's page, dark and light, 1600 and 1280 px wide (48 pictures).

## E3d (map export): any map, any era, any mode - the canaries (2026-11-19, branch `rust-e3d`)

`EDITOR_ARCHITECTURE.md` 3.12, "Round E3d". No Python replaced; `game\` only read. The project's target (era, mode,
donor) is an operation of its log; the export writes the project as it is plus the closure under the target era's
bundles.

**The canaries** (the user's list, 2026-11-19; `cargo run --release -p bc2-levelwrite --example e3d_matrix --
<game> <projects> <OPFOR work\mapedit>`): each a new project on a stock level, its target the first operation (the
donor's layer in, the base's out), a scripted edit (an object moved 8 m; a flag, base or M-COM moved by "Use
selection"), the export, then `mapexport check`. Release build, a shared PC (other rounds building), the bundle
cache warm:

| Canary | Base -> target | Layer in / base's out | Closure: copied (walked) | Check | Time |
|---|---|---|---|---|---:|
| Aurora -> WW2 Conquest, WW2 objects | `sp_01` -> WW2 Conquest (`guadal_canal`), a radar bunker and a Kurogane spawner placed from `guadal_canal`'s bundle, the 2 bases and 2 flags at E3's places | 277 / 395 | 1,686 (1,347): 719 `mp_common` (675 under `ww2_common`'s markers, 44 under `sp_01`'s), 952 `guadal_canal` (802 not loaded: WW2 kits, the bunker, the Kurogane), 14 `iwo_jima_s` / `wake_island_s`, 1 `ww2_common` | every line | 25 s |
| Rush -> Conquest | `mp_005gr` -> BFBC2 Conquest (`mp_001`) | 317 / 821 (the 4 Rush stages out) | 36 (374): `mp_common` under `mp_005gr`'s markers | every line | 10 s |
| Wake Island WW2 -> BFBC2 | `wake_island_s` -> BFBC2 Conquest (`mp_001`) | 317 / 256 | 242 (931): 241 `ww2_common` (no longer loaded), 1 `mp_001`; 2 BF1943 voice banks replaced by BFBC2's | every line | 12 s |
| Vietnam -> WW2 | `nam_mp_003cq` -> WW2 Conquest (`guadal_canal`), the layer scaled 0.26 into the map | 277 / 254 | 1,636 (1): 1,619 `guadal_canal`, 13 `mp_common`, 4 `ww2_common`; the height field kept in `nam_common`'s terrain bundle | every line | 12 s |
| BFBC2 -> WW2 Air Superiority | `mp_005` | - | - | PENDING: "Air Superiority is BF1943's mode, still queued in OPFOR's Pacific War work" | - |
| A squad mode | `mp_002` (BFBC2 Rush) -> Vietnam Squad Deathmatch (`nam_mp_002sdm`) | 164 / 1,104 | 2,526 (71): 2,374 `mp_common` under `nam_common`'s markers, 138 `guadal_canal`, 14 `iwo_jima_s` / `wake_island_s` | every line | 20 s |
| WW2 -> BFBC2 Rush | `guadal_canal` -> BFBC2 Rush (`mp_002`): 5 stages of 2 M-COMs | 1,088 / 291 | 236 (900): 234 `ww2_common`, 2 `mp_002`; 2 voice banks replaced | every line | 14 s |

**What the checks found on the way** (each fixed, each now a check line or a rule): the BFBC2 export of Wake
Island spoke BF1943's lines (the WW2 level's own `vo_common` / `vo_en` before `mp_common`: E3c's voices line failed;
the donor's versions now go first); the Vietnam -> WW2 export lost its ground (the height field is in `nam_common`'s
terrain bundle: the terrain block keeps the base's terrain bundles, a new *terrain* line checks it); `mp_005`'s
guard-tower effects name emitters only `guadal_canal` holds (DICE's dangling reference: never filled from an
unrelated level); OPFOR's Wake Island names its loading picture by its level (`WAKEISLAND2_NAME` ->
`wake_island_s.res`); a Rush base's M-COMs leave with its layer (a scripted edit on one failed until it picked from
the level as the target left it).

**Install, end to end** (`mapedit_install.py`, the suite's release `mapexport` via `BFBC2_MAPEXPORT`): `--plan` of
each of the six stages against the real game copy (read only); `--apply` of all six one after another on a scratch
copy of the shared files (`--game <scratch> --check-game <game>`): each section in `maps.ini` with its mode
(`Conquest=`, `Rush=`, `Squad Deathmatch=`), 5 or 11 `package.mft` blocks per level (the donor's vo blocks), the
startup list, the names and pictures; `--revert` of one, then `--revert-all`: the six shared files back byte for
byte with their last-writer records, no level folder or backup left. `game\`: 1,582 files with the same size and
write time before and after the whole round's runs, 26 key files (the shared files and every bundle the canaries
read) with the same sha256.

**Tests.** `tests/export_e3d.rs`: each era's modes from the data, each mode's layer (`mp_005gr` 4 stages of 2,
`mp_005sr` 2 of 1, `mp_009sdm` 5 teams and 4 kill counters, `nam_mp_002r` 3 of 2; `mp_009cq` 238 instances,
`mp_005gr` 821), Rush -> Conquest as a project operation (complete; deleting every capture point names the missing
base and flag; the undo gives the base back byte for byte), WW2 Rush and Air Superiority refused in words.
OPFOR's `test_mapedit_install.py`: every mode's `maps.ini` line in and out, `--check-game`.

## O1: the orchestration and command-line glue (2026-11-18, branch `rust-o1`)

The scripts and command lines that parse arguments, walk levels and folders, call the Rust core and print or write
results. The rounds before this one ported the core under them. This round makes the inventory and the plan for
the rest, and ports the first batch: FbrbTool's and DbxEdit's command lines.

**Provisional timings.** Other agents ran on the PC throughout. Free RAM was 0.7 GB at times, and
`ERROR_NO_SYSTEM_RESOURCES` (1450) stopped one unpack. Start-up figures are stable. Figures dominated by disk
writes varied up to 4x from run to run.

### Step 1: the inventory

The scope is every Python module that is orchestration or a command line rather than a format or compute kernel.
`git ls-files`, less the tests, has 59,103 lines of Python. 24,594 of them are in the 48 modules below: entry
points, drivers, walkers and report printers, counted whole (most mix a CLI with logic).

**Left out of the inventory:**

- **GUIs** move into the egui editor. These are the hub `BFBC2 Tool Suite.pyw`, `navkitGUI.pyw`, `inspector.py`,
  `studio.py`, `overlay.py`, `editing.py`, `core/bfbc2core/viewer.py`, `pick.py` and the window half of
  `errors.py`.
- **Tests** are ported after every tool's retirement. This includes `run_tests.py` and `rust/bench_*.py`.
- **Compute kernels**, ported with their E and F rounds: NavKit's `navbuild`, `navwalk`, `navdetail` and the like,
  and `testharness/checks`, `classify` and `ingame`.
- **Pieces other rounds own:**
  - F3: the remaining format readers;
  - S1: FBRB packing, BF1943 `Source`, shaderdb merging and indexing;
  - F2a: Granny, texwrite, terrain, the console mesh, skinmesh, shapes, decimate;
  - the editor crates.

**How the table was measured:**

- **Start-up** is a fresh `python` that imports the module without running its main, best of 3. It is the fixed
  cost every process call pays before any work.
- **A job** is a representative real one, cited from the round that measured it.
- **Callers** come from a survey of the suite and of OPFOR's tracked `.py`, `.bat` and `.ps1` files:
  - "sub" means the module is run as a process;
  - "imp" means it is imported.

**Shapes.** The table assigns each module one of four:

- **A, a subcommand of `bfbc2.exe`** (`rust/bc2-cli`). Use it when the module is a command line over core that is
  already Rust. The process then saves the interpreter, the imports and the module load (0.1-0.3 s per call).
  Callers that run the tool as a process gain the most: OPFOR's `.bat` and `.ps1` files, and its `subprocess`
  calls of aptcheck, aptgraft, culltree, renderqa and shaderdb.
- **B, a Rust library function behind the Python shim.** Use it when Python callers import the module (OPFOR's WW2
  pipeline imports bc2loc, meshview, bf1943import and shaderdb). The data stays on the Rust side, and the shim keeps
  the import working until its callers move.
- **C, both:** library first, then the subcommand on it.
- **G, the egui editor.**

Each port keeps the Python as reference and fallback behind a switch until a full tier passes. The switch for the
command lines is `BFBC2_CLI_PYTHON=1`.

| module | lines | callers | start-up | a real job (round) | core-bound? | shape, batch |
|---|---:|---|---:|---|---|---|
| `bfbc2-fbrb-tool/fbrbtool.py` (+ `bfbc2core.fbrbtool.main`) | 23 (+15) | hub (sub); OPFOR `build_menus.bat`, `merge_startup.ps1`, `campaign_fill.ps1` (sub, by `import_onslaught.bat`); test_cli_errors | 0.15 s | `merge_startup.ps1`'s unpack + pack: 1.64 s, Rust 0.32 s | yes | **A, done (O1)** |
| `bfbc2-dbx-tools/dbxedit.py` (+ `bfbc2core.dbxedit.main`) | 26 (+15) | by hand; test_cli_errors (OPFOR calls it in process) | 0.15 s | xml + roundtrip + check of one partition: 0.70 s, Rust 0.39 s | yes | **A, done (O1)** |
| `menu-tools/aptcheck.py` | 501 | OPFOR sub `check` / `verify` (console_icons, menu_install, duplicates, type95_icon, wake_ui, weapon_icons, ww2_crosshairs); aptrefs | 0.12 s | E4c: the model and checks in Rust | yes (E4c) | A, **O2** |
| `menu-tools/aptrefs.py` | 444 | tests; bench | 0.10 s | E4c | yes (E4c) | A, O2 |
| `menu-tools/bc2loc.py` | 429 | OPFOR imp (menu_install, names, wake_install, ww2_awards); menucommon | 0.11 s | F0: `LocTable` in Rust | yes (F0) | C, O2 |
| `menu-tools/menucommon.py` | 458 | aptcheck, aptrefs, bc2loc | (lib) | | mostly | B with its CLIs, O2 |
| `crash-reader/exeref.py` | 336 | tests, bench | 0.07 s | E4f: 24 commands as processes 11.7 s, in process 5.5 s | yes (E4f), but capstone answers what the core refuses | A with a Python fallback for refused instructions, O2 |
| `crash-reader/crashreader.py` | 485 | hub (sub); OPFOR `testrun.py` (sub, `<report> --exe --log`) | 0.08 s | E4f: 40 reports 2.4 s, Rust core 1.0 s | yes (E4f), the same capstone fallback | A, O2 |
| `navkit/tools/hpdtool.py` (CLI half) | 425 | sub navsample_offline (`roundtrip`, `meshtrip`); run_tests; imp everywhere | 0.07 s | F0: `hpd_parse` / `hpd_write` in Rust | yes (F0) | A for the CLI, O2 (the module stays a library) |
| `navkit/tools/hkpack.py` | 226 | navsample_offline; OPFOR research notes | 0.04 s | F1a: the Havok reader in Rust | yes | A, O2 |
| `console-convert/convert.py` | 204 | tests | 0.12 s | R2 / F2a: the conversions in Rust | yes | A, O2 |
| `anim-tools/aimanim.py` | 131 | bf1943import; OPFOR weapon_preview (imp) | 0.04 s | | yes (Granny reader E3i) | C, O2 |
| `core/bfbc2core/caches.py` (`python -m bfbc2core.caches`) | 686 | every cache (imp) | | prune / list: file walks, its own work | own | B (prune, day_log: the log naming is already Rust in `bc2-cli/src/pylog.rs`), O3 |
| `culltree.py` (CLI and `add`) | 1,409 | OPFOR `console_cull.py` (sub `add <tree> <level> --auto --misplaced N <refs>`) | 0.06 s | E1c: the reader and level search in Rust; `add` is Python | half | C, O3 (after F3) |
| `console-readers/apt/apt.py`, `stfs/stfs.py`, `xiso/xiso.py` (CLI halves) | 551 + 242 + 172 | hub (sub stfs / xiso); OPFOR imp | 0.04-0.12 s | F0 | yes | A for the CLIs, after F3 |
| `shader-tools/shaderdb.py` (CLI half) | 1,731 | OPFOR `console_shaders.py` (sub `needs`, `merge`); ~80 imports | 0.11 s | E4b / S1 | after S1 | A for `needs` / `merge`, after S1 |
| `bf1943-import/bf1943import.py` (CLI half) | 841 | ~30 OPFOR WW2 imports | 0.17 s | E3r / F2b | after S1 (`Source`) | C, after S1 |
| `menu-tools/aptgraft.py` | 3,291 | OPFOR sub (`mode-alias`, `mode-case`, `onslaught-timer`), imp (ww2 icons, menu_install, wake_install ...) | 0.10 s | E4c: model in Rust, the grafts Python | half | C, O3 |
| `console-convert/audio.py` | 668 | OPFOR imp (wake_level, wake_vo); tests | 0.23 s | E3a / F0: encoder and framing in Rust | mostly | C, O3 |
| `stockmp-ai/stockmp.py`, `pipeline.py`, `validate.py`, `config.py` | 278 + 512 + 602 + 326 | by hand (OPFOR docs: `stockmp.py install mp_005 --variant exp0 [--revert]`); tests | 0.10 s | E4a: Atacama generate + build + validate 15.7 s, 10 s of it packing (core) | mostly | C, O3: `bfbc2 stockmp` with the plan in Rust (`bc2-stockmp` has the model, authoring and checks) |
| `auto-navkit/navkit.py` | 998 | hub (sub `prepare`), navkitGUI (sub), Navigation Studio (sub `apply`, `install`, `restore`), editor's `prepare_level.py`; 19 tests | 0.11 s | E4e: `prepare` sp_004_b 56.7 s, sp_009 9.6 s (unpack, DbxEdit xml, levelobjects: core) | mostly | C, O4, after NavKit's remaining stage Python (navdetail, finalize) |
| NavKit's tool CLIs: `navregress`, `navparity`, `navcompare`, `levelobjects`, `detailed`, `ladders`, `hpdcheck`, `hpdview`, `sample_all`, `graphstats` | 301 + 336 + 101 + 247 + 125 + 63 + 96 + 109 + 71 + 29 | navkit, harness, tests (imp); run_tests (sub hpdcheck, navregress) | 0.08-0.71 s | E4e: parity measures 15 levels 158.7 s; navregress per level 3-16 s | mostly (E4e) | C, O4, each with its kernel's round |
| `test-harness/harness.py`, `testharness/runs.py`, `levelchecks.py`, `levelinfo.py` | 418 + 288 + 138 + 394 | bench_sweep; tests | 0.28 s | E4f: `run` sp_009 + sp_004_b 9 s (NavKit's tools inside) | half | C, O4 (after NavKit's tools) |
| `bf1943-import/dbxcatalogue.py`, `dbxparity.py`, `portparity.py` | 353 + 371 + 259 | OPFOR imp (bf1943_fx, bf1943_objects, qa_checks ...) | 0.09-0.22 s | E4d / F2b: the catalogue and port in Rust | mostly | B, O3 |
| `console-convert/texqa.py` | 1,364 | OPFOR imp (qa_checks); bench_sweep | 0.24 s | E3o: texqa's decoder in Rust | half | C, O4 |
| `shader-tools/renderqa.py`, `xdiff.py` | 2,399 + 438 | OPFOR sub (`ww2_level_checks.py`: `renderqa.py strips` / `patches`), imp (fidelity_checks, wake_preview; xdiff in OPFOR tests) | 0.26 s (renderqa: import needs the module's state) | GPU (shaderview.exe) + E3f's translator | half | C, O5 (after the Dx9 / shader rounds) |
| `mesh-viewer/meshview.py` (`find`, the `Partitions` / `Index` library) | 609 | ~55 OPFOR imports, 22 tests | 0.21 s | E1d / E3m: `Partitions` one pass per bundle | half | B for the library, G for the viewer, O3 |
| `rust/bc2-editor/prepare_level.py` | 90 | the editor (Rust starts it) | 0.03 s | runs `navkit.py prepare` | yes | into `bc2-editor` once `navkit prepare` is Rust (O4) |

What is left to port: 24,594 lines of the inventory, less the ported command lines' 49 + 30. The Python of those
stays as the reference until a full tier, as earlier rounds kept theirs. Most of the remainder is logic its own
rounds port. Once that logic is Rust, the glue around it becomes a thin subcommand, so the plan orders the batches
by what each module waits on:

| batch | modules | why then |
|---|---|---|
| **O2** | aptcheck, aptrefs, bc2loc, menucommon; exeref, crashreader; hpdtool's / hkpack's / convert.py's / aimanim's command lines | thin over core that is already Rust; OPFOR runs aptcheck many times a WW2 install and crashreader after every test run (0.1-0.3 s of start-up each) |
| **O3** | caches (prune, logs), stockmp (with `bc2-stockmp`'s plan), aptgraft, audio, dbxcatalogue / dbxparity / portparity, meshview's library, culltree's `add` (after F3) | glue whose logic is mostly in Rust already |
| **O4** | navkit.py and NavKit's tool CLIs, the harness, texqa, prepare_level | after NavKit's last stage Python and the harness's NavKit calls |
| **O5** | renderqa, xdiff; shaderdb's, bf1943import's and the console readers' command lines | after S1, F3 and the shader rounds |

### Step 2: the first batch - FbrbTool's and DbxEdit's command lines

**Why these first:**

- They are the most-called process command lines. The hub's Bundles tab runs them, and OPFOR's
  `build_menus.bat`, `merge_startup.ps1` and `campaign_fill.ps1` call `python fbrbtool.py`.
- They sit entirely over the core that is already Rust (E3b / E3k), so a process paid 0.15 s of start-up for
  nothing.
- They are the pattern the later subcommands follow.

**What was added:**

- **`rust/bc2-cli`, the binary `bfbc2.exe`.** `bfbc2 fbrbtool <args>` and `bfbc2 dbxedit <args>` run
  `bc2_formats::fbrb::tool::run` / `dbxedit::run`, then write what `fbrbtool.py` / `dbxedit.py` write.
- **The day log.** `pylog.rs` ports `caches.day_log`, `log_caps` / `_number` and `dbxedit.log_path` / `_log`:
  - the part numbering and the per-file cap (`BFBC2_LOG_FILE_MB`, read as Python's `float` reads it: Unicode
    digits, underscores, Python's whitespace);
  - `ntpath.join`, and `expanduser`'s fallbacks;
  - the stamp of `datetime.now()` (`GetLocalTime`).
- **The printed bytes.** `pystdio.rs` writes what Python 3's stdio writes on Windows:
  - **A console** (`GetConsoleMode`, not Rust's `is_terminal`, which also counts MSYS pipes) gets Unicode text.
  - **Anything else** gets `\r\n` in the ANSI code page: Python's own `cp1252` table, or UTF-8 under
    `PYTHONUTF8=1`, `PYTHONIOENCODING=utf-8` or code page 65001.
  - **stdout** is strict. A character cp1252 lacks raises `UnicodeEncodeError` before anything is printed: exit
    code 1, with the error naming the same character run and position.
  - **stderr** uses `backslashreplace`.
  - **The order on one pipe** is Python's. stderr is line-buffered and leaves at once. stdout leaves at exit, after
    stderr, when its encoded text is 8,192 bytes or less, and at once when longer (`TextIOWrapper` / `BufferedWriter`
    in 3.13). `PYTHONUNBUFFERED` makes it leave at once.
- **`bfbc2 --source-hash`** prints the fingerprint baked in by `build_native.py`, which now builds the binary with
  the module.
- **`bfbc2core.native.cli_command(tool)`** returns `[bfbc2.exe, tool]` only while that hash is this checkout's.
  Otherwise, or with `BFBC2_CLI_PYTHON=1`, it returns `[python, tool.py]`.
- **The hub's Bundles tab** runs through `cli_command`. `fbrbtool.py` and `dbxedit.py` stay as they are: the
  reference and the fallback.

**Recorded differences, by design** (none occurs in OPFOR's calls or the recorded lines):

- Python's traceback frames (file paths, source lines) on that `UnicodeEncodeError` are not printed. Its first and
  last lines are, and the test compares those.
- Under an ANSI code page other than 1252 or 65001, non-ASCII output is written as UTF-8 where Python would use
  that page.
- `PYTHONIOENCODING`'s error-handler part, and a `TZ` variable for the log stamps, are not read.

### Proof

**The recording.** `core/tests/cli_o1_golden.json` (722 command lines, 0.6 MB) was recorded from the Python command
lines before the port answered any (`test_cli_o1.py --record`). For each line it records:

- the exit code;
- stdout and stderr as bytes, the folders named;
- the case folder's digest;
- the log lines, with stamps taken out.

The 722 lines are:

- test_fbrbtool's 228 lines and test_dbxedit's 82 cases (what the retired exes answered, as command lines);
- 15 encoding, buffering and argument cases:
  - paths and entry names in cp1252 and outside it (stdout fails, stderr escapes), an emoji;
  - lists of more and less than 8,192 bytes, and a `big` that prints more or less than 8,192 bytes and then fails;
  - an empty argument, quotes and spaces, the usage;
- each of those lines again with stderr on stdout's pipe;
- the stdio cases again under `PYTHONUTF8=1`, `PYTHONUNBUFFERED=1` and `PYTHONIOENCODING=utf-8`.

**Who answered.**

- **Rust: 722 of 722 as recorded.**
- **Live, both ways** (`--live`): the Python command lines gave the same 722 answers again, with Python's own
  bytes. Python took 178 s of process time and Rust 50 s, 4 workers.
- **The order on one pipe** really flips at 8,192 bytes. The recorded `big` with 18,268 bytes on stdout prints
  stdout first; the 3,268-byte one prints stderr first. Rust gives both orders.

**OPFOR's real calls, replayed read-only** (`--game`; outputs in a temporary folder, the game and OPFOR's `work\`
only read). 28 FbrbTool lines and 36 DbxEdit runs, each alone and on one pipe, 192 runs: **Rust answered every one
as Python did**. Python took 22.9 s and Rust 10.0 s. The lines were:

- `build_menus.bat`'s unpack of `original\...\mainmenu-00.fbrb` and its pack;
- `merge_startup.ps1`'s unpack of `startup-00.fbrb` and its pack;
- `campaign_fill.ps1`'s two sp_005 unpacks;
- 12 bundles of the logged invocations (fbrbtool-<date>.log), each listed and unpacked;
- `xml`, `roundtrip` and `check` of 12 corpus partitions, copied out of `work\`.

A scripted line must succeed on the Python side, or the test says the replay is not of a working call. The first
run of this, in the long scratch folder, failed both ways identically on the 260-character limit, which led to
that check.

**Checks:**

- `cargo fmt`; `cargo clippy --workspace --all-targets -D warnings`;
- `cargo test -p bc2-cli -p bc2-formats` (`bc2-cli`'s: cp1252 and its errors, Python's float and digits,
  `ntpath.join`);
- ruff and mypy (every folder) clean;
- `python run_tests.py quick -j 2` (merged with main at F2a, `f61e970`): 118 of 119, 665 s.
  - The one failure was this round's own test. The recording had been made in a temporary folder of another
    length, and a `UnicodeEncodeError`'s position counts the folder's characters.
  - Recorded again in the default `%TEMP%` (8 lines changed, the positions). Then this test passed quick, `--live`
    (Python 271 s, Rust 91 s, a busier PC) and `--game` (192 runs; Python 81.0 s, Rust 30.4 s), so 119 of 119.
  - The tier ran with cargo hidden: drive C: had 5.9 GB free. Its workspace `cargo test` and the editor's headless
    test (a full debug build of the editor) reported themselves skipped. This round's Rust is `bc2-cli` plus a
    manifest line, and it was checked by the clippy and cargo tests above.

### Timings (`rust\bench_cli.py`, processes, best of 5, OPFOR's real calls)

| job | Python | Rust | |
|---|---:|---:|---:|
| start-up alone (the usage) | 0.147 s | 0.011 s | x13 |
| `list startup-00.fbrb` | 0.140 s | 0.017 s | x8 |
| `build_menus.bat`: unpack + pack `mainmenu-00` | 3.44 s | 1.36 s | x2.5 |
| `merge_startup.ps1`: unpack + pack `startup-00` | 1.64 s | 0.32 s | x5 |
| `campaign_fill.ps1`: unpack sp_005's two bundles | 3.93 s | 4.61 s | (noise, see below) |
| `dbxedit` xml + roundtrip + check, one MP_005 partition | 0.70 s | 0.39 s | x1.8 |

**The noise.** campaign_fill's unpacks write about 5,000 files, and the PC was short of memory. Both sides run the
same `tool::run`. An A/B of the larger unpack, the order alternating, gave Python 11.8 / 2.8 / 24.7 / 10.8 s and
Rust 5.8 / 3.5 / 8.4 / 5.9 s: the disk sets that time, not the command line.

**The saving** is the 0.13-0.14 s of start-up per call, plus the module load. OPFOR's scripted calls make one
process per command, and the hub one per click.

### For Project OPFOR (not edited)

- **`merge_startup.ps1` / `campaign_fill.ps1`:** run the binary when `-Tool` names it, else Python:
  `function Fbrb { if ($Tool -like '*.exe') { & $Tool fbrbtool @args } else { & python $Tool @args } }`.
  `import_onslaught.bat` then passes `-Tool "%OPFOR_TOOLKIT%\bfbc2-tool-suite\rust\target\release\bfbc2.exe"` when
  that exists.
- **`build_menus.bat`:**
  `if exist "%SUITE%\rust\target\release\bfbc2.exe" (set FBRBTOOL="%SUITE%\rust\target\release\bfbc2.exe" fbrbtool)`
  before its Python fallback.
- **`opforcore.util.dbx_run`** stays in process (no process to save).
- **Its WW2 / Onslaught `subprocess` calls of aptcheck, aptgraft, culltree, renderqa and shaderdb** can switch the
  same way when their subcommands land (O2-O5). `bfbc2core.native.cli_command(tool)` gives the right command line,
  falling back to Python.

## F3: the rest of the Python format readers and writers (2026-11-18, branch `rust-f3`)

**Provisional timings:** other agents' builds and tests ran beside every figure (below-normal priority); ratios hold
within a run.

Like F0 and F2a this round **ports**: each Python function stays as its reference and fallback (`*_reference`, the
module's `use_native(False)`, `BFBC2_PURE_PYTHON=1`) until a full test tier retires it. A binding answers `None`
where the reference raises, a value is a NaN (`struct`'s float conversion of a NaN is the interpreter's to define),
or a value is one the port does not read exactly as the reference does (a sequence that is not a list / tuple, a
`bytes`-like that is not `bytes`, a number of another type, an array of another dtype or shape); the Python then
runs, so a result is always the reference's and every error message stays its own.

### Inventory: the Python format code and its Rust (after F2a, `f61e970`)

| Python | Rust | round |
|---|---|---|
| `aimanim.parse` / `write` / `to_pc` (generated aim poses) | `bc2_formats::aimanim` (`aimanim_parse`, `aimanim_write`, `aimanim_to_pc`) | **F3** |
| `hpdtool.mesh_layout` / `mesh_coverage` (the runtime mesh's byte layout) | `bc2_formats::hpdlayout` (`hpd_mesh_layout`, `hpd_mesh_coverage`) | **F3** |
| `audio.xma_parts`, `xma2_riff`, `_xas_stream`'s blocks | `bc2_formats::audioframe` (`audio_xma_parts`, `audio_xma2_riff`, `audio_xas_blocks`) | **F3** |
| `navsample.load` version 3 (NumPy pass) | `bc2_navbuild::navsample::V3` (`navkit_navsample_v3`: the arrays filled in Python's own buffers) | **F3** |
| `navsample_offline.write`'s bytes (now `file_bytes`) | `bc2_navbuild::navsample::write_v3` (`navkit_navsample_write`) | **F3** |
| `hkpack.Packfile` (sections, fixups, objects, classes) | `bc2_formats::havok::Packfile` (F1a's reader) + `pointer_fixups` (`hkpack_parse`) | **F3** |
| `xenos.disasm`'s listing (labels, CFs, instructions) | `bc2_shader::disasm` (`xenos_disasm_body`) | **F3** |
| `navsample.load` versions 1 / 2 | `navsample::parse_runs` | E3c |
| APT: `apt.py`'s reader, `aptgraft`'s model and writer, aptcheck / aptrefs | `apt`, `aptmodel`, `aptscan` | E3n, E4c |
| `kymesh.parse_mesh` / `write_mesh` | `kymesh` | E3o, F0 |
| `xenos.parse_shader` (container, microcode) | `bc2_shader::xenos` | E3f |
| `audio` Wave / SPS / SNR / SNS; EA-XAS codec | `wave`; `bc2_convert::audio` | F0; E3a |
| hpd, loc, BIGF, STFS, XISO, MeshData / mesh sets, texture helpers, `havok.classes` | F0's | F0 |
| soot, `havok.convert_wrapper`, console terrain, mesh relayout / decode, texwrite, skinmesh, Granny writing | F2a's | F2a |
| Granny reader, DBX (`dbx.py`), FBRB, cull trees, terrain, water, collision | | E3i, F1a (Python retired) |
| `dbxbin.py` | `dbx::edit::EditDbx` | E4a |
| `menucommon.PEImage`, `xmlinst`, `navtune` | | F2b |

**Not ported (and why):** `xenos.disasm`'s header comment lines and `disasm_instruction` (one instruction of a tree
in Python; the header is read from the tree the caller holds, which may have been edited); `xenos_exec.py` (the
differential test's interpreter: the independent check of the translation, not a format); `audio.segment_stream`,
`decode*`, the ffmpeg glue (dispatch over Rust readers); `hkpack.read_object` / `read_value` (the reflection dump of
the CLI; BFBC2's 5.5 packfiles carry no class descriptions) and `navsample_offline._shape_parts` / `_hull_faces`
(NumPy geometry over `hkpack`); `meshview._values` / `load_mesh` (NumPy views over F0's MeshData reader);
`texqa.read_pc` / `read_xbox` (QA statistics over the Rust texture helpers); `culltree.Doc`'s string table;
`aimanim.renumbered` (a dict operation; `to_pc` renumbers in Rust); `hpdview.py` (a viewer); `shaderdb.py`'s reader
and writer (a later round, by the brief).

### Parity (`tests/test_f3_native.py`; quick and `--game`, both ways in one process and against the goldens)

The goldens (`tests/f3_golden_<area>.json`, 1-31 KB each) hold, per source file (keyed by its path and size) or quick
set, a digest of the reference's every answer in order: recorded from the Python (`--record`), then the Rust's
answers checked against them (a changed source counted, not compared). Real inputs handed back to the reference fail
the test; every one below was answered in Rust.

| area | real inputs (`--game`) | result |
|---|---|---|
| aimanim | every AimAnimation of the game copy (PC), `work\xbox`, `work\xbox_dlc` and BF1943, parse / write / `to_pc` (BF1943's with the importer's own soldier bone map, `granny.bone_map`, and without) | 9,875 identical, 0 different; quick 1,500 damaged and 1,500 changed dicts the same result or error |
| `mesh_layout` / `mesh_coverage` | every sector of the 144 path-data files of the game copy and `work` (8,858 sectors) and a damaged copy of each | 40,032 identical, 0 different |
| audio framing | 6,497 EA-XMA streams (BFBC2 Xbox, Xbox DLC, BF1943: parts and each part's RIFF) and 8,627 EA-XAS streams (the game copy's in-memory PC sounds, re-cut into DICE's blocks); OPFOR's call: `audio.convert` of 125 BF1943 Waves (the WW2 install's sound port, through ffmpeg) with the Rust framing and the Python framing | 24,055 identical, 0 different; 125 of 125 converted Waves byte-identical |
| navsample | 173 sample runs of `work\navsample` / `work\navsample_offline` (24 GB): every version-3 record (1,001,855,046 surfaces, 2,141,076,036 columns) through both readers and both writers in 823 column ranges of at most ~1.5 M records (each a file of its own), the version-3 runs up to 120 MB also read whole (88 runs whole, version 1 / 2 ones up to 40 MB: E3c's reader) | 3,234 identical, 0 different (every real input answered in Rust) |
| `hkpack.Packfile` | every HavokPhysicsData of the game copy and `work\ww2`, every `*_havok.res` of `work\levels` (NavKit's offline sampler reads them): 9,866 | 10,675 identical (with the quick tier's), 0 different |
| `xenos.disasm` | every program of BF1943's and BFBC2's Xbox shader databases and containers (8,320 distinct of 63,305), with and without the raw words and the header | 35,681 identical, 0 different; 34,788 answered in Rust (the rest: damaged quick blobs the decoder refuses, and the tree whose microcode was read) |

Quick (`run_tests.py`, about 2 s an area): 400 built aim animations in both byte orders with eight kinds of bone maps,
1,500 damaged, 1,500 changed dicts; 300 seeded runtime meshes (E3o's generator) and 2,000 damaged, 3 game files;
600 built EA-XMA streams, damaged blocks, 600 RIFF argument sets and XAS framings (bad rates, counts, types); 300
built sample runs written and read back (and damaged), 300 changed ones (float64 heights, NaN normals, columns over
255, header values past float32 / int32, a broadcast hit row, int32 flags); 8 MP_005 packfiles and 800 damaged; 300
seeded Xenos programs and their damaged copies. All identical; the inputs handed back are damaged or deliberately
off-type.

### Timings (`--game`, whole corpora, both in one process; the calls timed, not the reading of bundles; the recording run, the verifying runs within 20 %)

| workload | Python | Rust | |
|---|---:|---:|---:|
| `xenos.parse_shader` + `disasm`, 8,320 programs x 4 | 62.6 s | **13.1 s** | 4.8x |
| navsample load + write, every record of the 173 runs | 136.4 s | **104.1 s** | 1.3x (the writer's array copies in the shim; the version-3 read alone fills Python's buffers once) |
| `mesh_layout` + `mesh_coverage`, 8,858 sectors x 2 | 15.9 s | **9.1 s** | 1.7x (the result is a Python structure of every tile item) |
| aimanim parse / write / `to_pc` | 5.1 s | **3.7 s** | 1.4x |
| `hkpack.Packfile`, 9,866 packfiles | 5.8 s | 5.7 s | 1.0x: the Python dicts (`ptr`, `by_offset`, `classes`) are built from the Rust lists; their building is the cost |
| audio framing, 15,124 streams | 1.7 s | 1.5 s | the work is a few slices per stream |

Peak working set of the navsample game check: 778 MB (streamed: one column range in memory at a time; the first
version of the check, holding whole runs, reached 3.4 GB - the reference's version-3 pass holds the file, its records
and float64 normals, about 3.5x the file, and version 1 / 2's loop about 15x; the Rust fills the arrays in place: the
file plus the arrays).

### What remains

- The `*_reference` functions above, until a full tier retires them.
- `hkpack.Packfile` keeps Python dicts; a handle with `deref` / `by_offset` lookups in Rust would make it fast, with
  `navsample_offline.asset_bodies`' walk ported over it (its hull faces are NumPy geometry).
- `xenos.disasm`'s header lines (Python reads them from the caller's tree).

## S1: streamed packing, process-wide BF1943 restart points, the shader coverage index (2026-11-18, branch `rust-s1`)

Three suite changes from profiling OPFOR's WW2 install plan (`wake_install.py --plan`, 602 -> 321 s after OPFOR's own
fixes c3503fc / 58cf539). Every output is the same; what changes is memory and repeated work.

### 1. Bundle packing streamed (`bc2_formats::fbrb`, `zlib`)

Before: `pack_rows` read every file of the folder into one payload `Vec` (on all cores), deflated it whole into a
second `Vec` and copied that into the bundle - a WW2 level's 650-680 MB payload, its 186-341 MB compressed copy and the
copy again. Now the layout (offsets, padding, the header) comes from the files' sizes; the files are read in order on a
second thread a megabyte at a time (4 chunks ahead) and fed through the deflater into the output as it is made.

- `zlib::deflate_stream` / `gzip_net_stream` (new): the zlib 1.3.1 port reading from any `Read`, writing to any
  `Write` every 256 KB of output. The window is filled exactly as from one slice (a short read is asked again; only
  the input's end gives less), so the bytes are the slice's: `deflate` / `gzip_net` are now these on a slice.
- `fbrb::pack_rows_into` (new): a bundle to any writer. `pack_path` / `pack_many` / `fbrb.pack` / FbrbTool's `pack`
  stream into their temporary file; `pack_rows` / `pack_dir` write into a `Vec` (the bundle held, not the payload).
  A file that ends early or grows is the same error as before, naming it; a failed pack leaves no temporary file.

Proof:

| check | result |
|---|---|
| OPFOR's real WW2 pack calls replayed: each level's `pack_many` as `pack_level` made it (every staging folder of `work\ww2\install*`, read only, unchanged since the real run) into a scratch folder | **23 of 23 bundles byte-identical** to the bundles the real run's packer wrote (Wake's 8, Guadalcanal's, Iwo Jima's and Coral Sea's 5 each; 4 levels' `level-00` 186-341 MB) |
| `test_fbrb_pack_native.py --game` (every game bundle FbrbTool.exe's recording covers, up to 300 MiB) | **1,081 of 1,081 recorded bundles (6.83 GB) byte-identical** to FbrbTool.exe's (1,078 in the run; 3 that a full drive C: stopped mid-write, repacked alone: identical); 5 not recorded (over 300 MiB). Pack 4,042 s on one process |
| quick tier: zlib against Python's zlib (levels 4-9), 18 built folders in every layout against FbrbTool's goldens, the refusals, 8 small game bundles; FbrbTool's commands in process (228 command lines) | all pass |
| `streams_as_one_slice`, `streamed_files_as_from_memory` (new Rust tests): readers giving 1, 7, 4096 bytes a call and interrupted ones, chunk boundaries across entries, every layout, files that shrink or grow, a failing output | pass |

Memory (peak working set of the process, measured with `K32GetProcessMemoryInfo`):

| workload | before | after |
|---|---:|---:|
| the four WW2 levels' 23 bundles, one `pack_many` per level (`pack_proof.py`) | payload + compressed copy + its copy: about 1.35 GB for Wake's or Iwo Jima's `level-00` alone (680 MB + 2 x 341 MB; not re-measured: the old packer's run waits for the user's go-ahead) | **47 MB** for all four levels |

Time: Wake's 8 bundles 35.4 s, each other level's 5 bundles 22-24 s (one core deflates each bundle, as before; the
reads overlap the deflating instead of preceding it). With OPFOR's `PACK_LATER`, packing a level beside the next
level's build no longer adds the level's payload to the plan's peak (58cf539 measured 2.9 -> 3.5 GB for it).

### 2. BF1943 restart points for the process (`bf1943import.RESTARTS`)

`Source._points` (one decompression of a compressed bundle, a restart point every MB) is kept per process, keyed by
the bundle's path, size, modification time, payload place and the spacing; least recently used out beyond
`BF1943_RESTARTS_MB` (default 128; a point costed at 40 KB: 40,113 bytes by tracemalloc); a lock around the table, the
states only ever copied. Every read still checks the file unchanged first. `Source.stats['shared_points']` counts the
points a Source found made.

| check | result |
|---|---|
| every entry of every one of BF1943's 82 bundles (81 compressed; also the copies a name's first bundle shadows), the committed module against the new one | **29,465 of 29,465 entries identical** (906 MB) |
| the entries by name (14,027) | identical |
| a fresh `Source` per call, one entry from each of the 49 compressed bundles that win a name, three Sources in turn | old 3.00 / 2.98 / 3.27 s; new 2.93 / **0.20 / 0.21 s** |
| memory held for every bundle's points | 81 entries, 30 MB (as costed) |
| quick tests: a second Source reuses the points, `BF1943_RESTARTS_MB=0` keeps none, a budget of one point keeps within it, four Sources reading on four threads | pass |

OPFOR's plan opened a Source per level and step (about 12): about 36 s of repeated decompressing, which OPFOR's
`bf_source()` avoided for wake_install only; every other caller (ww2_level_build, the weapon and object builds) now
gets it too.

### 3. The shader coverage index (`shaderdb.level_cover_prefixes`, `level_coverage`, `donor_levels`; cache `shadercover`)

For each level database, every leading `/`-joined part of each lower-case entry name with the number of entries
under it: `coverage(db, meshes)` is `{m: prefixes.get(m, 0)}` for any meshes, so choosing donor levels needs no
database loaded. One pickle per level bundle and Dx in `%LOCALAPPDATA%\bfbc2-tool-suite\cache\shadercover`, keyed
by `COVER_VERSION` and the bundle's path, size and time (hashed); pruned and listed by `python -m bfbc2core.caches`
(cap `BFBC2_SHADERCOVER_CACHE_MB`, 64). A level without a database of the Dx keeps its error.

| check | result |
|---|---|
| every level of the game copy: each prefix of each entry name (and 200 of every other level's) answered as `coverage` answers it on the loaded database (`test_shaderdb.py --game`) | **89 levels, 406,242 names: every answer identical**; a second pass read only the index (1.19 s; loading all 1.4 s with the fbrb payload cache warm). (This test's older ONS_MP_008 flag checks fail on main as well: the game copy's ons_mp_008 now has flag entries) |
| the four WW2 levels (`shader_proof.py`): the stock levels (79) and the meshes their staged level draws that the base database lacks (81-82), the old loop (every stock Dx9 database loaded, `coverage`) against `donor_levels` | **identical donors** for all four (mp_002, nam_mp_002cq, sp_01) |
| each level's Dx9 / 10 / 11 database merged from either choice (`S.merge`) | **12 of 12 byte-identical** (56 entries added each) |

| time per WW2 level | old loop | index |
|---|---:|---:|
| first level (index built: 79 databases loaded) | 2.66 s | 2.83 s |
| the next three | 1.07-1.46 s (bundles in the fbrb payload cache) | **0.11-0.13 s** (79 pickles, 27 MB) |

The index is 27 MB for every stock level's Dx9 database.

### OPFOR changes these allow (not made here)

- `wake_install.shader_dbs`: the donor loop (`for lv, read in S.load_levels(stock, [9], cc.OPFOR): ...`) becomes
  `donors = S.donor_levels(stock, need, opfor=cc.OPFOR)`.
- `wake_install.bf_source()` can stay; it is no longer needed for speed (a plain `Source(...)` shares the points).
- `PACK_LATER`'s comment on the packing thread holding a level in memory no longer holds.

## D1: the disk budget (2026-11-19)

Drive C: filled up repeatedly on 2026-11-18. Measured on the shared PC (other agents building; "free" is the drive's).

**Rust build folders** (`native.target_dir`: `rust-targets\<checkout>`; README.md, "Where Rust builds").
- `build_native.py` (bc2-py + bfbc2.exe, release, `-j 4`): 0.36 GB, 134 s from nothing. Plus `--editor`
  (bfbc2-editor.exe, mapexport.exe): 1.35 GB in all, 270 s. Nothing is written into the checkout but `bin\` (three
  exes, 23 MB).
- The quick tier from nothing (workspace clippy + tests + the editor's tests, debug, `CARGO_INCREMENTAL=0`,
  `-j 4`): the folder peaked at 15.1 GB (13.7 GB of it `debug\`), 975 s for the tier at `-j 2`. That is the cost of
  a worktree's first quick tier; the old layout kept it inside every worktree for good (the main checkout's
  `rust\target\debug`: 47.8 GB). Deleting `debug\` after a round (the folder is outside the worktree now) frees it.
- sccache: not installed on the PC, and the round installs nothing; not measured. The per-checkout folders already
  avoid the locking a shared folder brings. Sharing dependency compiles between checkouts would save about 1 GB of
  release and most of a debug build per worktree; worth measuring once sccache is installed by the user.

**Scratch** (OPFOR `console_common.drop_scratch`, `--keep`): each console stage's `work\console_<stage>\<lv>`, combat
A / B's and sector offsets' working files deleted at the step's end. `console_vehicles --plan --level ons_mp_002`:
peak 221 MB, left 0 (was 217 MB kept); `console_shaders`: peak 332 MB, left 0 (was 332 MB). Outputs (the kept tree
with `--keep`, sha256 of every file, and the printed plan) identical to the code before.

**Streaming** (a bundle's few entries read in memory with `fbrb.read` / `dbx.to_xml`, not a whole unpack kept):
- `console_deps`: the source bundles (`work\console_deps\src`, 4.46 GB for the four levels). ons_mp_002's plan
  355 -> 100 s; its 1,260 files and printed plan identical.
- `console_mode`: Conquest from mp_common (`work\console_mode\mp_common`, 0.63 GB); the level's 507 files identical,
  `Conquest.dbx` byte-identical to the unpack's.
- `apply_ai_template --plan`: the level partition (`work\combat_a\<lv>\plan`, 1.47 GB); the printed plan identical.

**Steady state** (Project OPFOR's `work\`, 112 GB, by kind with `caches --report`): 25.4 GB is scratch; of it about
21 GB goes as each stage next runs (the per-level folders, the three caches above), the rest is what the stages keep
on purpose (console_shaders\shared, console_loadset\mp2_common, combat_a's source unpacks). The caches: 20.0 GB,
now under one 20 GiB budget (it was 16 GiB for fbrb plus every other cap, about 28 GiB in all).

**Proof**: the WW2 rebuild against the oracle, 10,852 of 10,852 identical (1,737 s, the PC loaded); the WW2 install
plan (wake_install.py --plan) 389 s, work\ww2 peak 9.86 GB, unchanged by the round (its code is not touched).

## O2: the menu, crash-reader and NavKit / converter command lines (2026-11-19, branch `rust-o2`)

The second batch of O1's plan: `aptcheck`, `aptrefs`, `bc2loc` (with `menucommon`'s pieces), `exeref`,
`crashreader`, and the command lines of `hpdtool`, `hkpack`, the console converter's `convert.py` and `aimanim`. Each
is a subcommand of `bfbc2.exe` (shape A; `bc2loc` and `aimanim` stay importable libraries for OPFOR, so their shape C
is the module itself plus this command line). Each `.py` stays as the reference and the fallback behind
`bfbc2core.native.cli_command` (`BFBC2_CLI_PYTHON=1`).

### What was added

- **`bc2-cli/src/pyproc.rs`: a Python command line's process.** O1's tools print once at the end; these print as they
  go, log through `errors.setup` and report through `errors.guard`. So each write is modelled as Python 3.13 makes it:
  - `\n` as `\r\n`, the ANSI code page off a console (stdout strict: a `UnicodeEncodeError` at that write, reported as
    the guard reports it; `errors='replace'` after `bc2loc`'s `reconfigure`), UTF-8 on a console, stderr
    `backslashreplace`;
  - the text layer's 8,192-byte pending chunk and `BufferedWriter`'s 8,192 bytes, so stdout reaches a pipe in
    Python's chunks and at Python's moments. Measured on 3.13: a write that takes the pending bytes past the chunk
    goes out together with them. stderr is line-buffered. An uncaught exception flushes stdout first (`flush_io`); a
    guarded one does not (the report, then stdout at exit). `PYTHONUNBUFFERED` writes through;
  - the day log: `<tool>-<date>.log` as `caches.day_log` picks it, the start line, an error's record;
  - `errors.explain`'s causes and fixes, word for word.
- **The hand-off.** Sometimes the Rust core refuses: the Python binding would return None or raise
  `NotImplementedError`. Sometimes the Python's answer is an exception text only Python makes: `struct.error`, expat's
  `ParseError`, pefile's errors. In both cases the run returns `Fail::Python`. Nothing has been printed or logged yet,
  so `bfbc2.exe` runs `python <the tool's .py> <arguments>` with its stdio and returns its exit code.
  - The interpreter is `BFBC2_PYTHON`, else `python.exe` on `PATH`; the suite folder is the one above the binary.
  - `BFBC2_PURE_PYTHON=1` hands every line off.
  - `BFBC2_CLI_TRACE=<file>` records who answered each run.
- **`pyfmt.rs`**: Python's text forms of values:
  - `repr()` of floats and strs (the unprintable ranges are generated from Python 3.13.15's unicodedata,
    `pyunprintable.rs`);
  - `format(x, 'g')`, `round(x, n)` and `{:,}`;
  - a module's `__doc__` read from its `.py` (`include_str!`), so the usages cannot drift.
- **The groups** `menu`, `crash` and `nav` are cargo features, all on by default. `bc2-cli` now has 7,722 lines of
  Rust, O1's included.
- **Callers.** `cli_command` knows the nine tools, and the hub's crash reader runs through it.

### Proof

The method is O1's (`core/tests/test_cli_o2.py`, cases in `cli_o2_menu.py`, `cli_o2_crash.py`, `cli_o2_nav.py`):

- **Recorded first.** Each group's golden was recorded from the Python command lines before that tool's Rust existed.
  The recording ran in a frozen copy of the checkout (`--ref`), whose Rust core module matched its sources while this
  branch's Rust changed. The goldens are 0.71, 0.72 and 0.61 MB.
- **Quick inputs are synthetic** (no EA content):
  - menu movies, bundles, localization files and PE files built in code;
  - 32-bit PE files with code, crash reports, symbols and logs;
  - path data, Havok packfiles, PE files with hkClass initializers, console textures and unpacked folders;
  - a fake game copy, through `OPFOR_ROOT`.
- **Each line** is compared on exit code, both streams' bytes, the case folder's files and the log records. The cases
  include every line on one pipe, and the encoding and long-output cases under `PYTHONUTF8`, `PYTHONUNBUFFERED` and
  `PYTHONIOENCODING`.

**Who answered** (the final run, after merging main: this checkout's own freshly built module and `bin\bfbc2.exe`):

| tool | recorded lines | Rust | handed to Python |
|---|---:|---:|---:|
| aptcheck | 174 | 166 | 8 |
| aptrefs | 82 | 82 | 0 |
| bc2loc | 116 | 114 | 2 |
| exeref | 138 | 128 | 10 |
| crashreader | 120 | 112 | 8 |
| hpdtool | 104 | 84 | 20 |
| hkpack | 64 | 60 | 4 |
| convert | 110 | 106 | 4 |
| aimanim | 58 | 40 | 18 |
| **all** | **966** | **892** | **74** |

- **Rust: 966 of 966 as recorded.** Every hand-off is a damaged or off-type input, or an instruction the decoder
  refuses (`--who` lists them):
  - menu: `struct.error` texts of cut constant files, BIGF indexes and histograms; two movies the decoder refuses;
  - crash: random code the x86 decoder refuses (capstone answers), XML only expat reads, pefile's errors, symbols
    that are not UTF-8;
  - nav: cut, NaN or random path data, cut packfiles, wrong-endian or poseless aim files, a non-game exe, PC
    physics files given as console ones.
- **`--live`, both ways:** the Python gave the same 966 answers again. Python took 172 s and Rust 35 s of process
  time (4 workers).
- **One case was dropped** after the recording: `convert havok` with no `--exe` and no `OPFOR_ROOT` exe. Then
  `default_exe` looks beside the suite, so the answer depends on where the checkout sits. The frozen copy found no
  game. This checkout sits beside `project-opfor-bfbc2` and found the real exe. Both sides agreed live, and the
  exe-from-`OPFOR_ROOT` case stays.

**OPFOR's real calls, replayed read-only** (`--game`): inputs are copied to the case folder with their times, the game
copy is read in place, and every output goes to the case folder. Each line runs alone and on one pipe, on both sides:

| group | lines | runs | Rust answered | Python | Rust |
|---|---:|---:|---:|---:|---:|
| menu | 42 | 168 | 42 of 42 | 157.3 s | 15.9 s |
| crash | 26 | 104 | 26 of 26 | 13.0 s | 4.4 s |
| nav | 60 | 240 | 60 of 60 | 32.3 s | 6.4 s |

All 128 lines were identical, with no hand-offs. They were:

- **menu:**
  - `aptcheck check` of 19 movies that OPFOR's WW2 / Onslaught steps edited (copied out of `work\`);
  - `verify pristine:main_nav`, and `check` of `main_nav` / `main_root` and their pristine copies;
  - `lint` of OPFOR's 12 `src\menus\*.patch` (plain, `--brief`, `--installed`);
  - `aptrefs table` / `movie` / `find` / `xref`;
  - `bc2loc langs`, `get` (English, Russian, Japanese), `search`, `movie`, `dump`, `set --lang all`.
- **crash:**
  - `testrun.py`'s exact line (`<report> --exe <game exe> --log <opfor.log>`) on the two real crash reports in
    Documents, with the 349 MB opfor.log read in place;
  - the same reports with the log found beside the exe, with `--no-code`, and the newest report found;
  - six reports at real crash sites from `symbols_795745.txt`;
  - 13 `exeref str` / `refs` / `dis` lines on BFBC2Game.exe.
- **nav:**
  - `hpdtool` `info`, `sector`, `roundtrip`, `coverage`, `meshtrip` on NavKit's sp_009, ons_mp_002 and sp_01 path
    data;
  - `hkpack` `classes` and dumps of MP_005's four Havok assets;
  - `aimanim` on 6 BFBC2 aim poses;
  - `convert`: `havok` x4 and `texture` x4 from Onslaught MP_002's console level with the real exe, and
    `verify-havok` / `verify-texture` on copied subsets and on whole folders.

**Recorded differences, by design.** None occurs in the recorded lines or in OPFOR's calls.

- **The log.** Its start line names `bfbc2.exe` where Python names its version. Under an error record, Python writes
  its own traceback and `bfbc2.exe` one line naming the exception. The test compares each record's first line.
- **Log pruning.** `bfbc2.exe` does not prune the log folder at start-up (`caches.prune_logs`, with its archive).
  Every Python tool still does. Porting it is O3's (caches).
- **`bc2loc`'s key names.** Where two candidate keys hash to the same entry, Rust takes them in sorted order and
  Python in its randomized set order. Only a true 32-bit collision could differ.
- **Handed to the Python:**
  - `aptcheck lint` with wildcards before the file name;
  - `convert`'s argparse help and errors when `COLUMNS` is set or stdout is a console;
  - option abbreviations and `--`.
- **`crashreader --symbols` as the last argument,** with stdout redirected to a disk file. Python stats and reads
  descriptor 1. Rust logs the same "not a file" warning, which is Python's answer on a pipe or a console.

**Checks:**

- `cargo fmt`; `cargo clippy -p bc2-cli --all-targets -D warnings` with all features and with each group alone;
  `cargo test -p bc2-cli` (10).
- ruff and mypy on the new and changed Python.
- `python run_tests.py quick -j 2` after merging main (E2b, R5c, D1): 117 of 121, 754 s. O2's own entry passed. None
  of the four failures is O2's:
  - `test_console_readers_native.py` and `test_dbxport_native.py` each failed within 2 s in the tier, then passed
    alone (8 of 8; all passed).
  - The workspace `cargo fmt / clippy / test` step and the editor's headless test need full debug builds of the
    workspace and the editor:
    - Before the merge, the workspace step died inside rustc (`STATUS_STACK_BUFFER_OVERRUN` compiling `windows`,
      the PC short of memory).
    - Rerun alone after the merge, it filled drive C:. The link failed with `LNK1140` at 0 MB free, and its 13 GB
      debug folder was deleted.
    - `bc2-cli`'s own checks above pass on the merged tree.

### Timings (processes, best of 5, OPFOR's real inputs)

| job | Python | Rust | |
|---|---:|---:|---:|
| start-up alone (`aptcheck` usage) | 0.162 s | 0.010 s | x16 |
| `aptcheck check main_nav` | 0.223 s | 0.058 s | x3.8 |
| `aptcheck lint` OPFOR's 12 patches `--brief` | 2.535 s | 0.831 s | x3.0 |
| `bc2loc get MENU_PLAY_NOW` | 0.163 s | 0.011 s | x15 |
| `bc2loc search browser` (keys named) | 0.534 s | 0.325 s | x1.6 |
| `aptrefs xref` (every movie set) | 0.820 s | 0.647 s | x1.3 |
| `crashreader`, testrun.py's line (real report, exe, 349 MB log) | 0.328 s | 0.137 s | x2.4 |
| `exeref str presenceRequestOnline --dis` | 0.252 s | 0.062 s | x4.0 |
| `exeref refs 0x006EFBC0` | 0.209 s | 0.029 s | x7.3 |
| `exeref dis 0x006EFF8A` | 0.223 s | 0.022 s | x10 |
| `hpdtool roundtrip` sp_01 (12 MB) | 0.334 s | 0.018 s | x18 |
| `hpdtool meshtrip` sp_01 | 0.544 s | 0.050 s | x11 |
| `hkpack classes` MP_005 StaticModelGroup | 0.180 s | 0.011 s | x17 |
| `convert havok` (one console physics resource) | 0.213 s | 0.029 s | x7 |
| `convert texture` (one console texture) | 0.184 s | 0.011 s | x16 |
| `convert verify-texture` (33 textures) | 2.143 s | 0.836 s | x2.6 |
| `aimanim` (one aim pose) | 0.149 s | 0.009 s | x17 |

Start-up is 0.13-0.16 s of Python against 0.01 s, so short jobs gain the most. The longer ones (`lint`, `xref`,
`verify-texture`) spend their time in the core, which was already Rust.

### For Project OPFOR (not edited)

- **aptcheck's `subprocess.run([sys.executable, ...aptcheck.py, 'check', ...])`** can become
  `bfbc2core.native.cli_command('aptcheck') + ['check', ...]`. Each call saves about 0.15 s, and the WW2 install makes
  many. The calls are:
  - `console_icons.py:306`
  - `menu_install.py:590` (`aptcheck()`, also `verify`)
  - `duplicates.py:114`
  - `type95_icon.py:201`
  - `wake_ui.py:79`
  - `weapon_icons.py:462`
  - `ww2_crosshairs.py:387`
- **`testrun.py`'s `crash_help`** can use `[<suite>\bin\bfbc2.exe, 'crashreader']` when the binary exists, else
  `[sys.executable, reader]`. Its arguments and `errors='replace'` capture stay as they are.
- **`bc2loc`, `hpdtool`, `aimanim`** are imported by OPFOR, not run, so nothing changes there.

### Left for O3

O1's O3 batch is unchanged: caches (prune and logs; then `bfbc2.exe` prunes too), stockmp with `bc2-stockmp`'s plan,
aptgraft, audio, dbxcatalogue / dbxparity / portparity, meshview's library, and culltree's `add`. O2 adds two items:

- **`menucommon`.** Its Python stays the library that `aptgraft` and OPFOR import. Its Rust twin is
  `bc2-cli/src/menu/common.rs`, which aptgraft's port can move into a library crate.
- **The hand-off's interpreter lookup** (`BFBC2_PYTHON`, else `PATH`). It matters once the binary ships without the
  suite's Python (the final release).

## O3: caches, stock-MP, aptgraft, audio, the BF1943 parity tools, meshview, culltree (2026-11-19, branch `rust-o3`)

The third batch of O1's plan. Each tool is a subcommand of `bfbc2.exe` (shape A; meshview's index build is also shape
B, behind its Python shim). Each `.py` stays as the reference and the fallback behind
`bfbc2core.native.cli_command` (`BFBC2_CLI_PYTHON=1`). The batch was ported in four helper worktrees and merged.

### What was added

- **Two library crates.**
  - `bc2-pyrt` holds what the command lines show of Python: exceptions and their texts (`err`), value texts (`fmt`),
    `ntpath`, `str` and `dict` pieces (`py`), and a pickle reader and writer.
  - `bc2-menu` holds menucommon's Rust twin (O2's `menu/common.rs`, moved) and aptgraft's grafts (`graft/`).
- **The log folder is pruned at start-up.** Every guarded tool `bfbc2.exe` runs now prunes the log folder as
  `errors.setup` does (`logs.rs`, `fsx.rs`). The order is Python's:
  - the start line is written;
  - the folder is pruned, the session's file kept;
  - the files the prune removes are zipped into `logs-archive\logs-<time>.zip` first.

  Here it happens when the run ends, because nothing is written before a run is known to be Rust's. The lock is
  shared with Python's (`msvcrt.locking` of byte 0 and Rust's whole-file lock exclude each other).
- **The overall cache budget runs after a bundle read** (`budget.rs`: `fbrb._budget` / `caches.maybe_prune_total`),
  for aptgraft's and meshview's bundle reads. `budget.rs` holds the one cache table; the caches command line uses it
  too.
- **The hand-off's Python** (`pyproc::python`):
  - `BFBC2_PYTHON` (a file, or a name on `PATH`; if it is set but not found, that is reported, never skipped);
  - else the suite's configured Python, `bin\python.txt` (`build_native.py` writes the interpreter it built with; a
    shipped suite names its own; relative paths are against the suite);
  - else `python.exe` on `PATH`.

  When none is found, the error names each place looked and what to do (exit code 1).
- **The test machinery** (`test_cli_o2.py`, reused by `test_cli_o3.py`):
  - `ROUND` / `GROUPS`, and `BFBC2_CLI_WORKERS`;
  - `lad_state`: the run's local app data compared (logs left by name, archives by their members, other files by
    bytes);
  - case files in the local app data (`{lad}/`) and folders with given times;
  - per-group `NORMS` / `LOOSE` / `LAD_SKIP`.
- **The subcommands:**

| tool | what answers in Rust | handed to the Python by design |
|---|---|---|
| `caches` | `--list`, `--prune`, `--clear` (caches, project caches, logs with the archive), the budget, `--report`, `--prune-targets [--yes]`, argparse at the pipe width | argparse's abbreviations, `--`, single-dash options, `--flag=value`, `COLUMNS` / console widths |
| `stockmp` | `install --plan / --apply / --revert`, `summary`, `generate --minimal`, the path-data error | the full `generate`, `build`, `validate`, `all` (DbxEdit and NavKit's path-data model: O4); damaged reports, partitions, non-finite path data |
| `aptgraft` | all nine subcommands, with the grafts in `bc2-menu` | none recorded |
| `audio` | the conversion (F0 / F3 framing, E3a encoder; ffmpeg's xma2 decoder run as the Python runs it) | `--` |
| `dbxcatalogue` | the summary from the catalogue and index caches | missing or stale catalogue cache (the Python builds it) |
| `dbxparity`, `portparity` | help and usage errors | every parsed run (bf1943import's `Source`, dbxport / meshport, NumPy PSNR: after S1) |
| `meshview` | `find`, from the index cache or building it (`bc2-formats::meshindex`, also behind `meshview.Index._build`) | drawing (NumPy, PIL); unreadable bundles, databases the Python leaves out |
| `culltree` | `info`, `verify`, `add` (`bc2_levelwrite::culltool`) | trees and levels whose fields the reference itself cannot read |

### Proof

The method is O2's. The goldens were recorded from a frozen copy of the checkout at 5aee34d (`--ref`) before the port
answered. Each line is compared on:

- the exit code;
- both streams' bytes, alone and on one pipe;
- the case folder's files;
- the log records;
- the local app data.

**Quick and live** (after the merge; this checkout's module and `bin\bfbc2.exe`):

| group | recorded lines | Rust | handed to Python | live: Python / Rust process time |
|---|---:|---:|---:|---|
| caches | 184 | 170 | 14 | 21 s / 4 s |
| stockmp | 236 | 214 | 22 | 370 s / 111 s |
| graft | 228 | 228 | 0 | 61 s / 5 s |
| audio | 182 | 180 | 2 | the same 182 |
| bf1943 (dbxcatalogue 30, dbxparity 36, portparity 36) | 102 | 70 | 32 | 33 s / 12 s |
| meshview | 98 | 88 | 10 | 31 s / 5 s |
| cull | 190 | 184 | 6 | 40 s / 8 s |
| **all** | **1,220** | **1,134** | **86** | |

- Rust: 1,220 of 1,220 as recorded. Live, both ways: the same answers.
- The caches group's 184 include 10 start-up prunes by aptgraft, stockmp and culltree over old logs. Each prune leaves
  the same files and the same archive members as Python's. The archive's members match; its compressed bytes are
  zlib-rs' rather than zlib's.
- O2's 966 recorded lines still pass after the merge.

**OPFOR's real calls, replayed read-only** (`--game`; each line alone and on one pipe, on both sides):

| group | lines | Rust answered | Python | Rust |
|---|---:|---:|---:|---:|
| caches (the real caches, project caches and work\ listed, the build root's orphans listed; a copy of the real 191 MB log folder listed, pruned, cleared, and pruned at a tool's start-up) | 10 | 10 | 47.1 s | 33.6 s |
| stockmp (install --plan / --apply / --revert of OPFOR's three builds into a game copy in the case folder, summary of six levels, generate --minimal of five) | 27 | 27 | 30.5 s | 20.9 s |
| graft (console_hud's mode-case / mode-alias on the four Onslaught levels, as .res and bundle:name, onslaught-timer x3, roundtrips, convert, deps) | 68 | 68 | 42.7 s | 6.6 s |
| audio (22 real Waves: BFBC2 Xbox, BF1943 voice-over and Wake effects) | 22 | 22 | | |
| meshview (find) / dbxcatalogue | 12 / 3 | 15 | 21.4 s | 10.9 s |
| cull (console_cull.py's add on the four Onslaught levels with every work\levels reference, info, verify) | 10 | 10 | 423.5 s | 87.5 s |

All identical, none handed to Python.

The caches group's `--report` lines run without the real build root. Another checkout's build (`-o4`) changed
`rust-targets\` between the two sides' runs and made one run differ. Run back to back by hand, the real `--report`
with the build root is identical (twice).

**Checks:**

- `cargo fmt --all`;
- `cargo clippy -D warnings` on `bc2-cli` with all features and with groups alone (caches, menu, crash; the helpers:
  each of theirs), and on the touched crates;
- `cargo test` of the touched crates;
- ruff and mypy on the touched Python;
- `python run_tests.py quick -j 2` (cargo hidden behind the round's build lock, `CARGO_INCREMENTAL=0`): 121 of 122,
  1,623 s. The one failure was O3's own entry, run with the checkout as its reference: from a checkout beside Project
  OPFOR the tools find it by walking up, so the caches' cases listed the real project caches, and the graft group's
  over-budget cases **pruned the real project caches in OPFOR's `work\`** (navstudio 1.3 GB, harness 2.2 GB, navwater
  332 MB of path-data loads; caches the tools rebuild on their next use, no output lost, but `work\` was written).
  `test_cli_o3.py` now runs from an isolated copy of the checkout in `%TEMP%` whenever its reference sits under a
  Project OPFOR copy (and refuses a `--ref` there). Rerun as the tier runs it: 1,220 of 1,220.

### Timings (processes, best of 5; `--report` and the log prune best of 3)

| job | Python | Rust | |
|---|---:|---:|---:|
| `caches` start-up (`--list hlsl`, empty) | 0.091 s | 0.008 s | x11.5 |
| `caches --list` (the real caches) | 0.116 s | 0.025 s | x4.6 |
| `caches --report --no-work` | 0.341 s | 0.219 s | x1.6 |
| `caches --report` (work\ by kind, 112 GB) | 13.7 s | 10.5 s | x1.3 |
| `caches --prune logs` (copy of the real 191 MB folder) | 1.85 s | 1.27 s | x1.4 |
| `stockmp summary mp_005`, warm | 0.229 s | 0.039 s | x5.9 |
| `stockmp generate mp_005 --minimal --variant exp0` | 0.426 s | 0.041 s | x10 |
| `stockmp install mp_005 --variant exp0 --plan` | 0.187 s | 0.062 s | x3.0 |
| `aptgraft` start-up | 0.185 s | 0.012 s | x15 |
| `aptgraft mode-alias hud_root` (bundle spec) | 0.397 s | 0.054 s | x7.4 |
| `aptgraft onslaught-timer` | 0.423 s | 0.045 s | x9.3 |
| `audio`, a BF1943 effect | 0.286 s | 0.057 s | x5.0 |
| `meshview find garand` | 0.413 s | 0.205 s | x2.0 |
| `dbxcatalogue` (cached) | 1.84 s | 0.90 s | x2.0 |
| meshview's index build on the game copy (behind the shim) | 6.1 s | 2.4 s | x2.5 |
| `culltree add`, console_cull's line on ons_mp_002 | 30.1 s | 5.9 s | x5.1 |
| `culltree verify` of its output | 29.3 s | 5.8 s | x5.0 |

### For Project OPFOR (not edited)

- **`console_cull.py:82`:** `[sys.executable, CULLTREE, 'add', ...]` can become
  `bfbc2core.native.cli_command('culltree') + ['add', ...]`. That saves about 24 s per level.
- **`console_hud.py` and the WW2 scripts that run aptgraft:** `mode-case`, `mode-alias` and `onslaught-timer` can use
  `cli_command('aptgraft') + [...]`. That saves 0.25-0.38 s a call.
- **The stock-MP docs and scripts:** `cli_command('stockmp') + ['install', 'mp_005', '--variant', 'exp0', ...]`.
- **meshview's ~55 imports, audio and aptgraft:** these are imported, not run, so nothing changes.
  `meshview.Index` builds through the Rust core already.

### Left for O4 / O5

- **stockmp:** the full `generate`, `build`, `validate` and `all` (NavKit's path-data model, DbxEdit, packing).
- **dbxparity / portparity's runs**, after S1 (bf1943import's `Source`, dbxport / meshport, NumPy's PSNR).
- **dbxcatalogue's cache build.** It is a pickled object graph of `dbx.Element`, `Counter` and frozensets in run
  order.
- **meshview's drawing** (NumPy, PIL): it goes to the editor (G).
- **Python bindings for the libraries OPFOR imports** (shape C's library side): aptgraft's grafts and caches'
  pruning.
- **O2's `aptcheck` / `aptrefs` bundle reads** (`pristine:` specs) still lack the budget run that Python's
  `fbrb.read` does. They can call `budget::after_bundle_read()` as aptgraft does.
- **culltree's ref-row cache.** Rust reads the references again every run; at 5 s a level there is no need for it.
## O4: NavKit's driver and tools, the editor's prepare, the test harness, texqa (2026-11-19, branch `rust-o4`)

The fourth batch of O1's plan, as `bfbc2.exe` subcommands (shape A) in four cargo feature groups of `rust/bc2-cli`
(each builds alone with `--no-default-features --features <group>`):

| group | subcommands (the Python they port) |
|---|---|
| `navkit` | `navkit` (`bfbc2-auto-navkit\navkit.py`), `prepare_level` (`rust\bc2-editor\prepare_level.py`), `levelobjects`, `ladders`, `ordercheck`, `navcompare` |
| `navtools` | `graphstats`, `hpdcheck`, `hpdview`, `detailed`, `navparity`, `navregress`, `sample_all` |
| `harness` | `harness` (`bfbc2-automated-map-and-bot-test-harness\harness.py`) |
| `texqa` | `texqa` (`bfbc2-console-convert\texqa.py`) |

Each `.py` stays the reference and the fallback behind `bfbc2core.native.cli_command` (`BFBC2_CLI_PYTHON=1`), with
O2's machinery (`pyproc`: Python's stdio, chunking, day logs, error reports, the hand-off).

### What was added

- **Shared pieces** (`bc2-cli/src`): `pyjson.rs` (`json.dumps` with and without `indent`, `json.loads` with Python's
  grammar, dict order and duplicate keys), `pyos.rs` (`ntpath` join / split / normpath / abspath / relpath, `glob`'s
  order and hidden-name rule, `os.walk`, `atomicio`'s writes, SHA-256 - on the CPU's SHA extensions when it has them,
  as `hashlib`'s OpenSSL runs it, else the portable rounds; the two are compared in the tests), and
  `Py::flush_now` in `pyproc.rs`: `print(..., flush=True)` for a tool whose reader follows its progress (after it the
  run cannot hand off).
- **navkit:** the steps whose core is Rust: `levels`, `prepare` (the common and the level's bundles unpacked and
  converted in process, stamped by SHA-256; `levelobjects`' JSON and `.npy` triangles with NumPy's float64 matmul and
  float32 comparisons; `ladders`; the sampling box and sector offsets), `compare`, `check` (ordercheck as its own
  process, as navkit.py runs it), `install`, `restore`. Handed to navkit.py: `build`, `apply`, `all` (navbuild's
  Python stages), `sample` (it starts the game), `status` (`time.ctime`) and `--no-detail`. Under
  `PYTHONUNBUFFERED` (navkitGUI) a prepare's lines leave as they come.
- **prepare_level:** NavKit's prepare and the terrain unpack (`level.heightfield_of`: the lock file, the stamp, the
  height field read), `@step` / `@log` / `@error` / `@done` flushed line by line for the editor's progress view.
  **The editor** (`bc2-editor/src/prepare.rs`) now runs `<suite>\bin\bfbc2.exe prepare_level` when that binary
  answers the command (asked once), else the script; `BFBC2_CLI_PYTHON=1` forces the script.
- **Callers:** the hub's Levels tab (`prepare`), navkitGUI's steps and the Navigation Studio's apply / install /
  restore run navkit through `cli_command`.
- **navtools:** graphstats, hpdcheck and `navparity show` whole (navparity's mean row adds as Python 3.12's `sum()`
  does, Neumaier-compensated: a plain sum printed 99.1 where Python prints 99.2); navregress, detailed, sample_all
  and hpdview answer their argument and setup errors and hand the work to the Python.
- **harness:** `show`, `list`, `compare`, `import-log`, `ingame`; `run` is handed off. The harness never starts the
  game on either side.
- **texqa:** decoding, the per-texture measures (NumPy's pairwise sums, `mean(0)` / `std(0)` order, SSIM windows,
  `np.round`), flags, score, report, cache, baselines.

### Proof (O2's method: `core/tests/test_cli_o4.py`, cases in `cli_o4_<group>.py`; `test_prepare_o4.py`)

Goldens were recorded from the Python first, in a frozen copy of 4710a60 with its matching core module
(`BFBC2_O2_REF`), from synthetic inputs (no EA content): 0.12, 0.16, 0.11 and 0.04 MB.

| tool | recorded lines | Rust | handed to Python | `--game`: real calls (Rust / Python) |
|---|---:|---:|---:|---|
| navkit | 110 | 104 | 6 | 13 (13 / 0) |
| prepare_level | 22 | 22 | 0 | (test_prepare_o4, below) |
| levelobjects | 18 | 18 | 0 | (inside prepare) |
| ladders | 6 | 6 | 0 | (inside prepare) |
| ordercheck | 30 | 28 | 2 | 6 (6 / 0) |
| navcompare | 16 | 16 | 0 | 6 (6 / 0) |
| graphstats | 32 | 30 | 2 | all Rust |
| hpdcheck | 58 | 54 | 4 | all Rust (DICE's 15 files, 12 regress builds) |
| hpdview | 14 | 12 | 2 | - |
| detailed | 24 | 20 | 4 | 1 (0 / 1) |
| navparity | 50 | 44 | 6 | all Rust (`show` of every real run) |
| navregress | 26 | 26 | 0 | all Rust (no-reference answer) |
| sample_all | 18 | 18 | 0 | - |
| harness | 118 | 112 | 6 | 36 (36 / 0) |
| texqa | 80 | 62 | 18 | 4 (3 / 1) |
| **all** | **622** | **572** | **50** | **187 lines, each alone and on one pipe, both ways: all identical** |

- **Quick: 622 of 622 as recorded** (618 before the harness's sorted compare, below).
- **The harness's `compare`, sorted (approved by the user, 2026-11-19):** `testharness/runs.compare` listed its four
  set differences (tests and levels missing from or new in a run) in Python's set order, which follows str hashes
  randomised per process, so the same two runs printed those findings in a different order each time. Python and
  Rust now both list them sorted by level, then scope, then test (`compare_report` keeps its order of kinds). Nothing
  else reads that order: `harness.py` prints it, and test_harness checks kinds and counts only (its suite passes);
  OPFOR has no reader. Recorded again from the updated Python: the compare lines (two levels missing / new, under two
  hash seeds) and the 12 real compares of the 7 OPFOR runs (both directions) are all Rust's now.
- **Earlier quick figure: 618 of 618 as recorded. `--live` both ways: the same answers** (process time: navkit Python 92 s /
  Rust 13 s; navtools 111 / 17 s; harness 56 / 8 s; texqa 53 / 21 s).
- **Cases that depend on where the checkout sits are not recorded** (O2's `convert havok` lesson): the first quick
  tier from this checkout (beside Project OPFOR and bf1943-tool-suite) answered three of them differently from the
  frozen copy that recorded them (prepare_level's `--opfor` without a folder walked up to the real OPFOR; texqa's
  BF1943 folder and DICE copies found the sibling projects). prepare_level's cases now name the fake folder in
  `OPFOR_ROOT`; the two texqa hand-off cases were dropped. Recorded again, then 618 of 618 from this checkout too.
- **`--game`** replays OPFOR's real calls read-only: navkit `levels` of the real folder; `compare` / `check` /
  `install` / `restore` in a fake OPFOR folder holding copies of a prepared level's files (DICE's path data, NavKit's
  build, the partition, `navkit.json`) for sp_009, sp_004_b, ons_mp_002, sp_01; ordercheck and navcompare on the
  real partitions and path data in place; hpdcheck of DICE's 15 files (run_tests' line); harness list / show /
  compare of the 7 real runs (copied) and import-log of the real 349 MB opfor.log (read in place); texqa over
  `work\ww2\bf1943_vehicles` and `bf1943_jp_bazooka` (outputs in the case folder).
- **The level prepare, byte for byte** (`test_prepare_o4.py --game`): a stand-in OPFOR folder (`game\` a junction to
  the real game copy, its own `work\` and LOCALAPPDATA, the same path for both sides), `prepare_level` of sp_009,
  sp_004_b and mp_005 in turn (the common bundles unpacked by the first), each again (all already unpacked), each
  with `--terrain-only`: **9 runs, the output and exit codes identical, 56,831 files under `work\` identical by
  sha256** (the unpacks and their XML, the `.source.sha256` stamps, `objects.json`, `group_instances.json`, spawn and
  order points, the five triangle `.npy` files, `ladders.json`, `navkit.json`, the terrain unpacks with their stamps
  and lock files), **the day logs' records identical** (fbrbtool / dbxedit). Rust answered all 9. The junction is
  removed with `rmdir` before the folder. (A first run in the long scratch folder failed both ways identically on
  FbrbTool's 260-character check; the test now runs under a short root.)
- **Who answered, real inputs:** every real navkit-group call is Rust's. The real lines still handed to the Python
  are detailed's sector choice and texqa's bench_sweep line (BF1943 sources, DICE's copies, contact sheets).

**Recorded differences, by design** (none occurs in the recorded lines or OPFOR's calls):

- prepare_level: past its first progress line the run cannot be handed to the Python (its lines are out); an input
  the port refuses there (a NaN collision vertex, an unreadable XML file the core's scan refuses) is reported as an
  `@error` in words instead of Python's answer.
- navkit `check` computes its summary before starting ordercheck, so a refusal hands off before ordercheck's log is
  written; an error it raises is raised where Python meets it.
- ordercheck's nearest node uses Rust's `hypot` (the C runtime's, as NumPy's).
- texqa: correlations are computed in double-double and answered only when the 4-decimal rounding cannot change
  within BLAS's error bound; otherwise handed off.
- O2's log differences (the start line names `bfbc2.exe`; no log pruning) hold here too.

**Checks:** `cargo fmt`; `cargo clippy -p bc2-cli --all-targets -D warnings` with all features and with each of the
seven groups alone; `cargo clippy -p bc2-editor --all-targets -D warnings`; `cargo test -p bc2-cli` (21); ruff and
mypy on the new tests (the changed GUI files keep their earlier mypy findings, none new).
`python run_tests.py quick -j 2` (this checkout's module and `bin\bfbc2.exe` built): 119 of 122, 1,027 s. The three
failures: O4's own entry (the location-dependent cases above, fixed and passing since), and the workspace
`cargo fmt / clippy / test` step and the editor's headless test, both stopped by Windows' paging file (os error 1455,
`failed to mmap` an rlib) while other sessions' builds and tiers ran on the PC - not a compile or test failure.

### Timings (processes, best of 5, real inputs)

| job | Python | Rust | |
|---|---:|---:|---:|
| `navkit` usage (start-up) | 0.128 s | 0.010 s | x13 |
| `navkit levels` (real OPFOR folder) | 0.128 s | 0.017 s | x7.5 |
| `navkit compare sp_009` | 0.332 s | 0.027 s | x12 |
| `ordercheck` sp_009 (DICE, NavKit) | 0.319 s | 0.029 s | x11 |
| `navcompare sp_009 --json` | 0.324 s | 0.024 s | x13 |
| `prepare_level sp_009`, fresh folder (common bundles too) | 56.2 s | 55.2 s | (the core's unpacks) |
| `prepare_level sp_004_b` / `mp_005` after it | 8.2 / 10.6 s | 7.9 / 9.7 s | |
| `prepare_level`, already prepared (sp_009 / sp_004_b / mp_005) | 2.7 / 3.1 / 2.6 s | 2.2 / 1.7 / 1.9 s | x1.2-1.8 |
| `prepare_level --terrain-only`, prepared | 0.4-0.5 s | 0.0-0.1 s | |
| `hpdcheck`, DICE's 15 files (run_tests' line) | 11.9 s | 1.9 s | x6.2 |
| `graphstats`, DICE sp_004_b | 0.217 s | 0.016 s | x13 |
| `navparity show`, two runs | 0.383 s | 0.013 s | x30 |
| `navregress`, no reference run | 0.399 s | 0.014 s | x29 |
| `harness list` (7 runs) / `show` (179 KB run) | 0.392 / 0.371 s | 0.014 / 0.015 s | x26-28 |
| `harness import-log` (349 MB opfor.log) | 0.957 s | 0.572 s | x1.7 |
| `texqa`, one texture / the 66 WW2 vehicle textures | 0.931 / 2.567 s | 0.201 / 0.703 s | x3.7-4.6 |

A fresh prepare is the core's unpacks and conversions (the same Rust either way); a prepared level's re-run is the
bundles' SHA-256 stamps, which the first Rust build (portable rounds, about a quarter of OpenSSL's speed) made slower
than Python's (5.0 against 2.0 s) until the SHA extensions were used.

### For Project OPFOR (not edited)

OPFOR runs none of these command lines: it imports texqa (qa_checks.py) in process, and NavKit / the harness are the
suite's own tools. Nothing changes there; a Rust library for texqa (shape B) would serve qa_checks later (O5).

### Left for O5

- **Python cores behind the hand-offs:** navkit `build` / `apply` / `all` and navregress's builds (navbuild's
  `walkable` glue, `navdetail`, `finalize_graph`); harness `run` (NumPy's seeded draws, NavKit's measures);
  navparity `run` (stale_nodes, ladder_ground, islands); detailed's sector choice; texqa's contact sheets (PIL),
  BF1943 sources (`bf1943import.Source`) and DICE copies (meshview); hpdview's picture (matplotlib); `status`
  (`time.ctime`: the local time of a file stamp).
- `sample` / `sample_all` start the game: they stay Python by rule.
- prepare as a library call inside the editor (no child process), once the editor links bc2-scene's collision.
- `bc2-cli/src/pyos.rs` (ntpath / glob / os.walk / atomicio / SHA-256) beside O3's `fsx.rs` and bc2-pyrt's `py`
  ntpath pieces: one set, in bc2-pyrt, once it may depend on bc2-formats' atomic writes (it has no dependencies now).

### Merged with main after O3 (8853a28)

- `pyjson.rs` moved into O3's runtime crate as `bc2_pyrt::json` (no dependencies: it uses `bc2_pyrt::fmt`); the
  binary aliases it as `crate::pyjson`, as it aliases `fmt` as `crate::pyfmt`.
- NavKit's `find_opfor` / `original_file` / `suite` are one copy, `bc2-cli/src/navshared.rs`, used by navkit and by
  navtools' / harness's `pycommon.rs` (which re-exports them).
- O3's `budget` module now allows dead code in any group built alone (its marker fields are read only by `caches`,
  its budget only by the bundle readers): clippy `-D warnings` failed for `graft`, `bf1943` and `meshview` alone.
- On the merged tree: cargo fmt and clippy `-D warnings` with all features and each of the 15 groups alone;
  `cargo test -p bc2-cli -p bc2-pyrt`; `test_cli_o2.py` (966 of 966), `test_cli_o3.py` (1,220 of 1,220) and
  `test_cli_o4.py` (622 of 622) quick; O4's `--game` (187 real calls, all identical; Rust 185, Python 2) and the
  prepare proof again.

## CT1: culltree add's memory and speed (2026-11-19, branch `rust-ct1`)

OPFOR's round D1f2 measured console_cull's `culltree add` line on the four Onslaught levels: `bfbc2.exe culltree`
(O3) took 7.6-8.7 s and about **1.65 GB**, against the Python's 7-8 s and 140-200 MB with a warm row cache.

### Where the memory went

O3's `ref_rows` parsed every reference level partition on all cores and kept them all (87 files, 127 MB of DBX, each
as a whole `Doc` of `El` nodes: a `Vec` for each element's attributes and another for its value), then parsed the
46 chosen trees the same way and kept them too, before the rows were taken. The rows themselves are a few MB.

### What changed

- **Streamed reference levels** (`culltool::scan_level`): the element stream is walked as `raw::elements` walks it,
  but only the first root's children are built, one instance at a time, read and dropped. The same objects, tree
  references and errors as `Level::read` (its order: element errors, then tags, then instances, then the object
  check). Also used to read the written level back.
- **Trees without their documents** (`CullTree::read_only`): the `Nodes` array's items become `NodeData` as they
  are read. The same nodes and errors as `CullTree::read`. Used for the reference trees, the written tree read back,
  `--client`, verify and info. Only the target tree, which is written, keeps its document; the target's documents
  are released once written.
- **Rows only, shared**: each reference level gives only its rows (`collect` of its objects in its tree). An
  `Instance`'s label and leaves are `Arc`s, so `collect`'s copies (and the second one after `--misplaced`) share
  them.
- **At most 8 levels at once** (`PRE_THREADS`): 16 threads cost about 25 MB more and gained no time.
- **The row cache** (`bc2_levelwrite::cullcache`): culltree.py's own pickles (below).
- **The listing**: the 37 `--ref-dir` folders are listed together (they were listed one after another). The walk
  takes each entry's type from the folder listing, with no stat per entry. The level mark is searched in 256 KiB
  pieces with `memmem` instead of whole files. A **listing entry** per folder keeps each file's listed size, time and
  answer, so unchanged files are not read again (825 MB were read every run; when the file cache had been evicted by
  other work, the listing alone took 12-28 s).

### The cache

- **Row entries are culltree.py's `_RowCache` entries**, read and written by both sides. There is one pickle per
  reference level file, `<sha1(normcase(abspath))[:24]>.pkl`:
  `{'code', 'path', 'level': (size, st_mtime_ns), 'trees': {tree: {'stat', 'rows' | 'error'}}, 'ref' | 'notlevel'}`.
  - **The key** is the level file's and its tree's `(size, st_mtime_ns)` and `code`, culltree.py's own id
    (`CACHE_VERSION` and a SHA-1 of the file). bfbc2.exe computes it from the copy of culltree.py it embeds, so an
    entry is used only by code that reads the levels as that file does.
  - **The writer** is protocol 4 (`pickle_write`, which now writes floats); Python reads it as its own.
  - **Unknown entries:** an entry of another code, or one this reader cannot read, is a miss and is written again.
- **Extra rows from the Rust:** when the Rust reads a level anyway, it keeps its tree's rows even if another file of
  the same name and tree size is chosen. A later run then needs no file again.
- **Listing entries** (`list-<sha1>.pkl`, the Rust's own): `{'kind', 'code', 'folder', 'files': [(path, size,
  mtime_ns, hit)]}`. A file that cannot be read is never kept: it is read, and raises, every time.
- **Pruning:** all entries are `.pkl` files of the culltree cache, so `bfbc2core.caches` counts them under
  `BFBC2_CULLTREE_CACHE_MB` and the overall budget. After a run that wrote, bfbc2.exe runs
  `caches::maybe_prune("culltree")`, then the overall budget. Entries are marked used as `caches.touch` marks them.
- **Settings:** `BFBC2_CULLTREE_CACHE=0`, and `BFBC2_CULLTREE_REFERENCE=1`, turn both caches off, as in the Python.

### Proof

console_cull's exact `add` line (`--auto --misplaced 2.0`, the Linux level as `--ref`, every `work\levels` folder as
`--ref-dir`) ran on each Onslaught level's installed bundle and on its `.opfor-cull-orig` bundle. Both were unpacked
into scratch; `work\levels` and `work\onslaught\linux_*` were read in place.

Eight ways were compared: Python cold, Python warm, old Rust (O3's `bin\bfbc2.exe`), new Rust cold, new Rust warm,
new Rust with `BFBC2_CULLTREE_CACHE=0`, new Rust on the Python's cache, and the Python on the Rust's cache. For all 8
runs of each way, `tree.dbx`, `level.dbx`, stdout (console_cull's `add_report.txt`), stderr and the exit code are
**identical** (32 of 32 files each).

Times and peaks below are wall time and peak working set. The machine was mostly idle: other rounds' processes were
open, at 5% load at the start.

| level / bundle | Python cold | Python warm | old Rust | new Rust cold | new Rust warm |
|---|---:|---:|---:|---:|---:|
| ons_mp_002 installed | 39.8 s / 621 MB | 8.7 s / 207 MB | 7.7 s / 1,643 MB | 2.9 s / 95 MB | **0.9 s / 78 MB** |
| ons_mp_002 orig | 30.8 s / 624 MB | 6.1 s / 165 MB | 7.2 s / 1,674 MB | 3.0 s / 96 MB | **0.8 s / 80 MB** |
| ons_mp_004 installed | 30.4 s / 586 MB | 6.7 s / 177 MB | 7.5 s / 1,665 MB | 2.5 s / 92 MB | **0.6 s / 82 MB** |
| ons_mp_004 orig | 31.5 s / 584 MB | 6.8 s / 175 MB | 7.3 s / 1,679 MB | 2.5 s / 98 MB | **0.7 s / 81 MB** |
| ons_mp_005 installed | 39.7 s / 684 MB | 7.0 s / 202 MB | 7.1 s / 1,649 MB | 3.0 s / 109 MB | **0.7 s / 86 MB** |
| ons_mp_005 orig | 40.3 s / 681 MB | 7.9 s / 194 MB | 7.1 s / 1,651 MB | 4.1 s / 104 MB | **0.8 s / 91 MB** |
| ons_mp_008 installed | 47.0 s / 584 MB | 6.7 s / 144 MB | 7.1 s / 1,635 MB | 2.9 s / 93 MB | **0.6 s / 81 MB** |
| ons_mp_008 orig | 41.0 s / 583 MB | 7.0 s / 176 MB | 7.3 s / 1,666 MB | 2.7 s / 95 MB | **0.7 s / 82 MB** |

The other three ways:

- **New Rust with the cache off:** 2.2-5.1 s, 92-101 MB.
- **New Rust on the Python's cache:** 1.2-3.1 s, 65-83 MB. The first run adds the listing entries and the
  duplicates' rows.
- **The Python on the Rust's cache:** 7.7-12.4 s, 131-193 MB. Its own listing is not cached, and the Rust's entries
  also carry the duplicates' rows.

**Warm, against the old Rust:** about 10x faster (7.3 to 0.7 s) and 20x less memory (1.65 GB to 80-90 MB).
**Against the Python warm:** about 10x faster and about half the memory. Cold, the new Rust is 2.5-4 s and about
100 MB.

**Tests:**

- `bc2-levelwrite/tests/culltool_ct1.rs` checks that the streamed readers answer as the whole-document ones, on:
  - every level partition, tree and description of `work\levels`: 87 levels and 260 other files;
  - damaged copies of the first of each every sixth folder (cut at 12 places, 12 bytes changed). Each gives the same
    values, or the same error kind and text.
- `cullcache`'s round trip, and its code id against Python's formula.
- `test_cli_o3.py --group cull --live`: 190 of 190 as recorded, and the same 190 both ways live (Rust 184, Python 6,
  as before).
- cargo fmt; clippy `-D warnings` on the workspace (all features) and on the cull and caches groups alone; the touched
  crates' cargo tests.
## O5: the shader tools, the console readers, the BF1943 importer, stockmp's pipeline, NavKit's leftovers (2026-11-19, branch `rust-o5`)

The last batch of O1's plan, plus the Python cores O3 / O4 still handed off. The batch was ported in three helper
worktrees (`rust-o5a` stockmp, `rust-o5b` shader, `rust-o5c` readers / importer) and in `rust-o5` (NavKit's
leftovers, the tidy-up), then merged. **Provisional timings:** other sessions' builds and test tiers ran beside every
figure.

### What was added

| group (feature) | subcommands (the Python they port) |
|---|---|
| `shader` | `shaderdb` (all eight subcommands, the merge's `Merger` / `entry_contents` / `verify_merge` and `level_meshes` ported: E4b had left them Python), `renderqa`, `xdiff` (both: argparse and the errors raised before any drawing) |
| `readers` | `apt`, `stfs`, `xiso` (`bfbc2-console-readers`) over the F0 / F3 readers |
| `import` | `bf1943import` (`list`, the options and the errors before work; `bf1943src.rs`: `bf1943import.Source` - the walk, `fbrb1943.Bundle`'s check, multi-member gzip, the current source index read) |
| `stockmp` | `generate`, `build`, `validate`, `all` (`generate.rs`, `build.rs`, `validate.rs`: `config.generate`'s placement on NavKit's path-data model - O4's ordercheck port -, `pipeline.build`'s staging, manifests, sector assets and packing, `validate`'s checks and wording; the feature now pulls in `navkit`) |
| `navtools` | `detailed`: `choose` itself (flagged objects' collision boxes through `bc2_scene::collision::havok_index` and the Havok triangle reader, the 0.8 m samples' stacked floors through `bc2_navbuild::navsample`) |
| `navkit` | `navkit status` (`clock::ctime`: `time.ctime` of a file stamp - the local time as the C runtime's `asctime` writes it) |

**Tidy-up (O4's list):** O3's `fsx.rs` is folded into O4's `pyos` (`bc2-cli/src/pyos/mod.rs`; its stat, utime,
the shared lock, `replace`'s tries and scandir as `pyos::files`; the duplicate `normcase` gone). Both stay in
`bc2-cli`: `pyos` answers in `PyResult` (the binary's `Fail`), which `bc2-pyrt` does not know.

### Proof (O2's method: `core/tests/test_cli_o5.py`, cases in `cli_o5_<group>.py`; O3's isolation)

Goldens were recorded from the Python first, from a frozen copy of the checkout at `7f21990` (f810613's sources with
their own module, in the scratch folder: no Project OPFOR walk-up), from synthetic inputs (no EA content).

| group | recorded lines | Rust | handed to Python | `--live` (Python / Rust process time) | `--game`: real calls (Rust / Python) |
|---|---:|---:|---:|---|---|
| shader (shaderdb 252, renderqa 52, xdiff 18) | 322 | 320 | 2 | same answers (115 / 10 s) | 30 (30 / 0): console_shaders' `needs` and Dx9 / 10 / 11 `merge` on the four Onslaught levels, `info level:`, `cover`, `roundtrip --levels`, `find` |
| readers (apt, stfs, xiso) | 212 | 206 | 6 | same (44 / 6 s) | 75 (75 / 0): the user's Update packages and disc image read in place, 8 OPFOR menu movies |
| bf1943 (bf1943import) | 46 | 42 | 4 | same (12 / 2 s) | 12 (10 / 2): 9 lists of the real BF1943 source, 3 source-index cases |
| nav (detailed 56, navkit status 36) | 92 | 72 | 20 | same (26 / 8 s) | 74 (74 / 0): `detailed` on the 35 prepared levels of OPFOR's `work\` with their sector offsets (4 with their 0.8 m samples), `navkit status` of the same 35 |
| stockmp (generate / build / validate / all, config check) | 73 | 71 | 2 | same (50 / 10 s) | 3 (3 / 0): OPFOR's full `generate` calls |
| **all** | **745** | **711** | **34** | | **194 lines, each alone and on one pipe, both ways: all identical** |

- **stockmp's builds, byte for byte** (`cli_o5_stockmp.game_check`): `stockmp all` of mp_005, mp_002 and mp_008cq in a
  stand-in OPFOR folder (`game` a junction to the game copy, removed with `rmdir` before the folder; its own `work\`
  and LOCALAPPDATA; both sides on the same path one after the other): every file under `work\` identical by SHA-256
  (35,688 / 34,466 / 32,257 files), both streams, exit codes and log records identical; Rust answered all three.
- O3's stockmp group still passes 236 of 236 as recorded; Rust now answers 218 of them (O3: 214).
- **shaderdb merge** is Rust end to end: the merged databases and `.merge.txt` reports byte-identical in every
  recorded case (Dx9, Dx10, Xbox, two donors, `--all-names`, level and bundle donors, a 260-entry database) and on
  the real Onslaught merges.
- **detailed's triangles:** the synthetic packfiles the tests can build give the reader no triangles, so the quick
  cases cover the flag rules, the no-packfile boxes, the refused packfiles and the samples (versions 1, 2, 3); the
  object boxes from real collision triangles are proven by the 35 real levels. The world box is
  `x*r0 + y*r1 + z*r2 + t` in float64 as levelobjects' port (proven on 56,831 files in O4); NumPy's `matmul` goes
  through BLAS, so a last-bit difference could only show where a box edge plus the 5 m margin falls exactly on a
  100 m sector line.

**Recorded differences, by design** (none in the recorded lines or OPFOR's calls):

- the cases that name the case folder's path in a hash (bf1943's source-index file names) are run both ways in
  `--game` only, not recorded (O4's rule for location-dependent cases);
- the bf1943 cases run with `PYTHONDONTWRITEBYTECODE=1` (the Python imported the case folder's `fbrb1943.py` and left
  a `__pycache__` the Rust does not);
- O2's log differences (the start line names `bfbc2.exe`) hold here too.

### Still handed to the Python, and why

- **renderqa's drawing** (render, compare, validate, wake, strips, patches): Direct3D 11 through `shaderview.exe`,
  NumPy measures of the pixels, Pillow contact sheets. bc2-render (R5) has the preparation and the draw but not the
  measures or the sheets; byte-identical sheets and GPU pixels are not a reasonable port. OPFOR's
  `ww2_level_checks` strips / patches lines therefore still draw in Python (Rust saves only the start-up).
- **xdiff's runs:** the Xenos interpreter `xenos_exec.py` (NumPy; kept Python since E3f by measurement), NumPy's
  seeded inputs, WARP draws. OPFOR calls `xdiff.main` in process, not the command line.
- **texqa's bench_sweep line** (`texqa <work\ww2\bf1943_vehicles> -o .. --cache .. --worst 4`): it always draws
  contact sheets of the worst rows with PIL (its bitmap font, its PNG compression), which is not a reasonable
  byte-identical port. With the sheets Python, answering the BF1943 `Source` (now in Rust, `bf1943src.rs`) and the
  DICE copies in Rust would not change who answers the line, so it stays handed off and the reference is unchanged.
  A sheets-only Python step (Rust writes `texqa.json`, Python draws) would need a new Python entry point.
- **bf1943import `import` / `check`** after the source opens: the `Importer` (the BFBC2 soldier skeleton from the
  game copy's index, Granny trees compared as Python objects, the undeploys' added tracks). Writing the source index
  (a pickle of the Python's own bytes plus the cache prune's stamps) also stays Python.
- **dbxcatalogue's cache build** (decided: kept). The pass itself is Rust since F2b (`catalogue_build`); what the
  Python adds is the pickle of `dbx.Element` trees, Counters and frozensets that OPFOR's importer modules unpickle in
  process. A Rust-side format would make the shim rebuild those Python trees on every load (slower than unpickling)
  for a build that runs once per game-copy change; the CLI's answer from a current cache is Rust (O3).
- **navkit `build` / `apply` / `all` and navregress's builds** (not feasible this round; what is missing, by
  function): `navbuild.stage`'s orchestration (navedits' hand edits, `deep_water`, `outside_bounds`, the border
  nodes' runs and corner sharing, the carried seam nodes, `drop_one_way` / `restore_one_way`'s rows),
  `navwalk.walkable`'s NumPy glue around the Rust grid walks, `detail_pass` / `navdetail` (D lines, F cells: the
  iteration order of its sets reaches the output), `sector_links` with the same / cross-grid resolvers,
  `merge_detailed`, `prune_order_nodes`, `finalize_graph`, `write_file` (`kymesh.write_mesh`, `hpdtool`'s writer),
  `navislands.note_nodes`; for `apply` `navtune.load` / its operations, `navlinks.apply`, `navtraversal.write`,
  `navregress.level_water` / `order_points_of` / `reference_bounds`. `sample` / `sample_all` start the game and stay
  Python by rule.
- **Damaged or off-type inputs** in every group (cut files, values of another type, text outside what the port
  reads as Python does), argparse abbreviations / `--` / `-oX`, `detailed --score` (evaluation only).

### Timings (processes, best of 5 unless said; real inputs)

| job | Python | Rust | |
|---|---:|---:|---:|
| `shaderdb` start-up (usage) | 0.205 s | 0.011 s | x18 |
| `shaderdb needs ons_mp_002` | 0.917 s | 0.115 s | x8.0 |
| `shaderdb merge ons_mp_002` Dx9 / Dx10 / Dx11 | 1.449 / 1.786 / 1.618 s | 1.096 / 1.620 / 1.351 s | x1.1-1.3 |
| `shaderdb --dx 10 info level:ons_mp_002` | 0.510 s | 0.176 s | x2.9 |
| `shaderdb cover ons_mp_002` | 1.119 s | 0.311 s | x3.6 |
| `shaderdb find` (the Onslaught extras over every level) | 10.70 s | 8.27 s | x1.3 |
| `shaderdb roundtrip --levels` (best of 3) | 90.1 s | 66.4 s | x1.4 |
| `apt` start-up / `apt info` a menu movie / `apt dump` | 0.182 / 0.190 / 0.195 s | 0.010 / 0.010 / 0.012 s | x16-18 |
| `stfs list` / `extract mainmenu-00`, an Update package | 0.171 / 0.170 s | 0.010 / 0.013 s | x13-17 |
| `xiso list` / `extract default.xex`, the disc | 0.170 / 0.172 s | 0.011 / 0.011 s | x16 |
| `bf1943import list garand` (real source, index off; best of 3) | 2.690 s | 1.716 s | x1.6 |
| `stockmp summary mp_005`, warm | 0.29 s | 0.08 s | x3.6 |
| `stockmp generate mp_005`, work folder unpacked (best of 3) | 0.79 s | 0.69 s | |
| `stockmp build mp_005` / `mp_008cq` (best of 3) | 26.0 / 13.4 s | 10.6 / 6.5 s | x2.1-2.5 |
| `stockmp validate mp_005` / `mp_008cq` (best of 3) | 2.17 / 1.51 s | 1.76 / 1.18 s | x1.2-1.3 |
| `stockmp all` mp_005 / mp_002 / mp_008cq (single runs, a fresh work folder: the stock unpack, the same Rust on both sides, sets it) | 56.3 / 53.4 / 40.1 s | 45.5 / 55.8 / 53.0 s | (disk-bound) |
| `detailed sp_009` / `sp_004_b` (OPFOR's work\, flagged objects' boxes) | 0.236 / 0.243 s | 0.046 / 0.047 s | x5.2 |
| `detailed ons_mp_002 --samples` (with the 0.8 m samples' stacked floors) | 0.301 s | 0.070 s | x4.3 |
| `navkit status sp_009` | 0.111 s | 0.011 s | x10 |

### For Project OPFOR (not edited)

- **`src\tools\onslaught\console_shaders.py:178` and `:188`** run `[sys.executable, SHADERDB, 'needs' / 'merge', ...]`
  (through `run`, which is `cc.dbx_run`): `cc.cli_command('shaderdb') + ['needs', ...]` / `+ ['merge', ...]` saves
  about 0.2 s of start-up a call (`needs` 0.9 -> 0.1 s) and runs the merge in Rust.
- **`src\tools\ww2\ww2_level_checks.py` `_renderqa`:** `native.cli_command('renderqa') + [command, '--db', ...]`
  saves only the start-up (strips / patches still draw in Python).
- **stockmp:** nothing to change (its lines go through `cli_command('stockmp')` since D1f2).
- **apt / stfs / bf1943import / xdiff:** imported in process by OPFOR, not run: nothing changes.

### Checks

`cargo fmt`; `cargo clippy -p bc2-cli --all-targets -D warnings` with all features and with each of the 18 groups
alone (single groups print cargo's "unused dependency" notes for other groups' crates, not clippy errors);
`cargo test -p bc2-cli -p bc2-pyrt`; ruff and mypy on the new Python. On the merged tree (`9d2ee0a`, this checkout's
module and `bin\bfbc2.exe` built): `test_cli_o5.py` quick 745 of 745 (533 s), then `--live --game` per group (all
identical; shader's real calls Python 446.9 s / Rust 290.4 s, readers 27.8 / 3.1 s, bf1943 49.1 / 31.7 s, nav 31.2 /
5.4 s), stockmp `--live` and `--game` with the three byte-identical builds again (mp_005 41.6 / 42.8 s, mp_002 46.1 /
40.7 s, mp_008cq 31.6 / 32.9 s); O1-O4's goldens unchanged: `test_cli_o1.py` 722, `o2` 966, `o3` 1,220 (Rust now
answers 218 of O3's stockmp lines), `o4` 622, all as recorded. `python run_tests.py quick -j 2` was **not run**: drive
C: had 25.9 GB free after the proofs (other sessions' build folders), and the tier's workspace and editor debug builds
take 13 GB or more.

## F4: F3's leftovers (2026-11-20, branch `rust-f4`)

F3 left six pieces in Python; F4 ports them. As in F3, each Python function stays as its reference and fallback
(`*_reference`, the module's `use_native(False)`, `BFBC2_PURE_PYTHON=1`); a binding answers `None` where the port
does not read a value exactly as the reference does or where the reference raises, and the Python then runs.

### What was added

| Python | binding | Rust |
|---|---|---|
| `culltree.Doc`'s string table (`_strings`) | `culltree_strings` | `bc2-py` (`formats_f4.rs`) |
| `xenos.disasm`'s header lines | `xenos_disasm` (header and listing in one call) | `bc2-py` (`xenos.rs`), the listing `bc2_shader::disasm` (F3) |
| `audio.convert` / `decode` (`segment_stream`, `decode_xma`, `decode_xas`, `ffmpeg_decode`) | `audio_convert`, `audio_decode` | `bc2_convert::wavport` |
| `hkpack.Packfile` as a handle; `read_object` / `read_value` / `deref` / `cstr` / `objects_of` | `hkpack_open`, `HkPackfile` | `bc2_formats::havok::reflect` (moved from `bfbc2.exe hkpack`, which now uses it) |
| `navsample_offline.asset_bodies` (`_shape_parts`, `_hull_faces`, `_box_faces`) | `navkit_asset_bodies` | `bc2_navbuild::hulls` |
| `meshview.load_mesh`'s part reader (`_values`; now `_read_part`) | `meshview_part` | `bc2-py` (`formats_f4.rs`) |
| `texqa.read_pc` / `read_xbox` | `texqa_read` | `bc2_convert::texqa` (moved from `bfbc2.exe texqa`, which now uses it) |

Notes:

- **The disassembly's header** is read from the caller's tree, as `_disasm` reads it (a tree may have been edited), and
  formatted as `%` does (`%g` through `bc2_pyrt::fmt`); `cf_slots` comes from the program, so the header no longer
  makes the tree read its microcode. A value of another kind (a `bool` for an `int`, a non-ASCII name or usage, an
  `int` past 64 bits) gives `None` and the Python writes the header.
- **The handle-based Packfile** builds its dicts (`sections`, `ptr`, `objects`, `by_offset`, `classes`,
  `class_by_name`) on their first read, in the reference's attribute order; until then the methods ask the handle.
  Once a dict was read (a caller may have changed it) or `d` replaced, the Python answers. Copies and pickles build
  the dicts first.
- **The hulls** reproduce NumPy bit for bit, including OpenBLAS' small kernels on this PC (the wheel's Haswell
  `ddot` / `dgemv` / `dgemm`, measured with scratch probes): a 3-vector dot is `fma(x2, y2, fma(x1, y1, x0 * y0))`, a
  `(k, 3) @ (3,)` row the same for k = 1 and `fma(a2, b2, fma(a0, b0, a1 * b1))` for k >= 2, the `(n, 3, 3) @ (3, 3)`
  transform the dot's chain per element, and every one of them +0.0 for a zero (the first game run differed on 98
  hulls by exactly that: an angle of -pi for pi). `np.unique(axis=0)` is a stable sort up to 17 rows; NumPy's
  argsort of the face angles is its SIMD sort, not stable, so a face with tied angles (collinear vertices:
  Railway_Switch's hulls, 5 files) is handed to the Python, as are rows equal with other bits past 17, NaN or
  infinite input, and the reference's errors outside `_shape_parts`' `except`. On another CPU the reference's BLAS
  may round otherwise; the port keeps this PC's.
- **Audio** runs ffmpeg as `ffmpeg_decode` does, with the path `audio.ffmpeg_path()` found (or none), without the
  interpreter's lock: `convert_many`'s threads now run the whole conversion side by side. Any error (an
  `AudioError`, ffmpeg failing) is the Python's: it runs the sound again and raises its own error.
- **texqa**'s `read_xbox` of a 4-byte texel raises the reference's `KeyError` (`('keyerror', bpb)`); a half-float
  NaN (its payload is NumPy's conversion's) and textures past 2^28 texels go to the Python.
- **Not ported:** `xenos.disasm_instruction` (one instruction of a Python tree), `audio.ffmpeg_decode_xas` (a
  check against ffmpeg's own decoder, not a path the tools take), `hkpack.all_members` / `struct_size` /
  `elem_size` / `member_class_name` (they return the dicts' own objects: the Python reads them from the dicts).

### Proof (`tests/test_f4_native.py`; goldens recorded from the frozen starting commit)

The goldens (`tests/f4_golden_<area>.json`) were recorded **before any change**, from a copy of `6a7dd68` (git
archive, its own matching module and `bfbc2.exe` from the main checkout) whose public functions were the Python:
quick first, then `--game`. The checks below ran on this branch's module: every answer against the recorded digest,
and live against the reference in the same process (fn and `*_reference` / `use_native(False)` both ways).

| area | inputs | result |
|---|---|---|
| culltree | quick 3,060 (60 real documents, 1,500 damaged, 1,500 built tables: invalid UTF-8, offsets past the end, no zero); game 6,549 documents (every `staticculltree*` of the game copy, `work\xbox`, `work\xbox_dlc`; every `.dbx` of mp_005 / sp_005) | 9,609 identical, 0 different; 8,754 answered in Rust (the 855 handed back are damaged / built) |
| disasm | quick 7,200 (300 built programs x 4, damaged, 16 header edits each); game 8,320 distinct programs of BF1943's and BFBC2's Xbox shader databases x 4 | 40,480 identical, 0 different; 37,529 in Rust (handed back: the edits the port does not format, damaged blobs) |
| hkpack | quick 528 (8 real, 400 damaged, 120 built packfiles **with class descriptions**: every member type, C arrays, arrays, structs, parents, repeated names); game 5,579 packfiles; per file the header values, `deref` of every slot (and non-slots), `objects_of` every class, `read_object` / `read_value` of 10 kinds at every object, `cstr`, then the dicts | 6,107 identical, 0 different; 6,076 in Rust (31 damaged handed back) |
| bodies | quick 640 (40 real, 600 damaged); game 9,866 collision assets (`work\levels` `*_havok.res`, every HavokPhysicsData of the game copy and `work\ww2`) | 10,506 identical, 0 different; 10,501 in Rust, 5 handed back (the Railway_Switch ties) |
| texqa | quick 1,155 (40 real, 600 damaged, 800 header variants, the on-demand wrapper); game 13,441 textures (every ITexture of the game copy, every XenonTexture of `work\xbox` / `xbox_dlc`, BF1943's) | 14,596 identical, 0 different; 14,569 in Rust (27 handed back, damaged) |
| meshview | quick 360 (60 meshes, 300 damaged); game 2,512 meshes (every `_lod0` of the meshview index, 5,455 parts) | 2,512 identical, 0 different; every part read in Rust |
| audio | quick 305 (BFBC2 Xbox / PC / BF1943 Waves, damaged, arguments of other kinds); game: every BF1943 Wave convert and decode, a third of the BFBC2 Xbox Waves converted, every PC Wave decoded | 260,881 identical, 0 different; 14,582 converted / decoded in Rust; the rest are sounds the reference refuses (a streamed segment without its `waveDataSns`, ...): every error is the Python's, and it raised the same 246,299 |

### OPFOR's calls replayed (`src\` grepped; game\ only read)

- **`ww2\wake_level.convert_waves`** (the WW2 install's `port_sounds` and `wake_vo.waves`: `audio.convert_many` in
  batches of 64), OPFOR's own function over this checkout's `audio` (an `OPFOR_TOOLKIT` junction to the worktree),
  every one of BF1943's 2,750 Waves: **2,750 of 2,750 byte-identical**, all answered in Rust; 28.4 s against the
  Python's 39.0 s.
- **`onslaught\console_cull.build`'s `culltree add`** with the Python command line (`culltree.py`, the fallback when
  `bfbc2.exe` is not built), in process, on the four Onslaught levels' installed bundles and their
  `.opfor-cull-orig` backups unpacked into scratch: **8 of 8 identical** (exit code, report, `tree.dbx`,
  `level.dbx`); 32 string tables, all read in Rust (the row cache serves the rest).
- **`ww2\qa_checks.check_textures`** (`texqa.run` over `work\ww2` and its install staging with BF1943's sources and
  DICE's copies, then `texqa.sheets`), in process, a fresh cache each way: 2,000 textures, 3 contact sheets: **rows, text report and sheets identical**; 3,666 reads, all in Rust; 134.0 s against the Python's 211.9 s.
- No OPFOR call reaches `hkpack`, `navsample_offline.asset_bodies`, `meshview.load_mesh` (OPFOR uses
  `meshview.Index`, ported in O3) or `xenos.disasm` (OPFOR's WW2 shader tools translate; they do not list).

### Timings (best of 5, in process; peak working set per process; `bench_f4.py` in the round's scratch)

Workloads from OPFOR's game copy and work folders; "Python" is the module's `use_native(False)` (or, for the
header, the F3 path: Python header over the Rust listing). Measured while another round's tests ran: ratios hold
within a run.

| workload | Python | Rust | |
|---|---:|---:|---:|
| `texqa.read_pc`, 45 PC textures (mp_005 level-00) | 2.33 s, 463 MB | **0.22 s**, 240 MB | x10.4 |
| `texqa.read_xbox`, 45 Xbox textures (mp_005 level-00) | 1.34 s, 269 MB | **0.11 s**, 134 MB | x11.8 |
| `asset_bodies`, 327 collision assets (`work\levels\mp_005`) | 34.1 ms | **7.6 ms** | x4.5 |
| culltree string tables, the 2,088 documents of mp_005 level-00 | 144 ms | **58 ms** | x2.5 |
| `meshview.load_mesh`, 21 meshes (textures off) | 9.4 ms | **6.9 ms** | x1.4 |
| `audio.convert_many`, 40 BF1943 Waves (ffmpeg included) | 0.61 s | **0.50 s** | x1.2 |
| `audio.decode`, 10 PC Waves (EA-XAS) | 8.3 ms | 10.5 ms | x0.8 (both decode in Rust already; the port adds the reading and a copy) |
| `xenos.parse_shader` + `disasm`, 1,000 built programs (header Python vs Rust; listing Rust both) | 66.6 ms | **62.6 ms** | x1.06 |
| `hkpack`: Packfile, objects_of, read_object, deref, 20 packfiles | 0.9 ms | **0.3 ms** | x3 |

Whole corpora (the `--game` check, both in one process; the calls timed): parse + disasm of 8,320 programs x 4,
the Python reference 56.4 s, the port **7.2 s** (F3: 13.1 s - the header no longer makes the tree read its
microcode); texqa's 13,441 textures 525 s / **58.6 s**; asset_bodies of 9,866 assets 12.9 s / **2.1 s**; the
hkpack probe of 5,579 packfiles 65.9 / 59.1 s (the probe's own Python calls dominate); OPFOR's WW2 sound port 38.7 /
**27.7 s**; its texture QA 211.9 / **134.0 s**. Peak memory is the same or lower everywhere (texqa halves it: no
NumPy intermediates).

### Checks

`cargo fmt`; `cargo clippy -D warnings --all-targets --all-features` on bc2-formats, bc2-convert, bc2-navbuild,
bc2-py, bc2-cli; `cargo test` of the same (all pass); ruff; mypy (`tests/test_types.py`); `spec/trace.py --check`
(RES-WRAP-1's citation moved with `unwrap` to `rust/bc2-convert/src/texqa.rs`); `tests/test_f3_native.py`
(hkpack, disasm, audio: its Packfile comparison now builds the dicts first). `python run_tests.py quick -j 2`: 123
of 127 (4,080 s, the workspace's cargo step included). The four failures, before the round's files were committed:
O3, O4 and O5's command-line tests build their stand-in checkouts from the tracked files, so the untracked new Rust
files gave them a stale module - all three pass once committed (O4's texqa group also needed its goldens recorded
again: texqa.py's `CODE`, a hash of its source, keys its cache rows); the NavKit driver's stand-in test failed once
under load and passes alone.

**Not ported, left for a later round:** `xenos.disasm_instruction`, `audio.ffmpeg_decode_xas`, hkpack's
dict-returning helpers (see the notes above); a tie-aware port of NumPy's SIMD argsort, if the five Railway_Switch
hulls ever matter; the `*_reference` functions until a full tier retires them.

## SD1: the shader database reader, end to end (2026-11-20, branch `rust-sd1`)

What the queue called "the shaderdb reader port" was mostly done before this round: E1c put the record reader and
writer in Rust (`bc2_formats::shaderdb::parse` / `write`), E4b kept a parsed database in a `ShaderDbHandle`, E3f / F4
read the Xenos containers inside Xbox programs (`xenos.parse_shader`, `bc2_shader::xenos`), and `bc2-render`, the
editor and `meshindex` already call the same `shaderdb::parse`. What was still Python in process was how a database
is found and opened: `ShaderDb.parse`'s dispatch (the byte order from the first bytes, Dx9 then Dx10 / Dx11, the
messages), `pick_database` and `dx_from_name`. `bfbc2.exe shaderdb` (O5) had its own Rust copy of the three.

### What moved

| Python (`shaderdb.py`) | binding | Rust |
|---|---|---|
| `ShaderDb.parse` (the dispatch and the reader in one call; the database kept in a `ShaderDbHandle`) | `shaderdb_open` | `bc2_formats::shaderdb::open::open` |
| `pick_database` (after `bundle_databases`) | `shaderdb_pick` | `open::pick_database` |
| `dx_from_name` (the search of the base name) | `shaderdb_dx_in_name` | `open::dx_in_name` |

- One implementation: `bc2-cli`'s `parse_kind`, `pick_database` and `dx_from_name` now call `open` (its copies gone).
  The CLI's pick lower-cased bundle entry names as ASCII where Python lower-cases latin-1; the shared code lower-cases
  latin-1 and hands a name past U+00FF to the Python (none in the game copy).
- The Python stays the reference and fallback: `ShaderDb.parse_reference` (the dispatch over `_parse_reference`, the
  `_Reader` code), `pick_database_reference`, `dx_from_name_reference`; `use_native(False)`, `BFBC2_PURE_PYTHON=1`.
  A binding answers `None` for what only the Python takes (`dx` not an `int` within `u32`, `big_endian` not a
  `bool`, a `str` subclass or one with a lone surrogate, a `bytearray` / `memoryview`).
- `load_levels` reads each level bundle's database list once instead of once per Dx (`_pick` with the list given).
- **Fixed:** `ShaderDb._parse` sent any `dx` to the handle: a float, negative or huge `dx` raised `TypeError` /
  `OverflowError` where the Python reads the file (with the Dx10 layout, as any `dx` but 9). Found by the goldens
  (21 of the quick tier's answers differed on the starting commit between its Rust and its Python).

### Proof (`tests/test_sd1_native.py`; goldens from a frozen copy of `102165d`, its Python via `use_native(False)`)

| area | inputs | result |
|---|---|---|
| parse, game | 745 databases: 436 in the game copy's and OPFOR's Xbox bundles, 5 BF1943, 304 OPFOR work files (`work/ww2`, `work/levels`); each with its kind, none, the other layout, and 6 damaged copies (cut, bytes changed) with none and its kind | **11,175 answers identical** (values or the same error), all 11,175 answered in Rust; 745 of 745 golden groups as recorded |
| parse, quick | built Dx9 / 10 / 11 / Xbox v120 / v117 databases, every kind of `dx`, `big_endian`, `where` and buffer, 40 mutations each, short files | 2,112 identical; 2,056 in Rust, 56 handed back (bytearray, memoryview, float / bool / negative / huge `dx`, `big_endian=1`, a surrogate) |
| pick, game | 148 bundles x Dx9 / 10 / 11 x their entries (none, each database's name, folder, case) | 10,068 identical, all in Rust |
| pick, quick | 6 built bundles (one per arrangement), Dx9-12, latin-1 and non-latin-1 entries | 433 identical; 385 in Rust, 48 handed back (entries past U+00FF) |
| load, game | `load_levels` of all 89 levels x Dx9 / 10 / 11 (and an unknown level), `load` of `level:` / bundle / file specs | 126 identical (inner calls: 357 parses and 349 picks, every one in Rust); 66 golden groups as recorded |
| load, quick | built files and bundles, wrong-kind names, missing files | 208 identical |
| names | 861 real database names and paths, 19 made-up | all identical; 1 handed back (a surrogate) |
| `--live` | the whole game tier again with `BFBC2_PURE_PYTHON=1` | 970 of 970 groups the same |

- **OPFOR's WW2 shaders step, in process** (`wake_shaders.merge_stock(wanted, out)` imported from OPFOR, each WW2
  level's `needs_bfbc2.txt`, the suite found through `OPFOR_TOOLKIT`; the game copy read only, outputs to scratch):
  the frozen copy, this round, and this round with `BFBC2_PURE_PYTHON=1`: **24 of 24 files byte-identical** (the
  Dx9 / 10 / 11 databases and `.merge.txt` reports of four levels).
- **OPFOR's `console_shaders` calls and O5's goldens** (`core/tests/test_cli_o5.py --group shader --live --game`):
  322 of 322 recorded lines as recorded (shaderdb 252 in Rust); the Python command lines both ways the same; the 30
  real calls (`needs` and Dx9 / 10 / 11 `merge` of the four Onslaught levels, `info level:`, `cover`, `roundtrip
  --levels`, `find`) each alone and on one pipe: Rust answered every one as the Python did.
- Checks: `cargo fmt`; `cargo clippy -p bc2-formats -p bc2-py -p bc2-cli --all-targets --all-features -D warnings`;
  `cargo test --release -p bc2-formats -p bc2-cli --all-features`; ruff and mypy on the changed Python;
  `spec/trace.py --check` (the `shaderdb.rs` citations after line 29 moved one line; SHDB-HDR-3 and SHDB-X-1 cite
  `open.rs` and the test); `python run_tests.py quick -j 2`: 127 of 128 (O4's navtools group failed once under the
  tier's load and passed alone, 222 of 222 as recorded: NavKit's command lines, which this round does not touch).

### Timings (`rust/bench_sd1.py`, best of 5, `load_levels` best of 3; the game copy, a quiet machine)

| | before (102165d) | SD1 | Python (`BFBC2_PURE_PYTHON=1`) |
|---|---:|---:|---:|
| `ShaderDb.parse` mp_005 Dx9 | 1.33 ms | 1.35 ms | 32.1 ms |
| `ShaderDb.parse` mp_005 Dx10, no kind given (Dx9 tried first) | 1.57 ms | 1.43 ms | 49.6 ms |
| the same cut in half (both layouts fail) | 0.27 ms | 0.30 ms | 22.5 ms |
| `pick_database` (a bundle header read each call) | 0.353 ms | 0.352 ms | 0.343 ms |
| `dx_from_name` | 1.55 us | 1.21 us | 1.61 us |
| `load('level:mp_005', 11)` | 15.1 ms | 14.5 ms | 44.8 ms |
| `load_levels`, 89 levels x Dx9 / 10 / 11 | 2.36 s | **2.09 s** | 10.4 s |
| peak working set of the bench | 1,066 MB | 1,116 MB | 1,579 MB |

OPFOR's `merge_stock` per WW2 level: 2.8-5.5 s in Rust (the frozen copy 3.2-4.1 s; the shared machine's noise),
9.7-10.7 s in Python. The gains are small by design: the reader itself was Rust already; this round makes the last
in-process steps one Rust implementation, and `load_levels` reads 89 bundle headers instead of 267.

### Still Python, and why

- `resolve` / `load`'s spec parsing (`level:`, `<bundle>:<entry>`, the `.fbrb` pattern), `level_bundles`,
  `find_opfor`, `bundle_databases`: path glue over `os.path` (the CLI's twins follow `pyos`'s Python path rules in
  `bc2-cli`); the bundle reads under them are Rust already. Nothing to gain in process.
- `Merger` (records, `clone_entry`, `retexture`, `finish`) in process: OPFOR changes `base.entries` / `groups` /
  `programs` between merges, so the database would have to live on the Rust side end to end (E4b's note). The
  command line's merge is Rust (O5).
- The reader for a `bytearray` / `memoryview` (its records keep the buffer's type) and the odd arguments above:
  the reference, by design.
- `xenos.parse_shader`'s Python for blobs the Rust decoder refuses (damaged blobs; E3f / F4).

## RT1: the full test tier, then the first Python twins retired (2026-11-20, branch `rust-rt1`)

### Part 1: the full tier on main

`python run_tests.py full -j 2 --mem 7` on `rust-rt1` at main `6a7dd68` (the module, `bfbc2.exe`, `bfbc2-editor.exe`
and `mapexport.exe` built fresh from it; build root `rust-targets\bfbc2-tool-suite-rt1`), 2026-11-20 01:30-06:33:
**231 of 232 passed in 18,110 s** (5 h 2 min). The one failure was caused by this round, not the code: while the
tier ran, C: fell to 21 GB free and the round's own `debug` build folder (25 GB) was deleted to stay above the
25 GB floor. The map editor's headless test had built its test binaries there, so cargo found
`debug\build\...\factions-*.exe` missing ("never executed", os error 3). Run again alone
(`run_tests.py full --only "map editor (rust), headless"`) it passed in 1,787 s. So **232 of 232** pass on main.

The longest tests: FbrbTool's round trip of every game bundle 3,890 s; the Rust FBRB unpack against FbrbTool.exe's
recorded unpacks 3,269 s; O5's command lines live with OPFOR's real calls 1,822 s; the cargo checks (fmt, clippy,
the workspace's tests, after a rebuild of the debug folder) 1,801 s; F2a's console converter on every BF1943 / Xbox
resource 1,666 s; DbxEditGUI's classes on the corpus 1,147 s; F2a's model tools 1,051 s; O3's command lines 902 s.
NavKit's regression on every level with DICE path data took 447 s. Nothing flaky this run. The coordinator reports
one earlier, load-related flake: O4's navtools failed once only because matplotlib printed "building the font
cache" on stderr (the first run after a cache reset), and passed on a re-run.

### Part 2: the inventory

Every Python twin of the E4a-f, F0, F2a, F2b, F3 and O1-O5 rounds, what Rust answers, what still goes to Python,
who imports it in process (the suite outside its tests, and Project OPFOR's `src\`), and its fate. "Hand-offs" are
the places where `bfbc2.exe` gives the whole command line to the Python (`Fail::Python`, `pyproc::hand_off`) or a
binding returns `None` and the Python reference answers.

**Two rules that held for every candidate.**

- **A command-line `.py` is usually also a library.** Of the 42 tools in `bfbc2core.native.CLI_TOOLS`, 28 are
  imported in process by other suite modules or by OPFOR (`hpdtool` by 20 suite modules, `shaderdb` by 54 OPFOR
  files, `meshview` by 47, `aptgraft` by 10, `bf1943import` by 27). Deleting the file would delete the library: those
  stay until their library part moves (a thin module over `bc2_native`) - a later round per module.
- **The hand-offs are not all damaged input.** 15 tools still do real work only in Python (below), and in a console
  (or with `COLUMNS` set) `--help` and argparse's usage errors of `caches`, `convert`, `audio`, `shaderdb`, `renderqa`,
  `xdiff`, `dbxparity`, `portparity` and `texqa` are handed off (argparse wraps to the terminal's width; the port
  answers only the 80-column form). Retiring any of those needs the width port first.

**The command lines (`CLI_TOOLS`)**

| tool | Rust answers | still Python (hand-offs) | in-process users (suite / OPFOR) | fate |
|---|---|---|---|---|
| `graphstats` | everything but path data the core refuses | refused path data | none / none | **retired (RT1)** |
| `prepare_level` | everything (the editor's prepare) | odd inputs before the first progress line | none (the editor ran it as a process) / none | **retired (RT1)** |
| `fbrbtool`, `dbxedit` | everything (in process since R4) | none | `bfbc2core.fbrbtool` / `dbxedit` are the libraries; OPFOR's `merge_startup.ps1`, `campaign_fill.ps1`, `build_menus.bat` name `fbrbtool.py` as their fallback | keep: 23 / 26-line entry points over the Rust core, not twins |
| `aptcheck`, `aptrefs`, `bc2loc`, `aptgraft`, `apt`, `stfs`, `xiso` | everything but damaged movies / packages / exe data, `lint` with wildcards first | damaged input | `aptrefs` imports `aptcheck`; `menucommon` imports `bc2loc`; OPFOR imports `aptgraft` (10), `bc2loc` (4), `apt`, `stfs` (2); the hub runs `stfs.py` / `xiso.py` | keep: libraries (OPFOR); `aptrefs` and `xiso` are the next CLI-only candidates (xiso's reader has a `BytesIO` path the Rust lacks) |
| `exeref`, `crashreader` | everything on BFBC2Game.exe | x86 the decoder refuses (capstone answers), pefile errors, `BFBC2_CRASHREADER_REFERENCE=1` | `exeref` imports `crashreader`; OPFOR's testrun runs `crashreader.py` without the binary | keep: capstone covers other exes; a later round with the binary required |
| `hpdtool`, `hkpack`, `aimanim`, `convert` | everything but cut / NaN / odd files; console-width usage (`convert`) | damaged input | `hpdtool` 20 suite modules + OPFOR; `hkpack` by `navsample_offline`; `aimanim` by OPFOR and `bf1943import`; `convert.manifest` by 3 tests | keep the files (libraries); their F3 references are below |
| `caches`, `audio`, `shaderdb` | everything (shaderdb's 8 subcommands with merge) | console-width usage, damaged input | 11 suite modules (`caches`), OPFOR `audio` (2), `shaderdb` (54) | keep: libraries; the argparse width first |
| `culltree`, `stockmp` | everything (OPFOR's real calls all Rust) | damaged trees / levels, config shapes the port leaves | tests only; OPFOR runs both through `cli_command` | keep this round: next candidates (`stockmp`'s config hand-offs need a look at real hand-written configs) |
| `levelobjects`, `ladders`, `ordercheck`, `navcompare`, `hpdcheck` | everything but refused inputs | refused inputs | `navkit.py`, `navregress`, the harness, `stockmp.config` | keep: libraries of NavKit |
| `detailed` | the choice (O5) | `--score` (evaluation) | `navkit.py`, `navregress`, `navsample_offline` | keep: `--score` |
| `navkit` | `prepare`, `check`, `compare`, `status`, the tools | `build`, `apply`, `all`, `sample` | the hub, NavKit's GUI, Navigation Studio, 10 suite modules | keep: the build stays Python (O5's list) |
| `navparity`, `navregress` | argument errors, "no reference run" | `run` / every regression build | 13 / 5 suite modules | keep: real work |
| `hpdview` | argument checks | every drawing (matplotlib) | none | keep: real work |
| `sample_all` | argument checks | starts the game | none | keep: by rule |
| `harness` | everything but `run` | `run` (NumPy draws, NavKit builds) | tests | keep: real work |
| `texqa` | the measures | the DICE copies, BF1943 sources, PIL contact sheets, all argparse help | OPFOR `qa_checks` | keep: real work |
| `renderqa`, `xdiff` | argparse, the errors before drawing | every drawing (D3D11, NumPy, Pillow), the Xenos interpreter | OPFOR (4 each) | keep: real work |
| `bf1943import`, `dbxparity`, `portparity`, `dbxcatalogue`, `meshview` | help, `list`, `find`, a current cache | import / check, the parity runs, the pickle cache build, the drawings | OPFOR (27 / 1 / 1 / 8 / 47) | keep: real work |

**The library references (`*_reference`, `use_native`)**

| module (round) | references -> Rust twin | still Python on real data? | recorded answers | fate |
|---|---|---|---|---|
| `aimanim` (F3) | `parse` / `write` / `to_pc_reference` -> `aimanim_*` | no (game, Xbox and BF1943 files: 0 refused) | `tests\f3_golden_aim.json` | **retired (RT1)** |
| `hkpack` (F3, F4) | `Packfile.reference`, `_init_reference`, `_read_classes` -> `hkpack_parse` / F4's `hkpack_open` handle; F4's `read_value_reference` / `read_object_reference` | no | `f3_golden_hkpack.json`, `f4_golden_hkpack.json` | keep: retired on this branch, then put back when main's F4 (merged here) made the handle fall back to `_init_reference` and `test_f4_native` read its Python answers through it (the coordinator: F4's references stay this round) |
| `navsample_offline.file_bytes` (F3) | `file_bytes_reference` -> `navkit_navsample_write` | no | `f3_golden_navsample.json` | **retired (RT1)** |
| `navsample.load` (E3c / F3) | `load_reference` -> `navkit_navsample_parse` / `_v3` | no | version 3 only (`f3_golden_navsample`); versions 1 / 2 compared live | keep: record versions 1 / 2 first |
| `hpdtool.mesh_layout` / `mesh_coverage` (F3) | -> `hpd_mesh_layout` / `_coverage` | no | `f3_golden_hpd.json` | keep: the binding returns `None` where the reference raises its diagnostic ("mesh trailer not found", "tile table at ..."), which `hpdtool verify` prints; port the messages first |
| `audio.xma_parts` / `xma2_riff` / `_xas_blocks` (F3) | -> `audio_*` | no | `f3_golden_audio.json` | keep this round: same message question (the CLI hands damaged headers off) |
| `xenos.disasm` (F3), `parse_shader_python` (E3f) | -> `xenos_disasm_body`, `bc2_shader::xenos` | trees whose microcode was read or edited | `f3_golden_disasm.json` | keep: `disasm` needs a path for edited trees; the decoder is kept by ruling (the translator's live reference) |
| `shaderdb` queries (E4b), its reader (SD1) | -> `ShaderDbHandle`, `bc2_formats::shaderdb::open` | no (0 of 13.6 M answers) | E4b: none (live); SD1: `tests\sd1_golden_*.json` | keep: SD1 (merged here) made them its reference and fallback; they stay this round (the coordinator) |
| `crashreader` / `exeref` `Image` / `Exe` (E4f) | -> `ExeHandle` | other exes (non-standard PE, unproved encodings) | CLI goldens only | keep: other exes |
| `aptgraft`, `effectport`, `meshport`, `testharness.ingame`, `emitteratlas.dedupe` | (references of a faster Python algorithm, not of a Rust port) | - | - | keep: not twins |
| `havokport`, `mopp`, `emitteratlas` (F2b), `terrain`, `mesh`, `skinmesh`, `texwrite`, `soot`, `granny` edits, `havok.convert_wrapper` (F2a), `audio` Wave / SPS, `bc2loc`, `menucommon`, `meshset`, `stfs`, `hpdtool.parse` / `write`, `kymesh` (F0), `navtune`, `xmlinst`, `dbxcatalogue` (F2b), `navsample_offline.terrain_blocks`, `navvehicle` (E4e) | -> each round's binding | no real-data fallback recorded (xmlinst: none needed a Unicode `\w`) | none: their tests compare live | keep: record goldens from the references first, then retire (the RT1 method below); a round of its own |
| `texture` `tiled` / `packed` / `layout` | -> `texture_*` | `tiled` with NumPy arrays (texqa) | none | keep: texqa's independent decoder (R-rounds' ruling) |
| `meshview._build_reference` | -> `meshview_index` | an unreadable bundle (it reports and skips) | CLI goldens | keep: move the skip-and-report into Rust first |

### Part 2: how the twins were retired (the method)

1. **Record first, from the reference.** The F3 goldens had one digest per group of inputs, the reference's answers
   in order. They were recorded again (quick groups; a checkout of main `6a7dd68` with its own module, so the
   references were there) with a second, *marked* digest: an input the Rust core refuses counts as `HANDED` whatever
   the reference said, and the count of refused inputs is kept. The game groups recorded in F3 had no refused input,
   so their digest is their marked one. `prepare_level` had no recording (its proof was live): `test_prepare_o4.py
   --record` wrote `core\tests\prepare_o4_golden.json` from `prepare_level.py` itself (9 runs on sp_009, sp_004_b,
   mp_005: exit codes, both streams, the 56,831 files under `work\` by sha256, the day logs).
2. **Delete, then check against the recording alone.** A retired function must give the recorded answer on every
   input the core answers and raise on exactly the inputs it refuses (`test_f3_native.py`, `RETIRED`). A retired
   command line's lines are compared with their recording where Rust answers, and must end with the retirement
   error where it handed off (`test_cli_o2.retired_check`).
3. **What changes on purpose is listed, not hidden.** O2's goldens keep the retired Python's answers; the four lines
   whose answer now differs are in `cli_o2_nav.NOW` with their new answer.

**What a user sees.** `bfbc2core.native.RETIRED_CLI`: `cli_command(tool)` gives `[bfbc2.exe, tool]` or raises
`CliMissing` ("needs the suite's command-line binary ... python rust\build_native.py --download ... or build it");
`BFBC2_CLI_PYTHON=1` does not bring the Python back. `bfbc2.exe` never hands a retired tool off: a line its port does
not read prints `bfbc2 <tool>: this command line is outside what the Rust port reads ... retired in round RT1` (exit
code 1; `BFBC2_CLI_TRACE` records `retired`). The other tools keep the fallback. `build_native.py --download` now
fetches `bfbc2.exe` with the module (native.yml publishes it), so the retired command lines work without Rust.

### Part 2: the results

| batch | what | proof |
|---|---|---|
| 1 (`839f91a`) | `graphstats.py`, `prepare_level.py` deleted; `RETIRED_CLI`, `CliMissing`, the retirement error in `bfbc2.exe`, the editor on the binary only, `--download` of `bfbc2.exe`, native.yml | `test_cli_o4.py` navtools 222 of 222 (graphstats 30 lines as recorded, 2 the retirement error), navkit 202 of 202 (prepare_level 22 as recorded); `test_prepare_o4.py --game` against the new recording: 9 runs, 56,831 files, the day logs as recorded, 9 of 9 answered by Rust (Rust 61.0 s, the Python 57.1 s when recorded: sp_009's first prepare 46.6 / 37.1 s on a busy PC, the rest 1.1-6.3 s against 1.5-7.8 s); `test_native_required.py` all passed (the download with a stand-in release: no `bfbc2.exe`, a wrong SHA-256, a file that does not run, the real one) |
| 2 (`a5355ce`) | aimanim's and navsample_offline.file_bytes' references deleted (hkpack's put back at the F4 merge, `a6bed07`) | `test_f3_native.py`: aim 6,024 cases (3,113 answered in Rust as recorded, 2,911 refused and raising, as recorded), navsample 1,500 (784 / 716), the four other areas unchanged; `test_cli_o2.py --group nav` 336 of 336 (2 lines in `NOW`); `test_aimanim.py`, `test_navsample_offline.py`, `test_navbuild_native.py`, `test_geometry_native.py`, `test_f4_native.py` (7 of 7 after the merge) all passed |

Retired on purpose, and what it changed (all on built or damaged inputs; no real input changed):

- aim animations: of the 2,911 inputs the core refuses, the retired reference had answered 288 (NaN values in a
  file or a dict, bone maps given as lists, damaged files it read without noticing): they now raise `AimError`. The
  header's own problems keep the reference's words ("too short for an aim animation", "not an aim animation of the
  known layout (version, size)").
- navsample writes: of the 716 refused, the reference had written 135 changed samples (float64 heights, int32 flags,
  a NaN normal); they now raise `ValueError` naming the dtypes the writer takes.
- `bfbc2.exe aimanim` on a file with a NaN (alone and on one pipe) now ends with the reader's error.

**OPFOR** (not changed; its in-process imports of the suite checked against this branch through a toolkit folder
whose `bfbc2-tool-suite` is a junction to the worktree, `OPFOR_TOOLKIT` and `OPFOR_ROOT` set):
`src\tools\tests\run_tests.py` 380 of 380 passed. No OPFOR file imports a retired name (`aimanim.*_reference`,
`navsample_offline.file_bytes_reference`, `graphstats`, `prepare_level`); `weapon_preview.py` and `bf1943_weapons.py`
use `aimanim.parse` / `to_pc`, which keep their signatures. OPFOR's mypy reads the suite at `..\bfbc2-tool-suite`
(its `mypy_path`), so its baseline is unchanged by this branch until merged; ruff unchanged (no OPFOR file changed).
The WW2 rebuild on this branch (`python src\tools\ww2\rebuild_all.py --no-qa` in OPFOR, the same toolkit folder): all 52
steps in 21 min, 10,852 files hashed, **10,852 of 10,852 identical** to the oracle (`round0\hashes_oracle.json`;
saved as `hashes_rt1.json`). `game\` unchanged: 1,582 files, every size and write time the same before and after.

### Left for later rounds

- The next retirements, by the same method: record goldens for the live-compared references (F0, F2a, F2b, E4b,
  E4e) and retire them; `navsample.load`'s versions 1 / 2; the hpd / audio F3 functions once their bindings carry
  the reference's error messages; the CLI-only files `aptrefs`, `xiso` (with a bytes entry point), `culltree`,
  `stockmp`, `exeref` / `crashreader` (with the binary required).
- argparse's console width in the Rust argument parser (the hand-off every help screen in a terminal takes for 9
  tools).
- Library modules that are also command lines: split the library from the `.py` entry point (or make it a thin
  module over `bc2_native`) so the entry point can go.
- The module text of `aimanim.py` (its usage, printed by `bfbc2.exe aimanim` and recorded) still names the retired
  references; change it with the recording (O2's goldens hold it).

## RT2: RT1's leftovers - references recorded then retired, argparse at the console's width (2026-11-20, branch `rust-rt2`)

### The method: recorded answers, replayed (`tests/replay.py`)

RT1 recorded F3's references as one digest per group of inputs. The F0 / F2a / F2b / E4e tests compared each port with
its reference live, call by call, in 18 differently shaped test files; RT2 gives them one mechanism instead of a
rewrite each. `Replay(name, folder, [(module or class, attribute), ...])` wraps each named reference:

- **Recording** (`<test> --record`, then `--game --record`, while the reference is there): every call's answer is
  stored under the reference's name and a key made from its arguments by content (bytes and arrays hashed, a path
  that names a file by the file's bytes, a temporary path by its last part, objects by their attributes), so the
  order of the calls does not matter and a game input changed since is counted as not recorded instead of failing.
  The answer is a digest of the outcome `('ok', canon(result))` / `('error', type, message)` with the arguments'
  paths and the temporary folder taken out of every string. A reference called by the port itself (its fallback: the
  caller is not a test module) marks the input as **refused by the Rust core**: `!` (the reference raised) or `+` (it
  answered) - RT1's marked digest, per input.
- **Replaying** (the reference deleted): a stub is put under the reference's name; `outcome(stub, ...)` gives a
  `Recorded` that equals the port's outcome when that digests to the recorded answer, and for a refused input equals
  the port's `Refused` error (or the reference's own answer, if the port now reads it). `R.pair(name, args, got,
  ref)` does the same for whole pipelines the tests compared through `use_native(False)`; `R.python()` marks a
  deliberate Python run while recording.
- **Files:** `<name>_replay.json` and `<name>_replay_game.json` (only what the quick run did not answer) beside each
  test, format 2: 17 characters an entry (the key's and answer's 48-bit digests in base64 and the mark), strings of
  100 entries, shards under 900 kB. All of RT2's recordings: 4.4 MB (a first format of hex dicts was 12 MB without
  the Wave readers' 12 MB, which were not kept: those references stay).

`bfbc2core.native.refused(message, base)` is what a retired reference's function raises where the binding answers
`None`: a `native.Refused` (a `ValueError`) that is also the module's own error class (`ConvertError`, `AudioError`,
`MenuToolError`, `PortError`, ...), with a message in the module's words ("mesh data: ... the header reader does not
read (cut short or damaged)").

### Item 1: the live-compared references (F0, F2a, F2b, E4b, E4e)

Two forks of this round did the per-test work in parallel (F0 / F2a; F2b / E4b / E4e), each test recorded quick and
`--game` from the unchanged references before anything was deleted, then checked against the recording alone.
"Refused, answered by the reference" counts the inputs the core refuses that the reference answered (they now
raise); a reference was retired only where no real input is among them.

| test | references | recorded (quick + game) | refused / of them answered by the reference | after: replayed, wrong | fate |
|---|---|---|---|---|---|
| `test_havok_classes_native` | `havok.classes_reference` | 752 | 244 / 0 | 502 quick, 508 game, 0 | **retired** (a file it cannot scan: "no Havok class initializers found (not BFBC2Game.exe, or damaged)"; a missing file its `OSError`) |
| `test_console_f2a_native` | terrain's 9, `soot_mesh`, mesh `console_layouts` / `decode` / `relayout`, `havok.convert_wrapper`, `texture.check` | 65,183 game | 1,698 / 5 (damaged wrappers) | 47,206 game, 0 | **retired** |
| `test_meshdata_native` | `mesh.parse` / `write`, `meshset.parse` | 86,493 game | 19,987 / 173 (damaged and changed objects only) | 5,444 quick, 47,386 game, 0 | **retired**; `meshset.write_reference` **kept** (OPFOR edits the dicts before writing: 37 changed dicts the core refuses) |
| `test_texture_helpers_native` | `texture.levels` / `decode` | 38,383 game | 1,113 / 0 | 2,899 quick, 38,383 game, 0 | **retired**; `tiled` / `packed` / `layout` kept (RT1's ruling: texqa's independent decoder) |
| `test_pe_native` | `PEImage._read` / `strings` / `find_cstring` / `find_dword` | 20,698 game | 964 / 0 | 16,505 quick, 20,698 game, 0 | **retired** (non-ASCII text, values outside 32 bits now `Refused`) |
| `test_bc2loc_native` | `menucommon.parse_bigf` | 2,328 game | 1,043 / 0 | 2,232 quick, 0 | **retired**; bc2loc's own **kept** (khash from another start, lone surrogates, LocFile's edits are Python state) |
| `test_console_readers_native` | stfs / xiso | - | - | - | **kept**: xiso's file-object reader has no Rust entry point; with the command lines (item 6) |
| `test_wave_native` | audio's Wave / SPS / SNR | 936,831 game | 121,450 / 102 (changed objects) | - | **kept**: the binding answers `None` wherever the reader gives its diagnostic and `bfbc2.exe audio` hands damaged files to it (the messages first, as item 3 did for F3); recordings not kept (12 MB) |
| `test_model_f2a_native`, `test_granny_f2a_native` | texwrite, skinmesh, Granny writing | - | - | - | **kept**: OPFOR's WW2 pipeline feeds them arrays and objects it builds; their tests compare the core on and off with nothing recorded per call |
| `test_havokport_native` | havokport's 14, mopp's 5 | 28,675 quick, 61,519 game | 1,585 / 206 quick (restructure 2, ray casts 204: NumPy-scalar rays); game: restructure refuses only the 11 calls without a hkUFloat8 table (OPFOR always passes one) | 28,675 / 61,519, 0 | **retired** |
| `test_navtune_native` | `validate`, `save_text`, `summary` | 57,298 quick (no side files in work\ yet) | 6,311 / 1,278 (seeded odd types: NaN, huge ints, non-JSON) | 47,997, 0 | **retired** |
| `test_xmlinst_native` | `instances`, `field_ref` | 3,000 quick, 791 game | 1,665 / 1,665 (the seeded non-ASCII names), 0 on the 791 real XMLs | 3,791, 0 | **retired** |
| `test_navtools_native` (E4e) | `navvehicle.obstacle_distance`, `navsample_offline.terrain_blocks` | 48 | 0 | 48, 0 (game tier against navtools_golden.json as before) | **retired**; the other inline Python paths kept (each serves inputs the core leaves to it by design) |
| `test_dbxcatalogue_native`, `test_emitteratlas_native` | `dbxcatalogue.build`, emitteratlas | - | - | - | **kept**: an unreadable chunk (the game holding a bundle) is recorded by the Python, the Rust refuses the whole build; emitteratlas hands float near-ties to the Python by design |
| `test_shaderdb_queries` (E4b) | the query references | - | - | - | **kept** with SD1's (the coordinator's ruling keeps SD1's references; the queries are what shaderdb's Python mode answers with) |
| `test_pathdata_native` | `hpdtool.parse` / `write`, `kymesh.write_mesh` | 22,589 game (7,022 distinct) | 1,471 / 50 (3 parse, 8 write, 39 write_mesh; all built / changed, none real) | (live) | **kept**: `parse`'s `struct.error` messages are not ported (O2's `hpdtool` lines show them: "info of a cut file", "roundtrip of a folder with damaged files"), and the writers take what NavKit's Python build assembles; the recording is committed for that round |

### Item 2: `navsample.load` versions 1 and 2

The F3 game groups already held every OPFOR sample file of up to 40 MB read whole by the reference (34 version 2,
2 version 1 among 173 files). The quick inputs were not recorded: a new F3 area `navsample12` (400 built files of
both versions x 3: as built, bytes changed or cut after the header, now and then the version changed; a record cut
short; heights including NaN and infinities) was recorded from `load_reference` (1,200 answers, 408 refused by the
core, none of them answered by the reference). A signalling NaN read by the reference's `float()` is quieted; the
core keeps the file's bits: the area compares heights with every NaN made one NaN (no real file has one). Then
`load_reference`, the record loop, NumPy's version-3 pass and `use_native` were deleted: `load` reads in the core
only; a record cut short raises `Refused` ("a version 2 sample file with a record cut short (damaged)") where the
reference raised `struct.error`. Checked: quick 1,200 as recorded (and version 3's 1,500), NavBuild's 12 cases on the
core's arrays.

### Item 3: the hpd / audio messages in Rust, then retired

- **`bc2_formats::hpdlayout`** stops where the reference raises, with its error (`hpdlayout::Error`): its
  `ValueError`s ("tile table at 0x54", "mesh trailer not found", "object 0x...: table 0x38 at ..., expected ...",
  negative values as Python's `#x` writes them) and `struct.error` in CPython 3.13's words ("unpack_from requires a
  buffer of at least N bytes for unpacking 4 / 12 bytes at offset O (actual buffer size is L)", the offset of the
  first read that fails, as the reference's loops reach it; a negative offset counted from the end). A NaN tile item
  is read as CPython's struct reads it (`c_double`: the quiet bit set, the payload moved up), so the core no longer
  refuses NaNs. `hpd_mesh_layout` / `hpd_mesh_coverage` raise `ValueError` / `struct.error`; `bfbc2.exe hpdtool
  coverage` prints `ERROR <file> sector <id>: <message>` itself (O2's lines were handed off).
- **`bc2_formats::audioframe`**: `Error::Audio` (the three EA-XMA messages), `Error::Struct` (struct.pack's
  `'H' / 'I' format requires 0 <= number <= ...`, checked in pack's order: channels, rate, byte rate, samples) and
  `Error::ZeroDivision` (a rate of -1279 to 0: the reference divided by a block of 0 samples); `Refused` is left for
  stream counts past 1,024, 2^24 blocks and arguments only the reference took (a float or `None` where a count goes,
  a negative sample count). The bindings answer `(type name, message)`; `audio.py` raises it.
- Recorded again from the references after the change (the quick groups' marks change: hpd hands nothing to the
  reference now), then retired: `hpdtool.mesh_layout` / `mesh_coverage` 4,804 answers, all in Rust (built meshes with
  NaN tile items poked in, 2,000 damaged, 3 real files); audio 2,434 answers (368 refused: the odd argument types).

### Item 4: argparse's console width in `bfbc2.exe`

`rust/bc2-cli/src/argfmt.rs` is Python 3.13's `HelpFormatter` and `textwrap`'s line filling over each parser's
width-independent pieces, which `rust/bc2-cli/gen_argspecs.py` takes from the Python parsers (the tools' `main()`
stopped at `parse_args`) into `src/argspecs.rs` (22 parsers: caches, convert and its 4 subcommands, audio, shaderdb
and its 8, renderqa, xdiff, dbxparity, portparity, texqa). The width is `shutil.get_terminal_size().columns - 2`:
`COLUMNS` when a positive whole number, else the console window stdout writes to, else 80. At the pipe's width the
ports print the 80-column texts recorded in O2-O5; elsewhere argfmt's. `texqa --help` alone is answered too (it was
handed off at every width).

Proof (`core/tests/test_argfmt.py`, quick tier): 41 command lines (every parser's `--help` and a usage error) x 23
values of `COLUMNS` (10, 24, 33, 40, 47, 55, 62, 70, 79, 80, 81, 90, 100, 120, 160, 250, and ` 100 `, `1_00`, `+64`,
`0`, `-5`, `abc`, unset): **943 of 943** identical to the Python command lines (stdout, stderr, exit code; recorded in
`argfmt_golden.json`), every one answered by `bfbc2.exe` (no hand-off in `BFBC2_CLI_TRACE`). `gen_argspecs.py
--check` fails the test when a parser changes without regenerating.

What is still handed to the Python in these tools: option abbreviations, `--`, attached short values, single-dash
options a port does not know, a second run of positionals, `COLUMNS` that only Python's `int()` reads (non-ASCII
digits).

### Items 5 and 6: not done this round, and why

- **The CLI candidates** (`aptrefs`, `xiso`, `culltree`, `stockmp`, `exeref` / `crashreader`): none can be deleted
  without the library split, because each `.py` is still imported: `aptrefs` by 3 tests and 2 benches
  (`extern_table`, `movie_refs`, `scan_movies`, `cross_reference`, and `refs_many_python`, E4c's reference);
  `xiso` by the hub (it runs `xiso.py`) and its tests (the file-object reader); `culltree` holds F4's string-table
  reference (kept by ruling) and is imported by 5 tests; `stockmp` by 4 tests, and its port still hands off config
  shapes it leaves (hand-written configs not yet compared); `crashreader` by `exeref` and as Project OPFOR's
  fallback when `bin\bfbc2.exe` is missing (OPFOR's `testrun`), and other executables need capstone. 
- **The library split** only pays with a retirement after it; with the candidates above blocked by their importers
  (tests, the hub, OPFOR's fallback) and by hand-offs on option abbreviations (argparse tools) and damaged input, it
  is a round of its own: per module, move the library to `<tool>lib.py` (re-exported from the old name), port the
  tests that call `main()` to `bfbc2.exe`, then add the tool to `RETIRED_CLI`.

### Item 7

`aimanim.py`'s usage (its docstring, which `bfbc2.exe aimanim` prints) no longer names the retired references; the
two O2 rows that recorded it take the new text. `hpdtool.py`'s usage changed with item 3: its six O2 rows likewise.
O2's nav group: 336 of 336.

### Proof (the whole branch, after main's GR1 / DOC1 / RV1 were merged in)

- **Quick tier** (`run_tests.py quick -j 2`, 129 tests, 6,527 s on a shared machine): 127 of 129; the two failures
  passed alone. O4's navtools failed only under load (passed alone, 222 of 222, as RT1 saw); O5's nav group showed
  two `detailed` lines on a cut version-1 sample run ending in `Refused` instead of `struct.error`: `navsample.load`
  now gives the retired loop's `struct.error` in its own words (`a2c65ca`), 92 of 92.
- **Game tier of the retired areas** (`test_f3_native.py --game --only=hpd,audio,navsample`, 566 s): path data 144
  files / 8,858 sectors, 40,032 answers all in Rust, 290 groups as recorded; audio 24,055 answers, 216 groups as
  recorded, 125 BF1943 Waves through `audio.convert` byte-identical; navsample 173 files (2.1 billion columns, 1.0
  billion surfaces in 823 ranges), 365 groups as recorded. The forks' game runs of their tests: 0 wrong, 0 not recorded.
- **Project OPFOR** (unchanged; its in-process imports of the suite pointed at this branch through a toolkit folder
  of junctions - `bfbc2-tool-suite`, `bf1943-tool-suite`, `operation-rising-sun`, `project-opfor-bfbc2` - with
  `OPFOR_TOOLKIT` / `OPFOR_ROOT`): `src\tools\tests\run_tests.py` 380 of 380. The WW2 rebuild (`rebuild_all.py
  --no-qa`): all 52 steps in 30 min, **10,852 of 10,852 files identical** to the oracle (`round0\hashes_rt2.json`);
  `game\` unchanged (1,582 files, every size and write time).
- **Checks:** `cargo fmt --check`; `cargo clippy -p bc2-formats -p bc2-py -p bc2-cli --all-features --all-targets -D
  warnings`; `cargo test -p bc2-formats` (hpdlayout, audioframe) and `-p bc2-cli` (argfmt); ruff; mypy per folder
  (`tests/test_types.py`); `spec/trace.py --check` (439 rules; citations moved with their code, navsample's retired
  loops uncited). The grep for every removed function: nothing in the suite, OPFOR's `src\` or `docs\` calls one;
  the tests reach retired references only through the replay stubs.

## GR1: the Map Inspector folded into the editor, then retired (2026-11-20, branch `rust-gr1`)

The first round of the user's GUI plan (2026-11-19): the five older GUIs inventoried (feature, in the editor?, plan:
`bfbc2-map-editor/EDITOR_ARCHITECTURE.md` section 9), then what the editor lacked of the Map Inspector added
and the inspector retired.

### What the editor gained (`rust/bc2-editor`)

- `inspect.rs`: an object's **property tree** (`PropNode`: name, tag, type in words, the value as DbxEdit's XML
  writes it, the `ref`, the children) read from the scene's own partitions - the level partition, or a project's
  edited partition (`Doc::partition`, re-read when `Doc::revision` changes), and the static groups' files for their
  members; the **links** (`Links`: every entity's references in tree order, resolved by guid; the referrers, the
  level's `LevelData` left out as the inspector did; the `EventConnections` of the level partition's instances, sent
  and received); the inspector's **measure** figures and water depth.
- `app/inspectui.rs`: Properties' *All fields* (collapsible containers, long lists paged by 400, a reference row
  goes to its object), the *Links* dock tab (below Properties in the default layout; four sections; rows go to the
  object, Back returns), the links drawn in every view (`view-link`, a new token: references solid, event
  connections dashed, arrows by direction; K), Measure (M; a path of clicked points and its figures beside the last
  point). Actions: Show links, Show or hide links (K), Measure (M); Links in the object context menus.

### Proof

- `tests/inspect_parity.rs` against the Map Inspector's answers recorded before it was retired
  (`tests/data/inspector_props_*.json.gz`: per object the length and SHA-1 of its tree rows, references and
  referrers as the inspector walked them; recorded by the round's `record_inspector_props.py`, removed with
  `inspector.py`):

  | level | objects sampled | fields | references | referrers | event connections | links read |
  |---|---:|---:|---:|---:|---:|---:|
  | SP_01 | 338 of 13,251 | 44,088 | 15,210 | 1,215 | 2,625 | 0.041 s |
  | MP_005 | 209 of 6,979 | 17,044 | 6,377 | 228 | 797 | 0.016 s |
  | MP_009GR | 284 of 10,005 | 19,933 | 6,605 | 184 | 954 | 0.024 s |
  | ONS_MP_004 | 243 of 7,100 | 21,178 | 7,071 | 312 | 1,244 | 0.012 s |

  All identical. The sample: of every entity type the first, middle and last, every 50th static group member, and
  the 5 most referencing and 5 most referenced objects (`LevelData`'s whole tree among them).
- `tests/gr1.rs`, headless: on the demo level (viewed, and as a project) the flag's Links rows, a click on one
  going there, Back, All fields' types and an array opened, the `view-link` pixels in the view and none after K,
  Measure's two points without changing the selection; on MP_005 in Dark and Light a flag's links and fields,
  followed and back (the round's screenshots).
- The editor's unit tests: `inspect::tests` (links both ways and events on the demo partition, the same read from a
  copy by guid; the tree's types; measure and water depth), the token tests with `view-link` (51 colour tokens, its
  contrast on the lit terrain 5.8 / 5.7, worst case 4.4, on the water 6.3 / 6.2).

### Retired

`bfbc2-map-inspector/inspector.py` and its `tests/test_inspector_offline.py`; its run in `tests/test_viewers.py`; the
hub's "Open in Map Inspector" (the Levels tab opens the map editor, also on a double-click); `bench_sweep.py`'s
`inspector_load`; `suite_paths`' `map-inspector`. `record_inspector.py` (G1's outliner lists) keeps the inspector's
layer rule itself. Nothing in OPFOR (`src\`, its scripts and docs' commands) used the inspector.

## SV1: the simplified view draws render meshes (2026-11-20, branch `rust-sv1`)

`EDITOR_ARCHITECTURE.md` 3.10, "Done in round SV1". The user: "in our simplified view trees and objects sometimes look
unrecognizable, like spike balls". No Python replaced.

**What changed.** The simplified (wgpu) view drew each blueprint's Havok collision; for vegetation that is the render
mesh's leaf cards without their alpha-tested textures (round RV1's finding). It now draws each object's render mesh,
read by `bc2_render::simple` with the game-accurate view's own rule and reader (`world::mesh_and_set`,
`material::prepare_untextured`: `prepare_set` without reading textures), as plain geometry instanced per blueprint:
alpha-tested parts cut by their diffuse texture's alpha (its level no larger than 64 texels, one atlas), a muted tint
from the texture's mean colour, blended parts left out beside others. The meshes are read on a thread beside the open
view (collision until they arrive). Invisible walls, blueprints without a mesh, meshes of blended parts only and
occluder meshes are the `invisible` group, hidden at the start. Show as (view, object, blueprint), levels of detail and
destruction states: `display.rs`.

**Reading the render meshes** (release, `examples/simple_meshes.rs`; the editor's debug build, on its loading thread):

| level | placed | render meshes (with more LODs) | destructible blueprints | LOD 0 triangles | read, release (cold / warm) | debug, in the editor |
|---|---:|---:|---:|---:|---:|---:|
| MP_005 | 4,831 | 146 (124) | 48 | 207,471 | 0.48 / 0.13 s | 0.5 - 1.0 s |
| SP_01 | 6,526 | 205 (114) | 38 | 201,789 | 0.57 / 0.15 s | 0.9 s |

**Frame times** (`tests/simplified_view.rs`: the view drawn at 1600 x 950 and the GPU waited for, the camera turning
0.01 rad a frame, the median of 10 after 2; GeForce RTX 4060 Laptop, Vulkan; two other rounds' builds beside it):

| level, view | triangles listed: collision / render meshes | collision (before), ms | render meshes (after), ms |
|---|---:|---:|---:|
| MP_005, the whole level (the opening camera) | 3,919,352 / 5,486,712 | 4.44 | 5.59 |
| MP_005, near (60 m from the barrack) | | 4.06 | 4.66 |
| SP_01, the whole level | 5,246,844 / 6,492,925 | 5.65 | 6.52 |
| SP_01, near (60 m from the guard tower) | | 4.82 | 5.12 |

Release build of the test. The render meshes list 25 - 40 % more triangles than the collision (the buildings' meshes
are finer than their collision; the vegetation's are the same cards) and the alpha test runs on the leaves: 0.3 - 1.2
ms more a frame, 5 - 7 ms in all, and the view draws only when the camera or the scene changes. Reading the meshes:
0.13 s a level warm (release), on a thread beside the open view.

Instanced per blueprint as before: one draw call per (blueprint, level of detail, state) shown, plus one per
edge mesh in the Both / Wireframe / Bounds displays.

**Proof** (`cargo test -p bc2-editor --test simplified_view`, MP_005 and SP_01 headless):

| what | result |
|---|---|
| the view draws the render mesh: indices against `bc2_render::world::World`'s prepared mesh of the same entity | mebush 1,422 / 1,422; acacia 5,250 / 5,250; rock formation 1,755 / 1,755; cliff 3,054 / 3,054; palm 5,610 / 5,610; rock_large 2,490 / 2,490; jungle bush 2,085 / 2,085 (all parts); barrack 18,270 of 18,330 (its glass part, blended, left out); guard tower 13,107 of 13,113 (its intact state: `ruin_downed` hidden) |
| a leaf-card blueprint renders with holes | MP_005 acacia 12,449 pixels with the leaves cut against 20,655 drawn solid (60 %); SP_01 palm 16,232 against 28,596 (57 %) |
| the invisible walls (SP_01, 396 `invisiblewall` placements) | hidden at the start; Outline: every one as edges; Solid: every one solid |
| Show as, by index counts (MP_005 barrack / SP_01 guard tower) | Visual 18,270 / 13,107 (render mesh); Collision 20,346 / 9,963 vertices (collision); Both: the mesh and the collision's edges; Wireframe: the mesh's edges; Bounds: a 36-vertex box |
| per object and per blueprint | Hidden draws nothing; Collision the collision; the blueprint's others as Bounds; the object's own override wins; the Collision view mode still draws only collision |
| an override survives the editor's settings saved, read again and the level reopened; Reset view overrides clears it | yes |
| the LOD picker | barrack LOD 0 18,270 / LOD 1 6,516 indices (its set: 18,330, 6,576, 2,583, 663); guard tower 13,107 / 3,528 (13,113, 4,497, 2,325) |
| destruction states | guard tower intact / destroyed / all parts 13,107 / 6 / 13,113 indices; MP_005's barrack 18,270 for all three (its mesh holds only its ruin and windows: its parts are another mesh's) |
| picking: a click where each sampled object is drawn picks it | MP_005 51 of 51, SP_01 37 of 37 |
| picking at 60 objects' centres, as before (collision drawn) and now | MP_005: the same object 55 times, the object itself 24 before / 23 now (two thin `mp05_sticks` lost: their mesh is thinner than their collision); SP_01: 35, 3 / 13 (the invisible walls and slabs drawn solid before covered the rest) |
| the six view modes | each draws |

Unit tests: `simple::tests` (DXT1 1-bit alpha, DXT5 alpha and the level chosen, the diffuse texture by name, DICE's
layered building materials, the guard tower's health states choosing its parts), `display::tests` (overrides in
order, saved and read, reset).

**Variants the data holds** (the user asked which): levels of detail - yes (the mesh set's `lods`, each with its
`_lod<k>_data` resource and layouts; 2 - 4 levels on 124 of MP_005's 146 and 114 of SP_01's 205 render meshes);
destruction states - yes, from the blueprint's `PartComponentData.HealthStates[].MeshName` (SP_01's guard tower:
`part_1` 500 / `part_1_dest` 400 / gone, `part_2` the same, `ruin` 250 / `ruin_downed` 0), matched to the mesh's
parts by name; collision per state - not available from what the scene reads (`bc2_scene` gives one triangle list per
blueprint; the Havok packfile's per-part bodies are not split out), so not offered.

Screenshots (not in the repository): MP_005's rock formation, bushes and barrack, SP_01's palms, invisible walls (also
outlined) and guard tower, before and after, Dark and Light; the barrack and the guard tower shown as collision and
as visual + collision.

## GR2: the Navigation Studio folded into the editor's AI workspace, then retired (2026-11-20, branch `rust-gr2`)

The second round of the user's GUI plan: what `EDITOR_ARCHITECTURE.md` 9.2 listed of the Navigation Studio added to
the map editor's AI workspace, proven on the Studio's recorded answers, then `studio.py` and `editing.py` retired;
`overlay.py`'s headless exports stay. The last commit with the Studio: a0a1379.

### What the editor gained (`rust/bc2-editor`, `rust/bc2-levelwrite`)

- `navdata.rs`: a path-data file as the Studio drew it (`navdata.load`: the areas, the 0.8 m cells by
  `bc2_navbuild::parity::area_cells` with the terrain's heights, the route graph, its islands by the same union-find
  and ordering, ladder links and ladder ground), `compare_cells`, the spawns' islands, the islands' gaps (Python's
  float floor division for the 10 m buckets), `navpath.route` (A*, `bc2_navbuild::astar`, the `fma` norm), and
  `overlay.py`'s AI layer (read from the binary partition), `NavIndex`, anomalies and compare rows.
- `navview.rs` (the colourings' classes and colours, the cells as quads, the line layers), `nav.rs` (the state, the
  side file in `navtune`'s form through the Rust core's `navtune::dumps` / `validate` / `summary`, `bfbc2.exe navkit`
  jobs, the install and restore plans, the exports), `app/navui.rs` (Navigation, Islands and spawns, Navigation
  edits, the confirm dialog, clicks, hover). `render.rs`: overlay meshes (uploaded once per key, slots reused).
  `bc2_levelwrite::navedit`: the edits as operations of the project's log (`Op::NavAdd`, `NavRemove`, `NavLoad`).
- 25 colour tokens (`view-nav-*`, `view-ref-*`): 76 in all; each 3.5:1 or more on the lit terrain and the water in
  both themes (worst 3.6).

### Proof

- `tests/nav_parity.rs` against the Studio's answers (`tests/data/navstudio.json.gz`, 7 KB of digests):

  | level | NavKit cells / islands | DICE cells / islands | read (both files, release) | path tests |
  |---|---:|---:|---:|---:|
  | SP_01 | 1,096,339 / 435 | 1,026,634 / 506 | 3.3 s | 3 |
  | ONS_MP_002 | 454,416 / 68 | 441,463 / 28 | 0.7 s | 3 |
  | ONS_MP_004 | 253,157 / 41 | 241,046 / 42 | 0.5 s | 3 |
  | SP_009 | 84,528 / 3 | 82,764 / 3 | 0.1 s | 3 |

  Per file: the summary, the areas (sector, index, tile, layer, pitch, heights, cells, island, ladder, boundary
  pieces), the cells and their heights to the centimetre, the nodes' islands, the islands, the five colourings'
  classes, the spawns' islands, the islands' gaps; per level the difference figures and both cell matches, the
  anomaly lists, the path tests (nodes, cost, length, joins, islands, the reason, the gap) and the cost change; and
  the compare layers (counts, HasAI, SectorOffsets, every row with its NavKit lookups) of ONS_MP_004 and MP_002
  against `level:ons_mp_002`, SP_009 against `level:sp_01` (129 patrols) and GUADAL_CANAL's (WW2) own layer. All
  identical. The Studio needed about 45 s for the four levels (Python and the Rust core); the editor 4.6 s.
- `tests/gr2.rs`: ONS_MP_004 in Dark and Light - both files drawn (8,285 main-network pixels), the five colourings
  each drawing its own picture, the outlines layer, a path test between two spawns (NavKit 970.1 m, DICE 817.9 m),
  211 reference markers; SP_009 as a project on a scratch OPFOR copy (DICE's file only: a DICE base) - a route edge, a
  ladder traversal, a link and a note drawn by clicks, undo and redo, saved with the project, the side file read back
  equal, `bfbc2.exe navkit apply` (173,492 -> 173,516 bytes), Install (plan, confirm: DICE's kept as `.opfor-orig`,
  the traversal annotations beside it), Restore (plan, confirm): the scratch game folder's 3 files byte for byte as
  before. The game copy is never written.
- `bfbc2-navigation-studio/tests/test_overlay_offline.py` (quick tier): `overlay.py`'s AI layer, sources and
  anomalies on made-up data.

### Retired

`studio.py`, `editing.py`, `tests/test_studio_offline.py`, `test_editing_offline.py`, `test_editing.py`,
`tests/test_viewers.py` (it ran only the Studio), `bench_sweep.py`'s `studio_load`, the review test's Studio parser
check, the hub's "Open in Navigation Studio" and its NavKit file field (now "Open navigation": `bfbc2-editor <level>
--navmesh`). Kept: `overlay.py` (its README describes only it), `suite_paths`' `navigation-studio`, `core`'s
`viewer.py` and `pick.py` (no tool uses them now; `test_errors.py` and `test_hub.py` do). Nothing in OPFOR's
`src\` or scripts called the Studio; its README and DEVELOPMENT_NOTES were updated.

### The project format's version (the coordinator's audit finding)

`bc2_levelwrite::project::VERSION` was 1 through E2b, E3b, E3d; GR2 makes it 2 (the navigation operations). Loading
migrates older files step by step (`migrate`; 1 to 2 changes nothing), refuses a newer file in words ("made by a
newer editor version (project format N); this editor reads up to 2") and names an operation of an unknown kind
(`ProjectError::UnknownOperation`) instead of a bare serde error. Tests: `project::tests` (a version-1 file loads
with the same log, a version-9 file and a `warp` operation give their messages); `tests/project_versions.rs` with
`BC2_PROJECTS` on the canary examples' projects (Aurora's first export and E3d's six: 2 - 4 operations each, the
mode choices with their donor layers) written as version 1: all 7 load with every operation, as version 1 and 2,
and the newer-version and unknown-operation cases on each. The full versioning work is round V1's.

## QW1: the audit's quick wins (2026-11-20, branch `qw1`)

Audit AU2's "best value for effort" list (`P2`, `S4` + `P5`, `P3`, `A6` + `A7`, `A10` + `F14`, `S1`, `F5` + `F10`,
`A19`, `A21`, `Q9`, `Q10`, `S9`). Measured on the development PC (16 threads, 15 GB) while two other rounds built and
tested beside it: the times are a shared machine's. Both quick tiers ran with `CARGO_INCREMENTAL=0` and
`CARGO_BUILD_JOBS=4` in this worktree's own build folder (`rust-targets\bfbc2-tool-suite-qw1`), the "before" on `main`
`fac36d0` with its debug folder deleted first, the "after" on this branch with its debug folder deleted first.

| | before | after |
|---|---:|---:|
| `python run_tests.py quick -j 2` | 4,677 s (126 of 127) | 2,276 s (125 of 127; see below) |
| the cargo entry (fmt, clippy, workspace tests) | 2,275 s | 1,642 s |
| the editor entry (`tests\test_editor.py`) | 2,073 s | - (its tests run in the cargo entry) |
| debug folder after the tier | 25 GB | 3.5 GB |
| of it `debug\build\bc2-editor` | 16 GB, 74 hash folders | 0.55 GB, 15 |
| test binaries in the workspace | 57 | 30 |
| editor relink (`bc2-editor\src\lib.rs` touched, `cargo test --workspace --no-run`) | 76 s | 18 s |
| a full `cargo test --workspace --no-run` from an empty debug folder (after clippy) | - | 143 s |
| bundle cache (`bfbc2 caches --report`, fbrb) | 16.0 GB, 129 files | 15.6 GB, 125 files |
| kept test durations (`test_times.json`) | 288 names, 60 of no tier | 228 |
| `--plan` on 15 GB (`--mem 11.5`, quick) | 6,579 s expected | 2,285 s expected |

The two failures after: O2's command lines (`OSError: [Errno 22] Invalid argument` opening a stand-in `b.exe` the
test wrote into its temp folder) fails the same way on `main` (the "before" run) and alone: this PC's, not QW1's;
`test_native_required` failed on its own new checks (a cargo debug build of the editor has no fingerprint; the
retired-tool check set the old cache) and passes after the test's fix (the commit after the tier).

**P2: the editor's tests once.** The quick tier ran the editor's, levelwrite's and the renderer's cargo tests twice:
in `test_rust.py`'s `cargo test --workspace` and again in `test_editor.py`'s `cargo test -p bc2-editor -p
bc2-levelwrite -p bc2-render`, which unified other features and so built a second copy of everything (the 74 hash
folders). Only the workspace run is left; it runs with `--nocapture` and prints the editor's evidence lines and the
parts skipped. The same test functions run: `cargo test --workspace -- --list` before and after, the three crates'
tests by file and function (`file::function` after, the binary's file name before): bc2-editor 41 integration and 55
unit, bc2-levelwrite 10 and 24, bc2-render 4 and 21 - identical sets; the workspace 592 tests before, 593 after (the
new `fbrb::cache::tests::superseded_payloads`).

**S4 + P5: debug info and test binaries.** `[profile.dev] debug = "line-tables-only"` and one `[[test]]` per crate
(`tests/main.rs` with each file as a module, `autotests = false`): bc2-editor's 19 test executables (each ~430 MB
with its PDB), levelwrite's 7 and render's 4 became 3. The files' tests used to run in separate processes one file
after another; in one process each test takes `gate()` first (two at a time), so the real-level tests never all hold
whole levels at once.

**P3: memory.** 25 declarations raised to their recorded peaks (the job object's `PeakJobMemoryUsed`, children
included): the cargo entry 2 -> 12 GB, NavKit's full regression 3 -> 15, O4 live 1 -> 6.5, F2a model tools 1 -> 5.7,
F0 textures 1.5 -> 5.4, E3o game 2 -> 5.5, F0 console containers 0.5 -> 2.8, ...; the four undeclared quick entries
declared. `make_jobs` takes the larger of a declaration and the last recorded peak, so a declaration that falls behind
is corrected by the next run; a test above `--mem` runs with nothing beside it (as before).

**A6 + A7 + A19: fingerprints per artefact.** On this checkout: the module's sources are 9 crates / 205 files,
`bfbc2.exe`'s 11 / 296 (with the 26 Python files whose docstrings it includes and `VERSION`), the editor's 10 / 237 (with its
three fonts), `mapexport`'s 5 / 138. `bin\*.exe --source-hash` equals `build_native.py --fingerprint <artefact>` for all
three binaries after `build_native.py` and `--editor` (359 s for the editor's release build). A stale `bfbc2.exe`
warns once per process; `BFBC2_STRICT_NATIVE=1` makes it an error.

**S1: superseded bundle-cache payloads.** The audit's "28 bundle names with 2-3 payloads, 3.8 GB superseded" were
not superseded: each name was two bundles, a level's `game\Dist` and `game\Package` copies (the cache file's name
keeps only the last two path parts; its header names the whole path), both current. Reading every payload's header:
129 payloads of 129 bundle paths; 4 payloads (0.43 GiB) named bundles rewritten or gone (EX1's temporary stage
folders, `work\mapedit\...\stage`). `bfbc2 caches --prune fbrb` deleted those 4 (436.7 MB): 16.0 GB -> 15.6 GB.
From now on writing a bundle's payload deletes its older payloads at once.

**A10 + F14 + A21 (Project OPFOR).** One suite locator (`opforcore.paths.suite_dir`; 12 copies replaced, one of which,
`ww2\menu_art.py`, ignored `OPFOR_TOOLKIT`) and `SUITE_REQUIRES = ('0.9.0', '0.10.0')` checked at import (a warning
once, `OPFOR_SUITE_STRICT=1` an error). One GUID helper (`opforcore.guid`, 10 named scopes) for the 24 inline
`uuid5(NAMESPACE_URL, ...)` sites: `test_guid.py` holds each site's old formula against the helper on real names and
300 random inputs (a number part too), every GUID equal. Against this branch (a toolkit folder of junctions -
`bfbc2-tool-suite` to this worktree, `bf1943-tool-suite`, `operation-rising-sun`, `project-opfor-bfbc2` - with
`OPFOR_TOOLKIT` / `OPFOR_ROOT`): OPFOR's tests 381 of 381 and opforcore's 75 of 75. The WW2 rebuild (`rebuild_all.py
--no-qa`): the first run stopped at step 30 - `type95_icon.py` gives the helper its image index as an int, which the
inline f-string had formatted and `str.join` refused - so a part is now formatted as an f-string formats it
(`format(part, '')`); the second run: all 52 steps in 30 min, **10,852 of 10,852 files identical** to the oracle
(`round0\hashes_qw1.json`); `game\` unchanged (1,582 files, every size and write time).

## EX1: every name of an export, its preview, the Diff panel; LevelDiff folded in and retired (2026-11-20, branch `rust-ex1`)

The user's requests (2026-11-20): every name of an exported level editable and checked together; the staged export
opened beside the original; a visual diff of a project against its base (and of any two levels); LevelDiff become
the editor's Diff panel and a `bfbc2.exe leveldiff` command, proven identical, then retired.

### `bfbc2.exe leveldiff` against LevelDiff.exe

`bc2_formats::dbxedit::leveldiff` is `LevelDiff.cs` ported on DbxEdit's own model (`EditFile`, `value_text`,
`PlacedObject`), its quirks kept (a list reorder reported only while the instance has no list entry added or removed
so far; a field path seen twice keeps its first place; texts cut at 120 UTF-16 units; .NET's `Math.Round(x, 1)` and
`G15` for positions). `bfbc2 leveldiff` prints as .NET's `Console` does (`\r\n`, the console's code page off a
console, Unicode on one) and logs to `leveldiff-<date>.log`.

LevelDiff.exe's answers were recorded **before** the port ran, on 21 cases over 13 partitions of OPFOR's game copy
and the E3d stages (stock against export: SP_01 / Aurora as WW2 Conquest, MP_005 / Atacama Rush as Conquest,
Guadalcanal / as BFBC2 Rush; mode variants MP_005 / MP_005GR, NAM_MP_002CQ / R and their descriptions; a WW2 level
against its rebuild, Guadalcanal against `work\ww2\install_guadal_canal`; MP_005's Dist copy against its Package
copy; a level against itself; and the bad input: one file, three files, a switch without its value, a file that is
not binary, a missing file, a `--json` folder that is not there) with every switch (`--match-objects`, `--type`,
`--ignore` twice, `--summary`, `--all`, `--json`), and on LevelDiff's own 7-instance test levels (9 cases). All 30
are identical: stdout and stderr byte for byte (the log's path and the pairs' folder normalised), the `--json` file
byte for byte, the exit codes (0, 1, 2, 3). `tests/leveldiff.rs` (bc2-cli) repeats them: the test levels against
LevelDiff.exe's full texts (`tests/data/leveldiff/synth`), the real pairs against their digests
(`tests/data/leveldiff/leveldiff_golden.tsv`; no game content in the repository).

| Case (best of 3, release, whole process) | LevelDiff.exe | `bfbc2 leveldiff` |
|---|---:|---:|
| SP_01 against Aurora as WW2 Conquest (9,616 / 8,109 instances) | 0.97 s | 0.46 s |
| the same, `--all --json` (2.7 MB text, 3.6 MB JSON) | 18.15 s | 0.46 s |
| MP_005 against MP_005GR, `--match-objects --all --json` | 8.13 s | 0.17 s |
| Guadalcanal against it as BFBC2 Rush, `--all --json` | 3.30 s | 0.15 s |
| NAM_MP_002CQ against NAM_MP_002R, `--json` | 0.33 s | 0.12 s |
| MP_005 against itself, `--json` | 0.25 s | 0.20 s |

LevelDiff.exe looked each listed object's placement up by scanning every placed object (`--all` on big levels); the
port indexes them once.

### Every name of the level (`bc2_levelwrite::names`)

From the data (OPFOR's game copy: 83 level descriptions in the startup bundles, every level folder, `maps.ini`,
OPFOR's `server_browser.cpp`, the loader movie's `initLoadingScreen`):

- the folder under `Package\levels\` (or `Dist\win32\levels\`) is the id in lower case on all 83 (8 ids mix cases:
  `BC1_Harvest_Day_CQ` ...): the id and the folder are one name; the form shows both and a new folder renames the id;
- the on-screen name is the description's `Name` key's text in each of the 8 languages; DICE gives every mode of a map
  the map's keys (82 keys end in `_NAME`, 14 are `<ID>_NAME`; `MP_005GR` reads `MP_005_NAME`; Napisal's
  `ONS_MP_005GR` has a literal, no key); the loading screen cuts the key at its first `_NAME` for its picture;
- the launcher and OPFOR's Server Browser show `maps.ini`'s `name=` (the browser's row is `OPFOR - <name> (<mode>)`,
  cut at 159 characters, its section found by the level id's last part); the browser's map column is the
  description's name key, localised;
- the menus' level list holds `<id>.dbx` and `<id>_description.dbx` for 78 of the 83 (Napisal's 5 Onslaught levels
  are in none: OPFOR starts them from its own menus).

`names::check` refuses in words: an id that is a stock level or another project's installed export (an earlier
install of the same project may be replaced), an id holding `_NAME` (the picture looked for would be another: the
black loading screen), an id whose `<ID>_NAME` / `<ID>_DESC` the localisation already has or whose loading picture
the awards bundles hold (`SP_05_A` would rename Heavy Metal), characters or lengths the files cannot hold (A-Z, digits,
`_`, 3 - 40, a letter first; the browser name plain ASCII without `"` or outer spaces and its row within 159; names
up to 64, descriptions up to 400), a folder that is not the id, a browser name another map has, a letter a language's
text table has no code for. `loc::encode` stores a text through its language's histogram (one-byte codes, else a
lead byte and `0x80 | low` as every two-byte character of the Japanese file is), checked by `decode`.

Proof:

- `tests/export_ex1.rs`: the rules against the game copy (each refusal's words; the build refuses what the check
  refuses, nothing staged); E3d's Atacama canary built as `MAPEDIT_EX1_RENAMED` with its own server-browser name and
  on-screen names (French and Russian of their own): in 12 - 16 s, the id in `LevelData.Name`, the description, the
  menus' list items, all 11 `package.mft` blocks and 11 `bundleManifest` lines, the name keys and their picture;
  `mapexport check` passing its 33 lines (two new: the names' rules, every place naming the same level); the overlay an
  install builds holding each language's text (French with its accents, Russian in Cyrillic,
  decoded through their histograms).
- OPFOR's `mapedit_install --apply` then `--revert` of that stage on a scratch copy of the six shared files,
  `--check-game` the real game copy: the plan's check passes with the names lines; while installed the level's folder,
  `maps.ini`'s `[MAPEDIT_EX1_RENAMED] name=Atacama renamed (EX1) Conquest=Levels/MAPEDIT_EX1_RENAMED`, the browser row
  `OPFOR - Atacama renamed (EX1) (Conquest)`, a `package.mft` block per bundle, 11 manifest lines, the description in
  `startup-00`, both texts in all 8 languages (French and Russian their own), the loading picture under the name key;
  after the revert every shared file byte for byte with its last-writer record, the folder gone; `game\`: its 1,582
  files kept their size and time and the shared files their sha256.

### The Diff panel and Preview export (`bc2-editor`)

`compare.rs` turns LevelDiff's answer into rows (added, removed, moved - the `Transform` changed -, changed), grouped
(the mode layer of either side, `modelayer::layer`; placed objects whose blueprint the export's closure copied;
the campaign content the export strips; the world), and the marks a view draws on either side. `app/diffui.rs`: the
Diff panel (filters, rows, a row's fields as LevelDiff prints them, Inspect), Compare with base (again by itself after
each edit), Compare with level, Preview export (the stage opened as a Package level, `compare::stage_level`, beside the
base, the project kept aside; T switches, the same camera; Close preview), and the 3D marks (tints, boxes, the moves'
lines). 4 new tokens: `view-diff-added` `#5cf0a8`, `-removed` `#ff8fb6`, `-moved` `#7cc8ff`, `-changed` `#e2a8ff`
(Light 6 per channel lighter), each at least 45 degrees of hue from the selection's orange and from each other, each
3.5:1 on the lit terrain and the water (`theme::tests`).

`tests/ex1.rs`, headless, Dark and Light:

| | rows | compared in | what it shows |
|---|---:|---:|---|
| Aurora made WW2 Conquest with Guadalcanal's radar bunker, against SP_01 | 1,662 (278 added, 395 removed, 989 changed) | 0.62 - 0.68 s | the WW2 layer added (277), the campaign's spawns, player and menus removed (215 campaign, the rest its old layer), the bunker added; a click on its row selects it |
| Atacama Rush as Conquest, the stage against MP_005GR (Preview export) | 1,268 | 0.25 s | the Conquest layer added (317), the Rush layer removed (821); T to the original and back; Close preview gives the project back |

The Export window's Names section with the canary's names (the per-language rows open), and an id holding `_NAME`
refused in words.

### Retired

`bfbc2-diff-workbench\` (`level-diff\LevelDiff.cs`, `build.bat`, `tests\test_leveldiff.py`, the folder's README of
planned tools, kept in `DESIGN.md` 4.7); the hub's Level diff tab runs `bfbc2.exe leveldiff`; `run_tests.py`'s three
LevelDiff entries (the build, the built levels, MP_005) are bc2-cli's cargo tests; `tests\test_cli_errors.py` checks
`bfbc2 leveldiff`'s error. OPFOR: `opforcore.paths.leveldiff()` is the command, `paths.exe('LevelDiff')` refuses with
it (nothing else in OPFOR called LevelDiff). `RETIRED.md` has the row.

## GW1: the game-accurate view opens the WW2 levels (2026-11-20, branch `rust-gw1`)

Round RV1 found the game-accurate view failing on Guadalcanal: "no WeatherSettings / WeatherSystem partition under
weather/wake_island/ ... r_guadalcanal/". **Root cause:** the view found a level's weather by guessing folders (the
weather entity's blueprint path up to `/weathersystementity`, the partition's weather folder, a folder a placed
blueprint lies in, `weather/r_<lv without _>`) and in each looked for partitions *named* `weathersettings*` and
`weathersystem*`. Guadalcanal's entity `weather/wake_island/guadal_canal_e` (in its own `level-00.fbrb`) names
`weathersystem_wake` and a settings partition named after the level, `weather/wake_island/guadal_canal`
(`ActiveState` 2); Iwo Jima's likewise (`.../iwo_jima`). So no settings were found. Wake Island and Coral Sea worked
only because they use `weathersettings_wake`.

**Fix (`bc2_render::world::resolve_weather`, `weather::entity` / `entities_in` / `read_system`):** the game's
reference chain. The level's `WeatherSystemEntityData` - an instance of the level partition (SP_004_B, Coral Sea,
Wake Island) or the object of a blueprint a `ReferenceObjectData` places (`ReferencedObject`: MP_005, Guadalcanal,
Iwo Jima) - names its `WeatherSystem` and `Settings` partitions by path; each is read through the level's stack
(first bundle wins, deleted markers hide). A caller's folder is read by name only when no entity resolves; with
nothing, `weather::neutral` and the status line says "No weather data found for this level; showing a default
sky" (`World::default_weather`, `GameView::status`) instead of failing. Spec: RNDR-WTHR-1 corrected, RNDR-WTHR-5 and
-6 new.

| level (read in place from OPFOR's game copy) | weather state drawn | load (release) | draws | terrain coverage, above / corner |
|---|---|---:|---:|---|
| Guadalcanal | `weather/wake_island/states/guadal_canal` | 16.6-25.7 s | 15,322 | 576/576, 477/477 |
| Iwo Jima | `.../states/iwo_jima` | 17.0 s | 12,263 | 484/484, 453/453 |
| Coral Sea | `.../states/sunny_day` (settings 0; see below) | 11.5 s | 12,618 | 900/900, 520/520 |
| Wake Island | `.../states/sunny_day` | 19.4 s | 17,848 | 576/576, 481/481 |
| NAM_MP_002CQ, NAM_MP_005R (Vietnam) | `weather/nam_mp_002/states/mp_002_default`, `weather/nam_mp_005r/states/default` | 7.4 s, 10.9 s | 8,398, 8,937 listed | - |
| ONS_MP_002 (Onslaught) | `weather/default/states/default` (Package: `weather/r_mp02/states/midday`) | 0.4 s | 18,974 listed | - |

Stock and Onslaught levels prepared under `work\levels` (37): the reference chain gives the state the old folder
read gave on all 36 where it worked; SP_004_B (folder `weather/r_sp04`, not its name) now resolves through its
partition's own entity. E3d's canary stages are projects, not installed levels: not openable as levels, not checked.

**Open (RNDR-WTHR-6):** 34 of 43 levels read have an enabled `WeatherStateEntityData` with `StartTransitionOnSpawn`
naming another state (Coral Sea's names 4, `coral_sea`; some name more states than the system has, so they belong
to scripted logic). Whether the engine applies it as the level loads is not known (no capture); the view draws the
settings' `ActiveState` and notes the transition. Unchanged from R5c: the sky is the fog colour (no sky pass), and
the WW2 shores show ragged terrain patch edges.

Tests: `bc2-render` `tests/weather_refs.rs` (the 4 WW2 levels' states from their bundles in place, other Package
levels, the 37 prepared levels against the folder read, the default sky), `weather::tests::weather_entities_and_spawn_states_read_from_a_partition`;
`bc2-editor` `tests/game_view_ww2.rs` (Guadalcanal and Iwo Jima by default, `BC2_GW1_LEVELS` for more, opened
through a temporary stand-in OPFOR folder whose `game` is a junction: each level's own state, the key light, and
whole_terrain.rs's coverage from above and from outside a corner, with a 5 m near plane for the 2 km maps).

## VW1: the editor's view toggles (2026-11-20, branch `rust-vw1`)

`bfbc2-map-editor/EDITOR_ARCHITECTURE.md` 3.10, "Done in round VW1". The user's requests of 2026-11-20: effects,
lights and sounds with their reach; the gameplay areas as the game uses them; a player's-eye camera; the navmesh and
cull tree modes checked; collision per destruction state with an up / down stepper. No Python replaced.

**What the data holds** (MP_005 / SP_01, `overlays::load`; every size read from the partitions):

| overlay | MP_005 | SP_01 | fields |
|---|---:|---:|---|
| effects (placed) | 270 | 484 | `GameEffectEntityData.Effect`; emitters' `SpawnPositionData.Pre` sphere (`Radius` x `Scale`) or box (`Dimensions`) |
| lights | 0 | 15 | emitters with `ActAsPointLight`: `PointLightRadius` (5 - 10 m), `PointLightColor`, `PointLightPivot` |
| sounds (emitters and areas) | 209 | 433 | template `MasterMinDistance` / emitters' `MinDistance`, `DistanceSlope`; `SoundAreaEntityData` `Geometry` / `OuterGeometry` |
| combat areas | 1 | 6 | `Geometry` (22 points on MP_005, `Height` 0: drawn 20 m, labelled unbounded), `SurroundingGeometry`, `Team` + `UseTeamRestriction` |
| capture areas | 5 | 0 | `Geometry` (area chain with `Height`, or `SphereGeometryEntityData` 36.8 / 26.3 m on MP_005's islands), `CaptureRadius` (RUS base 1 m), `UnableToChangeTeam` (2 HQs; the US base has no capture area) |
| spawn protection | 8 | 190 | `SoldierSpawnEntityData.SpawnEnemyRadius` (20 m) at each `AlternativeSpawnPoints` |

Not in BC2's data: light entities (lights are emitters), spot lights, a sound's maximum distance (the only
`MaxDistance` is the BigWorld one-shot settings'), safe-zone volumes, a prone pose. MP_005 places no lights (its three
light emitters belong to the repair tool and the detonation pack).

Reading them (release, in the test, the level's partitions already in the OS cache): MP_005 0.05 - 0.09 s, SP_01
0.15 - 0.25 s, once per level when its look is first applied.

**Frame times** (`tests/view_toggles.rs`: the whole level from above at 1600 x 950, the GPU waited for, the camera
turning 0.01 rad a frame, the median of 12 after 2, the best of three alternations; GeForce RTX 4060 Laptop, Vulkan):

| level | overlay lines, all on | every overlay off, ms | every overlay on, ms |
|---|---:|---:|---:|
| MP_005 | 66,184 | 5.61 | 5.67 |
| SP_01 | 149,411 | 6.28 | 6.44 |

The overlays are lines in the line pass the view already draws: 0.1 - 0.2 ms a frame with all of them on (SP_01's 190
spawn checks and 433 sound spheres are most of its lines).

**Proof** (`cargo test --release -p bc2-editor --test editor view_toggles::`, MP_005, SP_01, ONS_MP_004 headless):

| what | result |
|---|---|
| each toggle draws its colour's pixels, none when off (the app's look drawn alone, top view over its items) | MP_005: effects 7,193 px, sounds 9,033, combat areas 2,530, capture areas 1,384, spawn protection 12,458; SP_01: effects 8,253, lights 600, sounds 13,428, combat areas 2,539, spawn protection 61,131; 0 for each when off |
| drawn radii against the data (the circle at 45 and 225 degrees, nothing 10 % inside or outside) | SP_01 light 10 m drawn 10.000 m; MP_005 sound minimum distance 30 m drawn 30.000 m; capture radius 1 m drawn 1.000 m; spawn check 20 m |
| MP_005's combat area polygon | its 22 points (the scene loader's own outline of the same `Geometry`) each drawn; the scene's outline of it left out |
| player's eye: the data's values | eye 1.6 m standing, 1.0 m crouching (`EyePosition`); field of view 55 on MP_005, 45.1 on SP_01 (55 x 0.82); step 0.3 m, 4 / 2.5 m/s, sprint x1.5 |
| player's eye follows the ground | 4 s at 4 m/s: 16.00 m on both; on the terrain 105 / 116 of 120 steps the eye 1.6 m above the height field (worst 0.023 / 0.045 m, the mesh's stride against the field), the feet climbing 59.9 - 71.2 m (MP_005) and 84.9 - 86.7 m (SP_01); W held in the app walks; Esc restores the camera exactly |
| collision per destruction state (SP_01's guard tower, 3,321 triangles; parts `part_1` 486, `part_1_dest` 196, `part_2` 545, `part_2_dest` 272, `ruin` 682, `ruin_downed` 1,140 - the header's hashes equal `PartInfoDatas`' names) | Intact 1,713, Damaged 1 1,608, Destroyed 1,140, All parts 3,321 triangles drawn, each the sum of the parts its state shows; Page Down walks Intact, Damaged 1, Destroyed, All parts; Page Up back to Intact |
| navmesh mode | SP_01 and ONS_MP_004: NavKit's build and DICE's file read, the ground drawn (18,544 / 25,838 pixels changed); MP_005 has no path data (no NavKit build, DICE has none for multiplayer): the note says so |
| cull tree mode | MP_005 3,683 leaves (2,071 own slots, 469 none), SP_01 4,612 (2,588 / 303), ONS_MP_004 2,560 (1,327 / 305); no shared slots; each draws |

Unit tests: `havok::tests::part_hashes_and_alignment`, `simple::tests::destruction_states_pick_their_parts` (stages,
collision parts), `display::tests::destruction_steps_in_order`, `overlays::tests`, `walk::tests`.

**Collision per part in the scene.** `havok::parts` reads each packfile a second time for its part map (one `u16` a
triangle beside the 36-byte triangle; the merged list NavKit, snapping and picking use is unchanged). The view keeps
one collision mesh per (blueprint, state) drawn, uploaded when first drawn. `scene_bench` (release, best of 5, one
thread, the packfiles in memory): MP_005's 136 packfiles 0.002 s for the triangles, 0.001 s for the parts (49 split
per part); SP_01's 172: 0.002 / 0.001 s (40 split); the whole load 0.26 s on both, as before within noise.

Screenshots (not in the repository): MP_005 flag B with capture areas, spawn checks, sounds and effects; the combat
area; effects, sounds and lights; the player's eye; navmesh and cull tree; SP_01 lights and sounds, the guard tower's
collision in each state, the player's eye; ONS_MP_004 navmesh and cull tree; Dark and Light.

## XF1: the export fixes before E4 (2026-11-21, branch `rust-xf1`)

Audit AU1's XF1 items (the user, 2026-11-20). `EDITOR_ARCHITECTURE.md` 3.12, "Round XF1"; spec SHDB-LVL-1, SHDB-LVL-2,
RNDR-DRAW-6, XCULL-RES-1, XCULL-RES-3, XLOG-LOG-3, XLOG-APP-6, XLOG-APP-7. No Python replaced; `game\` only read.

### The ground truth

- **0x98BD6E** is in the draw-record builder at 0x98BCE0 (it zeroes a 0x58-byte record and fills it from the part's
  resolved draw state): `mov edx,[ebx+8]` ... `movups xmm0,[edx+8]` with no test of `edx`, where the state's +4
  pointer is tested just before. A part whose layout found no program leaves +8 null, and a mesh without entries in
  the level's databases has no program at all.
- **Where entries live.** No shared bundle holds a shader database (`mp_common`, `sp_common`, `ww2_common`,
  `nam_common`: none). Of the meshes a level reaches (every partition of its own bundle, every reference followed
  through the bundles it loads), its own Dx11 database covers: SP_01 165 of its own 166 and 157 of `sp_common`'s
  163; MP_005 25 of 25 own (Dist), 384 of `mp_common`'s 431, 11 of OPFOR's 33 Package additions; MP_002 433 of 455.
  So DICE registers the commons' meshes a level draws in the level's own files. Guadalcanal's radar bunker: 2
  entries and 12 mesh-part names in Guadalcanal's three databases, none in SP_01's.
- **Cull shapes.** Placed objects of the stock mode layers are slotted (MP_005GR's objectives 16 of 16, MP_002's 20 of
  20). Before XF1 the E3d canaries showed the template's shape: Aurora's placed bunker took slot 3636's one leaf (a
  map prop's); Guadalcanal as Rush gave its 20 grafted M-COMs, carrying Arica Harbor's `Enumeration` values (4874 -
  4930), the leaves of the Guadalcanal objects that own those numbers.

### E3d's canaries rebuilt (`cargo run --release -p bc2-levelwrite --example e3d_matrix`)

Release build, the bundle cache warm, other rounds building on the PC. "Lacked" is meshes the level reaches without
entries in the base's databases; the check's shader line counts the ones still without (allowed: the stock base
reaches them without entries too, or no level's database has them).

| Canary | Lacked / reached | Entries added (from) | Still without | Cull: re-slotted (shape from) | Build, check (before XF1) |
|---|---:|---|---:|---|---:|
| Aurora -> WW2 Conquest, Guadalcanal's bunker and Kurogane | 91 / 359 | 217 for 91 meshes (87 `guadal_canal`, 3 `mp_002`, 1 `sp_003_b`) | 0 | 2: the moved prop (its own), the bunker (Guadalcanal's, 2 leaves) | 29.4 s, 14.8 s (32 s) |
| Atacama Rush -> Conquest | 48 / 376 | 84 for 43 (10 `mp_001`, 20 `sp_004_b`, others DICE's) | 8 (MP_005GR reaches them without entries too) | 1 (its own) | 21.7 s, 9.5 s (14 s) |
| Wake Island -> BFBC2 Conquest | 4 / 331 | 0 | 4 (in no level's database) | 1 (its own) | 20.3 s, 8.5 s (16 s) |
| Vietnam -> WW2 Conquest | 77 / 209 | 198 for 77 (76 `guadal_canal`) | 0 | 1 (its own) | 18.9 s, 9.3 s (18 s) |
| Arica Harbor -> Vietnam Squad DM | 91 / 319 | 239 for 90 (76 `nam_mp_002sdm`, 7 `guadal_canal`) | 1 (nowhere) | 1 (its own) | 22.5 s, 18.9 s (31 s) |
| Guadalcanal -> BFBC2 Rush | 8 / 336 | 22 for 4 (`mp_002`) | 4 (nowhere) | 21: the moved lamp, 20 M-COMs (Arica Harbor's shapes) | 27.0 s, 9.9 s (19 s) |

Every check line passes on all six (Air Superiority stays refused, PENDING). The exports take 5 - 10 s more to build
(the reach walk and the merges, 4.8 - 10.0 s) and the checks 5 - 10 s more (the staged and the stock base's reach).

### Undo (`tests/xf1.rs::undo_on_a_mode_project`, release)

SP_01 made WW2 Conquest (the mode choice reads Guadalcanal's partition), then 40 moves; each result compared with
the log applied afresh in a new document (partition bytes, every transform, the deleted set).

| | before (full replay, donor read each time) | after (checkpoints, donor cached) |
|---|---:|---:|
| mode choice applied | 4,133 ms | 1,086 ms |
| undo (3 in a row) | 10,872 / 9,386 / 9,078 ms | 98 / 83 / 78 ms |
| redo (3) | 3,964 / 1,951 / 1,685 ms | 87 / 86 / 77 ms |
| History: after the mode choice / op 20 / the end | 1,475 / 1,442 / 1,404 ms | 75 / 73 / 98 ms |
| History: the base | 70 ms | 70 ms |

About 70 ms of each is the base scene's clone; at most 15 operations are replayed after a checkpoint.

### Paste and the typed operations

- `tests/xf1.rs::paste_is_a_deep_duplicate` (headless, the demo level): Copy and Paste of the flag make one
  operation copying its ring and marker (5 sub-objects); after the copy's ring point is moved and its marker renamed,
  the original flag and its 5 sub-objects are element for element as before. Before XF1, Paste's copies had no
  owned sub-objects.
- `tests/gameplay_xf1.rs` (Valparaiso, `MP_001`): a capture point to team 2, a spawner unlinked and linked (and the
  refusals), the tickets 200 -> 300 at 6 a minute, a point inserted into the 47-point combat area and removed (the
  partition's content as before), the area's named point removed (the area names its next, 46 points, closed);
  the same as five project operations applied from the log, saved and loaded at version 5 (VW2 took 4).

### Install round trip

`tests/export_xf1.rs::a_cross_set_object_gets_its_entries_and_shape` staged `MAPEDIT_XF1_AURORA_BUNKER` (Aurora as
WW2 Conquest with Guadalcanal's bunker from a template of another blueprint: its entries and part names in each
database equal to Guadalcanal's; its leaves Guadalcanal's shape). OPFOR's `mapedit_install.py --apply` with this
branch's release `mapexport` (`BFBC2_MAPEXPORT`), `--game` a scratch copy of the six shared files and `--check-game`
the real game copy: the plan's check passed every line, the level bundle installed byte for byte, 9 `package.mft`
lines and 5 manifest lines; `--revert`: the six shared files back with their sha256, the level folder gone.
`game\`: 1,582 files with the same size and write time before and after.

Not proven until a playtest (in-game tests paused): that the merged entries draw (a placed bunker visible and no
0x98BD6E), and that grafted and placed objects cull as on their own levels.

### WW2 rebuild

The shader-database merge moved from `bc2-cli` into `bc2_formats::shaderdb::merge`, so the WW2 rebuild (which merges
shader databases) was run against the oracle: OPFOR main 86220cd, `OPFOR_TOOLKIT` at a toolkit folder holding this
worktree, all 52 steps in 24 min, the QA checks clean, **10,852 of 10,852 files identical**.
## R5d: the game-accurate view's visual gaps (2026-11-21, branch `rust-r5d`)

`EDITOR_ARCHITECTURE.md` 3.11, "Phase 3b (R5d, done)"; spec RNDR-TERR-5, RNDR-WATER-2, RNDR-SKY-1 - 3, RNDR-LOD-1,
RNDR-VIEW-1 - 2, TERR-VT-7. No Python replaced. Measured with `examples/render_probe.rs` (a level's live frame with
sources left out, its near-black share, per-class HDR means, `--frames N` timings, `--dump` HDR for run-to-run
comparisons), `examples/terrain_probe.rs` (a visual terrain's patch map, tiles and flags, cell heights against the
water, each patch shader's programs and constants), `examples/entry_probe.rs` (any database entry's programs, key
bytes, reflection, blocks; `PROBE_DUMP` writes the DXBC for `fxc /dumpbin`) and `examples/world_probe.rs` (a world's
notes, weather fields, LOD settings). Development PC: GeForce RTX 4060 Laptop, Direct3D 11 hardware device.

### Wake Island's black ground, the sea under the carriers, the ragged WW2 shores: three root causes

Wake Island's lagoon view (`--view auto`, 1280 x 720) was 52.8 % near-black; dropping sources showed the black was the
terrain under a water that drew almost nothing. Decoded:

1. **The terrain's height scale** (RNDR-TERR-5). The patch pixel shaders take the world height as the height map's
   sample times `heightmapWorldScaleY`; the view left the records' stored value, 500, in place. 500 is the range of
   every stock height field read, but Wake's spans 0 - 1,000 m: BF1943's sea-floor layers (which darken by depth
   below the water at 215 m) saw the island's beach and shallows about 100 m deep and drew them black. The engine
   writes the range there (the vertex shader's `posScale.y` places the same height). Wake's `Wake_Grey` layer (12,803
   cells, one constant black output) is DICE's / BF1943's own and lies 165.9 - 203.4 m: at least 11.6 m under the
   water everywhere (`terrain_probe PROBE_HEIGHT=215`).
2. **The water's height map** (RNDR-WATER-2). Wake's water (BF1943's ocean shader, OPFOR's `wake_water.py`) and DICE's
   Vietnam water (`NAM_HaLongWaterShader`, `NAM_WaterGreen`) read the terrain's height map with
   `heightmapWorldScaleXZ` / `OffsetXZ` / `ScaleY` (DICE's code: `(x + 0.5) * scale + offset`); the view bound no
   height map and left the three at 0. Bound with the terrain's values, the lagoon draws as shallow green water over
   sand.
3. **The mask tiles span blocks of cells** (TERR-VT-7). A cell's mask flag is not "tiled" (8) but how many cells the
   tile spans on a side: 8 or 4 (Wake holds both). The cells naming a tile form one aligned flag x flag block: on
   DICE's MP_009 757 tiled masks name 51 tiles, MP_005 1,033 name 59, Wake 998 name 78, none outside its block. R5 to
   GW1 stretched the whole tile over each cell, so each cell repeated the block's mask: the blocky sand and the ragged
   cell-edged shores of the WW2 levels (GW1's leftover), and of DICE's own tiled layers. Each cell now reads its part
   of the tile; DICE's MP_009 draws continuous roads and clearings.

The open sea **is** in Wake's `VisualWater` (-4,236 .. 4,548 m at 215 m: the carriers stand on it; R5c's "no water"
was a water that drew nothing). Past the reefs BF1943's water writes colour and alpha 0 over the black floor, so the
deep sea stays dark until the haze over the scene (`EnvironmentHalfResMain`'s fog segments: the fog, item 7) is drawn.

| level, view (1280 x 720) | near-black before | after |
|---|---:|---:|
| Wake Island, the lagoon (`auto`) | 52.8 % | 0.2 % |
| Wake Island, 600 m out (deep sea in view) | 64.6 % | 53.3 % (the deep sea) |
| Wake Island, the north beach | 49.0 % | 13.5 % (the deep sea) |
| Coral Sea, `auto` / 900 m out | 10.2 % / 3.1 % | 0.6 % / 0.5 % |
| Guadalcanal, Iwo Jima `auto` | 0.1 %, 0.5 % | 0.1 %, 0.4 % |
| MP_005 west shore / `auto`, SP_01 `auto` | 0.1 % / 1.9 %, 42.3 % | the same (stock height fields span 500 m) |

### The sky (RNDR-SKY-1 - 3)

DICE's `Systems/Weather/Shaders/Surface/EnvironmentMain` drawn on a unit dome scaled to 0.9 x `g_viewDistance`
(4,000 m, the views' far plane) around the camera, first of the blended draws (after the depth pass it reads), its
main-pass variant by the panoramic textures present; the constants from the weather state's `Sky`, `Sun` and
`Clouds` by name. MP_005 west shore: 52,098 background pixels (the fog colour) before, 0 after; the top row's mean HDR
(0.266, 0.380, 0.588). The panoramic clouds are left out: with the data's values (`PcbmMultiplyAddAlpha`, threshold
0.3, the texture sRGB at 0.17 on average) they draw a black band the game does not show (RNDR-SKY-3). Wake Island
keeps BF1943's own skydome mesh (a placed object), now over a blue sky instead of the fog colour.

### Levels of detail (RNDR-LOD-1, RNDR-VIEW-2)

Every mesh set names a `MeshLodSettingsAsset` with `LodDistance0..4` (MP_005: 32 different settings over its meshes,
2 - 4 levels each): level k is drawn from `LodDistance(k-1)` to `LodDistance(k)`, one live source per level with its
distance band. Meshes with alpha-tested or blended parts keep level 0 (the vegetation's far levels are thin cards:
`mebush_01` LOD 1 54 indices; with them switched, MP_005's alpha-tested pixels fell 13,271 to 1,523 in the `auto`
view). Terrain patches: every sample within 200 m, then every 2nd / 4th / 8th sample at each doubling, each level's
outer edges whole (one shared index buffer of every level; `Step::range`): no cracks.

| view (1600 x 950, sky on) | draws, full -> LOD | build, ms | submit, ms | GPU, ms |
|---|---:|---:|---:|---:|
| MP_005 west shore | 12,629 -> 11,859 | 6.81 -> 6.60 | 3.94 -> 3.63 | 9.51 -> 9.33 |
| MP_005 `auto` | 9,036 -> 8,387 | 5.27 -> 5.04 | 2.79 -> 2.51 | 1.56 -> 1.25 |
| SP_01 `auto` | 11,632 -> 11,500 | 5.56 -> 5.78 | 3.68 -> 3.78 | 8.76 -> 8.70 |
| SP_01 overview (1,700 m back) | 28,199 -> 27,684 | 12.70 -> 13.48 | 7.80 -> 8.00 | 2.12 -> 1.81 |
| Wake Island `auto` | 6,709 -> 6,593 | 3.41 -> 3.57 | 2.39 -> 2.27 | 6.40 -> 6.28 |
| Wake Island 600 m out | 17,618 -> 17,311 | 7.59 -> 8.50 | 4.93 -> 5.06 | 2.97 -> 2.71 |
| NAM_MP_002CQ `auto` | 4,208 -> 4,197 | 2.27 -> 2.30 | 1.48 -> 1.44 | 4.07 -> 4.22 |

Medians of 12 frames, the camera turning 0.05 degrees a frame, the GPU waited for (`Device::finish`). On MP_005's
wide view (640 x 360, `tests/r5d.rs`) the terrain's indices fall 10.7 M to 1.7 M and the objects' 6.2 M to 2.9 M, but
the frame barely changes: the views are bound by DICE's pixel shaders (the patch shaders' per-layer blends), not by
vertices, and the draw count is unchanged by levels (the instanced programs, `StreamInstancingFloat`, would cut it;
not done). `tests/whole_terrain.rs` with levels: every sampled terrain pixel still drawn (no crack), 95 % or more
within 2 m of the height field (98 % in full).

### Labels hidden behind the game frame (RNDR-VIEW-1)

After each drawn frame the game view reads its depth back (`Device::read_depth`: 2.1 - 2.6 ms at 1600 x 950, the
whole depth-stencil copied; only when the frame is drawn again) and `GameView::hidden` tests a label's point against
the 3 x 3 pixels around it. MP_005's harbour view: 93 gameplay labels seen, 2 behind the buildings and hills.

### Package loads

Each round's bundles are read at once through the suite's bundle cache (`fbrb::read_many`: the in-process session,
and a large bundle read twice written whole to the shared disk cache inside its one budget). Wake Island's 11 rounds
read the 340 MB level bundle 11 times from its start before; the rounds' bundles now in parallel and from the cache:

| level | before (R5c's read: one streamed pass a bundle, one after another) | cache off (parallel only) | cold cache | warm |
|---|---:|---:|---:|---:|
| Wake Island | 17.65 s | 11.9 s | 4.60 s (writes 1.7 GB of payloads) | 1.43 s |
| Coral Sea | 13.84 s | | | 0.85 s |

The same bytes as a plain read (`tests/r5d.rs::package_reads_through_the_bundle_cache_give_the_plain_bytes`);
`BC2_RENDER_UNCACHED=1` reads as before.

### The far-water pixels

R5c's run-to-run differences at MP_005's west shore: 2 pixels (294 / 295, 411; 4.7e-4 in colour, the same depth and
alpha) differed in 2 of 68 runs, then in none of 400 (200 one after another, 120 as two processes at once, 80 more);
a hash of every CPU-side input of the frame (the steps, the constant arena, the draws' programs, textures, samplers,
vertex and index data, the texture bytes) was the same in all 400, so the CPU side is deterministic. Not reproduced
since, cause not settled. One latent hazard of the same family as R5c's fix closed: the device's material cache was
keyed by a prepared mesh's address without keeping the mesh (a dropped mesh's address could hand a new mesh its
material); it keeps it now.

### Screenshots (not in the repository)

`shots\before_*` / `shots\fix2_*` (`render_probe`, main against this branch, the same views on Wake Island, Coral
Sea, Guadalcanal, Iwo Jima, MP_005, SP_01), `shots_editor\before` / `shots_editor\after` (the editor's game view in
Dark and Light: MP_005's harbour and warehouses, the four WW2 levels; `r5d_game_view_dark` / `_light` with the sky
and the labels), `probe\wake_top_v0` / `wake_top2_v0` (Wake's sand and shores from above, before and after the mask
tiles), `probe\mp009_after_v0` (DICE's MP_009 terrain, continuous).

### Tests

`cargo test --release -p bc2-render`: 31 unit tests (new: `terrain::tests::patch_levels_cover_the_patch_and_keep_full_edges`,
`sky::tests` x3, `frame::tests::a_point_behind_drawn_depth_is_occluded`) and 14 integration tests (new
`tests/r5d.rs`: the mask blocks and continuous patches on MP_009 / MP_005 / Wake, Wake's and Coral Sea's ground, the
sky, the levels of detail, the cached Package reads; `whole_terrain::the_whole_terrain_is_drawn_with_levels_of_detail`).
`cargo test --release -p bc2-editor --test editor`: new `game_view_r5d` (the sky in the editor, labels behind the
frame). `tests/live.rs` keeps every patch in full (`Options::exact`) and draws no sky: the live list against R5b's
per-frame list, bit for bit as before; the editor's `game_view_live` compares like with like (shared against copied,
an edit against a fresh load) with the sky and the levels on, still bit for bit.

After merging main (2026-11-21): `cargo test --release -p bc2-render` 31 + 14 passed, `-p bc2-editor` 61 + 49 passed
(GR2's scratch-copy test needs the module `build_native.py` builds), clippy `-D warnings` clean on both crates,
`spec/trace.py --check` 674 rules, `run_tests.py quick -j 2` 124 of 125 (O2, Microsoft Defender holding a freshly
written .exe: environmental). `BC2_RENDER_UNCACHED` registered in `bfbc2core.envvars`.
## RT3: RT2's leftovers - the hpd / Wave readers and five command lines retired, abbreviations, one rule (2026-11-21, branch `rust-rt3`)

### Item 1a: `hpdtool.parse` and the Wave readers, their messages ported, then retired

- **`bc2_formats::hpd::parse_exact`** reads a path-data file read for read as `parse_reference` did and stops where it
  raised: `ValueError` ("not a Kynogon HPD file", "sector N (0x..): runtime mesh not where expected", "sector blocks do
  not cover the file") or `struct.error` in CPython 3.13's words at the first read that fails (the header's 144
  bytes, a 48-byte table entry, a block's 8-byte counts, then - where the records do not fit - the first node or edge
  that does not, at its offset in the sector's block, as the reference's record-at-a-time loop reached it). NaNs are
  read (the binding converts them as `struct` does: `hpdlayout::c_double`). `hpd::parse` (the CLI tools', the
  editor's) keeps its NaN refusal. `bc2_native.hpd_parse` raises the reference's errors; `hpdtool.parse` has no
  Python left (a bytearray / memoryview reads as its bytes, another type `Refused`); `write_reference` stays.
- **`bc2_formats::wave`**'s readers stop where the reference raised: its `AudioError` messages, and `struct.error`
  where a size word lay past the end that its checks let through (the stream-information word after a padded blob,
  an SPS header block shorter than its two words - one 8-byte read - or an audio block shorter than its sample word);
  a NaN BF1943 rate is read. The bindings answer `(exception name, message)`, which `audio.py` raises; the readers'
  references are deleted, the writers' (`write_wave_pc_reference`, `write_sps_reference`) stay.
- **Proof (replay).** Path data against RT2's recording (`f0_pathdata_replay*.json`): quick 1,585 answers replayed
  (1,026 inputs the old core refused, 4 of them answered by the reference - NaN files - and now by the port), game
  2,113 replayed over 138 files (645 MB), 8,643 meshes, 414 damaged copies: 0 wrong, 0 not recorded. The Wave
  readers' answers were recorded now from the references (a frozen checkout at `f826078` with its own module, the
  sibling projects through junctions): quick 8,124 answers, game 936,831 (286,350 distinct; 2,750 BF1943, 45,834 PC
  and 35,813 Xbox Waves, their streams, 3 damaged copies of each), `f0_wave_replay*.json` 4.8 MB; then, the readers
  retired, quick 4,042 and game 426,208 replayed: 0 wrong, 0 not recorded, every input answered by the core (the old
  core had left 121,450 to the reference). O2's `hpdtool` lines re-recorded from the Python (which now reads through
  the core): only the 6 that print its usage changed; the 20 lines `bfbc2.exe hpdtool` hands to the Python kept their
  answers.

### Item 1b: five command lines split and retired (`aptrefs`, `xiso`, `culltree`, `stockmp`, `crashreader`)

Each library moved with `git mv` to `<tool>lib.py` beside it (old public names kept; `exeref.py`, tests, benches and
the hub import it); the usage `bfbc2.exe` prints lives in `rust\bc2-cli\src\<group>\<tool>_usage.txt`, reworded for
`bfbc2 <tool>`. The goldens were re-recorded from the Python first and were unchanged (crash 258, cull 190, stockmp
236 + 73, readers 212, aptrefs 82); exeref's two "symbols missing" lines re-recorded (its warning names
`crashreaderlib.py:66`, which `bfbc2.exe exeref` emulates). The 38 lines that print the reworded usage are in
`cli_<round>_<group>_now.json`, merged over the goldens (which keep the Python's text). `native.RETIRED_ROUND` names
each tool's round; `test_native_required` checks it against `pyproc.rs`' `RETIRED` and that each is `CliMissing`
without the binary (the `--download` test passes).

| tool | lines (quick) | answered by Rust | handed to the Python before, the retirement error now |
|---|---|---|---|
| crashreader | 120 | 112 | 8 (not XML, UTF-16, `--exe` not a PE file, `BFBC2_CRASHREADER_REFERENCE`) |
| aptrefs | 82 | 82 | 0 |
| culltree | 190 | 184 | 6 (an extra field, a damaged reference tree) |
| stockmp (O3, O5) | 236, 73 | 218, 71 | 18, 2 (odd synthetic inputs, some of which the Python answered with exit 0: a plan report that is not JSON, an infinite path-data position, a degenerate level's snapped generate) |
| xiso | 44 | 42 | 2 (the root cut short) |

Game tier (OPFOR's real calls in Rust alone): crash 15 s, menu 186 s, cull 42 s, O3 stockmp 52 s, readers 37 s, O5
stockmp builds 194 s (mp_008cq 32,257 files), all as recorded. OPFOR: `testrun.py` runs `bin\bfbc2.exe` only (a clear
message without it; OPFOR commit `2450330`); its tests 381 of 381 against this branch.

### Item 1c: argparse's option abbreviations

`argfmt::long_option` is `_parse_optional` / `_get_option_tuples` for a `--` argument over the parser's option
strings in `_option_string_actions` order; in 3.13 the ambiguity is raised in `consume_optional`, in order with the
other errors. The shader tools' parser (shaderdb, renderqa, xdiff), caches, convert, dbxparity / portparity and texqa
resolve with it; audio's own matched already. `core/tests/test_abbrev.py` (quick): 193 command lines built from the
Python parsers themselves (each long option's shortest unique prefix alone, `=v` and with its value, the `--help`
prefix, every shared prefix; 26 ambiguous), each ending where argparse stops before the tool acts (checked in process
first): 193 of 193 identical to the Python's recording, 170 answered in Rust, texqa's 23 argparse errors handed off by
design. `test_argfmt` 943 of 943; O2 nav, O3 caches / audio / bf1943, O4 texqa, O5 shader / bf1943 as recorded. Still
handed off: single-dash abbreviations and attached short values, `--`, a flag or `--help` given a value, an unknown
`--...` with a space, texqa's argparse errors.

### Item 1d: the kept references of RT2, decided

| reference | decision |
|---|---|
| `hpdtool.parse_reference`; `audio.read_wave_reference` / `read_sps_reference` / `read_snr_reference` | **retired** (item 1a) |
| xiso's file-object reader | **in Rust** (`xisolib.find_partition_bytes` / `walk_bytes`, item 1b) |
| `hpdtool.write_reference`, `kymesh.write_mesh_reference`, `meshset.write_reference`, `audio.write_wave_pc_reference` / `write_sps_reference` | keep: the writers take objects NavKit's build, OPFOR and the converters assemble (RT2 counted 47, 37 and 102 changed objects the core refuses that the reference wrote); the A17 rule's first half (the reference answers while alive) |
| bc2loc's own (`khash_reference`, `LocFile._parse_reference`, `decode` / `to_bytes`) | keep: khash from another start, lone surrogates, LocFile's edits are Python state |
| stfs (`test_console_readers_native`) | keep: `stfs.py` is a live command line and OPFOR's library |
| texwrite, skinmesh, Granny writing | keep: OPFOR's WW2 pipeline feeds them arrays and objects it builds; no per-call recording possible |
| `dbxcatalogue.build`, emitteratlas | keep: an unreadable chunk is recorded by the Python (the Rust refuses the whole build); float near-ties by design |
| shaderdb queries (E4b), SD1's reader references | keep: the coordinator's ruling |
| hkpack's F4 references | keep: the coordinator's ruling. Found: `hkpack.py dump` through the F4 handle prints a simple array of variants one list deeper than the pure Python and the recorded answer (`m_odd: [[(?, 3)]]` / `[(?, 3)]`, O2 nav "hkpack: dump", on main too) - a bug of the handle, left for a later round |
| `xenos.disasm`, `parse_shader_python`; crashreader / exeref `Image`; texture `tiled` / `packed` / `layout`; `meshview._build_reference` | keep (RT1's reasons: edited microcode, other executables with capstone, texqa's independent decoder, the skip-and-report) |

### Item 2: one rule when the Rust port cannot answer (audit AU2 A17)

While a tool's Python is alive it answers: `bfbc2.exe` hands the whole command line to it (`Fail::Python`), a binding
answers `None` (or `Refuse` in bc2-stockmp / bc2-harness) and the Python reference runs. Once retired, a plain error,
never a silent fallback: the retirement error (exit code 1, naming the round: `pyproc.rs` `RETIRED` is (tool, round),
`native.RETIRED_ROUND` the same list), and `native.Refused` from a binding's function. Written in README "bfbc2.exe",
`native.py` and `pyproc.rs`; `test_cli_o2.main` (O2 / O3 / O5) now applies `retired_check` to retired tools as O4 did.

### Items 3-6

- **`run_tests.py --help`** ran the quick tier (every unknown argument was ignored). `-h`, `--help`, `/?` print the text;
  an unknown argument, a missing or wrong value, two tiers are a usage error (exit code 2, reported through
  `errors.report`). `tests/test_runner_args.py` (quick): 16 command lines on a stand-in test list.
- **Test-file registration:** `tests/test_repo.py` fails when a `tests/*.rs` of a crate with `autotests = false` is
  not a `mod` of its `[[test]]` main.rs (a `#[path]` module counts), with a self-check on a stand-in crate (it must
  name exactly the forgotten file and a commented-out one).
- **navkit.py's usage** names the map editor's AI workspace: O4's navkit golden re-recorded, 6 of 202 rows changed,
  each in that one line. **viewer.py and pick.py** retired (nothing but two tests imported them; RETIRED.md).
  **f3_golden_hpd.json**: the 12 `work/import_backup` groups removed (OPFOR deleted the folder).
- **O2's environmental failure** was Microsoft Defender: `Get-MpThreatDetection` lists every
  `cli_o2_*\cccccccccccc\b.exe` as `Trojan:Win32/Bearfoos.A!ml`; the detection is by content (the same bytes named
  `b.bin` fail, one changed byte passes) and the block is permanent (Windows refuses the open with
  `ERROR_VIRUS_INFECTED`, which Python reports as `EINVAL`), so neither another name nor a retry helps. The stand-in
  carries a text mark in the unused end of `.data2`; aptrefs' 82 lines re-recorded (34 changed, only their case-folder
  digest). `test_cli_o2.py` (all three groups) passed 3 times in a row: 966 of 966 each.

### Proof (the whole branch)

After merging main (c6cf4a4: GW1, VW1, EX1's `BC2_EX1_STAGE` and GW1's `BC2_GW1_*` in `envvars.py`) and
rebuilding the module and `bfbc2.exe` (sources `bb00c3ff5f9d`):

- `run_tests.py quick -j 2`: 127 of 127 passed (3,530 s); 2 with parts skipped for want of data (the Rust core's
  cargo checks, run separately below; the stale-module case).
- `core/tests/test_cli_o2.py` 3 runs in a row: 966 of 966 each (menu 372, crash 258, nav 336).
- `cargo fmt --check`; clippy `--all-targets -D warnings` on bc2-cli, bc2-formats and bc2-py; their tests (bc2-cli,
  bc2-formats): 286 passed, 0 failed.
- `spec/trace.py --check`: 665 rules in 45 files. ruff and mypy (every folder): nothing found.
- Project OPFOR against this branch (a toolkit folder of junctions, `OPFOR_TOOLKIT`): `src\tools\tests\run_tests.py
  quick` 374 passed, 0 failed, 7 skipped (the game-copy and staged-export tests that need `OPFOR_TEST_GAME` or a
  staged export); `src\tools\opforcore\tests\run_tests.py` 75 of 75.
- The WW2 rebuild (all 52 steps, 22 min) against the oracle: 10,852 of 10,852 files identical.

## VW2: the mode view layer, weather in the views and in exports (2026-11-21, branch `rust-vw2`)

`bfbc2-map-editor/EDITOR_ARCHITECTURE.md` 3.12, "Round VW2"; spec GAME-RUSH-9, RNDR-WTHR-6 (corrected), RNDR-WTHR-7 -
9 (new). Research round WX0 (83 levels, 355 states) is the source of the weather facts. No Python replaced. Nothing
written to `game\`; the game never launched (in-game tests paused): what only a playtest proves is listed at the end.

### The mode view layer

`modelayer::rush_play` ports L1's simulation (`l1_measure.simulate`) to Rust: every instance from its `Enabled`, a
stage played while all its counters are enabled, its M-COMs' `onDestroyed` followed through the event glue
(`Enable` / `Disable`, never another stage's counters). `modelayer::layer_view` gives the layer and, per stage, what
is not in play (the other stages' M-COM clusters; the bases, combat areas and spawns disabled, with their clusters).
One root cause found on the way: `Enabled` is a two-byte value, not a text (`modelayer::bool_field`); read as a text
every base was "enabled" in every stage.

| level | stages | bases in play per stage (Rust = L1, `l1_stages.py` on the same game copy) | layer | hidden per stage |
|---|---:|---|---:|---|
| MP_005GR | 4 | RUS Base + South Island; SI + Stranded Ship; SII + North Island; North Island + US Harbor | 821 | 319, 307, 303, 277 |
| MP_005SR | 2 | US Base + Stranded ship; Stranded ship + RU Base | 279 | 46, 67 |
| NAM_MP_002R | 3 | NVA Fish Market + US drop zone; US Fish Market + NVA Shrine; US Shrine + NVA Temple | 507 | 179, 132, 93 |
| BC1_OASIS_GR | 5 | US Base + Farm; Plantation + Farm; Suburbs + Plantation; Streets + Suburbs; Plaza + Suburbs | 1,082 | 325, 331, 322, 312, 304 |

Two combat areas in play in every stage, no ambiguous order (as L1). In the editor (MP_009GR, 10,005 entities, its
layer 971): "Mode layer only" hides 9,034 (the outliner 10,005 -> 971 rows), "Without the mode layer" 971, each
stage 481 - 509, exactly the layer data's hidden instances that are entities of the scene; the game-accurate view
draws 7,422 objects with every stage and 7,406 with stage 1 (the other stages' M-COMs and props left out). The
game-accurate view now leaves out what the project deletes or hides too (it drew them before).

### Weather in both views

`bc2_render::world::level_weather` / `weather::LevelWeather` (the entity chain of GW1, the system's states,
`ActiveState`, the spawn switch); `World::set_state` (light, post-process, clear colour and sky dome again, the live
list planned again); the game-accurate view opens on `LevelWeather::starts_in` (the switch when it names a state;
MP_005: `day_1` instead of the dark `default`). `weather::look` predicts a state's look from its values (a grey
ground lit by the key light at the view's sun and the sky colour, exposed and tonemapped as the post-process does);
`weather::tint` (the look over a neutral day's) tints the simplified view's scene-lit colours and its sky
(`view.wgsl`'s `weathered`, a fourth `Globals` vector).

MP_002SR's 7 states in the game-accurate view (1600 x 950, the view's middle measured, sRGB 0..1):

| state | frame mean | look's ground | blue / red: frame, light |
|---|---|---|---|
| 0 default | (0.288, 0.286, 0.213) | (0.638, 0.645, 0.656) | 0.74, 1.09 |
| 1 nolighting | (0.283, 0.277, 0.199) | (0.625, 0.625, 0.625) | 0.70, 1.00 |
| 2 midday (switch, ActiveState) | (0.373, 0.353, 0.264) | (0.633, 0.624, 0.611) | 0.71, 0.89 |
| 3, 4 the win states | as midday | as midday | 0.71, 0.89 |
| 5 ons_mp_002_night | (0.072, 0.132, 0.145) | (0.000, 0.058, 0.148) | 2.00, 8.55 |
| 6 ons_mp_002_dawn | (0.326, 0.287, 0.196) | (0.741, 0.724, 0.689) | 0.60, 0.77 |

Brightness: Pearson 0.86 between the frames and the looks; colour (blue over red): Spearman 0.79; the night a third
of midday's brightness and the only blue frame. The model saturates for daytime states (the exposure adapts them all
to middle grey), so it ranks day states only roughly: it is a tint, not a render. The simplified view: midday
(0.213, 0.225, 0.202), the night (0.057, 0.058, 0.082). The picker reads the weather from a prepared level's folders,
or a Package level's bundles in rounds (`world::weather_files`), or the loaded game view's files.

Found: the prepared `work\levels\ons_mp_002` places SP_01's weather entity (`weather/r_sp01/weathersystementity_default`,
an old preparation), while the game copy's ONS_MP_002 places MP_002's: the view of that prepared level lists SP_01's
44 states. Not changed here (a stale preparation in the user's work folder; preparing the level again fixes it).

### Weather in exports (VW2a - c)

`bc2_levelwrite::weather` and `Op::SetWeather` (project format 4; a version-3 file loads as it is); the export's
step after the mode layer, every new partition under `weather/<level>/` in the level's own bundle, the base's stock
partitions never changed; `stage.json`'s `weather` record (each copy's source, which `mapexport check`'s dangling
reference rule reads); the check's weather lines. Canaries (`cargo run --release -p bc2-levelwrite --example
vw2_canaries -- <game> <projects> <stages>`): each built twice, with and without its weather (the baseline: the same
id, another folder), the two stages compared entry by entry, each new partition against its source (XML with guids
numbered and the renamed paths mapped back), then `mapexport check`:

| canary | base | weather | level bundle: same / new | level partition differs in | each copy differs from its source in | check | time |
|---|---|---|---|---|---|---|---:|
| `state` | MP_002 | own state 6 `ons_mp_002_dawn` (ActiveState 2 -> 6, switch 2 -> 6) | 55 of 56 / 2 | `State`, `ReferencedObject` | settings: `Name`, `ActiveState`; entity: `Settings`, `Name` | 38 lines | 17 s |
| `state_wake` | Wake Island (entity in the level partition, no switch) | own state 1 `iwo_jima` | 33 of 34 / 1 | `Settings` | `Name`, `ActiveState` | 40 | 23 s |
| `borrow_append` | MP_005 | MP_008CQ's `mp_008_ons_fog` appended as state 25 | 21 of 22 / 12 | `State`, `ReferencedObject` | the 8 state partitions: `Name`; the system: `Name` and one `States` item | 39 | 20 s |
| `borrow_swap` | MP_005 | MP_002's whole system (57 partitions), state 6 | 21 of 22 / 61 (+ the panorama texture the closure copied) | `State`, `ReferencedObject` | every copy `Name` only | 39 | 19 s |
| `night` | MP_005 | `ons_mp_002_night` appended, `ReflectionEnvmapScale` 1 -> 0.1, panoramic `Trancparency` 0.02 -> 0.005 | 21 of 22 / 12 | `State`, `ReferencedObject` | `Name`; the two night values | 39 | 23 s |
| `rain` | MP_005 | SP_03's `afterrain` appended (the state only) | 21 of 22 / 12 | `State`, `ReferencedObject` | `Name` | 39 | 22 s |

The terrain, ingame and loader bundles byte for byte as the baseline's in all six; the only changed entry of the level
bundle is the level partition. Each new system's `WeatherSystem` resource: 4 bytes, 72. Night and rain without
"Build it anyway" are refused in words. OPFOR's `mapedit_install --apply` then `--revert` of the `state`, `night` and
`borrow_swap` stages on a scratch copy of the six shared files, `--check-game` the real game copy (this branch's
release `mapexport`): installed (11 `package.mft` blocks, the `maps.ini` section, the level folder), then every
shared file and its last-writer record back byte for byte, the folder gone; the real game copy's shared files with
the same sha256 before and after.

### Left, with reasons

- **VW2c's lamp tool**: point-light emitters (`ActAsPointLight`) from SP_01's effects, placed by the user. Placing an
  effect blueprint of another level's bundle with its emitters needs E2b's "+bundle" placement to carry the emitter
  system and atlas entries, and XF1's shader entries for objects from other levels (in progress); not started.
- **VW2d's effects and sounds**: rain is exported as SP_03's `afterrain` state only; the grid of camera-proximity
  area effects (SP_03's `fx_amb_sp3_rain_area20`, MP_007's snowflakes) and the sound areas need the effects' emitters
  under the target level's emitter namespace (WX0 A.5): the same cross-level emitter copy as the lamps.
- **VW1's leftovers**: the overlays follow the mode view layer and the project's hidden and deleted objects, not
  moved ones (they are read once per level); damage and clear-area triggers, wall collision for the walk camera, the
  FOV and box-evaluator readings: not started.
- **Only a playtest can prove**: that the engine starts in the spawn switch's and ActiveState's state; a borrowed
  state under the target's sky; the swapped system's sky; the night's exposure and readability; no checksum crash
  from the new partitions (they are inside the level's own bundle, registered as before).

### Tests

`cargo test --release -p bc2-levelwrite --test levelwrite vw2::` (the stages as L1 plays them on 4 levels; the
weather as the export reads it on MP_005, MP_002, ONS_MP_002, Wake Island, Coral Sea; the night template resolves
through MP_002's bundles, in the Onslaught Package); `-p bc2-editor --test editor vw2::` (the mode view layer on
MP_009GR in every view; the weather states on MP_002SR in both views, measured; the game view opens on MP_005's spawn
switch); unit tests `modeview::tests`, `weatherview::tests`, `weather_ui::tests`, `weather::tests` (levelwrite).
Screenshots (not in the repository): `vw2_mode_*` and `vw2_weather_*`, Dark and Light.

## DF2: install and revert safety - one install ledger (2026-11-21, branch `df2`)

Audit AU4's DF2 findings, the suite's half (Project OPFOR's stages, migration and import scripts are OPFOR's own DF2
branch). Nothing here is a speed change: the measure is that no order of installs and reverts, and no interruption,
can lose a game file.

**The store.** `bc2_formats::gameledger` (Rust) and `bfbc2core.gameledger` (its Python twin) read and write
Project OPFOR's install ledger (`<game>\opfor\ledger`, `opforcore.ledger`'s format `opfor-ledger/1`): one record per
owner, the bytes each install replaced in `blobs\<sha256>.bin`, a lock (`.lock`, a stale one - its pid gone - cleared
by the next operation), a journal (`.txn.json` + pre-images, flushed before the first write). The suite writes
whole-file contributions (kind `file`, `extra.derived`); other owners' records of other kinds are kept as they are.
Per file the owners stack: install on top (refused when the file changed outside the ledger), uninstall of the top
owner writes its prior back, of a buried owner splices it out unless the owner above is `derived` (then refused,
naming what to revert first). spec `transforms/install.md` XINST-GL-1..3.

**What moved onto it.**
- NavKit install / restore (`bfbc2 navkit`, `navkit.py`): owner `navkit:<lv>`, the traversal annotations in the same
  commit; `original_file()` in the three places (`navkit.py`, `navshared.rs`, the editor's `navdata.rs`) answers the
  bottom layer's prior; the editor's install / restore plans read the ledger.
- stock-MP install (`bfbc2 stockmp install`, `stockmplib.install`): owner `stockmp:<lv>`, one commit (DS-5), a revert
  any later command can finish (DS-6), the installed files from the record and the level's own folders (DS-9), no
  stale backup (DS-19); `pipeline.stock_file` / `level::stock_file` read the stockmp owner's prior.
- Markers of before the ledger: each tool adopts its own family on first use (one journalled commit), another's are
  refused in words ("migrate them first - Project OPFOR: python src\tools\opforcore\migrate.py --apply").
- `bfbc2 gameledger status|holders|original|recover <game>`.
- Also: `game_running()` with a 30 s timeout (BUG-20); NavKit's level unpack swapped in under a lock (DS-14, both
  twins); per-process temporary names for the harness run, the stock-MP report and texqa's JSON (CQ-18); one SHA-256
  (`bc2-stockmp`'s moved to `bc2_formats::sha256`; CQ-9); `stockmp::installed_files` (the whole-game walk) removed.

**Proof (all on scratch game copies in temporary folders; the real game copy is never written).**
- `bc2-formats` unit tests (`gameledger_tests.rs`, 12): AU4-DS-1's exact sequence; every order of NavKit and stock-MP
  applied and reverted, with and without an original, checked byte for byte after each step; an interruption at
  each write step of each of the four operations, as a failed write (rolled back at once) and as a crash (journal
  and stale lock left; the next operation rolls it back), then the sequence finished; a held and a stale lock;
  legacy adoption; OPFOR's records of other kinds kept byte for byte with their blobs; a spliced record's other
  fields kept; a dry run writes nothing.
- `core/tests/test_gameledger.py` (quick tier): the same on the Python twin (28 hook interruptions, plus a child
  process really killed by `BC2_LEDGER_CRASH_AT` before each write step); legacy refusals; then with `bin\bfbc2.exe`:
  the Rust store reading the Python one, DS-1 through the real command lines (`bfbc2 navkit install`, `bfbc2 stockmp
  install --apply`, `bfbc2 navkit restore`, `bfbc2 stockmp install --revert`) on a stand-in OPFOR folder - every file
  of the game copy byte for byte as before at the end - the same mixing the Rust NavKit with the Python stock-MP, and
  `bfbc2 stockmp install --apply` killed before each write step, recovered, run again and reverted.
- Updated: `test_navkit_offline` (install / restore / adoption / refusal on the ledger), `test_stockmp`,
  `test_deep_scan_fixes` (a lasting lock on the Package bundle part way: every file back, exit 1),
  `test_review_fixes` / `_2`, the editor's `gr2` install / restore on a scratch game folder.

**Project OPFOR's half (branch `df2` of project-opfor-bfbc2).**
- `opforcore.stages`, the install-marker module on `opforcore.ledger`: whole-file layers (`derived` for every OPFOR
  stage), one journalled commit per run, a redo built on the stage's own prior, a revert refused in words under a
  later layer, an interrupted commit rolled back by the next run (`resume`). `ledger.py`: fsync before the rename,
  derived layers, `keep_others` / partial uninstall, file-vs-slot conflicts, tree copies of exactly the planned files
  (BUG-25).
- On it: the 14 `console_*` stages and the three `apply_*` fixes (owner `<stage>:<level>`; DS-2: no `shutil.copyfile`
  into the game copy any more; DS-10: no forwarded `-applied` records), `console_onslaught` (one commit of bundles,
  path data and registration) and `console_port --from N` (reverts N..14 first), `wake_install` (owner `wake`; the
  opfor.ini refresh re-states the prior; the manifest repair its own owner: SM-6; latin-1 registration: BUG-28;
  DS-7), `menu_install` (`--mark` records the bat's build whenever the stage is installed: SM-5; `--revert` of a
  bat-built bundle leaves it to `build_menus.bat` and exits 2: DS-11), the Pacific opt-out exiting 1 when the menu
  mode is not out (BUG-7), `staged.json` written atomically and a damaged one asking for a new plan (BUG-22),
  `mapedit_install` (owner `mapedit`), the import scripts `merge_startup.ps1` / `merge_manifests.ps1` /
  `campaign_fill.ps1` (owner `import`, through `stages.py install`; `$LASTEXITCODE` checked, Dist fallback: DS-8),
  `import_onslaught.bat` (the re-imported levels' owners dropped: `stages.py forget-under`). Readers of old backups
  (`names.py`, `minimap_edges.py`, `console_markv.py`) read the ledger's priors (`stages.before_path`), the old
  backup while a game copy is not migrated.
- `opforcore.migrate`: the old markers into the ledger (backups hard-linked into blobs, markers moved to
  `game\opfor\legacy-markers`), idempotent, inconsistent files refused in words and left alone. **This PC's game
  copy (dry run, not migrated):** 23 marked files, 23 migratable, 0 refused. On a scratch copy of those 23 files and
  their 129 markers: migrated (a second run: nothing to do, no marker left), the ledger's stacks and originals equal
  the plan's, `console_input --revert` / `console_shaders --revert` (the real tools, GAME pointed at the copy) gave
  back their old backups byte for byte, then all 45 owners reverted top-down, each step byte-identical to that
  layer's old backup and every file at the end its bottom backup (DICE's or the import's original); no blob left.
- Tests (OPFOR): `opforcore/tests/test_stages.py` (every order of two tools, DS-1, derived order, an exception at
  every write - store and game - and a process killed at every rename of an install and of a revert, then resumed),
  `test_migrate.py`, `test_suite_ledger_compat.py` (OPFOR's ledger, the suite's Python twin and `bfbc2.exe gameledger`
  on one store), `tests/test_df2_installs.py` (the tools on fake game copies, a real stage's revert interrupted at
  every write, a lint that no tool writes the game copy in place), the old-protocol tests rewritten; tools quick
  367 / 367 (7 skipped), opforcore 89 / 89; `game\` unchanged (1,582 files, sizes and write times) before and after.
- WW2 rebuild against the oracle (OPFOR main with df2 merged, 40b9cef; the readers of old backups changed): all 52
  steps in 22 min, 10,852 of 10,852 files identical.
## DF1: the editor's state and gating (2026-11-21, branch `df1`)

Audit AU4's 19 DF1 findings, each read at its code location on `main` after XF1 (none was already fixed by XF1),
kept minimal: OP1 later turns the side fields into explicit state enums. No project or export-file format changed.

| Finding | Root cause | Fix | Proof |
|---|---|---|---|
| SM-1 | `pending_target` taken after `show_scene`'s early returns | taken at the top of `show_scene`, used only for `Then::Create`, cleared with the generation | `df1::a_failed_create_leaves_no_target_for_the_next_project` (fails with the old behaviour: "era and mode were not set" on X) |
| SM-2 | the Export window had no project identity | `EditorApp::generation` (bumped on every show / Close map); `ExportWindow::bound` / `unbind()`; `window()` acts only on the doc whose path is its project | `df1::the_export_window_lets_go_of_a_closed_project` |
| SM-3 | Install now recomputed the stage from the live form (untrimmed) | `Install::plan` keeps the stage and its `stage.json`; `stage_of` from `level_id()` in one place; stale plans refused | `df1::install_now_takes_the_planned_stage_only` |
| SM-4 | Close map left the preview parked; preview jobs had no identity | Close map ends the preview; `PreviewJob::generation`; a projectless preview closes to the start screen | `df1::a_preview_being_opened_goes_with_its_project` |
| CON-1 | Export gated only on its own worker | Export waits for Install / Uninstall and the other way round, in words; checked again at the click | `df1::export_and_install_wait_for_each_other_and_a_dead_worker_is_reported` |
| CON-2 | NavInstall / Restore ignored a running build; its Job was dropped | `NAV_RUNNING` gates Install, Restore, Build and Close map; `nav::Job` has a `Drop` (kill, remove `partial`) | `df1::navkit_commands_wait_for_the_one_running` |
| CQ-6 | nav install / restore `wait()`ed on the UI thread | started like the build, its end reported by `nav_poll` (polled in every workspace) | `gr2` waits for the job |
| BUG-4 | `if let Ok(r) = rx.try_recv()` ignored `Disconnected` | `answer()` maps it to an error line; workers run under `guarded` (panic text) | the CON-1 test |
| DS-4 | saved = no autosave file | saved = `doc.saved` of the window's own project | the SM-2 test |
| DS-13 | no close handler; the install's pipes were the editor's | `close_requested` / Wait - Close anyway modal (`CancelClose` in `main.rs`); install output to a log file | the CON-1 test (`busy_work`, `close_requested`) |
| BUG-14 | `let _ = fs::write(...)` | `export_ui::write_whole` (sync, retried move); a failure goes to the log | `df1::paths_settings_and_picks` |
| BUG-15 | `if let Ok(e) = g.pick(..)` | `game_pick`: an error picks nothing and says why | `df1::paths_settings_and_picks` |
| CQ-8 | `PathBuf` equality | `same_file` (case and slashes on Windows) | `df1::paths_settings_and_picks` |
| CQ-2 | no `free_texture` when a view went | `Drop` for `Viewport` and the game view's `Opened` | `df1::closing_the_map_frees_its_textures` |
| BUG-9 | `finish()` broke out of its loop on a `GetData` error and returned `Ok` | the error returned (`Device::removed`, `DEVICE_REMOVED`); a lost view is dropped and reloads when turned on | `d3d::tests::only_rgba8_formats_are_read_as_rgba8` (a working device reports none); a real removal needs a TDR |
| BUG-26 | rows read as `w * 4` bytes whatever the format | `is_rgba8` checked, `RowPitch >= w * 4` checked | `d3d::tests::only_rgba8_formats_are_read_as_rgba8` |
| CON-11 | nothing waited for wgpu's sampling of a slot | `device.poll(Wait)` (1 s cap) before Direct3D writes the next shared frame | reasoning only (a race) |
| BUG-10 | `bundleManifest` read with `unwrap_or_default` | `NotFound` alone is "none"; other errors fail `Game::open`; voice-over blocks without one refused (spec XEXP-REG-2) | `df1::a_missing_or_unreadable_bundle_manifest_is_never_read_as_empty` (levelwrite) |
| CQ-10 | `remove_dir_all(work)` only on success | a `Scratch` guard in `patch_startup` / `repack` | `df1::a_bundle_failing_after_its_unpack_leaves_no_scratch_folder` (fails without the guard) |

### Left, with reasons

- **The game view's load on a worker** (CQ-6's other half): `GameView` holds the Direct3D device and COM objects;
  moving the load off the UI thread needs a loading state on `ViewState` and Send-safe handover - OP1's state split.
- **A keyed-mutex handover of the shared frames** (CON-11's lasting fix): needs Vulkan's keyed-mutex import through
  wgpu-hal; the poll wait closes the race at the cost of the pipelining the game view never used (its frames are
  waited for by an event query already).
- **navkit apply writing `--out` in place** (CON-2's Python side, `navkit.py _apply`): the editor no longer installs
  beside a build or keeps a half-written output after a kill; temp + rename in `navkit.py` is NavKit's (NK1).
- **Unsaved edits on close** (AU1 G-N20, named with DS-13): the autosave keeps them; a save prompt is OP1's.
- Only a playtest or a driver reset can prove BUG-9's recovery and CON-11 end to end.

## TS1: faster test tiers (2026-11-21, branch `ts1`)

Audit AU3's testing findings TS-1 to TS-8, TS-12 to TS-14 and AU4's DF4. Measured on the development PC (16 threads,
15 GB) while other rounds (XF1, VW2, then DF1, DF2, OR0's WW2 rebuilds) built and tested beside it; every time here
is that shared machine's. `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=4`, the build folder
`rust-targets\bfbc2-tool-suite-ts1`.

### TS-5: where the cargo entry's time went

Main `f1921de`, its own layout (all opt-level 0): every test binary of `cargo test --workspace --no-run` run one
after another as cargo runs them, with libtest's JSON events and `--report-time` (`RUSTC_BOOTSTRAP=1` for the test
processes only), the gated binaries at `--test-threads 2` (= `GATE`), so each time is the test's own. Cold test
build 158 s; the 31 binaries 3,032 s: `editor` 2,018 s, `levelwrite` 821 s, `render` 172 s, the other 28 together
21 s; 594 tests (2 ignored: the corpus tests). The 30 slowest (seconds; they run two at a time, so they sum to more
than the binaries' wall time):

| # | crate | test | s |
|---:|---|---|---:|
| 1 | `bc2-editor` | `view_toggles::sp_01_lights_collision_states_walk_and_modes` | 1,342.1 |
| 2 | `bc2-levelwrite` | `export_e3c::a_ww2_export_takes_its_donors_announcer` | 678.5 |
| 3 | `bc2-editor` | `every_level::levelwrite_on_every_prepared_level` | 542.1 |
| 4 | `bc2-levelwrite` | `export_e3b::a_vietnam_export_with_its_own_name_and_picture` | 413.1 |
| 5 | `bc2-editor` | `view_toggles::mp_005_overlays_walk_and_modes` | 265.1 |
| 6 | `bc2-editor` | `ex1::names_form_and_the_export_preview` | 175.1 |
| 7 | `bc2-levelwrite` | `export_ex1::every_name_lands_in_a_staged_canary` | 164.6 |
| 8 | `bc2-levelwrite` | `culltool_ct1::streamed_readers_answer_as_the_whole_document` | 141.9 |
| 9 | `bc2-editor` | `game_view_ww2::ww2_levels_open_in_the_game_accurate_view` | 109.4 |
| 10 | `bc2-levelwrite` | `export_sp01::sp_01_exports_as_a_new_conquest_level` | 101.4 |
| 11 | `bc2-editor` | `nav_parity::navigation_as_the_studio_showed_it` | 73.4 |
| 12 | `bc2-editor` | `ex1::diff_panel_on_aurora_made_ww2_conquest` | 69.6 |
| 13 | `bc2-render` | `weather_refs::ww2_levels_find_their_weather_through_the_entity` | 66.3 |
| 14 | `bc2-render` | `r5d::wake_island_draws_its_beach_and_lagoon` | 56.2 |
| 15 | `bc2-editor` | `simplified_view::the_simplified_view_draws_render_meshes` | 41.5 |
| 16 | `bc2-editor` | `inspect_parity::property_tree_and_links_as_the_map_inspector_showed_them` | 41.4 |
| 17 | `bc2-editor` | `gr2::ai_workspace_on_ons_mp_004` | 40.9 |
| 18 | `bc2-editor` | `real_level_editing::placement_on_real_terrain` | 39.7 |
| 19 | `bc2-render` | `weather_refs::stock_levels_resolve_as_their_folder_did` | 37.2 |
| 20 | `bc2-editor` | `view_geometry::both_views_show_the_same_objects` | 35.5 |
| 21 | `bc2-editor` | `game_view_live::game_view_shares_picks_outlines_and_follows_edits` | 31.8 |
| 22 | `bc2-levelwrite` | `export_e3d::rush_to_conquest_as_a_project_operation` | 31.7 |
| 23 | `bc2-levelwrite` | `export_ex1::names_are_checked_against_the_game_copy` | 30.5 |
| 24 | `bc2-render` | `r5d::mask_tiles_span_aligned_blocks_and_read_on_across_cells` | 29.8 |
| 25 | `bc2-render` | `live::mp_005_live_list_is_the_frame_edits_and_picks` | 26.3 |
| 26 | `bc2-render` | `whole_terrain::the_whole_terrain_is_drawn` | 24.1 |
| 27 | `bc2-render` | `whole_terrain::the_whole_terrain_is_drawn_with_levels_of_detail` | 23.6 |
| 28 | `bc2-render` | `weather_refs::other_package_levels_find_a_weather_state` | 20.8 |
| 29 | `bc2-render` | `world::mp_005_world_draws_and_matches_the_harness` | 19.7 |
| 30 | `bc2-editor` | `game_view::game_accurate_view_of_mp_005` | 17.9 |

These 30 are 4,691 s of the 4,904 s the 594 tests' times add up to. `tests\test_rust.py` now keeps every test's
time on each run (`%LOCALAPPDATA%\bfbc2-tool-suite\rust_test_times.json`) and prints the slowest.

### TS-3: what moved to the game tier

Every test in the table except the last two, plus `inspector_parity::each_level_draws_with_its_selection_outlined`,
`gr2::edit_build_install_restore_on_a_scratch_copy`, `asset_browser::every_placeable_object_of_the_content_set`,
`view_toggles::ons_mp_004_navmesh_and_cull_tree`, `game_view_r5d::...`, `r5d::levels_of_detail_draw_fewer_indices` /
`the_sky_dome_is_drawn_behind_the_level`, and from the merges XF1's `export_xf1` pair (more than 900 s each with two
threads, no result within 3,000 s alone) and `xf1::undo_on_a_mode_project` (43 s): 38 tests, each
`#[ignore = "game tier: <why>"]`, run by the game tier's `Rust core: the game tests` entry (`test_rust.py run
--game`). The quick tier keeps one real level per crate: the editor's MP_005 game view, MP_005 project and outliner
parity (4 levels, 2 s), levelwrite's edits on three levels, the renderer's MP_005 world against the harness,
leveldiff's real pairs.

Coverage, before (main `f1921de`) against after (this branch with main merged), from `cargo test -- --list` and
`test_rust.py list` and `run_tests.py <tier> --list`: no Rust test lost; 594 before (592 quick, 2 corpus); after,
every test of before in a tier (quick, game or corpus), none in no tier (the quick run fails on an ignored test
whose reason is not "game tier"), plus the tests merged from main since (XF1, VW2, DF1). Python: every entry of
before in the same tier after; added: the four Rust entries (in place of the one), the game tier's Rust entry,
`test_rust_skip.py`, and TS-8's `test_aimanim.py`, `test_column_sectors.py`, `test_trees.py` (quick, and `--game`
in the game tier).

### TS-2: optimised test builds

| | before (all opt 0) | dependencies at 2 | + workspace at 1 |
|---|---:|---:|---:|
| cold `cargo test --workspace --no-run` | 158 s | 381 s | 536-626 s (in the tier, beside its other entries) |
| the workspace's crates rebuilt alone | - | - | 265 s |
| `every_level::levelwrite_on_every_prepared_level` | 542 s | 574 s | 98 s |
| `export_e3c::a_ww2_export_takes_its_donors_announcer` | 679 s | 676 s | 558 s (its exports' Python and C# steps) |
| `game_view::game_accurate_view_of_mp_005` | 18 s | 20 s | 4.7 s |
| the game group (the 35 tests then), one binary at a time | - | 48.9 min | - |

The dependencies alone gained nothing: the time is in our own crates (the scene, the views, levelwrite). The
workspace at opt-level 1 is 1.2 to 5.8 times faster on the whole-level tests; debug assertions and overflow checks
stay on. Both stay: the compile costs about 380-470 s more cold, once per build folder, and a warm build is 20 s.

### Skip unchanged, the split, the compile cache

- **Green crates.** A warm quick tier (nothing changed since a green run): clippy 0.7 s (113-137 s cold), the build
  entry 20 s, the quick tests leave out every green crate.
- **The split.** The build entry first (cpu 4), then the quick tests beside clippy (cpu 4 each), fmt and the Python
  entries; the old entry held every CPU for 3,154 s.
- **The compile cache.** sccache 0.19 (`cargo install sccache --locked --no-default-features`, 204 s): a second
  build folder with an empty cache took 0 of 208 compiles from the cache; with `SCCACHE_BASEDIRS` set to the build
  folders, 2 of 208 (a crate with no dependencies hits; anything with a build script keeps its folder's `OUT_DIR` in
  the key, and every crate above it misses in turn). Cold builds A / B: 247 s / 308 s with the cache, against 265-381 s
  without (the machine shared). So it is opt-in (`BFBC2_COMPILE_CACHE=1`) and serves rebuilds of one folder only. One
  shared `CARGO_TARGET_DIR` was rejected: cargo's lock keeps it consistent, but the release binaries have one path per
  folder, so one worktree's `bfbc2.exe` could be copied into another's `bin\`.
- **Levels read once.** `bc2_editor::app::share_level_reads` (called by the editor tests' `gate()`): a level's read is
  reused while its partition, static groups and height field are unchanged (size and write time), at most three
  kept.

### TS-7, TS-8, DF4, TS-12 to TS-14

- simplified_view, view_geometry and view_toggles: a level in the game copy that does not open, a game view that
  does not start, render meshes not read: a failure (`level_here`); a level not in the copy: "skipped". Every
  "skipped" line of a test's output is counted and named per test.
- `test_repo.py` fails on a `test_*.py` no tier runs (one allowed: `tests\test_editor.py`), on a new runner entry
  named by a round code (60 kept by digest), `test_envvars.py` on a new round-coded variable (22 kept).
- `run_tests.py`: a time limit per test (`--timeout`, default 4 x the kept duration, at least 30 min / 3 h / 8 h);
  `--only` matching nothing is exit code 2; job objects `KILL_ON_JOB_CLOSE` (a runner killed mid-test leaves no
  test process); results and the total in the day log. `build_native.install` retries and puts the live file back.
  `test_rust.py` ends a binary still running after 900 s (quick) / 3 h (game) and names the tests that never ended.
  `test_runner_parallel.py` checks each.

### The hang found on the way

With the editor's binary run on its own, `g3::sets_from_the_data` and `g4::create_and_open_projects` (once
`g2::view_tokens_reach_the_frame` and `g3::mode_cards_fall_back_to_the_tile`) start together and never end: 2 runs in 5 at
`--test-threads 2` (opt-level 0 and 1 alike), 1 in 6 with the dependencies at opt-level 0, 1 in 3 at libtest's
default threads, and in 3 of the 4 quick tiers below. Before TS1 it went unseen: the old entry had no limit and nobody
saw which test held it. Round HG1 has it (and the `export_xf1` slowness).

### Proof

| `python run_tests.py quick -j 2` | wall | result |
|---|---:|---|
| main before (the coordinator's last, 2026-11-21) | 3,593 s | 127 of 127 |
| first cold run (ts1 + main `465ca1b`, quick group at `--test-threads 2`) | 1,647 s | 131 of 134: the editor binary ended at 900 s by the hang |
| cold (debug folder and green record deleted; default threads) | 1,636 s | 131 of 134: the hang again, O2 (Defender), trace.py (OPFOR's DF2, fixed on main since) |
| warm (straight after, nothing changed) | 1,278 s | the same three; build 20 s, clippy 0.7 s |
| final (ts1 + main `abcbae2`; the module rebuilt, 273 s; warm debug folder) | **854 s** | **133 of 135**: O2 (Defender's Errno 22, environmental) and O4's texqa (74 of 80; fails the same on main `abcbae2`) |

In the final run the Rust entries took: build 285 s (the merge's crates), clippy 23 s, fmt 3 s, quick tests 44 s
(all passed; 8 parts skipped for want of data, named). Without the hang a cold tier is about 740 s (1,636 - 900 + the
44 s the tests take): the critical path is now the cold build (536-626 s), then the Python entries. The game tier's
new entry: the 35 whole-level tests of the time ran 48.9 min one binary at a time with the dependencies optimised and
the workspace not; the workspace at opt-level 1 cuts the longest 1.2-5.8 times (table above); XF1's `export_xf1`
pair, now in it, did not end within 3,000 s and is HG1's. Not rerun whole at opt-level 1 for that reason.

Finish checks: cargo fmt, clippy `--all-targets -D warnings` on the workspace and the quick cargo tests (in the tier),
ruff and mypy over every folder (the tier's entries), `spec/trace.py --check` clean on `abcbae2`;
`test_runner_parallel.py`, `test_rust_skip.py`, `test_repo.py` and `test_envvars.py` pass. No WW2 path touched: no
rebuild.

## QW2: the audits' leftovers (2026-11-21, branch `qw2`)

Small items from audits AU1-AU4, and three test-health items. No WW2 path touched: no rebuild.

**O4's texqa (the quick tier's red entry since DF2).** Not noise and not a race. texqa's rows-cache key is
`sha1([VERSION, CODE, texture sha1, source, DICE copy, .orig sha1, name])` with `CODE` the first 12 hex digits of the
sha1 of `texqa.py`'s own source (the Rust reads it through `include_bytes`). DF2 edited `texqa.py` twice and kept the
golden. Measured: one `bfbc2.exe` gave `694b2c3a35369c07` on two runs; the Python at three revisions gave one digest
each ("a cache written"): 55d5f0a `ea9e1100b7cf74c1` (the golden), 40d0890 `f631b04e3938f794`, 62d5001
`694b2c3a35369c07` - DF2's "f631 then 694b" were builds of two of its commits. Under parallel load O4 failed for the
same reason. Fix: a group's `REKEY` in `test_cli_o2`'s file snapshot; texqa's rewrites each key the case's own
textures rebuild under the current hash to the same ident with `'<texqa.py>'` in its place. After re-recording (6
lines changed), the Python at 55d5f0a gives the recorded `de73bea880c180fe`; a `bfbc2.exe` built from another
`texqa.py` still fails (its keys do not rebuild). O4 texqa 80 of 80.

**O2's stand-in exe and Defender.** `exe_795745()` wrote a PE whose sections sat at 0x1000, 0x2000, 0x36F000 and
0x55D000 with virtual sizes equal to their few KB of data (holes of megabytes in the image) and no import table.
Written and read back from a scratch folder: blocked (Errno 22, Trojan:Win32/Bearfoos.A!ml) 4 of 4 times. With each
virtual size reaching the next section and an import table (KERNEL32!ExitProcess) at the end of `.text`: readable
7 of 7 times (variants: contiguous alone 4 of 4, imports alone 1 of 1). The menu tools map min(virtual, raw) bytes
and scan only data sections, so no answer moves; aptrefs' Python is retired, so the golden was re-proved: with the
new bytes swapped for the old in memory before the digest, 372 of 372 menu lines as recorded; plainly, 34 differ in
their files digest only, which were written. O2: all passed.

**Feature subsets.** `cargo clippy -p bc2-cli --no-default-features --features <group> -- -D warnings`, each of the
17 groups: 10 passed, 7 (nav, caches, audio, bf1943, cull, texqa, shader) failed on dead code in `argfmt.rs` (each
uses part of it); after the module's `allow(dead_code)` only when not every user group is on: 17 of 17. Cargo's own
"unused dependency" warnings remain for subsets (bc2-convert, bc2-harness, bc2-menu, bc2-navbuild are not optional
per group); they do not fail the build.

**Crate moves.** `guid` and `pe` into `bc2-formats`: `bc2-levelwrite` (and the editor) no longer compile
`bc2-stockmp`, `bc2-menu` no longer compiles `bc2-harness` (iced-x86 among it). Left: culltree's command-line model
(`culltool`, `cullcache`, 3,042 lines with `bc2-pyrt`) in `bc2-levelwrite` - the binary is the wrong home under
PR1b's tool-logic-in-libraries; and `bc2-convert` in levelwrite for the award pictures (acceptable per the audit).

**Not done: the Python shader Merger (AR-5).** `bc2_formats::shaderdb::merge::Merger` has `new`, `add_donor` and
`finish`; OPFOR's 21 in-process uses also call `clone_entry` (wake_install, the weapon ports) and `add_part`, which
have no Rust yet, and OPFOR is not this round's to change. Moved to OPFOR in Rust stage 3 (ROADMAP).

Proof: `python run_tests.py quick -j 2` in the worktree, 135 of 135 passed in 837 s (O2 and O4 among them; the editor's tests did not hang this run; 8 parts skipped for want of data, named). cargo fmt, clippy `--workspace --all-targets -D warnings`, each bc2-cli group alone, the changed crates' cargo tests (bc2-formats, bc2-stockmp, bc2-harness, bc2-menu, bc2-levelwrite, bc2-cli: all passed), ruff and mypy (every folder), `spec/trace.py --check` (693 rules) clean. Release builds: `build_native.py` and `--editor`; `bfbc2 --version` prints 0.10.0.
## HG1: the hangs and the slow tests of the Rust test binaries (2026-11-21, branch `hg1`)

Four causes found and fixed in the product, each with its evidence. Measured on the development PC (16 threads,
15 GB, RTX 4060 Laptop + Radeon iGPU) while main's quick tier, QW2 and OR0 ran beside it. `CARGO_INCREMENTAL=0`,
`CARGO_BUILD_JOBS=4`, build folder `rust-targets\bfbc2-tool-suite-hg1`, main merged at TS1 (workspace opt-level 1,
dependencies 2). Nothing written to `game\`; the game never launched.

### 1. The g3 / g4 editor-test hang: a loader-lock deadlock in NVIDIA's driver

Reproduced on TS1's profile with a loaded PC (the editor binary's quick tests, `--test-threads 2`): 4 hangs in 9 runs,
always two Headless / `Gpu::headless` tests starting together (g3::sets_from_the_data + g4::create_and_open_projects,
g2::view_tokens_reach_the_frame + g3::mode_cards_fall_back_to_the_tile). On the earlier opt-level-0 profile and an
idle PC it did not show in 38 runs. Every thread waited (no CPU); Frida could not even attach (its agent needs the
loader lock). A full dump (procdump, then dbghelp out of process: `rust\hang_stacks.py`) showed, in all four:

- a thread holding the loader lock (`PEB.LoaderLock` owner) inside `nvoglv64.dll!DllMain`, reached from
  `RtlExitUserThread -> LdrShutdownThread`: a thread that had used the driver exiting, the driver's thread-detach
  waiting on its own SRW lock (two dumps) or spinning on `GetExitCodeThread` for its worker thread (two dumps, the
  "100 % CPU" seen before);
- another thread in `wgpu_hal::vulkan::Adapter::open` / Vulkan instance creation -> `nvoglv64.dll` (holding that
  lock) -> `LoadLibraryExW` -> waiting for the loader lock.

`nvoglv64.dll` serves NVIDIA's Vulkan and OpenGL. Each test made its own wgpu instance and device on its own thread
(`Gpu::headless`), and libtest's threads exit after each test, so a device made while another test's thread exited
deadlocked for good. The instance also had the OpenGL backend, which loaded the driver's OpenGL side and ran a hidden-
window thread per instance that exited with it.

Fix (`bc2_editor::render::Gpu::headless`): one device per process, made on the first call and shared (`OnceLock`),
on an instance with the primary backends only (Vulkan, Direct3D 12). A real user's editor makes its window device once
on the main thread (eframe); the headless path is the tests' and the screenshots'.

| editor quick tests, `--test-threads 2`, TS1's profile, loaded PC | runs | hangs |
|---|---:|---:|
| before (merged main, `Gpu::headless` per test) | 9 | 4 |
| after | 30 | 0 |

A guard for the next one: the editor test binary's `gate()` records the tests holding a place; one held past
`BC2_TEST_HANG_S` seconds (default 1200; the quick tier sets 450, half its limit) ends the binary with the running
tests' names and times, and with `BC2_TEST_HANG_STACKS=1` (the quick tier sets it) first prints each thread's stack
(`rust\hang_stacks.py <pid> [dump]`: a full minidump written with dbghelp, nothing injected, each thread's stack scan
named from the PDBs, the loader lock's owner named).

### 2. ex1's shared folder

`ex1::names_form_and_the_export_preview` previewed its stage as `MAPEDIT_EX1_PREVIEW`, prepared into OPFOR's shared
`work\editor\levels\mapedit_ex1_preview`, so two runs at once wrote and deleted the same folder ("Access is denied").
The canary's id is now the run's own (`MAPEDIT_EX1_P<pid>`), its prepared copy removed by a guard even when the test
fails. Passes (44 s); nothing left in `work\editor\levels`.

### 3. XF1's slow levelwrite tests: one decompression per partition

The tests run with the bundle cache's disk files off (`BFBC2_FBRB_CACHE=0`, as the quick tier sets it), and every
stock level bundle's payload (`mp_common` 300 MB, `sp_01` 221 MB, `ww2_common` 302 MB compressed) is over the
in-process budget (128 MB): each read decompresses the payload from its start to the entry. Profiled (Frida stack
samples, dbghelp names): all the time in `zlib_rs::inflate`, under:

- `shaders::reach` (the export's shader reach, `stock_reach` in the check): a depth-first walk reading one partition
  at a time, thousands of partitions, each a pass over hundreds of MB. Now it walks a step at a time, each step's
  partitions read in one call (one pass per bundle per step). The meshes found are the same set (a mesh's recorded
  partition is now the first by name of the first step that reaches it; it was HashMap order before, and only the
  names are used);
- `export::closure`'s copies (a WW2 export copies hundreds of partitions from `ww2_common` / Guadalcanal): read one
  copy at a time; now a round's copies are chosen one by one as before and read together.

| release build, `BFBC2_FBRB_CACHE=0` | before | after |
|---|---:|---:|
| `export_xf1::stock_levels_carry_their_meshes_entries` | over 240 s (stopped) | 11.2 s |
| `export_xf1::a_cross_set_object_gets_its_entries_and_shape` | over 25 min (TS1) | 275 s (616 s with the reach fix alone) |
| `mapexport build` of that test's canary (SP_01 as WW2 Conquest with Guadalcanal's bunker) | 1,886.7 s | 96.1 s |
| the same, the bundle cache on (two runs each) | 20.1, 17.7 s | 14.8, 16.3 s |

The canary's stage from the before and after `mapexport` (main's and this branch's), cache on and off: all eight
bundles and files byte for byte the same; `stage.json` and `report.txt` differ only in their timing lines. The rest of
the cross-set test is the check's own passes (each step of the base's reach, each database read) and stays a game-tier
test.

### 4. The SP_01 walk: every terrain triangle per ray

`query::ray_hit_except` (the walk camera's `ground_below` every frame, placing on a surface, snapping) tested all of
the terrain mesh's 2,093,058 triangles for every ray, and measured each placed object's blueprint radius from all its
triangles for every ray. Now the scene keeps (`bc2_scene::raygrid`, built on the first ray, about 0.13 s in release):
a plan grid of the terrain's triangles (about 4 a cell; each triangle's bounds and the ray's path padded by 1 mm, so
every triangle the ray can meet is listed) and each blueprint's radius. The candidates are tested in mesh order with
the same test, so the hit is the same, bit for bit.

`cargo run --release -p bc2-editor --example ray_bench -- <OPFOR> <level> 1000` (1,000 rays of each kind, the same
deterministic rays in both builds):

| per ray, release | SP_01 before | SP_01 after | MP_005 before | MP_005 after |
|---|---:|---:|---:|---:|
| vertical (`ground_below`) | 34.8 ms | 0.50 ms | 30.9 ms | 0.37 ms |
| picking (any direction) | 39.6 ms | 0.67 ms | 35.4 ms | 0.57 ms |
| `surface_below` | 36.7 ms | 0.38 ms | 33.5 ms | 0.26 ms |
| digest of every hit | `706ea3bb` | `706ea3bb` | `b423931c` | `b423931c` |

`view_toggles::sp_01_lights_collision_states_walk_and_modes` (TS1's profile): 52.1 s -> 10.0 s (573 s at opt-level 0
before TS1); the walk's lines are the same.

## R5e: the WW2 open sea drawn black (2026-11-21, branch `r5e`)

`EDITOR_ARCHITECTURE.md` 3.11, "Phase 3c (R5e, done)"; spec RNDR-DRAW-8, RNDR-WATER-3. No Python replaced. Tools:
R5d's `examples/render_probe.rs` (its `--pixel x,y` read-back) and `examples/entry_probe.rs`
(now printing each block's header bytes 0x18 - 0x1f), `fxc /dumpbin` on the dumped DXBC.

### Root cause: the water's material never reached its shader

Wake Island 600 m out was 53.5 % near-black. The deep-sea pixels read back as HDR `(0, 0, 0, 0)`: the water wrote
nothing (`--drop patches,strips`: the fog-coloured clear showed through, alpha 0 again). R5d read this as "BF1943's
water writes alpha 0 past the reefs until the haze is drawn"; it is not the data's and not the haze.

Decoded from the water's pixel shader (`Terrains/Wake_island_s/Shaders/Wake_Water`, program 11861, ps_5_0, 560
slots): the output is `(colour x a, a)` with
`a = min(1, ((1 - n.v)^8 + smoothstep((t + 30) / 85)) x smoothstep(t / 5))` and `t` the scene's view distance
(`DepthBuffer`) less the water's own, both through `cb0[13]` - `waterMaterial[3]`, OPFOR's `g_depthScaleFactors`
record (`wake_water.py`: BF1943's colour programs translated, their material array named `waterMaterial`). With
`cb0[13] = 0` both distances are 0 (the shader guards its divisions to 0), `t = 0`, `a = 0`: the water draws
nothing anywhere, the lagoon included. R5d's green lagoon was the terrain's sea-floor layers, not the water.

The view (`frame::cbuffer`, `live::plan`, ports of `renderqa.cbuffer`) wrote a block's constants only inside a
`$Globals` variable named `constants`. The Wake water has none: `heightmapWorld*` reached it only because R5d writes
them through `ps_extra` at their registers; `g_depthScaleFactors` is a per-frame value (`Dyn::DepthFactors`) and went
through the material path, so it was dropped. What the engine goes by is the program block's header: bytes 0x19 /
0x18 are the material's first register and the `$Globals` end (FORMAT.md section 1). Measured over every program of
six databases (`tests/r5e.rs`):

| level | programs whose `constants` is the header window | without `constants`, reading none | read by name (terrain) | `waterMaterial` |
|---|---:|---:|---:|---|
| MP_005 | 1,840 (all) | 4 | 0 | - |
| SP_01 | 1,790 (all) | 1 | 0 | - |
| Wake Island | 1,793 (all) | 184 | 39 entries | `wake_water` |
| Guadalcanal | 1,792 (all) | 216 | 40 entries | `guadal_canal_water` |
| Iwo Jima | 1,794 (all) | 196 | 37 entries | `iwo_jima_water` |
| Coral Sea | 1,755 (all) | 206 | 12 entries | `cs_water` |

### The fix

`query::block_registers` reads the window; `Draw::ps_window` keeps it (materials, water, terrain, sky);
`frame::material_window` is the shader's `constants` where it has one (every DICE program: the same registers), else
the header's window. The water then gets its depth factors as a per-frame hole like DICE's water. No reflection pass
was needed: the shader's reflection is the level's static cube (`outdoorLightReflectionEnvmap`, as bound since R5b)
and its own `Sky_Beach_Wake` cube, both already bound.

### Before and after (`render_probe`, 1280 x 720; R5d's views)

| level, view | near-black (< 8) before | after | dark (< 40) before | after |
|---|---:|---:|---:|---:|
| Wake Island `auto` (the lagoon) | 0.2 % | 0.1 % | 1.8 % | 1.5 % |
| Wake Island 600 m out | **53.5 %** | **0.0 %** | 55.6 % | 0.3 % |
| Wake Island north beach | 13.5 % | 0.0 % | 16.5 % | 0.6 % |
| Coral Sea `auto` / 900 m out | 0.6 % / 0.5 % | 0.4 % / 0.1 % | 3.9 % / 2.2 % | 3.5 % / 1.2 % |
| Guadalcanal `auto` / 1,200 m out | 0.1 % / 0.0 % | 0.0 % / 0.0 % | 5.5 % / **48.6 %** | 3.3 % / **0.0 %** |
| Iwo Jima `auto` / 1,200 m out | 0.4 % / 0.0 % | 0.3 % / 0.0 % | 9.3 % / **56.7 %** | 4.2 % / **0.0 %** |
| MP_005 west shore / `auto` | 0.1 % / 1.9 % | the same, HDR bit for bit | 35.4 % / 14.2 % | the same |
| SP_01 `auto` | 41.7 % | the same, HDR bit for bit | 86.9 % | the same |

Guadalcanal's and Iwo Jima's deep sea was a dark grey plane (above the near-black threshold, so R5d's table missed
it). The opaque and alpha-tested classes' means are unchanged on every view (the terrain shaders without `constants`
now also get their stored block values; their engine-named ones were and are written by `ps_extra`). A deep-sea pixel
on Wake: `(0, 0, 0, 0)` -> `(0.034, 0.110, 0.220, 1.0)` HDR. Wake's sea mean (blended pixels) red 0.333, blue 0.468
on `tests/r5e.rs`'s 640 x 360 view. SP_01's 41.7 % is its own dark interior view, not water (unchanged, R5d's).

### Left

- The in-game picture (`drive\ar_ok.png`, Wake from the Japanese carrier) shows whitecaps out to the horizon and a
  stronger blue; the view's far sea is smoother and greyer near the horizon (the static reflection cube where the game
  reflects a live one, `time` 0 so no moving waves, the haze over the scene not drawn: RNDR-WTHR-3). Not proven
  against a capture.
- Thin bright seams along a few far water triangles' edges (Guadalcanal, Iwo Jima at 1,200 m): cracks in the water
  mesh or a depth-tie at a shared edge; not decoded.
- The sun's highlight is wide and bright on Guadalcanal's 1,200 m view (the light's direction is RNDR-WTHR-4's
  placeholder).

### Screenshots (not in the repository)

`scratchpad\r5e\shots\before_*` / `after_*` (`render_probe`, main against this branch, the views above),
`shots_editor\gw1_<level>.png` / `gw1_<level>_light.png` (the editor's game view, dark and light, the four WW2
levels: `tests/game_view_ww2.rs` now saves both themes), `r5d_game_view_dark` / `_light` (MP_005's harbour,
unchanged).

### Tests

`cargo test --release -p bc2-formats --lib`: 215 passed (new `shaderdb::query::tests::material_registers`).
`cargo test --release -p bc2-render`: 33 unit tests (new `frame::tests::a_shader_without_constants_takes_the_material_in_its_block_window`;
`live::tests::a_plan_filled_gives_the_cbuffer` also with the window) and the integration binary, 4 + 12 game-tier,
all passed (new `tests/r5e.rs`: `block_windows_are_the_material_registers` over the six databases,
`the_ww2_open_sea_is_drawn` on Wake Island and Guadalcanal: the water plans `Dyn::DepthFactors`, under 1 % near-black
and 5 % dark, the sea bluer than red). `cargo test --release -p bc2-editor --test editor` `game_view_ww2` (the four
WW2 levels, both themes saved) and `game_view_r5d` passed. clippy `--all-targets -D warnings` on bc2-render,
bc2-formats, bc2-editor and `cargo fmt --check` clean. `spec/trace.py --check` 695 rules; release builds (`build_native.py`, `--editor`); `python run_tests.py quick -j 2` 135 of 135 passed in 769 s (2 with parts skipped for want of data).

## GR3: the mesh viewer folded into the asset browser's preview (2026-11-21, branch `gr3`)

**The inventory.** The mesh viewer's GUI was its page for a browser (`meshview.py <mesh>`: three.js from a CDN, the
models and textures embedded): orbit / pan / zoom, a switch per part with its triangles and colour texture,
wireframe, several meshes stacked, a level of detail; behind it the index of the game copy (bundles and every
level's Dx9 shader database, cached), `find`, `load_mesh` and the picture (`--png`: side, top, front, size). Each
feature's outcome is in `bfbc2-map-editor/EDITOR_ARCHITECTURE.md` 9.3: all carried into the editor's Asset preview
and Meshes mode; the page file itself is not (nothing to share or fetch); the picture command line stays. Added
beyond it: destruction states, textures by channel, normals, bounds and measurements, part picking in the picture,
Save picture. Not added: a skeleton (the mesh viewer has none; skinned meshes in their rest pose, as there) and
collision (not the mesh viewer's; the level views' Collision mode has it).

**How it is drawn.** `bc2_convert::meshview` is a port, not a re-implementation: `Index` reads the Python's cache
(`meshview_index.pkl`, `bc2_pyrt::pickle`; built by `bc2_formats::meshindex` and written byte for byte when stale -
the cache code moved there from `bc2-cli`'s `gameindex`, which calls it), `load_mesh` the same parts in the same
order (surfaces by the blueprint's XML and the same pattern, the colour texture by the same name rules,
`texcheck::levels` / `decode` as `texture.py`), `stack_models` and `render` with NumPy's arithmetic: float32 screen
positions (Python numbers weak under NumPy 2: `scale` rounded to float32 before the multiply, `h` an int cast in),
float64 barycentric weights and depths, the facing's shade in float32 after a float64 dot product, `np.mod`'s sign,
truncating casts. The editor draws on the CPU with that loop (`Canvas::fill`); the orbit camera only projects
differently. A GPU path was not needed: the preview redraws only when something changes, and a 29,000-triangle
soldier reads and draws three 400 px pictures in 0.34 s (debug test build, opt-level 1).

**Parity** (`rust/bc2-editor/tests/gr3.rs`, the answers recorded by `bfbc2-mesh-viewer/tests/record_preview_parity.py`:
SHA-1 digests and counts only, `tests/data/meshview_parity.json.gz`, 15 KB):

| Mesh | LODs | Parts | Triangles (LOD 0) | Pictures |
|---|---|---|---|---|
| M1Garand_Mesh (weapon, async bundle) | 1 | 5 | 5,841 | 3 of 3 |
| GuardTower_Mesh (destructible building) | 3 | 21 | 4,371 | 9 of 9 |
| Barrack_Mesh (building, 4 LODs) | 4 | 21 | 6,110 | 12 of 12 |
| PalmTree_01_Banger_Mesh (alpha-tested vegetation) | 1 | 4 | 2,053 | 3 of 3 |
| M1A2_Mesh (vehicle) | 4 | 17 | 15,298 | 12 of 12 |
| Us_Infantry_mesh_Mesh (skinned soldier, half2 UVs) | 2 | 20 | 29,048 | 6 of 6 |
| AmmoCrate_Mesh | 2 | 5 | 5,588 | 6 of 6 |
| Parrot_01_Mesh | 1 | 1 | 1,136 | 3 of 3 |
| M1Garand + Jap_Type5 stacked (`stack_models`) | - | - | - | 1 of 1 (and the offsets) |

94 parts (name, surface, colour texture, vertex and triangle counts, positions, normals, texture coordinates,
indices, the decoded texture) and 55 pictures identical, byte for byte, on the first run of the port. The index
loads from its cache in 0.4 s (166,054 entries); the whole parity test takes about 3 s, so it is in the quick tier
(skipped without the game copy). Save picture of the stacked rifles at 1200 px is identical to
`meshview.py ... --png --size 1200`'s file (checked by hand against the command line's output, and in the screenshot
test against `render`).

**The page kept.** Project OPFOR's WW2 rebuild calls `meshview.page()` twice (`attachments.py`, `scopes.py`: their
preview pages in `work\ww2\preview`), so `page()` and `PAGE` stay unchanged (deprecated in their docstring only);
`meshview.py`'s library path the rebuild uses (`Index`, `load_mesh`, `stack_models`, `render`, `page`) is byte for
byte the same code: the round changed only the module docstring, `page()`'s docstring and `main()`.

Proof: `python run_tests.py quick -j 2` in the worktree, 134 of 135 passed in 795 s - the one failure is O2, whose
stand-in `b.exe` Microsoft Defender blocked on this PC (`OSError: [Errno 22]`, the known environmental case; O2
covers no command line this round touched), and O3 (meshview's recorded answers, its usage lines re-recorded from the
Python) passed. cargo fmt, clippy `--all-targets -D warnings` on bc2-convert, bc2-render, bc2-editor and bc2-cli (and
bc2-cli's meshview, bf1943 and caches groups alone), their cargo tests (the editor's g3 / g4, round HG1's, ran in the
quick tier), ruff and mypy on the changed Python, `spec/trace.py --check` (693 rules) clean. Release builds:
`build_native.py` and `--editor`; `bfbc2 --version` and `bfbc2-editor --version` print 0.11.0. Screenshots (Dark and
Light: the guard tower with its edges and bounds, destroyed, the rifles from the Meshes mode, the normal-map channel,
the demo panel) went to the session's scratch folder, not the repository.

## OP1: the map editor's core before E5 (2026-11-21, branch `op1`)

Audit AU1 R14 / R17 / R18 / R24 (ROADMAP's OP1 row), the later audits' OP1 items (AU3 AR-6 / D4 / D7 / D9 / D12 /
D13, AU4 DS-20, BUG-12, BUG-13, CQ-7) and DF1's three left items. Measured on the development PC (16 threads, 15 GB)
while GR3 and R5e built and tested beside it; `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=4`, the build folder
`rust-targets\bfbc2-tool-suite-op1` (the dev profile is optimised with debug info).

### Part A: operations on any file of a level

| What | Where | Proof |
|---|---|---|
| Every operation names its targets: partition, static group file, height field, paint layer, navigation side file, project | `bc2_levelwrite::files::Target`, `Op::files` | `files::tests`, spec XLOG-FILE-1 |
| `files`: edits of several files in one operation (partition operations, group members' transforms, each row's fourth word kept), applied on copies, all or nothing | `files::LevelFiles::apply`, `write::apply_partition_edits` | a failing second edit leaves the partition's delete unmade and no group file taken |
| The editor keeps a writer per file in its checkpoints: one undo step for a multi-file operation, History jumps across checkpoints | `edit::Doc` (`files: LevelFiles`) | `op1::a_multi_file_operation_is_one_step_whole_or_not_at_all`: 40 member moves, `go_to(20)` replays 4 from the checkpoint at 16, files and scene back at every jump |
| A base file's sha1 recorded on its first edit; the export applies the log to all files and writes edited group files in place of the base's (renamed with the level); a target it cannot write yet refuses in words | `project::MapProject::note_file`, `export::build` | spec XLOG-FILE-2 (implemented: no game export of a group edit was run: SG1 makes the first) |
| Project format 6: `base.files`, `min_reader`, a 5 -> 6 migration, versions read whole (BUG-13) | `project.rs` | `versions_and_min_reader`; the synthetic corpus of versions 1, 2, 4, 5 (`tests/data/projects`) |

**Old logs replay the same.** The corpus's four files replay to the partition sha1
`9eb95861781c3387b0aab68b318af4faa4b46888` on this branch and on main at 082f2d2 (a throw-away worktree of main ran
the same four files through the version-5 `apply`); `apply_files` on a log of the kinds of versions 1 - 5 equals
`apply_with` and gives no other file (`files::tests`). The export's code path for such a project is the old one plus
an empty map of edited group files.

### Part B: delta tiles and the journalled autosave

**Strokes.** 64 x 64 tiles, each file `BC2TILE1` + width + height + little-endian samples, kept once by sha1 in
`<project>.mapedit.json.tiles`; the log's `stroke` names the tiles' places and sha1s before and after. Rasters keep
tiles behind `Arc`s shared with the checkpoints. Commit time on a 2048 x 2048 height field (dabs of radius 24, 1 - 4
tiles each), 400 strokes, as it was tuned:

| Step | Commit median | p95 | Undo |
|---|---:|---:|---:|
| First version (each tile file synced, read back from disk to apply) | 14.64 ms | 19.60 ms | 8.2 ms |
| Tiles not synced one by one (a short tile fails its sha1 and the stroke is refused, never applied wrong) | 11.38 ms | 15.05 ms | 7.1 ms |
| Written in place | 10.78 ms | 14.31 ms | 7.1 ms |
| The tile folder keeps the tiles last written or read in memory (4,096, about 32 MB) | 2.18 ms | 3.54 ms | 3.1 ms |

The profile of the first version: applying 7.4 ms (reading back the tiles just written: Windows file opens with the
scanner), the tile diff 2.2 ms, the checkpoint 0.0, the journal line 0.03.

10,000 strokes (`op1::stroke_budgets_on_10000_strokes`, ignored by default): 26.7 s in all; commit median 2.57 ms,
p95 4.13 ms, max 70.5 ms (a journal compaction); undo 3.4 ms, a History jump of 40 2.2 ms, redo 2.4 ms - within
AU1 P16's budgets (16 ms a stroke, 100 ms an undo on a 10,000-operation log). The project file is then 8.1 MB (about
800 bytes a stroke: 4 tiles' sha1s, number, author, time), the journal 5.0 MB, the tiles 233 MB: tiles of undone
strokes are never collected (left for E5, with grouping a drag's dabs into one stroke).

**The journal.** One CRC-32-checked line per change after a first line holding the project; flushed at once, synced
at most once a second, compacted after 4,096 lines or 8 MB. A session's journal cut at every byte gives the project
after its last whole line (`journal::tests`); a writer killed six times (`op1_journal`: three times through the
editor's writer, three times writing each line in flushed 1 KB pieces) left a cut-short line twice (1,024 and 7,168
bytes); every time operations 1 ... k were recovered with nothing guessed, resumed, and three more appended after the
last good line. The organisation now follows each new operation (`Organisation::apply`) instead of replaying the log
per commit (AU1 D7).

### Part C: typed errors, explicit states, the save prompt, the game view on a worker

- 155 `Result<_, String>` in `bc2-editor` (89) and `bc2-render` (66) became `EditorError` / `RenderError` (0 left);
  the words shown are the same (`Display` is the old text). Converted by a rewrite script over the signatures and
  the string-made errors, then by hand where the compiler said (47 places in the editor, 10 in the renderer).
- `Doc::problems` are `Problem`s (severity, code, operation, objects).
- DF1's side fields as states: `Closing`, `ExportFor`, `PreviewState`, `Mode::Preparing { job, then }`,
  `Then::Create(.., target)`, `InstallRun`, the navkit job's `running` / `outcome`, `GameSlot`.
- Close map and the window ask about unsaved edits after the busy question (`op1::closing_with_unsaved_edits_asks_first`).
- The game-accurate view's world (`gameview::read_world`) is read on a worker; the device is made on the window's
  thread (`GameView::finish`); headless runs wait for it in `run_action`, so the game-tier tests read it as before.

### Part D: the god objects split

`ViewState` 74 fields -> 45 (eight parts: `browser` 11 fields, `menus` 5, `outline` 3, `measures` 3, `inspect` 6,
`props` 4, `pointer` 4, `game_view` 2); `EditorApp` 37 -> 22 (`command` 3, `new_project` 5, `start_ui` 9,
`catalog_state` 3). Done by a script (the fields and the two literals moved) and a compile-driven rewrite: every
access rustc reported as a missing field (E0609 / E0615) rewritten at its span to `<part>.<field>`, 382 places in
`src` and `tests` over three passes; nothing else changed. Proof of identical behaviour: the editor's tests (lib 67,
`editor` 51 run, 20 ignored as before) and seven screenshots of the main views through `bfbc2-editor --screenshot`
(the start screen dark, light and WW2; the demo level dark, light, with a selection, the navigation view; 1600 x 950)
byte for byte the same before and after - and between two runs before, so the comparison means something. The
screenshots stayed in the session's scratch folder.

### Left, with reasons

- **SG1**: member edits in the views (selecting, the gizmo, delete / add a member: removing an `Assets` item renumbers
  the `#k` guids, so SG1 decides how a member is named across edits) and the first export of a group edit on a real
  level (XLOG-FILE-2 is implemented, not yet verified on game data).
- **E5**: the terrain and paint brushes (`Doc::stroke` is their entry point), the terrain view following a raster,
  writing rasters in the export (the height field's tail sections must be recomputed: refused in words until then),
  grouping a drag's dabs into one stroke, and collecting tiles no operation names any more (233 MB after 10,000
  strokes).
- **The export window's workers** (`running`, `scanning`, `previewing`) stay `Option<Receiver>`: each is already a
  two-state (idle / running) value with its result kept in the form; no DF1 generation travels with them.
- **Part D went as far as a mechanical split goes**: `ViewState` still has 45 fields (the viewports, the doc, the
  selection, the workspaces' own states such as `navs`, `diff`, `mode_view`); the workspace / tool trait of AU1 R21
  is the post-Rust improvements' round.
- No in-game test (paused): nothing here changes what an export writes for a project without group edits.

### Proof

- `cargo fmt --check`, `clippy --all-targets -D warnings` (bc2-levelwrite, bc2-editor, bc2-render) and their cargo
  tests: levelwrite lib 40, `levelwrite` 13 run; editor lib 67, `editor` 51 run (the game-tier ones skip or are
  ignored as before); render lib 64.
- Release builds: `python rust\build_native.py` (bc2_native.pyd, bfbc2.exe; 3 min 27 s) and `--editor`
  (bfbc2-editor.exe, mapexport.exe; 5 min 20 s) on a machine shared with two other rounds.
- `spec\trace.py --check`: 699 rules (four new: XLOG-FILE-1, XLOG-FILE-2, XLOG-AUTO-1, XLOG-DELTA-1; XLOG-LOG-2
  updated).
- `python run_tests.py quick -j 2`: 132 of 135 in 631 s at first; the two OP1 failures fixed (the journal test's
  child variable renamed `BC2_JOURNAL_KILL_CHILD` for what it does, AU3 TS-13; the 10,000-stroke measurement marked
  `game tier:`) and those entries passed again (the Rust core's eight entries in 122 s). O2 fails on this PC as noted
  in the rules (Microsoft Defender holds its stand-in `b.exe`: environmental).
- ruff and mypy on the changed Python (`core\bfbc2core\envvars.py`).
## AV1: O2's stand-in exe out of Defender's reach (2026-11-21, branch `av1`)

**The problem.** O2's aptrefs cases write `exe_795745()`, a synthetic 32-bit PE with build 795745's time stamp and
its two getters' switch tables at their real addresses (0x76F000 and 0x95D000, megabytes apart). Microsoft
Defender's machine-learning scan blocked reads of it (Trojan:Win32/Bearfoos.A!ml, Errno 22). QW2's loadable layout
passed for a few hours. Then `Get-MpThreatDetection` listed it again at 17:26, 17:32 and 17:44: 33 detections on
this PC, all `b.exe` under `cli_o2_*` or QW2's probes. One was `b.bin` (20 Nov, 23:32), so the extension does not
matter; the content does. Any synthetic program is a new, unknown file to the cloud model.

**What aptrefs needs.** `bc2_menu::common::PeImage::open` reads the file and `bc2_formats::pe::raw::parse` wants
`MZ`, `PE\0\0` at `e_lfanew` and the PE32 magic. It then reads the time stamp and the section table. The getter
addresses are fixed and absolute, and the PE32 magic rules out the suite's own binaries (`bfbc2.exe` is PE32+), so
neither a real compiled exe the suite builds nor a non-PE file works as it stands.

**The design.** The case writes the stand-in with `SI` in place of `MZ` and `SI\0\0` in place of `PE\0\0`. These
are the only 4 bytes that change, at 0x0-0x1 and 0x80-0x81. Without the signatures the file is not a program to any
scanner. The cases that read it set `BFBC2_TEST_STAND_IN_EXE=1` (registered in `envvars.py` as a test variable).
Only then, and only for a file with both marks, `PeImage::open` puts the signatures back before parsing. The tools
therefore read exactly the bytes they read before, through the same command line, process, logs and files. The
other aptrefs cases still read real on-disk PEs unchanged (`x.exe`, the game copy's exe, the error cases), and the
`--game` tier reads the real `BFBC2Game.exe`. Other options I ruled out: a further hand-made layout (the same
moving target), and an exe from the suite's own builds (all 64-bit).

**The proof.** aptrefs' Python is retired, so the golden was re-proved, not re-recorded:

- The restored bytes equal `exe_795745()` byte for byte, which is the file the golden's digests were taken over.
- With the kept bytes swapped back before the digest, every line holding the stand-in is as recorded: all 34, on
  code, both streams, files and log.
- Run plainly, those 34 differ in their files digest only. Those digests were written to the golden (34 lines).
- `bc2-menu` has a unit test: both marks restored, a program read as it is, one mark alone or a short file
  untouched.

O2 then ran 6 times in a row with Defender on: all passed, 966 lines each. `Get-MpThreatDetection` went from 33 to
34 entries. The new one (18:04:15) was raised for OP1's worktree binary
(`bfbc2-tool-suite-op1\bin\bfbc2.exe`) reading OP1's own old-layout `b.exe`. None was for this round's files.
The count was still 34 after the quick tier below, which runs O2 too.

The rest of the checks:

- `python run_tests.py quick -j 2`: 135 of 135 passed in 868 s, O2 among them (1 with parts skipped for want of
  data).
- `cargo fmt`: clean.
- clippy `-p bc2-menu --all-targets -D warnings`: clean.
- `bc2-menu`'s tests: passed.
- ruff and mypy on the changed Python: clean.
- `spec/trace.py --check`: 695 rules, clean.

## GR4: DbxEditGUI folded into the map editor as the DBX inspector, then retired (2026-11-21, branch `gr4`)

The suite's last C# tool, DbxEditGUI (`bfbc2-dbx-tools\DbxEditGUI.cs`, with `DbxEdit.cs`, the C# DBX classes it was
built on), became the map editor's **DBX inspector**: `rust/bc2-editor/src/dbxinspect.rs` (the model: DbxEditGUI's
`MainWindow` method by method, every edit through `bc2_formats::dbxedit`, the code `bfbc2.exe dbxedit` runs) and
`dbxpanel.rs` (the window, in the editor's theme), with a thin hook in `app.rs` (one field, one call, one action) so
the concurrent rounds' edits of `app.rs` meet few lines. `bc2_formats::dbxedit::encode_value_rounded` gives
`EncodeValue`'s `rounded`.

**The inventory** (`bfbc2-map-editor/EDITOR_ARCHITECTURE.md` 9.4): open (a file, a drop), the tree, Apply of
attributes and value, Go to reference, Undo, Find / Next, the Instance menu (remove, graft, copy field, add item,
copy GUID), the Edit menu (replace, replace prefix, remove a field everywhere, set a bool everywhere, the partition's
GUID), the Level menu (check, against a cull level, fix slots; a guid on an output line goes to its instance), Save
(`.bak` first), Save as, Export / Import XML, Save edit script: all carried over. New: a bundle's entry opened in
memory and saved into a copy of the bundle (FbrbTool's unpack and pack: every other entry as it was), the open
level's files, the project's level partition (a field edit is `Op::SetField` of the project, carrying exactly the
bits the file would take: a decimal from its bits, a denormal included; a value the schema's type cannot carry
exactly is refused, and the structural menus are left out there because they would bypass the log), a note under a
4-byte number of 65,536 or more that the XML prints as a whole number though it may be a tiny decimal. Not carried
over: the Help menu (FORMAT.md, README: the editor's Help and the folder's README), `ToolLog`'s file a day and the
crash window (the editor's own error handling), Ctrl+Z / Ctrl+F / F5 as the window's keys (the editor's own
shortcuts; the inspector's are buttons, F3 kept). The edit script's header names `bfbc2.exe dbxedit edit`, as
DbxEdit.exe is retired.

**The denormal trap.** DbxEdit's XML prints a 4-byte value whose top byte is 0 as a whole number (7.3e-40 as 524288,
its bits). The inspector's value box shows the XML's text, and an Apply types it as DbxEdit's import does (the
original bits' kind decides between the integer and the decimal reading), so the same text gives the same bits back
and a typed decimal is rounded and said. Recorded: 39 denormals typed as decimals (`7.3e-40`), the same text typed
again (unchanged), a neighbour's whole number, `1.5`: all as DbxEditGUI; the project route carries the bits
(`f32::from_bits` widened exactly, checked to narrow back to the same bits). Nothing compares XML text for values:
the parity compares the files' bytes.

**Parity with DbxEditGUI** (`rust/bc2-editor/tests/dbxedit_parity.rs`, game tier). The round's recorder (C#, compiled
with DbxEditGUI.cs and DbxEdit.cs at `f46b083`, run once; not kept) drove DbxEditGUI's own window through reflection,
as `DbxTests.cs`' window test did, its prompts answered from a timer; the file-dialog commands ran their method
bodies with the dialog's answer. It worked on copies of 19 files of the game copy's prepared levels: MP_005's,
MP_005SR's and SP_01's level partitions, MP_005's description and terrain, and the smallest files with denormals,
8-byte and 2-byte numbers, bytes, multi-number values, whole-number fields and lists. Kept as
`tests/data/dbxeditgui_gr4.json.gz` (57 KB): the inputs (element paths, values typed, guids, prompt answers) as
`core/tests/dbxedit_golden_game_*.json` keeps them, the outputs as SHA-1s (file bytes, output lines, script lines,
messages). Replayed on `DbxDoc`, **1,232 of 1,232 steps identical**:

| Step | Count | Checked |
|---|---|---|
| tree | 19 | every element's label, heading, attributes and value (SP_01: its whole tree) |
| Apply | 618 | the file's bytes, the script line, the output, the outcome: 513 changed, 41 unchanged, 64 refused (DbxEditGUI's message); 125 rounded decimals said |
| Save (`.bak`), Save as, Undo all | 19 each | the saved bytes, the `.bak`'s bytes, the file back to the original |
| edit script | 38 | every line (`setfield=`, `# edited ...`, `drop=`, `graft=`, ...) |
| Find | 114 | the matches' paths, the status line, the output |
| menu commands | 194 | replace 30, replace prefix 28, set bool / byte 43, partition GUID 19, add item 32, remove field 14, remove instance 28: bytes and output |
| graft, copy field | 57, 57 | bytes and output |
| check, against MP_005SR's cull trees, fix slots | 19, 1, 1 | the output (every finding), the bytes |
| Export XML, Import XML of an edited export | 19, 19 | the XML's bytes; the edited XML reproduced (its SHA-1), the imported bytes and the report |

`every_level_file_round_trips_through_the_inspector` (game tier): every unique binary `.dbx` of the prepared levels'
`dist` folders, **31,037** of 89,337 files, opened and written back byte for byte, its XML imported back byte for
byte (33 s on 4 threads) - what `DbxTests.cs --corpus` (the full tier, 1,147 s) asked of the C# classes.

`tests/gr4.rs`: the window headless - the start screen's link opens it, a row clicked selects, Apply gives
`dbxedit edit setfield=`'s bytes, a denormal's text applied again changes nothing, Save as through its question,
Undo, Find; and (game tier) MP_005's project partition: a decimal (`0.30000000000000004`, rounded), a whole-number
field's `7` and `524289` in a decimal field applied in the inspector are each one `SetField` operation, after which
the project's partition holds the bits the same edit gives the file; Ctrl+Z three times gives the base partition
back in the inspector.

**Nothing points at it** (`RETIRED.md`'s checklist): the hub's More tools > DbxEditGUI is More tools > DBX inspector
(`bfbc2-editor --dbx <file>`); `run_tests.py` lost "DbxEditGUI build (when stale)", "DbxEditGUI and the DBX classes it
is built on" (quick), "DbxEditGUI's DBX classes on the corpus" (full), the `dbxtests` resource, the no-game part and
`tests/build_if_stale.py`, which only it used; the find_tool tests' stand-in became `ExampleTool.exe`; NavKit's
"close DbxEditGUI" message names the map editor. Project OPFOR BFBC2 calls no DbxEditGUI: `opforcore.paths`'
`TOOL_DIRS = {'DbxEditGUI': ...}` and two tests use the name as a stand-in only (left for OPFOR to tidy; nothing
runs it).

**Tests.** `cargo fmt`; clippy `--all-targets -D warnings` on bc2-editor, bc2-formats, bc2-cli: clean. Their cargo tests: 417 passed, 0 failed (24 ignored: the game tier), and the game-tier GR4 tests run here: `gr4::` 2 of 2, `dbxedit_parity::` 2 of 2 (after merging main's GR3, OP1 and AV1). `python run_tests.py quick -j 2`: 133 of 133 passed in 348 s (O2 included). `spec\trace.py --check`: ok, 695 rules. Release builds: `build_native.py` and `--editor` (`bfbc2-editor 0.13.0`). The version: 0.13.0 (a removal, after GR3's 0.11.0 and OP1's 0.12.0).

**Left.** Static group files have no generic field operation in OP1's `Op::Files` (only members' transforms), so the inspector edits a group file through its own save path, not the project; OPFOR's `opforcore.paths.TOOL_DIRS` still names `DbxEditGUI` (unused) and its `SUITE_REQUIRES` must move to `('0.13.0', '0.14.0')`.

## SG1: static group members editable (2026-11-21, branch `sg1`)

ROADMAP's SG1 row (audit AU1 A6 / G-N4, OP1's left items). Measured on the development PC (16 threads, 15 GB) beside
the other rounds; `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=4`, the build folder `rust-targets\bfbc2-tool-suite-sg1`.
Spec: LEVEL-MG-5 - LEVEL-MG-8, HAVOK-RUN-3, XLOG-MEM-1 - XLOG-MEM-3 (new), XLOG-FILE-2 (now verified), XLOG-LOG-2.
No Python replaced; `game\` only read.

### The ground truth: how DICE's levels hold a group

Read from the game copy's level bundles: 131 model groups of 31 levels (BFBC2 Dist and Package, Vietnam, BFBC1, the
four WW2 levels), 107,103 members; every rule below holds on all of them (`tests/groups_sg1.rs::
every_stock_group_keeps_the_rules` re-checks 63 groups of 16 levels, 60,723 members, 504 rule checks, in the game tier).

| File | What it holds per member |
|---|---|
| level partition | the group entity's `MemberDatas`: one member type per blueprint and shader set, `InstanceCount`, one `SpawnDatas` record per instance (`Enumeration` = its client-tree leaf, `MeshShaderSet`), `FirstPhysicsPart` / `PhysicsPartCount` and (static groups) `FirstNetworkId` / `NetworkIdCount` **cumulative over the types, the "count" fields being ends** (MP_005's palms: 291 -> 317 for 13 x 2 parts) |
| `<Group>_HavokAsset.dbx` | `Assets`, `Transforms` (16 numbers), `Enums` (= the `SpawnDatas` enumerations, in order), all in member-type runs |
| `<Group>_HavokAsset.res` | collision **per member type** in its own space (MP_005's static group: 32 types, 33 rigid bodies and MOPP shapes, every vertex within 10.1 m of its origin; no instance placement anywhere), and per physics part of every instance one wrapper word and one material byte |

The model-group wrapper (spec HAVOK-RUN-2 left it undecoded): the n "part hashes" are **each part's owning instance's
leaf**, repeated per part; the c boxes and the table's k2 words are one per type part; the m end positions the
cumulative per-instance parts of the m types; the undecoded bytes are HAVOK-RUN-1's index run over the instance parts
(`d`, `t`, one byte per part when d is not 1), the same bytes for every instance of a type (78 of 78 long runs).
Members' leaves are 1 .. L of the client tree, each once (16 of 16 levels). No member transform is scaled (23,780 of
23,780 on six levels).

### The identity scheme

Deleting a member must leave no gap (every number above is cumulative, and a type's collision and network ids are
indexed through them), so DICE's places shift. Tombstones were rejected: a tombstoned instance keeps its collision,
its network id and its leaf, and parking it anywhere is a guess about the physics world's bounds. So:

- a base member is `<group instance>#<k>` (k its place in the **base's** file) for the whole project; an added one
  `#+<n>` (per instance, one more than any in the log and redo stack); JSON: the number as OP1 wrote it, `{"added": n}`;
- `LevelFiles` keeps each edited instance's current order; an edit finds a member's place through it;
- the files are always in DICE's layout: a delete takes the `Assets` item, the 16 numbers and the `Enums` item out,
  the type's `SpawnDatas` record, `InstanceCount - 1`, and makes the cumulative numbers again; an add copies a member
  of the kind to the end of its kind's run (its leaf 0 until the export);
- the last member of a kind is refused (no DICE group has a type without one; its collision stays in the `.res`);
  a kind the group does not hold cannot be added (its collision is not in the group's physics): placed loose instead.

### The editor

`bc2_editor::memberedit::lower` turns the views' operations into member edits: Move / Rotate (gizmo, typed numbers,
align, distribute, snap), Delete, Duplicate, Deep duplicate, Paste, and the asset browser's new **Place into** list.
An operation naming no member is written exactly as before. `Doc::refusal` applies an operation to copies of the files
first, so a refused one (the last of a kind) never enters the log; `Doc::created_by_last` selects the copies. History
names members (`Delete group member <G>#12 (barriersandbag_1) (2 files)`). Properties says which group, kind and place
a member is. GR4's ask: `FileEdit::GroupField`, a typed field set on any instance of a group file (typed by the group
file's own data; the member arrays refused), so the DBX inspector can edit group files through the project.

Proofs: `tests/sg1.rs::members_are_edited_like_objects` (synthetic: mixed object and member move, deletes, the
refusals, duplicates of a base and an added member, History jumps to 2, 4, 0 and the end equal to a fresh replay -
the files, the orders, every shown transform - save as version 7, reopen); `tests/sg1.rs::
wake_members_through_the_interface` (game tier: Wake opened headless, a `barriersandbag_1` member of the static
group's 1,345 selected, dragged along the gizmo's X arm on a 1 m grid: one `files` operation of one member transform,
5 m; Delete; Ctrl+D on another of its kind gives `#+1`, selected; two undos).

Screenshots (`BC2_EDITOR_SHOTS`, the session's scratch folder `sg1\shots`): `sg1_wake_member_moved_dark.png`,
`sg1_wake_member_moved_light.png` - the moved member selected with its gizmo, Properties and History.

### The export

`bc2_levelwrite::groupexport::stage` (only for a log with member edits):

- a moved member keeps its client leaf, reseated (`CullTree::reseat`: taken from under its parent, inserted where
  the spheres hold it; its sphere carried by its old and new transforms); a deleted member's leaf is freed; an added
  member takes a freed leaf, else a new leaf L + 1; new client leaves move the exclusive tree's `EnumOffset` and every
  placed slot up by as many, after XF1's reslot (`culltree::shift_slots`);
- the added members' leaves written to `Enums` and their `SpawnDatas`;
- each edited group's `.res` laid out again (`GroupPhysics::relaid`): part words and material bytes; the packfile,
  boxes, table and material list kept. With its own layout it is the base's bytes on all 63 groups read.

`mapexport check` gains a line per model group holding the staged files to LEVEL-MG-5 - 7 and HAVOK-RUN-3.

`tests/groups_sg1.rs::an_export_deletes_moves_and_adds_members` (release-like dev profile):

| Level | Static group | Edits | Leaves | `.res` parts | EnumOffset | Build |
|---|---:|---|---|---:|---|---:|
| Wake Island (WW2) | 1,345 -> 1,346 | 1 deleted, 1 moved 6 m, 2 added | freed 1, reused 1, new 3000 | 2,125 -> 2,126 | 2999 -> 3000, 1,034 slots moved | 8.4 - 12.5 s |
| Atacama Desert (MP_005) | 1,773 -> 1,774 | the same | freed 1, reused 1, new 2297 | 5,940 -> 5,941 | 2296 -> 2297, 2,071 slots moved | 14.3 - 15.9 s |

Every check line passes on both (the group lines, the cull tree, shapes and slots); the type's run grew by one with
the moved member first, the added ones at the run's end; the freed leaf went to the first add; the `.res` member
types, table, tail and packfile byte for byte the base's; the other six group files of each level byte for byte as
an export of the same project without the member edits.

**The canaries.** The six E3d canary projects built with main's `mapexport` (8e2a657) and this branch's into the same
stage folder: **80 of 80 staged files byte for byte** (`stage.json` and `report.txt` with their timing lines left
out), `check` passing on both.

**Install round trip.** The Wake and Atacama stages through OPFOR's `mapedit_install.py --apply` (this branch's
release `mapexport` as `BFBC2_MAPEXPORT`, `--game` a scratch copy of the six shared files, `--check-game` the real
game copy): every check line passed (the four group lines each), both level bundles installed byte for byte, 24
`package.mft` and 16 `bundleManifest` lines; `--revert` of both: the six shared files back with their sha256, the
levels' folders gone. `game\`: 1,582 files, the same sizes and write times before and after.

### Left, with reasons

- **Not proven until a playtest** (in-game tests paused): that the game loads the relaid groups (the cumulative
  numbers and the `.res` are what its reader indexes by; no crash at 0xAB4E58), that a moved member's collision
  follows its transform (LEVEL-MG-8 is inferred from the files), that a reseated or new client leaf culls the member
  where it stands, and that placed objects still cull after their slots moved up.
- **A kind the group does not hold** cannot be added to it: its collision would need a new rigid body and MOPP shape
  in the packfile (no MOPP builder yet); such objects are placed loose.
- **The last member of a kind** is refused: whether the game takes `InstanceCount` 0 is unknown.
- **Member properties** beyond the transform (the instance's `MeshShaderSet`) are not edited; `GroupField` covers the
  group file's own fields only. GR4's inspector can now route group-file edits through the project (not wired here).
- **Scale**: not offered for members (DICE never scales one; one collision shape per kind).
- A member moved far keeps its old ancestors' spheres (they stay valid, only looser); a rebuilt client tree would
  be tighter.

### Proof

- `cargo fmt`, clippy `--all-targets -D warnings` (bc2-levelwrite, bc2-editor) clean; cargo tests: levelwrite lib 45,
  `levelwrite` 13 run (11 ignored, the game tier); editor lib 71, `editor` 53 run (24 ignored). Game tier run here:
  `groups_sg1::` 2 of 2, `sg1::wake_members_through_the_interface`, the canaries above, `export_xf1::` 2 of 2, editor `xf1::undo_on_a_mode_project` and `op1::stroke_budgets_on_10000_strokes`.
- Release builds: `python rust\build_native.py` (bc2_native.pyd, bfbc2.exe) and `--editor` (4 min 45 s).
- `spec\trace.py --check`: 707 rules (eight new).
- `python run_tests.py quick -j 2`: 132 of 133 in 860 s, O2 passing (main's AV1 merged); the one failure the environment-variable check (the stage variable was named by the round's code), renamed `BC2_MEMBER_EDIT_STAGE` and that entry passed again.
- ruff and mypy on `core\bfbc2core\envvars.py` (`BC2_MEMBER_EDIT_STAGE` registered).
## OR1: OPFOR in Rust, stage 1 - the WW2 install's game-copy side (2026-11-21, branch `or1`, with Project OPFOR's `or1`)

**What moved.** Project OPFOR's WW2 install (`wake_install.py --plan / --apply / --apply-staged / --revert`,
`ww2_level_install.py`; audit AU3 OP-1, OP-6) is two halves: the build (each level's closure, the ported assets, the
shader databases, the textures, the terrain, `write_stage` and the packs, ww2_common, the shared bundles' patches:
`wake_install.build` and `build_shared`, about 3,400 lines) and the install (the guard, the copies, the ledger
writes, the registration, the verification, the opt-out, the staged record). The install is the new crate
`rust/bc2-install`, run by `bfbc2 install`; the build stays Python until stage 2 (the level build) and stage 3 (the
shader glue).

| Project OPFOR (Python, removed from `wake_install.py` / `ww2_level_install.py`) | `bc2-install` |
|---|---|
| `mft_parse`, `mft_text`, `mft_with_level`, `bm_with_level`, `register` | `text`, `run::Run::register` |
| `ini_value`, `ini_with_level`, `ini_with_key`, `ini_without_key`, `maps_with_levels`, `ini_text_new`, `set_ini`, `text_writes`, `all_text_writes` | `text`, `run` |
| `ww2_marks`, `guard_backup`, `check_backup`, `ini_backup_refresh`, `refresh_ini` | `marks::Guard`, `run` (OPFOR's `ww2_marks` is a thin caller: `bfbc2 install marks`) |
| `install_patch`, `stings_state`, `mark_stings`, `mark_tips`, `write_record`, `verify` | `run::Run::install` |
| `install_all`'s guard and write half, `revert_all`, `revert` (less the menu mode) | `run::Run::install`, `run::Run::revert` |
| `save_staged`, `load_staged`, `fingerprint`, `registration_lines`, `write_registration_text` | `staged`, `run` |
| `opforcore.stages.Stages.install` (keep_others), `restate`, `check_install` | `bc2_formats::gameledger::Ledger::install_keep`, `restate`, `file_layers`, `slot_holders` (`gameledger_keep.rs`); `run::Run::check_install` |
| `rebuild_all.locked` (now `ww2\rebuildlock.py`, taken by the install too) | `lock` (the same file and fields, plus the driver's token) |

OPFOR's Python keeps its entry points as thin callers: `ww2_level_install.install_all` builds every level and the
shared patches as before, writes the install plan (`bfbc2-install-plan/1`, `work\ww2\_rebuild\install_plan.json`:
the staging and OPFOR's decisions as data - each level's bundles, report and gates, its registration blocks, the dev
ini's and maps.ini's values, the DLL's markers and the names they need, what marks WW2 content for the guard, what
the verification checks, the staged record's walk) and runs `bfbc2 install plan` or `apply`; `--apply-staged` is
`bfbc2 install apply --staged work\ww2\staged.json` (the record now holds the plan); `--revert` is
`bfbc2 install revert`, then `menu_install.py --revert` (stage 5's). `wake_install.py` 4,520 -> 4,085 lines,
`ww2_level_install.py` 455 -> 393 with the plan writer; the two test modules of the moved rules
(`test_ww2_ini_backup.py`, `test_ww2_apply_staged.py`) and the install half of `test_ww2_install.py` retired, their
cases in `bc2-install`'s tests and OPFOR's new `test_ww2_installer.py`.

**The rebuild lock** (OR0 left it open). `wake_install.py` (every mode that installs or plans) and
`ww2_level_install.py <level>` (not `--atlas-only`, a step of `rebuild_all`, which holds the lock) take
`work\ww2\_rebuild\rebuild.lock` for the whole run, as `rebuild_all` does, with a token they hand to
`bfbc2 install --lock-token`; run alone, `bfbc2 install` takes the lock itself. A second run is refused in words
(exit code 3); a lock whose process is gone is taken over with a note.

**Found and fixed on the way.** The Python read the shared text files (the guard, the list of files the registration
writes) before its first ledger operation rolled an interrupted commit back: after a kill in the registration's commit
(package.mft written, bundleManifest not), the next run's guard saw Wake's blocks in package.mft that no record held
and refused ("already holds the WW2 install's own content"). The installer rolls an interrupted commit back before it
reads any file.

### Proof

- **The Python installer against the Rust one, on scratch copies** (never `game\`): the WW2 install's 7 shared files,
  their 22 old markers and the 5 WW2 level folders of the author's game copy, copied with their write times and
  migrated into the ledger (`opforcore\migrate.py --apply`: 8 files, owners `import`, `loadset`, `loading`,
  `minimap-edges`, `manifestfix`, `wake`, none refused); one copy for each installer, each running the opt-out, an
  apply of the real staging (`work\ww2\install*`, the last plan's: 4 levels, ww2_common, the 3 shared patches), then
  the opt-out again. OPFOR main's Python (1b9ec17) against OR1's plan through `bfbc2 install`:

  | Step | Files | Identical | Ledger records | Printed lines |
  |---|---|---|---|---|
  | revert (the opt-out) | 37 / 37 | 37 | 5 of 5 identical (less their ids and times) | 13 / 13 identical |
  | apply | 74 / 74 | 72 + the DLL's 2 markers (each `<size> <FILETIME>` of its own installed bundle, checked) | 6 of 6 identical | 820 / 820 identical |
  | revert | 37 / 37 | 37 | 5 of 5 identical | 12 / 12 identical |

  Each copy after its second revert is byte for byte (every file's sha256, every record) the copy after its first:
  the apply and the opt-out give the files back. The real game copy: 1,582 files, every path, size and write time
  the same before and after.
- **A kill at every write step** (`BC2_INSTALL_CRASH_AT=<n>`, OPFOR's `test_a_kill_at_every_write_step_recovers` on a
  fake game copy): an apply has 38 write steps (5 level bundles, the record, ww2_common and its record, the 2
  markers, then the ledger's journal, blobs, files and records of 6 commits), a revert 15; killed before each one,
  the next run finishes with the files and records of a run never interrupted, and no lock left.
- **The install oracle** (`compare_oracle.py --install`, 21,197 staging digests; the coordinator's plan from OPFOR's
  main with both merges, 230 s): 21,192 identical at first, 5 differing, decoded: `registration_all.txt` and the 4
  levels' `report.txt` were written with LF, where Python's text mode writes CRLF (fixed: `text_mode`; with CRLF
  `registration_all.txt` is the oracle's byte for byte); and each `report.txt`'s mesh-flags line lists the switched
  off flags in the closure's set order - Python's per-process string hash order, so two Python plans differed there
  too (noted at OPFOR c3503fc): every one of the 4 reports matched the oracle under some order of that one list
  (searched: 5!, 9!, 9!, 10! orders). OPFOR now sorts that list and records the 4 reports' new digests (the only
  change of the oracle; a second plan must give 21,197 of 21,197).
- Tests: `bc2-install` 9 unit + 3 integration (a fake game: plan, apply, opt-out; a failed gate writes nothing;
  `install_keep`'s order and `restate`), `bc2-formats`, `bc2-cli`; clippy `-D warnings`; OPFOR's quick tier and
  opforcore's tests (`test_suite_ledger_compat` both ways), ruff and mypy clean; `spec/trace.py --check` (rules
  XINST-WW2-1 to -3).

### Timings (this PC, other rounds running)

| | Python (OPFOR main) | Rust (OR1) |
|---|---|---|
| the plan's tail (the guard, the registration's report, the reports, `registration_all.txt`, the staged record: the fingerprint and 1.8 GB of staged bundles' sha256) | 2.4 - 4.4 s (3 runs) | 2.9 - 3.3 s (3 runs, the plan JSON written in 0.03 s) |
| apply (the real staging onto a scratch copy) | 4.5 s | 3.4 s |
| revert | 1.9 - 2.5 s | 2.2 - 2.6 s |

The tail reads and hashes the 1.8 GB the plan staged: the disk sets its time, in either language. The whole plan
(OR0: 4 min 14 s) is the build's: stage 1 does not change it; the coordinator's oracle run gives this round's figure.

### Left for stage 2 (the level build)

`wake_install.build` and what it calls, still Python: the stock index and `our_files` (the inputs), `port_missing`
(BF1943 assets ported at install time), `patch_ours` / `sky_patch` / `kit_voices` / `edit_level` (the level and our
partitions), `root_files`, the closure, the emitter atlas's re-pointing, `mesh_flags`, `ondemand_waves`,
`write_stage` and `pack_level` (the staging folders and their packs), `undergrowth_files`, `terrain_files`,
`model_group_check`, `icon_gate` / `level_names`, `build_ww2_common`, `description`, and the shared patches
(`build_shared`: `patch_bundle`, `level_list_query`, `air_superiority_mode`, `loading_pictures`); then `shader_dbs`,
`autoclone`, `fix_layouts`, `register_parts`, `zonly_programs` and the textures they name, which use the Python
shader `Merger` (stage 3, the shader glue). The map export's install (`mapedit_install.py`, its own copies of the
registration rules) moves onto `bc2-install` with IN1. The Python twin `bfbc2core.gameledger` has no `install_keep`
/ `restate` (the suite's Python installers do not need them).

## NK1: NavKit's apply in Rust, first stage (2026-11-21, branch `nk1`)

Audit AU3 AR-4 named NavKit's build and apply the largest Python left (12.5k lines) with no round owning it. The
build is too big for one round to port and prove byte for byte: its Python glue alone is about 4,400 lines around
the Rust core (`navbuild.stage` / `merge_detailed` / `write_file`, `navwalk.walkable`, `navdetail`, `navislands`,
`navsample.load`, `navedits`, `detailed.choose` and navregress's level inputs), with NumPy float order and, in
`navdetail`, Python set iteration order reaching the output. So NK1 took the clean, self-contained stage: **the
apply's edits**, which every apply runs (on a DICE base they are the whole apply), and leaves a precise plan for the
build (below).

### What moved

| Python (retired) | Rust | where it runs |
|---|---|---|
| `navlinks.apply` (links, route edges added / removed, the closest node pair per sector, costs, first-edge indices) | `bc2_navbuild::graphedit::graph_edits` | `bfbc2.exe navkit apply` (DICE base); `bc2_native.navkit_graph_edits` through `navkit.graph_edits` (the NavKit base's apply, after its build) |
| `navtraversal.write` (`<level>.traversal.json`: each end's route node and island, `Counter.most_common`'s ranking) | `graphedit::traversal` | the same; `navkit.write_traversal` |
| `navkit._apply`'s DICE branch (the side file loaded and checked, the copy, the read-back, the report, `--install`) | `rust/bc2-cli/src/navkit/apply.rs` (`navtune.load`'s checks through `bc2_navbuild::navtune::validate`) | `bfbc2.exe navkit apply` |

`bfbc2.exe navkit apply` answers a DICE base. It hands the line to `navkit.py`, before printing anything, for a
NavKit base (a rebuild with the walkability edits: `navbuild.build`), for `--no-detail`, for a side file it does not
read (not JSON: json's error text is Python's; a value the checks refuse) and for a path-data file the stage refuses
(a NaN or an infinity in a node or a bound, a position past 2**53, a cost past 32 bits). Where the reference raised,
its exception is kept: `ValueError` / `struct.error` from the reader, `IndexError` for an edge whose node is past its
sector's nodes, `SameFileError` for an `--out` naming DICE's file.

### Parity

- **Recorded from the Python before the retirement** (`core/tests/cli_o4_navkit.py`, `apply_cases`; the golden
  re-recorded: its 202 earlier lines unchanged, 50 new): 19 apply command lines, with their merged and stdio
  variants, on a path-data file shaped as a real one (4 sectors, border nodes shared, one sector's edges unsorted,
  islands) with 15 operations of every kind (links, one-way and two-way route edges added, one from int positions,
  edges removed inside a sector and between border nodes in every sector holding both, traversals of every kind, one
  with no node near, notes, a link no sector can hold) and the error paths (no side file, not JSON, another format, a
  newer version, a bad operation, region edits on a DICE base, another base kind, no DICE data, DICE's file not path
  data or cut short, a changed base, `--out` / `--navtune`, `--install`, names outside ASCII, stale annotations
  removed). **252 of 252 as recorded** through `bfbc2.exe` (Rust answers 150 of the group's 160 navkit lines; the 10
  handed: build, `--no-detail`, not JSON, a NavKit base), and `--live` with the new Python: the same 252 both ways.
- **Every level, against the old Python** (`rust/parity_nk1_apply.py`: the main checkout's
  `navkit.py` with `navlinks.py` / `navtraversal.py` against this branch's `bfbc2.exe navkit apply`, a stand-in OPFOR
  folder per run, 36 generated operations per level, 26 of them graph edits): the 18 DICE path-data files of OPFOR's
  game copy and the 15 NavKit regression builds (`work\navkit\regress\<level>.hpd`, as the stand-in's DICE file):
  **33 of 33 identical**: the edited `.hpd`, `<level>.traversal.json`, the report and stdout (the timing line's
  seconds aside); 23-25 of 26 graph edits applied per level, every file changed.
- **The NavKit base** (the Python build, then the Rust edits): `test_apply.py` (sp_004_b, two builds with
  walkability edits), `test_links.py`, `test_traversal.py` (sp_004_b), `test_navkit_offline.py`,
  `test_tool_edges.py`: all pass. `test_traversal.py` failed on main before this round (its install section predated
  DF2's install ledger: "not a path inside the game copy"); its stand-in game copy now holds the ledger.
- **DICE parity cannot move**: the generator (`navbuild.build`) is untouched, and the edit stage is byte-identical to
  the Python it replaces, so no content argument (navparity / hpdcheck) is needed for this stage.

### Time and memory (whole `navkit apply` processes, a job object's peak; other agents' builds beside it)

| | old Python | Rust | |
|---|---:|---:|---:|
| 18 DICE files (0.17-24.7 MB), all | 12.4 s | **1.5 s** | 8.2x |
| 15 NavKit builds (0.4-41 MB), all | 13.0 s | **1.8 s** | 7.1x |
| sp_005_b NavKit build (41 MB) | 1.19 s, 870 MB | **0.20 s, 124 MB** | |
| sp_009 DICE file (0.17 MB) | 0.36 s, 511 MB | **0.04 s, 2 MB** | |
| the largest peak | 920 MB | **124 MB** | |

(The Python's floor is about 500 MB: the interpreter with NumPy and the core module loaded.)

### Retired

`bfbc2-auto-navkit/tools/navlinks.py` (120 lines) and `navtraversal.py` (105): Python twins inside a living tool
(RETIRED.md rule 4: recorded here and in CHANGELOG, not as a RETIRED.md row); the last commit with them is `4a515a4`.
No command line went (`navkit.py apply` stays and answers the NavKit base) and nothing outside NavKit imported them
(Project OPFOR has no caller of navkit build / apply, navlinks or navtraversal), so `VERSION` stays (RELEASING.md).

### What is left: the build (NK2-NK5)

The build's Python, in an order that keeps each stage self-contained and provable byte for byte against
`tests/navbuild_golden.json` (sp_009, sp_004_b, sp_008_b, ons_mp_004, the build clock fixed) and navregress:

1. **NK2, graphs to file.** `navislands.prune` / `note_nodes`, `prune_order_nodes`, `finalize_graph` (nodes by
   unsigned id, the border owner bits, the sorted-list indices `b` / `c`), `write_file`, and the per-sector mesh
   assembly with `sector_links` and both resolvers (`kymesh.write_mesh` and `hpdtool.write` are Rust already). Input:
   the stage's graphs; one binding; the build clock passed in (`time.localtime` today).
2. **NK3, `navdetail` (D lines, F cells).** Its sets' iteration order reaches the output (E4e): port it with
   CPython's tuple hash and set probing reproduced, or prove on every golden level that an order-free rule gives the
   same bytes before switching. Proof: `test_detail_features.py` and the goldens.
3. **NK4, walkability end to end.** `navsample.load`, `navwalk.coarse`, `walkable`'s NumPy glue (seeds, anchors,
   the ladders' re-floods, objective floors), `navedits.edits_for`, `water_tops` / `deep_water`, `outside_bounds`:
   the `keep, gx, gz, stair` arrays and the stats lines from the files. Proof: the arrays bit for bit on the golden
   levels, and `test_water_rule`, `test_navedits`, `test_reference_clip`, `test_headroom`, `test_lips`,
   `test_roof_prune`, `test_objective_floor`.
4. **NK5, the stage and the driver.** `stage` (sector discovery, specs, border-node runs, carried seam nodes, the
   one-way drop, the detail loop, graph jobs and ids), `merge_detailed`, `build`'s log lines; `navkit.build_inputs`
   (`detailed_sectors` with its cache key, `stale_detailed_samples`, navregress's `level_water` / `reference_bounds`
   / `order_points_of` / `area_orders_of`). Then `bfbc2.exe navkit build`, `all` (after the sampling hand-off) and
   `apply` on a NavKit base answer in Rust, the editor's AI workspace can call `bc2_navbuild` in process (AR-4's
   three layers), and `navbuild.py` retires once its other callers have moved: `navregress` (the regression builds),
   `navvehicle.stage_on_grid` (vehicle path data runs `stage`), `navwater_mesh`, `navsample_offline`, `hpdcheck`,
   `bfbc2-navigation-studio/overlay.py`, `rust/bench_navkit.py`. Retiring `navkit.py build` / `apply` removes
   command-line paths: a VERSION bump then (RELEASING.md).

## V1: the export's compatibility and validation layer (2026-11-21, branch `v1`)

Round V1 (ROADMAP "V1, L1, A1"; audit AU1 A9, G-N5 to G-N7, G-N19, G-N39 to G-N41, R22, R33, R35, RISK-3, RISK-4,
RISK-9): every way an exported level can fail in the game, or be refused by the export, as one rule registry, checked
live in the editor, by the build and by the stage's check; and round L1's study of DICE's stock maps turned into
placement warnings.

**The registry** (`bc2_levelwrite::rules`, `mapexport rules`, `spec/transforms/validation.md`): 108 rules - 59 of
the export's own (`XVAL-<section>-<n>`) and L1's 49 placement rules (`L1-CQ-002` ...; L1-HT-001 left out: it needs
the terrain's height field). By severity: **51 errors, 55 warnings, 2 notes** (the XVAL rules 51 / 6 / 2; every
placement rule a warning, its stock-band findings notes). By where they are checked (a rule may be in several):
live in the editor 19 (and the 49 placement rules), by `mapexport build` 18, by `mapexport check` 44, by the DBX inspector 5.

Gathered under codes: every `mapexport check` line (each line now prints its code; `CheckReport.codes`), the
build's refusals (their words start with the code), EX1's name rules (`names::Problem.code`, XVAL-NAME-1 to 7; the
stage's XVAL-NAME-8 / 9), XF1's cull-shape and cull-slot lines (XVAL-CULL-2 / 3) and shader line (XVAL-SHDR-1),
QW2's streamed-wave check (XVAL-SND-1), OP1's typed Problems (`ProblemCode::rule`: XVAL-EDIT-1 to 3), GR4's DBX
inspector Level > Check (`rules::dbx_check_code`: XVAL-OBJ-1 to 3, XVAL-REF-1 / 3 / 4, XVAL-CULL-5 / 6).

New checks, from the crash-risk audits and the memory notes:

| rule | what | where | from |
|---|---|---|---|
| XVAL-SND-2 | a deleted marker in a staged bundle hiding a sound (a null wave: 0xFF2AC2, SND-NULL-1) | check | AU1 RISK-4 |
| XVAL-TERR-1 | the staged terrain bundle is the base's own, entry for entry (patch shaders with their own mask atlas: TERR-VT-5, black tiled cells) | check | terrain patch donor rules, AU1 RISK-9 |
| XVAL-CONS-1 | no Xbox 360 texture (big-endian header) or Havok packfile (layout rules 04 00 00 01) in a stage | check | console resource formats |
| XVAL-SHDR-2 | the 40 km/h road sign never drawn with another level's shader entries (0x98BD6E, OPFOR's Isla Inocentes test) | live, check | console resource formats (the road-sign crash) |
| XVAL-GRP-1 | an edited static group keeps DICE's layout (SG1's per-group lines: members, leaves, cumulative parts and network ids, physics) and its kinds: none left without a member, none new; SG1's refusal of the last member of a kind is a GRP-1 refusal (0xAB4E58, LEVEL-MG-4) | build, check | AU1 RISK-3, SG1 |
| XVAL-REG-4 | a note on every stage: install on the server and every client (CSUM-CHK-4) | check | new-level registration |
| XVAL-CULL-4 | an object moved or placed beyond +-2048 m refused in words (before: `culltree::encode` failed mid-build) | live, build | AU1 G-N39 / G-N40 |
| XVAL-MODE-4 | the donor's partition changed since the mode was chosen (before: a report note only) | live, build | AU1 G-N7 |
| XVAL-EDIT-4 | the partition does not parse (before: silent) | live | AU1 G-N19 |
| XVAL-PLACE-1 | two vehicle spawners of one team within 1 m (pairs the base already has are DICE's) | live | AU1 A9 |
| XVAL-OBJ-1 to 3 | GR4's Level > Check live: an empty object-list entry, a world object without physics, an object not listed | live | GR4 |

**Placement** (`bc2_levelwrite::placement`): L1's measurements ported from `l1_measure.py` (the same metric names
and definitions: capture zones, spawn points, combat-area unions on L1's raster, margins to the union's boundary
sampled every 2 m, the Rush stages as `modelayer::rush_play` plays them). On the 64 stock levels of `maps.ini`
(Conquest, Rush, Squad Rush, Squad Deathmatch; BFBC2, BFBC1, Vietnam, WW2), against `l1_measure.py`'s
`l1_measurements.json` (8,771 records, 33.8 s): **3,539 of 3,539 values the rules read equal to 0.1** (the 67
`cq.flag.prominence` values are L1-HT-001's, not ported). Each finding quotes the stock range, the levels and the
rule's ID: "Flag A is 58 m from the nearest other flag; DICE keeps flags 58-450 m apart (median 121 m).
[L1-CQ-002: stock 58 to 450.4, 16 levels]".

| measure | value |
|---|---|
| the 64 stock levels: errors / warnings / notes | 0 / 0 / 29 |
| the notes, per rule | CQ-002 8, CQ-003 3, CQ-004 1, CQ-005 2, RU-003 4, RU-004 1, RU-004s 1, RU-008 2, RU-011 4, RU-012 2, RU-013 1: exactly L1's `info_flagged` lists (`l1_rules_check.md`) |
| DICE's own levels, the rules that report only edits (raw) | XVAL-CULL-4: 37 objects beyond +-2048 m on stock levels (sky domes, backdrops); XVAL-PLACE-1: 1 stacked pair; the DBX check: 0 |
| the 64 levels, each read, validated and measured, release (`examples/v1_rules.rs`) | 23.8 s (160-500 ms a level, most of it reading the partition) |
| L1's measurements of a level, release | 2-26 ms |
| DbxEditGUI's Level > Check of a partition (written, parsed, checked), release | 30-102 ms |

DICE's levels hold objects beyond the cull tree's range and one stacked pair of vehicle spawners, so those two rules
(and the DBX check, whose stock findings would be DICE's) report only what the edits bring: objects the edits moved
or made, pairs the base does not have, findings the base's own check does not have.

**The six export canaries** (round VW2's `examples/vw2_canaries.rs`, built again on this branch: Arica at dawn,
Atacama with Arica's sky, with MP_008CQ's fog, at night, after rain, Wake Island with Iwo Jima's state; each also
built as its baseline): every one passes `mapexport check`, 50-52 lines under 36 rules, 4.1-7.3 s a check (release);
the six new lines (XVAL-REG-4's note, XVAL-SND-2, XVAL-TERR-1, XVAL-CONS-1, XVAL-GRP-1, XVAL-SHDR-2) pass on all.
`mapexport validate` on their six projects: no error and no warning on any; two notes - the
Arica dawn canary's first-guess Conquest layer has two flags 31 m apart in height (L1-HT-002: DICE's within 24 m),
and Wake Island's HQs are 65 m apart in their nearest-flag distance (L1-CQ-005; Wake Island itself is one of L1's
two stock outliers at 94 m). `mapexport rules`: 108 rules, 51 errors, 55 warnings, 2 notes.

**Synthetic broken projects** (`tests/v1_rules.rs`, no game data): small synthetic levels built element by element
(a Conquest layout of two HQs, three flags, spawners with alternative points, a combat area of `Next`-linked
outline points, ticket counters; a Rush layout of three stages of two M-COMs with their counters' Enable / Disable
events, growing attackers' areas and per-stage defenders' areas, a life counter; Squad Rush; a Squad Deathmatch
layout of four squads sharing a grid of 20 spawn points, a vehicle, kill counters). The four clean layouts raise
nothing and are recognised as their mode; **49 of 49 placement rules** each raised by a layout broken one way, at
their severity, with their citation; **every edit-phase rule** but the log's three raised by a broken project (a
paint stroke, Onslaught chosen, a layer without flags with the donor changed, five names of the wrong form, two
names the game already has, a prop 3 km out, the 40 km/h sign, two stacked spawners, a null object-list entry, a
model group with a null `PhysicsData`, an unlisted terrain entity, a dangling local reference). Game tier
(`tests/v1_stage.rs`): the build refuses XVAL-REG-3, XVAL-MODE-1, XVAL-EDIT-5, XVAL-CULL-4 and XVAL-EDIT-2 with the
code first, and `validate::project` gives XVAL-EDIT-1 / 2 / 3; a staged canary (Atacama's Rush as BFBC2 Conquest,
24.9 s) passes its 43 lines (35 rules), and a copy broken once per rule (a sha1, an entry count, a manifest line, a
missing target, the menus' list, a name key, the donor, the sign's merged entries, streamed wave data, a deleted
sound marker, a console texture, a terrain entry) fails exactly those 12 rules, checked in 5.5 s. A test keeps
`validation.md` and the registry the same.

**The editor** (`rust/bc2-editor/tests/v1.rs`, game tier): E3d's canary (Atacama's Rush as BFBC2 Conquest) opened
as a project raises nothing; two edits through the document - flag B moved 20 m from flag A, a prop moved 3 km out -
and the panel lists 5 rows - 1 error (XVAL-CULL-4), 2 warnings (L1-CQ-002 for the two flags now 20 m apart) and 2
notes - errors first, each naming objects the scene has; an undo takes the error away. Live validation after an edit: **65-89 ms**
(the dev profile, optimised; the base's own DBX check and the donor read once). Before `LevelWriter::transform_at`
(V1: a transform by node; `find` walks every instance) the same validation took 3.3-4.3 s, the placement and range
loops looking each object up by guid. Screenshots (Dark, Light): `v1_problems_dark.png`, `v1_problems_light.png`
(`BC2_EDITOR_SHOTS`).

Left for A1 and later: engine limits (instances, spawns, event connections: RE first, AU1 G-N39), spawns over water
or in rocks and a vehicle spawner's room and domain (G-N44 / G-N45: the scene's collision and water are there; no
stock statistic yet), the exported level's memory (R34), Xbox 360 meshes in a stage (their layout is not detected),
the WW2 profile's kit rule on placements (G-N41), L1-HT-001. A1's auto layout can use `placement::measure` as its
objective and `l1_a1_params.json` as its start.

**After merging SG1 and OR1** (main ddaf6d6): SG1's member-edit exports (`tests/groups_sg1.rs`, Wake Island as WW2
Conquest and MP_005 as Conquest, a member deleted, one moved, two added) pass every check line, their 5 XVAL-GRP-1
lines each, and `validate::project` raises no error (Wake: 1 finding below error, MP_005: none; `validate::project`
now reads the base's group files as the export does). Deleting every member of a group's smallest kind is refused as
`XVAL-GRP-1: ...` on both (Wake's heattank, MP_005's invwall_nohealth; `rules::apply_code`).
## NK2: NavKit's build, graphs to file (2026-11-21, branch `nk2`)

NK1's plan, stage 1 of the build: everything a build does once its route graphs exist, and the neighbour links of
the sectors' runtime meshes. A build now hands its graphs to the Rust core once (`navbuild.write_file`, called with
the build's anchors), and the core prunes, lays out and writes the file; the stage's two link resolvers run in the core
from the stage's own areas.

### What moved

| Python (retired) | Rust | where it runs |
|---|---|---|
| `navislands.components`, `shadowing` and `prune`'s loop (the shadow-island rule: small route islands mostly on objects, holding no anchor, beside a large one) | `bc2_navbuild::graphfile::shadow_islands`, `drop_edges` | `navbuild.write_file` (with the anchors: a build); `navislands.prune` is a thin call (`bc2_native.navkit_shadow_islands`) for other callers |
| `navbuild.prune_order_nodes` (order nodes in islands under `ORDER_MIN_ISLAND`; `ORDER_STATS['removed']`) | `graphfile::prune_order_nodes` | `write_file` |
| `navbuild.finalize_graph` (nodes without an edge dropped, sorted by unsigned id, costs from the `float32` positions, first-edge indices, the sorted-list fields `b` / `c`, the border owner bit) | `graphfile::finalize` | `write_file` |
| `write_file`'s assembly (each sector's record, the meshes written, `hpdtool.write`) | `graphfile::write_file` (`kymesh::write`, `hpd::write`; the meshes read from the build's `kymesh.Mesh` values and written in parallel) | `bc2_native.navkit_write_graphs` |
| `sector_links` (links grouped by sector and area across, the portals' offsets in their area objects) with `border_cell`, `same_grid_resolver` and `cross_grid_resolver` | `graphfile::sector_links` with `Border::inner_cell`; the resolvers in `bc2-py`'s `navgraph::sector_links` on the stage's `NavLevel` / `NavAreas` | `NavLevel.sector_links` (a `NavLinkScope` per stage: `navbuild.link_scope` / `link_across`), called by `stage` and `merge_detailed` |

What stays Python until NK5: the stage builds the `kymesh.Area` / `Mesh` values (their tiles, height ranges,
rectangle, bounds) from the traced areas and D / F details, because the stage's result is read as Python values by
navvehicle, navwater_mesh, the golden test's whole-stage digests and `merge_detailed`, which sets the links of meshes
already made. The mesh assembly's only computation, the links, is the core's; the rest is the values' construction,
which moves with the stage.

### Parity

- **Every regression level, against the old Python** (`rust/parity_nk2_file.py`: this checkout builds each level
  through `navregress.build_level` with the build clock fixed; inside the build every `sector_links` call also runs
  the old Python's `sector_links` with the stage's resolver, or `merge_detailed`'s same-grid / cross-grid pair, from
  the main checkout's `navbuild.py`, and `write_file` also runs the old `navislands.prune` and `write_file` on the same
  graphs; then each checkout builds the level once more alone): **15 of 15 levels identical**: every sector's links
  (5,698 calls), the file, the log lines, the island statistics and `ORDER_STATS['removed']`; and the old checkout's
  own build, the new checkout's own build and the compared build have one SHA-1 per level.
- **The golden builds** (`tests/navbuild_golden.json`, the build clock fixed): sp_009, sp_004_b, sp_008_b and
  ons_mp_004 as recorded, in the parity run and in `test_navbuild_native.py --game --levels
  sp_009,sp_004_b,sp_008_b,ons_mp_004`; its quick tier (the E2a / E3c / E3h recorded answers, among them whole stages
  with their meshes' links) unchanged.
- **NK1's apply parity**: 33 of 33 (`rust/parity_nk1_apply.py`, main's `navkit.py` - NK1's, itself 33 of 33
  against the Python before NK1 - against this branch's `bfbc2.exe navkit apply`: the 18 DICE files and the 15
  regression builds; the apply's edit stage is untouched).
- **Python's order**: nothing a set or dict iteration orders reaches the output. The islands dropped are a set of
  ids (which island a node is in does not depend on the union-find's order: the roots differ, the members do not);
  the order nodes dropped, a set; `ORDER_STATS['removed']` follows the graphs' order; `sector_links` sorts its
  groups; every sort is the reference's stable sort on the same keys (unsigned ids; the build's float64 positions,
  finite). No NumPy arithmetic is in the stage but the `float32` rounding of positions for the costs (`as f32`:
  round to nearest, as `np.float32`) and the cross-grid resolver's float64 heights (IEEE, the same). Proven on all
  15 levels as above, which include every golden level.
- **DICE parity cannot move**: the files are byte-identical to the Python's, so no content argument (navparity /
  hpdcheck) is needed.

### Time and memory

The stage alone (its inputs - graphs, meshes, anchors, order and terrain ids - pickled from the build and loaded in
a fresh process; old: `navislands.prune` + `write_file`, new: `write_file`), and the links inside each checkout's
whole build (old: `sector_links` with its Python resolvers; new: `NavLevel.sector_links` with the scopes). Peak:
the process's peak working set above its working set once the inputs were loaded (other agents' builds beside it).

| level | file (MB) | links compared | stage alone, old -> new (s) | stage peak above inputs, old / new (MB) | links in the whole build, old -> new (s) | whole build, old -> new (s) |
|---|---:|---:|---:|---:|---:|---:|
| ons_mp_002 | 7.9 | 146 | 0.33 -> **0.16** | 39 / **38** | 0.03 -> 0.02 | 11 -> 11 |
| ons_mp_004 | 7.3 | 72 | 0.34 -> **0.15** | 35 / **38** | 0.01 -> 0.02 | 6 -> 6 |
| sp_003_b | 27.5 | 645 | 1.47 -> **0.47** | 131 / **92** | 0.11 -> 0.12 | 24 -> 22 |
| sp_004_b | 7.9 | 107 | 0.40 -> **0.15** | 38 / **36** | 0.02 -> 0.02 | 8 -> 8 |
| sp_005 | 37.2 | 615 | 2.16 -> **0.69** | 181 / **121** | 0.98 -> 0.45 | 58 -> 46 |
| sp_005_b | 39.1 | 597 | 1.66 -> **0.93** | 182 / **130** | 0.95 -> 2.11 | 47 -> 41 |
| sp_006 | 33.9 | 841 | 2.03 -> **0.69** | 171 / **113** | 1.36 -> 1.22 | 60 -> 66 |
| sp_007 | 31.8 | 677 | 1.44 -> **0.66** | 151 / **113** | 2.22 -> 1.35 | 57 -> 62 |
| sp_008 | 12.0 | 111 | 0.82 -> **0.36** | 56 / **52** | 0.04 -> 0.04 | 17 -> 15 |
| sp_008_b | 8.1 | 75 | 0.40 -> **0.15** | 37 / **37** | 0.02 -> 0.02 | 9 -> 9 |
| sp_009 | 0.4 | 22 | 0.07 -> **0.05** | 4 / **4** | 0.00 -> 0.00 | 2 -> 2 |
| sp_01 | 17.4 | 320 | 0.73 -> **0.35** | 85 / **66** | 0.16 -> 0.15 | 21 -> 22 |
| sp_02 | 13.0 | 290 | 0.57 -> **0.29** | 64 / **51** | 0.10 -> 0.04 | 18 -> 17 |
| sp_03 | 31.2 | 679 | 1.39 -> **0.87** | 150 / **105** | 1.60 -> 1.09 | 66 -> 46 |
| sp_04 | 20.9 | 501 | 1.74 -> **0.55** | 104 / **72** | 0.87 -> 0.65 | 50 -> 35 |
| all 15 | | 5,698 | 15.6 -> **6.5** (2.4x) | largest 182 / **130** | 8.5 -> 7.3 | 454 -> 406 |

The links are 1-3 % of a build either way, and their time is noisy here (the builds ran beside other agents'
work; the first lookup of a stage's areas builds the core's per-sector area maps once, 0.1-1 s on the large levels,
whichever caller comes first): 8.5 -> 7.3 s over the 15 levels, no claim beyond "not slower". The whole builds
(old and new checkout each alone, one after the other) moved by the stage's time and the machine's load: 454 -> 406
s. A whole build's own peak (7-8.5 GB on sp_005 / sp_005_b / sp_006 / sp_04) is the stages before this one (PR1b's
"NavKit regression's 15 GB"); this stage's part is the table's.

The new stage's time is mostly reading the build's `kymesh.Mesh` values into Rust (0.08 of 0.12 s on sp_004_b; with
the meshes' bytes given, 0.04 s): the meshes are Python values until NK5 moves their assembly.

### Retired

`navislands.components` / `shadowing` and `prune`'s own loop; `navbuild.prune_order_nodes`, `finalize_graph`,
`border_cell`, `same_grid_resolver`, `cross_grid_resolver`, and `write_file`'s and `sector_links`' bodies: Python
twins inside a living tool (RETIRED.md rule 4: recorded here and in CHANGELOG, not as RETIRED.md rows); the last
commit with them is `08b1382`. No command line went and nothing outside NavKit called them (Project OPFOR has no
caller of navbuild or navislands), so `VERSION` stays (RELEASING.md). `test_navislands.py` checks the file
`write_file` writes in place of `finalize_graph`'s tables.

### What is left: NK3-NK5

NK1's plan stands for NK3 (`navdetail`: its sets' order reaches the D lines and F cells) and NK4 (walkability end to
end). NK5 now also takes the meshes' assembly: in `stage`, the `kymesh.Area` per area (height range from the pass-1
`ylims`, portals / borders / outline from the trace, D / F from `navdetail`), the tile items, the rectangle and the
sector bounds; then the stage's graphs and meshes can stay in Rust from the areas to `graphfile::write_file` (no
`kymesh.Mesh` read back: the 0.08 s above), `merge_detailed`'s links run on its two stages' cores directly, and
`navvehicle` / `navwater_mesh` / the golden test read a Rust stage result.
## PR1a: the post-Rust improvements' small fixes that stop drift (2026-11-21, branch `pr1a`)

Audit AU5 section 6 re-scoped PR1 and made PR1a the first round: the small fixes that stop drift. Its seven items are
below, in AU5's order, with what was done, what was proved and what is left. The rule held throughout: outputs stay
byte-identical, or a change is recorded and explained. The three changed outputs are listed under item 5, and each
was nondeterministic before.

### 1. One whole-file writer (AU5 ARC-8)

`bc2_formats::atomic` is now the only implementation:
- `temp_name`: `<target>.tmp-<pid>-<n>`, unique per process and per write;
- `write` / `write_with`: fill, flush, `sync_all`, `replace` with 11 tries; the temporary file is removed on any
  error; `WriteError` says whether the temporary file or the move failed, so each caller keeps its error text;
- `write_rebuildable`: the same without `sync_all`;
- `locked`: the lock between processes;
- `temp_writer` / `remove_dead_temps`: a killed writer's temporary files.

Callers moved onto it:

| writer | before | now |
|---|---|---|
| FbrbTool's entries and manifest (`fbrb/tool.rs`) | own counter, no flush to disk | `write_rebuildable` (unpacked files are rebuilt from the bundle; `sync_all` measured at about 1 ms a file, 1,500 files 1.5 s -> 2.9 s, and a bundle holds thousands) |
| FbrbTool's pack (`fbrb/tool.rs`, `fbrb/pack.rs`) | `.tmp-<pid>`; `pack_path` not flushed | `temp_name`; both flushed |
| DbxEdit's `AtomicFile` (`dbxedit/cli.rs`) | own counter | `write` (the temporary file's name in a write error, the target's in a move error, as before) |
| the install ledger (`gameledger.rs`, and its Python twin) | the fixed `<path>.opfor-ledger-tmp`: two writers of one path shared it | `write`; a recovery removes the old fixed name and dead writers' temporary files |
| meshview's cache (`bc2-convert`) | own rename loop retrying every error; own `locked` | `write`, `locked` |
| bfbc2.exe's gameindex | pid-only name; its own copies of the cache's version, path and signature check | `write`, `locked`, `bc2_convert::meshview::cache`'s |
| `pyos` (`atomic_write`, `locked`, `replace`) | pid-only name; own lock and retry loop | `pyos::files::write_atomic` over `atomic` |
| harness runs, the log archive, stock-MP's report, texqa's JSON | pid-only names, no flush to disk (the harness, stockmp, texqa) | `write` |
| navkit apply's traversal file | the fixed `<file>.tmp` | `write` |
| editor: settings, catalogue record, nav side files; levelwrite: project writes and the cull cache | four separate wrappers: fixed or pid-only names, two without a flush to disk | `write` |
| bc2-install: write, copy, the staged record | `.opfor-tmp-<pid>` / `.tmp-<pid>` | `write` / `temp_name` |

`bfbc2core.atomicio._tmp` gained the same per-write counter. The golden normalisers already accept
`.tmp-<pid>(-<n>)`.

The shader-database helpers XF1 left in `bc2-cli` (`latin`, `to_latin`, `hex`, `entry_programs`, `program_refs`,
`coverage`) are gone: `bc2_formats::shaderdb::merge`'s are used. New tests in `atomic.rs`:
- names per write;
- whole-or-nothing, and no temporary file left;
- 4 threads x 10 writes to one path;
- dead writers' files;
- `locked` making its folder.

### 2. Stable test ids and the round-code rule (PR0 item 1, AU5 NAM-1)

`run_tests.tests()` entries are `(id, name, tier, argv)`:
- 240 ids, lower-case words joined by `-` with an optional `:variant`, named by what each entry tests. The 20 ids
  of round-named scripts were written by hand, for example `cli-menu-crash-navkit-convert` for O2.
- `NEEDS`, `CI_STEPS`, `NOT_GAME_FREE`, the kept durations (`test_times.json` on the PC, `tests/test_durations.json`
  in the repo) and the round-code allowlist are keyed by id.
- A durations file still keyed by display names is read (`by_id`) and rewritten by id.
- `--list` shows the ids, and `--only` matches them too.

The seed file: 191 entries rekeyed, and 1 stale name dropped (the old single cargo entry's).

`tests/test_repo.py`:
- the 60 grandfathered round-named entries are now listed by id, and every id must be well formed, unique and free
  of round codes;
- new: Rust test files, `tests/data` files and `mod` names under `tests/` and `src/` must not carry a round code
  (21 pre-TS1 test files are grandfathered by path);
- new: JSON keys in a fixed order (item 5);
- new: no round code in the usage or the crates' descriptions (item 6).

Renamed (git mv; mod lines, spec citations, EDITOR_ARCHITECTURE, RETIRED and the dbx-tools README updated):

| before | after |
|---|---|
| `bc2-editor/tests/gr3.rs` | `asset_preview_parity.rs` |
| `bc2-editor/tests/gr4.rs` | `dbx_inspector_window.rs` |
| `bc2-editor/tests/op1.rs` | `editor_core.rs` |
| `bc2-editor/tests/sg1.rs` | `group_members.rs` |
| `bc2-editor/tests/data/dbxeditgui_gr4.json.gz` | `dbxeditgui_answers.json.gz` |
| `bc2-levelwrite/tests/groups_sg1.rs` | `model_group_members.rs` |
| `bc2-levelwrite/tests/op1_journal.rs` | `journal_kill.rs` |
| `bc2-render/tests/r5e.rs` | `deep_sea_materials.rs` |
| `bc2-py/src/f2a.rs` | `converters.rs` |
| `bc2-py/src/formats_f3.rs` | `aim_pathmesh_audio.rs` |
| `bc2-py/src/formats_f4.rs` | `culltree_audio_havok.rs` |
| `bc2-navbuild/src/e3c_tests.rs` | `graph_route_tests.rs` |
| `bc2-navbuild/src/e3h_tests.rs` | `sector_flood_tests.rs` |

Every test is still in its tier. The renamed modules' tests are listed by `cargo test --workspace -- --list`:

| module | tests |
|---|---|
| asset_preview_parity | 3 |
| dbx_inspector_window | 2 |
| editor_core | 6 |
| group_members | 2 |
| model_group_members | 2 |
| journal_kill | 2 |
| deep_sea_materials | 2 |
| graph_route_tests | 9 |
| sector_flood_tests | 4 |

Workspace total: 754 tests listed (the 6 new atomic tests included).

### 3. `bc2-pyrt` out of the editor (AU5 ARC-1)

- **The editor:** its one call, `preview.rs`'s `py::abspath`, is now `std::path::absolute`. Both use
  GetFullPathNameW, as Python's `os.path.abspath` does. `bc2-editor` no longer depends on `bc2-pyrt`.
- **`bc2-convert`:** its meshview cache (the pickle shared with `meshview.py`) is behind a new feature,
  `pickle-cache`. `bfbc2.exe` enables it. The editor's asset preview enables it too, because it reads the same cache.
  `meshview::Tables` moved out of the cache module.
- **`bc2-navbuild`:** NK1 added `bc2-pyrt` for `graphedit`, which writes the traversal annotations as `json.dump`
  writes them and uses Python's `round()`. That is our own sidecar file, not game data. Only `bfbc2.exe navkit apply`
  and bc2-py's `navapply` (navkit.py's in-process call) use it. It is now behind a new feature, `graphedit`, which
  `bc2-cli` and `bc2-py` enable. The rest of the crate, which the editor links, has no `bc2-pyrt`.
- **`test_rust.py`:** its clippy subset (`-p`) runs with `--all-features`, so a crate is still linted alike alone or
  in the workspace.

`cargo tree -p bc2-editor -i bc2-pyrt` now shows `bc2-pyrt` only through `bc2-convert`'s `pickle-cache` and
`bc2-levelwrite` (the cull cache's pickle and culltool's Python error model). See the census below.

### 4. AV1's test switch out of `PeImage::open` (AU5 ARC-10)

`bc2_menu::common::PeImage::open` now only reads and parses. `PeImage::from_bytes` is new, and `restore_stand_in` is
public. `bfbc2.exe`'s menu tools open the exe through `menu::open_exe`, which:
- reads `BFBC2_TEST_STAND_IN_EXE` once per process, at the command line's edge with `bfbc2.exe`'s other test
  switches;
- puts the stand-in's signatures back before `from_bytes`.

O2 still writes no program to disk and needs no AV exclusion; see Proof.

### 5. Determinism as a rule (PR0 item 2, queued item 4)

**Rust.** `clippy::iter_over_hash_type` is a workspace lint. It found 42 loops over a `HashMap` / `HashSet`:

| | sites | what |
|---|---|---|
| reached an output, fixed | 3 | see below |
| reviewed, order cannot matter | 37 | each carries `#[allow(clippy::iter_over_hash_type, reason = "...")]`: sums, inserts into maps or sorted sets, lists sorted next, per-viewport UI updates, assertions |
| not separate sites | 2 | `package.rs`'s other two loops over the same map, fixed with it |

The three outputs fixed:
- **The editor's prepared-copy record** (`prepared.json`). Its `files` list was in hash order, so it changed from run
  to run. `read_entries` now returns a `BTreeMap` keyed by `(layer, entry)`, so the list comes out in that order.
- **`bc2_native`'s scene collision dict**, now in blueprint-name order.
- **`vw2_canaries`' path mapping back**: longest names first, then by name. A name contained in another could
  otherwise be replaced first.

**Python.** AU5 counted 73 unsorted JSON writes with grep; the AST count is 65 in the suite's non-test Python.
- 10 were in scripts whose outputs nothing compares (the bench scripts, the NK1 parity script, the placement study
  scripts). They now pass `sort_keys=True`.
- The other 55 are the tools' outputs. Their key order is the order the code builds each dict in, the same under
  any hash seed (checked below). Their Rust ports and OPFOR compare those bytes, so sorting them would change outputs.
  `tests/test_repo.py`'s `JSON_ORDER_KEPT` counts them per file, and a file may not gain one. They are sorted, or
  given a spec rule, when their Python twin retires (PY1).
- New writes use `sort_keys=True` or `bfbc2core.atomicio.dumps_stable`.

**Two-seed check.** Each command ran under `PYTHONHASHSEED=0` and `=4242`. Exit codes and stdout bytes were compared; the test scripts check their answers against their recordings under both seeds.
- **Same stdout bytes under both seeds:** `spec/trace.py --json` and `--coverage`, `rust\native_census.py --markdown`, the NavKit driver on a stand-in OPFOR folder, the harness on built data, the install ledger and NavKit's tool edges.
- **Passed under both seeds, stdout differing:** stock-MP on synthetic levels and texqa. The only difference is the names of their temporary folders in the printed lines (`tmpXXXX`).
- **Rust:** `bfbc2.exe` and the test binaries draw a new hash seed in every process, so every O-series run is a two-seed run. O1-O5 passed alone and on one pipe.
- **The rebuild:** the WW2 rebuild below matched the oracle with Python and Rust both on fresh seeds.

### 6. Round codes out of user-facing text, and the "part hashes" misnomer (AU5 NAM-2, NAM-4)

- **`bfbc2.exe`'s usage** now names each tool group by what it does. It used to say "round DF2", "round OR1", "round
  O3/O4/O5". The BF1943 importer's catalogue and parity tools got their own line.
- **The crates' descriptions** lost their round codes: bc2-cli, bc2-editor, bc2-install, bc2-levelwrite, bc2-menu,
  bc2-pyrt, bc2-render.
- **Other user-facing text** lost them too: the editor's export hints and Help text, and levelwrite's "not written
  yet" message.
- `test_repo.py` checks the usage and the descriptions.
- The usage is not in any golden; the O-series lines still match (see Proof).

The "part hashes" misnomer:
- **HAVOK-WRAP-1's table**, the console converter's **FORMAT.md 1.1** and **`bc2-convert`'s `convert_wrapper` doc**
  now say "part words": in blueprints the part names' hashes (`havok::part_hash`), in model groups each part's
  owning instance's leaf (HAVOK-RUN-3).
- **Two open questions** were stale:
  - the part-hash function is now known; only the material hashes stay open;
  - the model-group material table is decoded by HAVOK-RUN-3, so that question was removed.
- **Code citations:** the spec's citations into files this branch changed were remapped by a line diff against
  `7a18632`, and `rules.json` was re-emitted. `spec/trace.py --check` passes: 710 rules.

### 7. The native-backend registry and the `bc2_native` census (queued item 5, AU5 ARC-12)

**The registry's skeleton** is in `bfbc2core.native`:
- `register_backend(name, needs, switch)`, `backend(name)`, `use_backend(name, on | None)` and `backends()`;
- one switch for every optional piece: `BFBC2_PURE_PYTHON`, the piece's own `BFBC2_<TOOL>_PYTHON` variable, or this
  process's choice.

`hkpack` and `kymesh` moved onto it with the same behaviour.

The other 22 `use_native` copies move when their modules are next touched, or retire with their twins:
- `granny`, `hpdtool`, `navnative`, `harnesscore`, `importcore`;
- `audio`, `texqa`, `texture`;
- `apt`, `stfs`, `xisolib`;
- `culltreelib`;
- `aptcheck`, `aptgraft`, `aptrefslib`, `bc2loc`;
- `meshview`, `skinmesh`, `texwrite`;
- `shaderdb`, `xenos`, `stockcore`.

The whole move was more than "small": 22 modules, each with its own switch variable, tests that poke its cache, and
fallbacks into the Python. So PR1a stopped the drift: there is one place for new pieces, and two modules show the
pattern.

**The census.** `rust\native_census.py` reads the exports from `bc2-py/src/lib.rs` and counts their callers in the
suite's tools, the suite's tests and OPFOR's `src`:
- 237 exports in 38 files;
- 223 called by a suite tool;
- 8 called by OPFOR directly;
- 14 called by no tool: `havok_triangles`, `fbrbtool_gunzip`, `zlib_deflate`, `stockmp_author_refusal`,
  `stockmp_new_guid`, `harness_x86_decode`, `xas_encode`, `AptLines`, `navkit_centre_cells`, `navkit_float_ops`,
  `navkit_nearest_cells`, `navkit_round3`, `xenos_float_repr`, `stfs_refusal`. These are test-only probes, or
  classes returned rather than called by name.

It is a report for now. PR1b-2 makes it a check once the twins retire.

### The `bc2-pyrt` census (for PY1)

The user's principle (2026-11-21): the target is compatibility with BFBC2 and DICE, not with Python. `bc2-pyrt` is a
porting crutch that proves a port byte-identical while its Python twin exists. Every use is classed:
- **1:** matches the game or engine (keep; cite the DICE evidence);
- **2:** arbitrary Python behaviour in our own outputs (a spec-defined deterministic rule once the twin retires);
- **3:** Python behaviour that may differ from what DICE's tools or the engine produced (decide against DICE ground
  truth).

No behaviour was changed for any of them in PR1a.

**Who depends on it:**
- always: `bc2-cli`, `bc2-py`, `bc2-levelwrite`, `bc2-menu`;
- behind a feature: `bc2-convert` (`pickle-cache`: bc2-cli and the editor), `bc2-navbuild` (`graphedit`: bc2-cli
  and bc2-py);
- not at all: `bc2-formats`, `bc2-scene`, `bc2-shader`, `bc2-stockmp`, `bc2-harness`, `bc2-render`, `bc2-install`,
  and the editor directly.

**The library crates:**

| crate | where | item | used for (called by) | Python twin (exists?) | class |
|---|---|---|---|---|---|
| bc2-convert (`pickle-cache`) | `meshview/cache.rs` `walk`, `bundles`, `cache_path` | `py::join` | the stock bundles' paths and the cache's path (`load`; bfbc2.exe gameindex) | `bfbc2-mesh-viewer/meshview.py` (yes) | 2 |
| bc2-convert | `cache.rs` `same_sig`, `read`, `cache_bytes` | `pickle::{loads, Value}`, `pickle_write` | `meshview_index.pkl` read and written byte for byte as `Index._write` (`load`, gameindex, the editor's preview) | meshview.py (yes) | 2 (our cache; a native format once meshview.py's reader goes, ARC-4) |
| bc2-levelwrite | `cullcache.rs` (`norm`, `PickleStore`, the row and listing entries) | `py::{abspath, join, OMap}`, `pickle`, `pickle_write` | culltree's reference-row cache shared with the Python (bc2-cli `cull`) | `bfbc2-culltree/culltreelib.py` (yes; `culltree.py`'s command line retired) | 2 |
| bc2-levelwrite | `culltool.rs` (about 60 functions) | `err::{Fail, PyErr, PyResult, os_error}`, `fmt::{str_repr, float_repr}`, `py::{basename, splitext, abspath, dirname, join, isfile, isdir, strip, OMap}` | `CullTreeError` / OSError texts, reference-level lookup and labels, rows by blueprint (bc2-cli `culltree info/verify/add`) | culltreelib.py (yes) | 2 (the tree's keys use culltool's own `py_round`, see below) |
| bc2-navbuild (`graphedit`) | `graphedit.rs` `num`, `point`, `op_name`, `two_way`, `graph_edits` | `json::Value` | the side file's operations, parsed (bc2-cli `navkit apply`; bc2-py `navapply`) | `navkit.py` (yes); `navlinks.py` retired | 2 |
| bc2-navbuild | `graphedit.rs` `nearest`, `rise`, `traversal` | `fmt::round_to`, `json::dumps(indent=1)` | `<level>.traversal.json`'s numbers and layout (our sidecar for OPFOR's bot layer, not game data) | navkit.py (yes); `navtraversal.py` retired | 2 |
| bc2-menu | `common.rs` re-exports and `find_opfor`, `out_dir`, `bundle_layers`, `resolve`, `find_exe`, `parse_patch`, `glob` | `err::*`, `py::{join, nt_split, abspath, dirname, isdir, isfile, splitext, basename, read_text, splitlines, strip, split, suite_root}` | menucommon's paths, OPFOR's patch text, MenuError / OSError texts (bc2-cli menu tools, caches, budget) | `bfbc2-menu-tools/menucommon.py` (yes) | 2 |
| bc2-menu | `common.rs` `bundle_movies`, `parse_bigf`, `apply_patch` | `py::{OMap, Members}` | BIGF members and movies in the order the file holds them | menucommon.py (yes) | 1 (likely: the source file's member order; an insertion-ordered map, nothing Python-specific) |
| bc2-menu | `graft/mod.rs`, `graft/edit.rs`, `graft/crosshair.rs` (about 100 functions) | `err::{Fail, PyErr, PyResult}`, `read_file`, `py::{basename, isfile, splitext}` | AptError texts, the hand-off to aptgraft.py, loading `.big` / `.apt` / `.const` | `bfbc2-menu-tools/aptgraft.py` (yes) | 2 |

**`bc2-py` (the module `bc2_native`):**

| where | item | used for | twin (exists?) | class |
|---|---|---|---|---|
| `navapply.rs` | `json::{loads, Value}` | parsing the JSON text `navkit.py` passes in | navkit.py (yes) | 2 (goes with the JSON bridge) |
| `xenos.rs` `h_g` (from `xenos_disasm`) | `fmt::float_g` | `%g` of literal constants in the disassembly header | `bfbc2-shader-tools/xenos.py` (yes) | 2 if only read by people (to confirm: `%g` keeps 6 significant digits; if anything reassembles or compiles from it, 3) |

**`bc2-cli` (`bfbc2.exe`).** About 1,700 lines in 26 modules use it, mostly through `crate::pyproc` (re-exporting
`err::*`), `crate::pyfmt` (`fmt`) and `crate::pyjson` (`json`). This is the Python-process edge itself: the exception
model and the hand-off to the Python, argparse, `os`, `json.dumps` of our own reports and settings, the report
numbers' `round_to` / `float_repr` / `float_g`, ntpath spelling, and `str` methods.

Class 2 throughout, except one class 3 (likely): `navkit/ladders.rs:85` `round_to(x, 3)` on the ladder boxes written
to `ladders.json`, which NavKit turns into ladder links in path data.

| module | lines | twins (exist?) |
|---|---|---|
| `pyproc`, `argfmt`, `pyos` | 81 | the process edge |
| `stockmp/` | ~324 | `stockmplib` / `config` / `validate.py` (yes); `stockmp.py`'s command line retired |
| `navkit/` | ~263 | `navkit.py` (yes); `prepare_level.py` retired |
| `shader/` | ~149 | `shaderdb.py`, `renderqa.py` (yes) |
| `nav/` | ~135 | `hpdtool`, `hkpack`, `convert`, `aimanim` (yes) |
| `navtools/` | ~120 | `hpdcheck`, `hpdview`, `detailed`, `navparity`, `navregress`, `sample_all` (yes); `graphstats` retired |
| `harness/` | ~101 | `harness.py` (yes) |
| `texqa/` | ~98 | `texqa.py` (yes) |
| `crash/` | ~75 | `exeref.py`, `crashreaderlib.py` (yes); `crashreader.py` retired |
| `menu/` | ~71 | `aptcheck`, `bc2loc` (yes); `aptrefs` retired |
| `readers/` | ~67 | `apt.py`, `stfs.py` (yes); `xiso.py` retired |
| `graft/` | ~39 | `aptgraft.py` (yes) |
| `audio/` | ~38 | `audio.py` (yes) |
| `caches/`, `logs.rs`, `budget.rs` | ~43 | `bfbc2core/caches.py`, `fbrb.py` (yes) |
| `cull/` | ~24 | `culltreelib.py` (yes) |
| `bf1943/`, `bf1943src.rs`, `import/` | ~49 | `dbxcatalogue.py`, `bf1943import.py` (yes); `pickle::loads` of their caches |
| `gameindex.rs` | 17 | meshview.py (yes); `pickle::loads` of `meshview_index.pkl` |
| `meshview/`, `navshared.rs`, `main.rs` | ~19 | - |

**Totals:**
- class 1: 1 group (bc2-menu's member order);
- class 3: 1 site (`ladders.json`'s `round_to`);
- class 2: everything else.

**Class-3 candidates outside `bc2-pyrt`** (Python behaviour copied without the crate; PY1 should check these against
DICE ground truth too):
- `bc2-cli/src/stockmp/pyx.rs`: Python's `round` (half to even), `fixed`, Neumaier `sum` and `hypot`, used for the
  AI spawner and order positions written into levels (`generate.rs:147, :152`, `build.rs:251, :547`);
- `bc2-navbuild`'s `pymath`: `dist3` (CPython `math.dist`) for path-data edge costs (`graphedit` `cost_tag`), and
  `round3`;
- `bc2-levelwrite/src/culltool.rs` `py_round`: half-to-even rounding of the cull tree's sphere keys written to game
  trees.

**Stale text:** `bc2-cli/src/cull/mod.rs` and `stockmp/mod.rs` still name the retired `culltree.py` / `stockmp.py`
scripts (guarded by `RETIRED`), and `cull/mod.rs` / `culltool.rs`'s module docs still say damaged input is "handed
to the Python". Left for PY1.

### Proof

- **WW2 rebuild** (OPFOR main checkout, `OPFOR_TOOLKIT` a scratch folder whose `bfbc2-tool-suite` was a junction to this worktree, its fresh `bc2_native.pyd` and `bin\bfbc2.exe`, `--jobs 2`): all 55 steps in 22.0 min (the 3 level chains side by side in 536 s). `compare_oracle.py`: **10,852 of 10,852 identical** (differ 0, missing 0, extra 0). `qa_checks.py` passed. OPFOR's `SUITE_REQUIRES` (< 0.14.0) warned about the new version, as expected. OPFOR was not changed and nothing was installed.
- **`python run_tests.py quick -j 2`:** 133 of 133 passed in 713 s. That includes:
  - O1-O5 (O2 among them, with no program on disk);
  - the cargo build, the quick tests, clippy `-D warnings` and fmt;
  - ruff and mypy on every folder;
  - the repository rules, the env-var registry and `spec/trace.py --check` (710 rules).
- **The runner's own tests:** `test_runner_parallel.py`, `test_runner_args.py` and `test_runner_output.py` pass, including the new stable-id checks.
- **Release builds:** `rust\build_native.py` and `--editor` (the module, `bfbc2.exe` 0.14.0, `bfbc2-editor.exe`, `mapexport.exe`; sources 3d22f2ee69da).
- **The game tier's whole-level tests and the VW2 export canaries were not run.** In the export, levelwrite and editor code this round changed only:
  - the writers of the project, the cull cache and the prepared copy;
  - one message;
  - lint attributes;
  - the prepared record's file order, now fixed.
  Their quick tests passed.

### Left

- **The registry:** 22 `use_native` copies (listed in item 7).
- **The census as a check:** PR1b-2.
- **The grandfathered round names** (PR1c):
  - 60 runner entries' display names;
  - 21 Rust test files and their `mod` names (`df1`, `e2b`, `ex1`, `g2`-`g4`, `game_view_r5d`, `gr1`, `gr2`, `vw2`,
    `xf1`, `culltool_ct1`, `export_e3b`/`e3c`/`e3d`/`ex1`/`xf1`, `gameplay_xf1`, `r5d`);
  - the round-coded Python test scripts and goldens.
- **The 55 insertion-ordered JSON writes:** with their twins (PY1).
- **The `bc2-pyrt` removals:** PY1.
- **`gameindex.rs`' own pickle reader:** it keeps `Rc` rows for memory, so it is not the cache module's `read`. It
  goes when the cache's Python reader does (ARC-4).
- **Module docs that cite rounds:** rewritten as files are touched (NAM-2).

### Merging main (V1, NK2) into PR1a

PR1a's rules ran over what V1 and NK2 added while the round was under way.

- **Round names.** V1's test files were named by round: `bc2-editor/tests/v1.rs`, `bc2-levelwrite/tests/v1_rules.rs`
  and `v1_stage.rs`. The rule had let them through, because its exception for project-format files (`v1.mapedit.json`
  ... `v6`) matched any `v<n>`. The exception now covers only a name that is a version and nothing else. The three
  files are now `problems_panel.rs`, `export_rules.rs` and `export_rules_stage.rs`; their mod lines,
  `spec/transforms/validation.md` and `rules.json` were updated to match.
  - V1's test functions are still named `v1_*` (the rule does not cover function names).
  - The example `examples/v1_rules.rs` is outside the rule and was left (PR1c).
  - NK2's `parity_nk2_file.py` lives under `rust/` and is outside the rule too.
- **Test ids.** Neither round added a `run_tests.py` entry, so there were no new ids to give.
- **Hash order.** The lint flagged 7 new loops:
  - 2 reached an output and now run in a fixed order:
    - the export's XVAL-GRP-1 check walked the staged level's entries in hash order, and its message lists the first
      five problems; it now walks them in name order;
    - `validate`'s vehicle-spawner findings (XVAL-PLACE-1) were grouped by team in a `HashMap`; it is now a
      `BTreeMap`, so the Problems panel and `mapexport validate` list them the same way on every run.
  - 5 in NK2's `graphfile` were reviewed and annotated: counts, set membership, and bucket lists only tested with
    `any()`.
- **JSON key order.** `parity_nk2_file.py`'s 5 unsorted JSON writes now sort their keys.
- **`bc2-pyrt` in NK2.** `graphfile.rs` used `bc2_pyrt::fmt::round_to` for one thing: `round(v, 2)` of the dropped
  order nodes' positions, which only reach the build's report. It now uses `bc2_navbuild::pymath::round_to`, the same
  body. So `bc2-navbuild` links `bc2-pyrt` only with its `graphedit` feature, and `graphfile` (which the editor's
  dependency closure includes) does not. For PY1 this is class 2: a report number.
- **The atomic writer.** V1 and NK2 added no file writer in the Rust tools; V1's `v1_rules` example writes a scratch
  measures file. NK2's `navbuild.write_file` (the Python build's path-data file) now writes through
  `atomicio.write_bytes`: the same bytes, whole or not at all.
- **The `bc2_native` census.** It now counts 240 exports in 39 files (NK2's `navkit_write_graphs`,
  `navkit_shadow_islands` and the class `NavLinkScope`). 226 are called by a suite tool, 8 by OPFOR, and 14 by no tool (the same 14 as
  before).
- **The spec.** The merged spec's citations were remapped from the side each line came from (main's or PR1a's) to the
  merged files, and `rules.json` was re-emitted: 769 rules.
## AIS: the bot AI specification (2026-11-22, branch `ais`)

A specification round: no tool's behaviour changed. Two spec files (`spec/engine/bot_ai.md` 41 rules,
`spec/model/ai_orders.md` 16 rules): 5 verified, 43 observed, 9 inferred.

**Method.**
- **Data.** `rust/bc2-formats/examples/ai_probe.rs` reads the 59 AI / template partitions of `sp_common` and the 15 of
  `mp_common`, the level partitions of DICE's 13 campaign levels (Package copies) and of the 5 Onslaught levels OPFOR
  installs: field ranges, enumerator tallies, order / spawner / squad counts, event wiring. 8.9 s (release, cold
  bundle cache), structure and counts only.
- **Corpus tests.** `rust/bc2-formats/tests/ai_corpus.rs`: 4 game-tier tests (the shared AI partitions, the three
  difficulty modes, one OrderReadiness per order type, the campaign orders' shapes and the events into orders), 5.4 s
  in release.
- **The executable.** Read statically with a scratch copy of OPFOR's analysed Ghidra project (headless, read-only;
  REA's own Ghidra analysis of the 19 MB executable does not complete). Nine probe runs; no game launch.

**Findings for OPFOR's bot module:**

| Finding | Rule |
|---|---|
| a bot is a server player with the AI flag +0xD84, made by the spawner's spawn 0x72D160 | BOT-CTL-1 |
| the AI drives its soldier through an `EntryInputState` its input object (0x5A7F00, vtable 0x014198A8) pushes into the player's input handler: the server treats it as a remote human | BOT-CTL-2 |
| DICE's AI sends 13 actions (move, fire, alt fire, zoom, jump, interact, reload, sprint); aim is set on the soldier directly; no stance, grenade, gadget or vehicle-entry input | BOT-CTL-3 |
| sensing, decisions and behaviours run time-sliced per bot under budgets and priority bands | BOT-RT-4 |
| every player, human too, has an AI proxy; humans are Kynapse entities for avoidance | BOT-RT-5 |
| no AI class knows a game mode: orders are issued by the level's event graph | AIO-MODE-1, AIO-EVT-2 |
| the nine AI partitions are byte-identical in `sp_common` and `mp_common`; spawn templates are `sp_common`'s only | BOT-DIF-1 |
| campaign difficulty weakens enemies to 0.25-0.4 health and protects the human | BOT-DIF-2 |
| DICE's bots teleport when stuck 3 s, never board vehicles, use no class gadgets | BOT-MOV-3, BOT-VEH-3, BOT-CMB-4 |

**What is left** (ROADMAP AIS-rest; `spec/engine/bot_ai.md` "Reverse engineering still needed"): a play test with
probes on the AI input object, the behaviours' writes, bots as scored and named players, the mode state's run-time
objects, the bot object's class and senses, the order and squad runtime, the session's AI difficulty.

## NKS-deep A: the Kynapse generator's parameters and public documentation (2026-11-22, branch `nksd`)

A specification round: no tool's behaviour changed. New rules KYG-PAR-1 (verified), KYG-PAR-2 (observed), KYG-PAR-3
(inferred), KYG-NODE-6 (verified), KYG-DOC-1 (observed), KYN-FILE-2 (verified), HPD-TAB-4 (verified); a pointer from
KYG-NODE-4 to KYG-PAR-1 and one open question added.

**Method.**
- **The executable.** `BFBC2Game.exe` (build 795745) read statically and read-only with capstone / pefile scratch
  scripts (deleted): DICE's configuration writer (`FUN_00F194F0`), Kynapse's map-builder parameter reader
  (0x011483D0), its defaults (0x01146822) and info-file format (0x014C2EF8), and the HPD writer (0x011C5140). No
  Ghidra project, no game run.
- **DICE's settings.** The four `AI/Pathfinding` partitions read out of `sp_common` / `mp_common` `level-00.fbrb`
  into scratch (identical in both; deleted).
- **Data.** Scratch Python over the 35 files, then four game-tier tests in `kynapse_corpus.rs`
  (`header_tables_and_record_layout`, `node_ids_count_from_101_in_table_order`, `sector_times_are_the_write_time`,
  `nearest_node_at_dist_propagation`): 1.6 s together in the test profile.
- **The web.** Searches for Kynapse manuals, SDK pages and talks; five pages opened (KYG-DOC-1 lists them).

**Findings:**

| Rule | Finding | Evidence |
|---|---|---|
| KYG-PAR-1 | DistPropagation = GraphAccuracy + 0.35 m: DICE writes GraphAccuracy before EntityRadius, and the reader derives DistPropagation from the radius it holds then (Kynapse's default 0.35). 3.35 / 0.85 / 15.35 m is KYG-NODE-4's step exactly; DistDensityMax 0.65 x it, DistDensityMin 0.35 | nearest node exactly at it: coarse 7,505 (control 96), detailed 2,455 (199), vehicle 41 (0); coarse median nearest-neighbour 3.350 m |
| KYG-PAR-2 | DICE's settings under the generator's names: OverConnectionRatio 1.2 (= KYG-EDGE-3's spanner), MaxDeltaHeight 1000 (no height limit on edges), EdgeRadius 0.41 / 2.51, NbSectors 32 / 128 / 64, StepMax / StepLength 0.28 / 0.36 and 0.4 / 0.4; Kynapse defaults for the rest | the partitions; the reader's field offsets |
| KYG-PAR-3 | the break slope is StepMax / StepLength (0.778): the 0.62 m link step at pitch 0.8, NKG3's 0.775-0.780 slope band, the detailed 0.156 m (measured about 0.17) | inferred |
| KYN-FILE-2 | header words from the HPD writer: 0 = 1 = the version 2, 3 the mode, 4-7 mode-0 floats, 8-31 two tables of four {id, offset, size} data descriptors ({2, 0, 8}: the node's 8 bytes of lookup lists), 32-34 sizes, 35 the path objects' count; end slots hold the debug heap's 0xBAADF00D (83) | 35 of 35 files; 2,605 of 3,120 sectors hold an edge longer than the +20 float, so the third-party "longest edge" reading is wrong for BFBC2 |
| KYG-NODE-6 | node ids count from 101, sector by sector in table order, ordinary and border interleaved; 80 % of the numbers used | 35 of 35 files start at 101 |
| HPD-TAB-4 | the table's date / time are the write time (one-second steps, a file within 9 s, infantry before vehicle) | 3,120 sectors |
| KYG-DOC-1 | no public Kynapse manual or SDK page; articles confirm the design (collision-world sampling, tiles, hierarchy); one third-party claim contradicted | five pages |

**Ranked: what would raise NavKit's parity most if used (for parts B and C).**
1. **Propagation at DistPropagation (KYG-PAR-1) for node placement.** The node generator steps 3.35 / 0.85 / 15.35 m;
   placing NavKit's nodes by propagation from the seeds at that step (with a minimum spacing of DistDensityMin and
   the coverage of KYG-NODE-5) is the first rule with a decoded constant for KYG-NODE-1, the largest gap
   (`nodes_same` 0.66 %, `nodes_near` 47.6 %). Part B: fit where a propagation starts (the step pairs' centres)
   against the raw samples; read the map builder's propagation and density code (0x01140000-0x01160000).
2. **The walkable step StepMax / StepLength x pitch (KYG-PAR-3)** for the area link rule (0.622 m, not 0.6) and the
   D / F break at both pitches (0.156 m detailed). It aligns NavKit's area rule with DICE's, which KYG-AREA-2 says
   must happen before the off-grid portals can go (`portals` 6.5 %, `areas_same` 5.2 %), and gives part C's vehicle
   run its threshold (1.0 m).
3. **Node ids from 101 in table order with one range per sector (KYG-NODE-6)**: cheap, and the unused numbers in
   DICE's ranges show which nodes the later step dropped, which part B can use to fit KYG-EDGE-3's 37 % of
   unexplained missing edges.
4. **The derived density limits** (DistDensityMax 2.1775 / 0.5525 / 9.9775 m, DistDensityMin 0.35 m): no trace in
   the data yet; part B tests them on the raw samples before NavKit uses them.
5. **OverConnectionRatio 1.2 and MaxDeltaHeight 1000** (KYG-PAR-2): NavKit already uses both (NKG2's spanner, no
   height limit); now named as DICE's settings, nothing to change.
6. **The header and record names (KYN-FILE-2, HPD-TAB-4)**: byte-level only; NavKit already writes DICE's words.

**Left:** the density limits' effect, the StepMax / StepLength rule against raw samples, NbSectors, EdgeRadius and
HoleMax's traces in the data, the map builder's propagation code, and the C portals' direction vector (KYN-C-1).

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
