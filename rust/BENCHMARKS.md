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
| `shaderdb` queries (E4b), its reader (SD1) | -> `ShaderDbHandle`, `bc2_formats::shaderdb::open` | no (0 of 13.6 M answers) | E4b: none (live); SD1: `testssd1_golden_*.json` | keep: SD1 (merged here) made them its reference and fallback; they stay this round (the coordinator) |
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
