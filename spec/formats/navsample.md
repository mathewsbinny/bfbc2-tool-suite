# Navigation samples (`navsample` `.bin`)

The suite's own file, not a game format: a level's collision world sampled on a regular grid, NavKit's input for
building path data (`hpd.md`). Two writers make it:

- **in game**: Project OPFOR BFBC2's `[dev] navsample` option (`opfor:src/dll/navsample.cpp`), which casts rays and
  capsules through the engine's own physics queries while the level runs, into `work\navsample\<name>.bin`;
- **offline**: `navsample_offline.py` (its writer in Rust, `bc2_navbuild::navsample::write_v3`), which computes the
  same samples from the level's terrain and Havok collision without starting the game, into
  `work\navsample_offline\`.

Readers: `navsample.load` (`bfbc2-auto-navkit/tools/navsample.py`) and its Rust core (`bc2_navbuild::navsample`).

**Corpus.** 165 sample files in OPFOR's `work\navsample` and `work\navsample_offline` (surveyed 2026-11-19): 129
version 3, 34 version 2, 2 version 1.

## 1. Layout

All values little-endian.

| Part | Version 1 / 2 | Version 3 |
|---|---|---|
| header | 56 bytes (NAVS-HDR-1) | the same |
| records | per column: u8 surface count, then the column's records | every record of every column, back to back |
| counts | (in the records) | u8 per column, nx x nz bytes |
| trailer | none | u64 offset of the counts, then `CNT3` |

### NAVS-HDR-1: Header

Status: **verified**

56 bytes, `<4sI3f2i6fI`:

| Offset | Type | Field |
|---|---|---|
| 0 | char[4] | `ONS1` |
| 4 | u32 | version: 1, 2 or 3 |
| 8 | f32 | pitch, the grid spacing (m) |
| 12 | f32 | min x, the grid origin |
| 16 | f32 | min z |
| 20 | i32 | nx, columns along x |
| 24 | i32 | nz, columns along z |
| 28 | f32 | y top, where each column's cast starts |
| 32 | f32 | y bottom, where it stops |
| 36 | f32 | capsule radius |
| 40 | f32 | capsule bottom (above the surface) |
| 44 | f32 | capsule top |
| 48 | f32 | ray (sphere-cast) radius |
| 52 | u32 | reserved: 0 |

A reader refuses another magic or version.

[code: bfbc2-auto-navkit/tools/navsample.py:36] [code: bfbc2-auto-navkit/tools/navsample.py:84-88]
[code: rust/bc2-navbuild/src/navsample.rs:12] [code: rust/bc2-navbuild/src/navsample.rs:240-251]
[test: rust/bc2-navbuild/src/navsample.rs::version_3_written_and_read] [test: tests/test_f3_native.py::area_navsample]
[data: 165 of 165 files have the magic, a known version and reserved 0]

### NAVS-HDR-2: Grid

Status: **observed**

Column (i, j) is the vertical line at (min x + i x pitch, min z + j x pitch); min x and min z are whole multiples of the
pitch (to f32 precision), as DICE's generator samples at whole multiples of the pitch so that outlines run halfway
between samples. Columns are stored **j-major**: column index = j x nx + i.

[code: bfbc2-auto-navkit/tools/navsample.py:71-76] [code: opfor:src/dll/navsample.cpp:292-296]
[data: 165 of 165 files have min x / pitch and min z / pitch within 0.001 of an integer]

### NAVS-HDR-3: Run settings

Status: **observed**

The header records the settings of the run: pitch 0.8 (normal sectors) or 0.2 (detailed sectors), capsule radius 0.4,
top 1.9 (the soldier's EntityHeight), bottom 0.28 (StepMax: the "normal" run) or 0.6 (the "high" run for stairs and
landings), ray radius 0.05. Other radii (0.2-0.3) come from experiments with narrower capsules.

[data: 165 files: (0.8, 0.4, 0.28) 42, (0.2, 0.4, 0.28) 41, (0.8, 0.4, 0.6) 40, (0.2, 0.4, 0.6) 38, 4 experimental radii; top 1.9 and ray radius 0.05 in all]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:383-394]

### NAVS-REC-1: Surface record

Status: **verified**

One record per surface found in a column, from the top down:

| Offset | Type | Field | Versions |
|---|---|---|---|
| 0 | f32 | y, the surface's height | all |
| 4 | 3 x i8 | the hit normal x, y, z, each times 127, rounded half to even, clipped to +-127 | all |
| 7 | u8 | flags: bit 0 the capsule overlaps geometry (no room to stand), bit 1 a query faulted | all |
| 8 | 3 x u32 | the hit: shape vtable of body A (+0x54), of body B (+0x50), body B's word at +0x1C | 2, 3 |

Version 1 records are 8 bytes, versions 2 and 3 20 bytes. A reader gives the normal as component / 127. The offline
writer stores zeros for the hit words when it has none.

[code: opfor:src/dll/navsample.cpp:178-190] [code: rust/bc2-navbuild/src/navsample.rs:252-273]
[code: rust/bc2-navbuild/src/navsample.rs:304-338]
[test: rust/bc2-navbuild/src/navsample.rs::records_and_an_interrupted_run] [test: rust/bc2-navbuild/src/navsample.rs::version_3_written_and_read]
[test: tests/test_f3_native.py::area_navsample]

### NAVS-COL-1: Columns in version 1 and 2

Status: **verified**

Each column is a u8 surface count followed by that many records. A file cut short (an interrupted run) ends early: the
columns not reached are read as empty.

[code: rust/bc2-navbuild/src/navsample.rs:304-345]
[test: rust/bc2-navbuild/src/navsample.rs::records_and_an_interrupted_run]

### NAVS-COL-2: Counts block and trailer in version 3

Status: **verified**

Version 3 writes the records of all columns first, then the nx x nz count bytes (j-major), then a u64 with the offset
of the counts and the four bytes `CNT3`. The records start at byte 56; column *c*'s records follow those of columns
0 .. *c* - 1 (a reader takes the running sum of the counts). A file without the trailer is an unfinished run and is
refused. The counts block's offset equals 56 + 20 x the total surface count, and the file ends 12 bytes after the
counts.

[code: rust/bc2-navbuild/src/navsample.rs:131-151] [code: rust/bc2-navbuild/src/navsample.rs:274-278]

[test: rust/bc2-navbuild/src/navsample.rs::version_3_written_and_read]
[data: 129 of 129 version 3 files: trailer present, counts offset + nx x nz + 12 = file size, (offset - 56) a multiple of 20]

### NAVS-COL-3: At most 255 surfaces a column

Status: **implemented**

A column's count is one byte. The in-game sampler records at most 32 surfaces a column; the offline writer refuses a
column of more than 255.

[code: opfor:src/dll/navsample.cpp:57] [code: rust/bc2-navbuild/src/navsample.rs:229-237]

### NAVS-SMP-1: What a sample is

Status: **verified**

Per column a sphere of the ray radius is cast straight down from y top; after a hit at y the next cast starts just
below it, so every surface of the column is found (storeys, bridges, terrain under overhangs). A cast that hits within
about the ray radius of its start began inside geometry and is not recorded: the start steps down 0.5 m, at most 10 m
in a row. Each surface is then tested with a capsule of the run's radius from y + bottom to y + top (flag bit 0). The
offline sampler reproduces this from the triangles of the terrain and the placed objects' collision: since round
NKS-deep C in Rust (`bc2_navbuild::sampler`, `bfbc2 navkit sample <level> --offline`), bit for bit the NumPy it
replaced (`navsample_offline.py`'s sampling, now a call into it).

[code: opfor:src/dll/navsample.cpp:144-195] [code: rust/bc2-navbuild/src/sampler/mod.rs:682-785]
[code: rust/bc2-navbuild/src/sampler/mod.rs:839-1145]
[test: rust/bc2-navbuild/src/sampler/tests.rs::the_cast_layers] [test: rust/bc2-navbuild/src/sampler/tests.rs::the_capsule]
[test: rust/bc2-navbuild/src/sampler/geom.rs::the_interval_against_brute_force]
[test: rust/bc2-navbuild/src/sampler/tests.rs::the_file_is_write_v3s]
[data: round NKS-deep C: the Rust sampler's files against the Python's (NumPy) on the 15 regression levels at pitch 0.8, both capsule runs: 30 of 30 byte-identical; at pitch 0.2 over the detailed sectors (sp_009, sp_004_b, ons_mp_004): 6 of 6; on the in-game files' grids (`like`; sp_009, sp_004_b, 0.8 and 0.2, both runs): 8 of 8]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:168-184]

### NAVS-SMP-2: The offline geometry covers every square a run samples

Status: **implemented**

The offline sampler crops the level's collision to a box before sampling (objects within 300 m, triangles within
2 m). The crop covers every square the run samples: the level's box, the sector squares it touches, the flagged
detailed sectors and DICE's (KYG-DET-1). A 0.2 m run over a detailed sector outside the level's box (sp_009: 6
flagged sectors at x 500-600, the box ends at x 100) otherwise samples that sector's terrain without its objects -
`navsample_offline.sample_level` and `stage` cropped to the level's box until round NKS-deep C (its `sample --pitch
0.2` cropped to the detailed box, and so was right). A wider crop leaves every run inside the box the same bytes: no
triangle 2 m outside a box reaches its columns (the widest reach is the capsule's 0.4 m).

[code: rust/bc2-navbuild/src/sampler/level.rs:758-790]
[data: round NKS-deep C: sp_009's 0.2 m run cropped to the level's box against the detailed box: 1,678,603 against 1,723,700 surfaces, 20,768 columns differing, all in the flagged sectors east of x 500; with the wider crop the 0.8 m runs of the 15 regression levels are unchanged (30 of 30 identical)]

### NAVS-SMP-3: A vehicle-shaped run

Status: **inferred**

A vehicle run samples the same casts at the vehicle pitch (1.0 m) and tests each surface with the vehicle type's
entity (KYG-PAR-2: EntityRadius 2.5, EntityHeight 4, StepMax 0.4; CollisionModel cylinder): a vertical cylinder of
radius 2.5 from 0.4 to 4 m above the surface (a 32-sided prism inscribed in it, within 1.2 cm) meets no collision
triangle, and the height field under it rises no more than 0.4 m above the surface's own plane (slopes are the
build's GroundSlope rule, not the entity's). The header records radius 2.5, bottom 0.4, top 4; the file is
`<level>_navkit_veh.bin`. No in-game vehicle-shaped run exists to check it against (KYG-DF-2).

[inferred: KYG-PAR-2's vehicle settings and its cylinder collision model; the step against the surface's plane as navvehicle's obstacle rule takes it (CLEAR_LOW above the local ground plane)]
[code: rust/bc2-navbuild/src/sampler/mod.rs:600-632]
[test: rust/bc2-navbuild/src/sampler/tests.rs::the_vehicle_cylinder]

### NAVS-SMP-4: How far the offline samples agree with the in-game ones

Status: **observed**

On the in-game files' own grids (`bfbc2 navkit sample <level> --offline --like <files>`), over the 15 regression
levels, per in-game surface: a partner within 0.1 m in the same column 95.3-99.7 % at 0.8 m and 95.4-99.3 % at 0.2
m; the walkable flag agreeing on matched surfaces 96.3-99.7 %; the blocked flag 96.6-99.7 %; object surfaces found
82.4-97.2 % (the weakest: sp_01, sp_03, sp_003_b, ONS_MP_002 - surfaces of objects the in-game world holds and the
offline geometry does not, or holds elsewhere; not decoded per object).

[data: round NKS-deep C, rust/BENCHMARKS.md "NKS-deep C": 60 in-game files (0.8 m and 0.2 m, both capsule runs, 15 levels; 0.2 m: 1.6 to 25.6 million sampled columns a file), the measures of `navsample_offline.compare`]

## Open questions

- The hit words (body vtables and the filter word) are build-specific addresses of `BFBC2Game.exe` 795745; which
  values mean terrain, object or soldier is recorded in NavKit's code, not here.
- Version 1 and 2 files have no trailer, so an interrupted run cannot be told from a complete one by its bytes.
