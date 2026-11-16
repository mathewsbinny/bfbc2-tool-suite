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
  candidate for E1, measured first.

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

## Not ported yet

- The DBX **writer** is in Rust (`dbx::write`, tested by round trips) but Python's `dbx.write` still writes the
  files the suite produces: the Python tools build Python trees, and converting them to Rust to write would cost
  more than it saves today. E1's `levelwrite` (Rust trees) is where the Rust writer takes over.
- **FBRB packing** stays with FbrbTool (`pack`, with DICE's padded layouts, `fbrb.manifest` and size2 rules):
  the Rust reader has only a small test writer (`fbrb::build`, as `fbrb.build`). E1's `levelwrite` is where packing
  moves to Rust.
