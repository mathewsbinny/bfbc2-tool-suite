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

## Retired Python (2026-11-17, branch `retire-e1`)

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
`havok_tests.rs`), `test_havokport.py` now 11 s (`ix.read_many`). Seeds in `tests	est_durations.json`.

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

**Parity.** `tests	est_navbuild_native.py` (quick), section E3h, the Rust side strict and its calls counted: 10 whole
`stage()` runs on random levels over the corner of four sectors at pitch 0.8 and 0.2 (40 sectors, 3,903 areas, 3,017
route nodes, 196 D lines / F cells; meshes, areas, `area_of`, border nodes, graphs, ids, the build's counters) - every
E3h path ran each time; 12 sector graphs with ladder links; 16 `walkable` runs with unseeds (292 surfaces dropped);
`navdiff.area_cells` on 61 random outlines; `round(v, 3)` on 180,000 values bit for bit: 40 calls, 0 differences. The
E3c checks (456 calls) and the recorded grid walks unchanged. Rust: `e3h_tests.rs` (a property test of the union-find
floods and islands against the breadth-first walks, 160 random multi-layer levels; the sector graph), `parity.rs`;
40 tests in the crate, `tests	est_rust.py` passes. Whole levels against `navbuild_golden.json` (the build clock
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
`bfbc2-bot-ai-debugger`, `bfbc2-tactical-map-authoring-tool`; `bfbc2-diff-workbench` is LevelDiff, C#: 2.4 s on MP_005).

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
