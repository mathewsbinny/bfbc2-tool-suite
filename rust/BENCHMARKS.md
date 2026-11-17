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
