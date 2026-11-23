# Kynapse path data (`pathdata.hpd`)

The AI's navigation data: one file per level and pathfinding type, written by DICE's Kynapse generator
(`Kaim::CMapBuilder`, a licensed middleware tool) and read by the Kynapse runtime inside `BFBC2Game.exe`. It holds,
per 100 m sector, a **route graph** (nodes and edges the path search runs on) and a **runtime mesh** (the walkable
ground as areas inside tiles, with their outlines, portals and step records) that the runtime uses to resolve points,
test straight moves and cross sector borders.

The suite reads and writes it byte for byte: `bc2_formats::hpd` (the file, its sector table and route graphs),
`bc2_formats::kymesh` (the runtime-mesh record as areas, portals, outlines, cells and links), `bc2_formats::hpdlayout`
(every byte of a record labelled), ported from NavKit's `hpdtool.py` / `kymesh.py`. NavKit (`bc2-navbuild`,
`bfbc2-auto-navkit`) generates new files; OPFOR's goal is 100 % content parity with DICE's path data, so this file
states both the byte layout and the invariants DICE's files hold and the game relies on. The full research record,
with the measurements in the order they were made, is `bfbc2-auto-navkit/research/KYNAPSE_SPEC.md`.

**Corpus.** "The 35 files" are the game copy's `Output/win32/ai/levels/<level>/{infantry,vehicle}/pathdata.hpd`
(surveyed 2026-11-19): DICE's data for the 13 campaign levels (13 infantry, 12 vehicle: sp_009 has none) and the
console Onslaught builds ONS_MP_002 / 004 / 005 / 005GR / 008 (5 + 5). Together 3,120 sectors, 596,069 route nodes,
1,694,066 edges and 151.9 MB of runtime mesh. Earlier rounds (`KYNAPSE_SPEC.md`) counted 74 files of the base game
and Napisal's install, 7,944 runtime meshes.

**Variants.** The file is little-endian on every platform; DICE's campaign files are byte-identical on the PC and the
Xbox 360, and the Onslaught files are DICE's console builds used unchanged on the PC. BF1943 has no Kynapse path data
in the suite's scope.

## 1. Where the file is and what selects it

### HPD-USE-1: File location and pathfinding types

Status: **observed**

A level has at most two path-data files, `Output/win32/ai/levels/<level>/infantry/pathdata.hpd` and
`.../vehicle/pathdata.hpd`, one per `PathfindingType` (`OutputName` Infantry / Vehicle), selected by the level's
`PathfindingTypes` (`AI/Pathfinding/Infantry`, `AI/Pathfinding/Vehicles`). There is no air or water type: BFBC2's
navigation data is ground only. Both files have the same format (sections 2-4); they differ in their values (HPD-SET-1).

[data: 35 files in the game copy, 18 under infantry folders and 17 under vehicle folders; every level with DICE data has a vehicle file except sp_009]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:17-23] [doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:34-44]

### HPD-USE-2: Sector offsets

Status: **observed**

The sector grid of a file is placed by the level's `Levels/<level>/Precomputed/{Infantry,Vehicle}/SectorOffsets`
partition (`XOffset`, `ZOffset`), which the runtime reads at level load into its path-data configuration. Without the
partition the loader returns a default object and the runtime uses offsets 0, 0; a file built on another grid then
fails to resolve requests in some sectors (ONS_MP_002's PC port: 74 of 1,155 requests failed at 0, 0, all starting in
one sector; 0 of 6,737 with DICE's -3, -7). The offset a file was built with can be read back from its own sector
ids: per sector `floor(centre / 100) - grid id`, the most common value (27 of 27 shipped assets).

[exe: probes at 0x00F20AD2, 0x00F20B48, 0x00F20BB0 read SectorOffsetX / SectorOffsetZ at level load]
[data: 27 of 27 shipped SectorOffsets assets equal the per-file most common floor(centre / 100) - grid id; 7 of 13 shipped vehicle assets do not follow the "one below the extent" rule]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:383-394] [doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:846-852]

## 2. File layout

All values are little-endian. A file is the magic, a fixed header, the sector table and the sector blocks:

| Offset | Size | Content |
|---|---|---|
| 0x00 | 12 | `Kynogon HPD` and a zero byte |
| 0x0C | 36 x u32 | header words (HPD-HDR-2); word 2 is the sector count *n* |
| 0x9C | *n* x 48 | sector table (HPD-TAB-1) |
| 0x9C + 48*n* | sum of the blocks' sizes | the sector blocks, in table order, back to back (HPD-BLK-1) |

### HPD-HDR-1: Magic

Status: **verified**

The file starts with the 12 bytes `Kynogon HPD\0`. A file that does not is not path data.

[code: rust/bc2-formats/src/hpd.rs:28] [code: rust/bc2-formats/src/hpd.rs:173-175]
[test: rust/bc2-formats/src/hpd.rs::refusals] [data: 35 of 35 files]

### HPD-HDR-2: Header words

Status: **verified**

36 u32 words follow the magic (bytes 0x0C-0x9B). Word 2 is the sector count. The other 35 are the same in every file:
`2, 2, n, 1, 0, 0, 0, 0, 2, 0, 8`, then zeros up to word 31, then `36, 12, 1, 0`. The suite keeps all 36 words as read
and rewrites only word 2 from the sector list when writing.

[code: rust/bc2-formats/src/hpd.rs:30-32] [code: rust/bc2-formats/src/hpd.rs:179-183] [code: rust/bc2-formats/src/hpd.rs:292-301]
[test: rust/bc2-formats/src/hpd.rs::round_trip]
[data: 35 of 35 files have exactly these 35 words besides the count]

### HPD-TAB-1: Sector table entry

Status: **verified**

The table has *n* entries of 48 bytes (`<I6f5I`) at 0x9C:

| Offset | Type | Field |
|---|---|---|
| 0 | u32 | sector id, grid coordinates `0xXXYY` (x in the high byte, z in the low byte; 0x0101 = 257) |
| 4 | 6 x f32 | bounds: min x, max x, min y, max y, min z, max z (world space, Y up): the box of the sector's route nodes (KYN-SEC-2) |
| 28 | u32 | the sector kind: 0 (KYN-SEC-1) |
| 32 | u32 | block size in bytes |
| 36 | u32 | block offset, from the start of the block area (0x9C + 48*n*) |
| 40 | u32 | build date `YYMMDD`, the month **zero-based** (100007 = 2010-01-07) |
| 44 | u32 | build time `HHMMSScc` (hundredths of a second) |

[code: rust/bc2-formats/src/hpd.rs:186-201] [code: rust/bc2-formats/src/hpd.rs:303-315]
[test: rust/bc2-formats/src/hpd.rs::round_trip] [test: bfbc2-auto-navkit/tests/test_pathdata_native.py::main]
[data: reserved word 0 in 3,120 of 3,120 sectors; build months 0 (January 2010) in the 25 campaign files, 3-4 (April-May 2010) in the 10 Onslaught files]

### HPD-TAB-2: Block offsets are a running sum

Status: **verified**

Each entry's offset is the sum of the sizes of the entries before it; the blocks are stored in table order with no
gap, and they end exactly at the end of the file. The suite's reader refuses a file whose blocks do not cover it
exactly; its writer recomputes sizes and offsets from the blocks.

[code: rust/bc2-formats/src/hpd.rs:240-242] [code: rust/bc2-formats/src/hpd.rs:302-315]
[test: rust/bc2-formats/src/hpd.rs::refusals] [data: 35 of 35 files covered exactly]

### HPD-TAB-3: Sector ids and the runtime mesh agree

Status: **observed**

A sector's table id equals the sector id stored at +0x1C of its runtime mesh (KYM-HDR-1). Sector ids are unique in a
file and name the sector's 100 m grid square after the file's sector offsets (HPD-USE-2).

[data: 3,120 of 3,120 sectors have the same id in the table and the mesh header]

## 3. Sector block and route graph

### HPD-BLK-1: Block layout

Status: **verified**

| Part | Layout |
|---|---|
| counts | u32 node count, u32 edge count |
| nodes | node count x 36 bytes (HPD-NODE-1) |
| edges | edge count x 12 bytes (HPD-EDGE-1) |
| runtime mesh | the rest of the block; it starts with `KynogonRuntimeMesh` (section 4) |

The suite refuses a block whose runtime mesh does not start with the marker right after the edges.

[code: rust/bc2-formats/src/hpd.rs:202-227] [code: rust/bc2-formats/src/hpd.rs:255-281]
[test: rust/bc2-formats/src/hpd.rs::refusals] [test: rust/bc2-formats/src/hpd.rs::round_trip]
[test: bfbc2-auto-navkit/tests/test_pathdata_native.py::main] [test: bfbc2-auto-navkit/tools/hpdtool.py::main]

### HPD-NODE-1: Route node record

Status: **verified**

36 bytes, `<I3fIf3I>`:

| Offset | Type | Field |
|---|---|---|
| 0 | u32 | id; bit 31 set: a **border node** shared with neighbouring sectors (HPD-NODE-4) |
| 4 | 3 x f32 | world position x, y, z |
| 16 | u32 | flags: always 0x80000000 |
| 20 | f32 | DistEdgeMax on the sector's first node (infantry 15, vehicle 50), 0.0 on every other node |
| 24 | u32 | *a*: index of the node's first outgoing edge + 1, 0 when it has none (HPD-NODE-3) |
| 28 | u32 | *b*: low 16 bits the k-th entry of the node indices sorted by x, high 16 bits sorted by y |
| 32 | u32 | *c*: low 16 bits the k-th entry sorted by z; bit 16 the border-node owner flag (HPD-NODE-5) |

[code: rust/bc2-formats/src/hpd.rs:76-93] [code: rust/bc2-formats/src/hpd.rs:153-166]
[test: rust/bc2-formats/src/hpd.rs::round_trip]
[data: flags 0x80000000 on 596,069 of 596,069 nodes; first-node value 15.0 in 2,626 infantry sectors and 50.0 in 494 vehicle sectors, 0.0 on all 592,949 other nodes]

### HPD-NODE-2: Node order

Status: **verified**

Within a sector the nodes are sorted by id as an unsigned 32-bit value: ordinary nodes ascending, then border nodes
(bit 31 set) ascending. The runtime binary-searches these arrays when it checks a path's start and destination ids.
Unsorted nodes made go-to requests fail ("destination id not in the result", 29 -> 0 once sorted) and crashed the
game on leaving the level (a pure virtual call while freeing a Kynapse container).

[test: bfbc2-auto-navkit/tools/hpdcheck.py::check] [data: 3,120 of 3,120 sectors sorted, border nodes last]
[exe: the go-to request's result-set binary search; the unload crash in a Kynapse container's destructor, 2026-10-20]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:559-565] [doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:587-590]

### HPD-NODE-3: First-edge field

Status: **observed**

Field *a* is the index of the node's first outgoing edge plus 1, and **0 when the node has no outgoing edge** (border
nodes whose edges are stored in the owning sector). A non-zero value on a node without outgoing edges makes the
runtime read another node's edges and crash.

[data: the rule holds for every node of 3,120 of 3,120 sectors]
[exe: crash in game with a non-zero a on an edgeless node, 2026-10-15]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:444]

### HPD-NODE-4: Border nodes

Status: **observed**

A node with id bit 31 set lies on a sector border band and appears, with the same id and exactly the same position, in
every neighbouring sector that shares it (two to four sectors: a node at a sector corner is one id in three or four
sectors). Border nodes lie in a band beside the 100 m grid line (DICE: median 2.2 m from it, at most 15.7 m), not in
a single row.

[data: 84,688 of 84,688 high-bit node ids of the campaign files appear in a neighbouring sector at the same position; sp_004_b: 55 border ids in 3 sectors, 36 in 4]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:440] [doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:601-607]

### HPD-NODE-5: Border-node owner flag

Status: **observed**

Bit 16 of field *c* is set only on border nodes, on the copies in the sectors that do **not** own the node; each border
id has one owner, the sector whose 100 m square holds the node, and only the owner's copy carries the node's edges.
Ordinary nodes never have the bit. With the bit set on every copy, the runtime's searches failed to expand ("no node
reached"). DICE's files are not perfectly regular here: some border ids have no owner copy and some two or more, so
`hpdcheck` does not check this rule.

[data: bit 16 set on 127,880 nodes of the 35 files, every one a border node; sp_004_b: 1,379 of 1,445 border ids owned by the sector whose square holds the node]
[exe: read as the byte at node +0x22: edges into a node with no loaded owner copy dropped (0x0105C8C0), non-owner copies skipped by the node lookup (0x0105E786); KYRT-PATH-3]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:566-570] [doc: bfbc2-auto-navkit/tools/hpdcheck.py:14-15]

### HPD-NODE-6: Spatial lookup lists

Status: **observed**

Fields *b* and *c* hold three lists interleaved, one entry per node slot: the node indices sorted by x (low half of
*b*), by y (high half of *b*) and by z (low half of *c*). The k-th node's fields hold the k-th entry of each list, not
the node's own rank. The tie-break order among equal coordinates is not known. The runtime's node lookup
binary-searches the three lists and walks one of them in order (KYRT-NODE-2), so the order of equal coordinates is
read.

[data: 93 % / 98.7 % / 92.8 % of entries exact over the 2,776 campaign sectors measured, the rest ties]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:445-450]

### HPD-NODE-7: No lone nodes

Status: **observed**

Every node has at least one edge, incoming or outgoing, somewhere in the file.

[data: 0 nodes without an edge in the 3,120 sectors of the 35 files]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:367-368]

### HPD-EDGE-1: Edge record and order

Status: **verified**

12 bytes, three u32: cost, source node index, target node index (indices into the sector's node list). Edges are
sorted by source node; most pairs exist in both directions. Every index must name a node of the sector.

[code: rust/bc2-formats/src/hpd.rs:95-104] [code: rust/bc2-formats/src/hpd.rs:212-223]
[test: bfbc2-auto-navkit/tools/hpdcheck.py::check] [test: rust/bc2-formats/src/hpd.rs::round_trip]
[data: 3,120 of 3,120 sectors have their edges sorted by source]

### HPD-EDGE-2: Edge cost

Status: **observed**

cost = `0x80000000 | floor(L x 16)`, L the 3-D distance between the two nodes' stored f32 positions (units of 1/16 m).
DICE's files have no zero-cost edge and no edge between two different nodes at the same position.

[data: 1,694,058 of 1,694,066 edges exact, 8 differ by one unit (float rounding of the length)]
[test: bfbc2-auto-navkit/tools/hpdcheck.py::check]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:448]

### HPD-SET-1: Per-type values

Status: **observed**

The values a file's sectors carry follow its pathfinding type:

| | Infantry, normal sector | Infantry, detailed sector | Vehicle |
|---|---|---|---|
| pitch (mesh +0x24) | 0.8 m | 0.2 m | 1.0 m |
| resolution (mesh +0x20) | 0.025 | 0.05 | 0.02 |
| tiles (mesh +0x38 / +0x3C) | 3 x 3 (40 m) | 6 x 6 (20 m) | 3 x 3 |
| entity radius (mesh +0x40) | 0.4 m | 0.4 m | 2.5 m |
| DistEdgeMax (first node) | 15 | 15 | 50 |
| sectors in the 35 files | 2,055 | 571 | 494 |

A sector is detailed entirely or not at all: no detailed sector also holds pitch-0.8 areas. Which sectors are detailed
follows the blueprint flag `NeedsDetailedPathfinding` of objects placed in or near them (partly measured).

[data: 3,120 sectors: (infantry, 0.025, 0.8, 3 x 3) 2,055, (infantry, 0.05, 0.2, 6 x 6) 571, (vehicle, 0.02, 1.0, 3 x 3) 494; radius 0.4 in all 2,626 infantry sectors, 2.5 in all 494 vehicle sectors]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:532-542]

## 4. The runtime mesh record

The rest of a sector block (`KynogonRuntimeMesh`) is a relocatable memory image: every offset in it counts from the
start of the record (the marker), and **0xFFFFFFFF means null**. The game's loader turns the offsets into pointers in
place. `hpdlayout` explains every byte of every record (no gap, no overlap).

### KYM-REC-1: Every byte is explained

Status: **verified**

A record consists exactly of: the header, the tile table, the tiles, the area objects they name (with their tables and
data), the tile table's end slot, the neighbour-link table with its lists and link objects, and the trailer. There are
no other bytes.

[code: rust/bc2-formats/src/hpdlayout.rs:196] [code: rust/bc2-formats/src/hpdlayout.rs:301]
[test: rust/bc2-formats/src/hpdlayout.rs::walks_a_record] [test: tests/test_f3_native.py::area_hpd]
[data: 7,944 of 7,944 runtime meshes of the base game and Napisal's files, KYNAPSE_SPEC 4.4]

### KYM-HDR-1: Record header

Status: **verified**

0x50 bytes:

| Offset | Type | Field |
|---|---|---|
| 0x00 | char[20] | `KynogonRuntimeMesh` and two zero bytes |
| 0x14 | u16 | version: 2 |
| 0x16 | u16 | flags: 0 in files (the loader sets bit 0) |
| 0x18 | u32 | 0 in files (the loader stores the record's address) |
| 0x1C | u32 | sector id (= the table's, HPD-TAB-3) |
| 0x20 | f32 | resolution (HPD-SET-1) |
| 0x24 | f32 | pitch, the sampling grid's spacing (HPD-SET-1) |
| 0x28 | 4 x f32 | the sector rectangle: min x, max x, min z, max z (KYM-HDR-2) |
| 0x38 | u32 | tiles across x |
| 0x3C | u32 | tiles across z |
| 0x40 | f32 | entity radius |
| 0x44 | u32 | offset of the tile table: always 0x50 |
| 0x48 | u32 | number of neighbour-link entries |
| 0x4C | u32 | offset of the neighbour-link table, 0xFFFFFFFF when there are none |

[code: rust/bc2-formats/src/kymesh.rs:362-373] [code: rust/bc2-formats/src/kymesh.rs:570-583] [code: rust/bc2-formats/src/kymesh.rs:664-666]
[test: rust/bc2-formats/src/kymesh.rs::reads_the_record] [test: rust/bc2-formats/src/kymesh.rs::writes_what_it_reads]
[data: version 2, flags 0, word 0x18 0 and tile table at 0x50 in 3,120 of 3,120 sectors; the 74 sectors without links have offset 0xFFFFFFFF]

### KYM-HDR-2: Sector rectangle

Status: **observed**

The rectangle is the sector's 100 m square extended by margins: the minimum side starts one cell of the **finest pitch
in the sector** early (-0.8 m in normal sectors, -0.2 m in sectors with detailed zones) except in **edge sectors** (on
the minimum x or z of the level's sector grid), which start at the square's edge; the maximum side extends by 1.1 x
pitch (infantry 0x0102: z -0.8 .. 100.88), except at the grid's maximum where DICE drops it. The rectangle's minimum is
the origin of the tile grid: the runtime files an area under the tile it looks in from this origin, so a wrong origin
misfiles a one-cell band along each tile line.

[data: 3,725 normal-sector axes at -0.8 m, 1,028 detailed-sector axes at -0.2 m, 369 edge-sector axes at 0.0 over 17 levels; the edge-sector rule matches 3,798 of 3,798 axes]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:470] [doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:713-727]

### KYM-TILE-1: Tile table and tiles

Status: **verified**

At 0x50, tiles x times tiles z + 1 u32 offsets. Entry *t* is tile (x = t / tilesZ, z = t mod tilesZ): column-major.
The last entry points at a 4-byte **end slot** the loader never reads (DICE's tool left uninitialised memory there;
a writer keeps it for exact copies and writes 0 otherwise).

A tile is a u32 item count *k*; when *k* > 0, *k* items of 12 bytes {u32 area offset, f32 min height, f32 max height}
and then a u32 offset of the end of the tile's data. An empty tile is just the count. Each area is named by exactly
one tile in DICE's files (the tile holding it); its item's heights equal the area's (KYM-AREA-3).

[code: rust/bc2-formats/src/kymesh.rs:301-325] [code: rust/bc2-formats/src/kymesh.rs:586-620]
[test: rust/bc2-formats/src/kymesh.rs::writes_what_it_reads] [test: tests/test_e3o_native.py::quick_kymesh]
[data: end slot zero in 442 of 3,120 sectors, other bytes in the rest]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:478-482]

### KYM-AREA-1: Area object

Status: **verified**

An area (a "mesh object": one walkable area inside one tile) has a 0x50-byte header, then its data:

| Offset | Type | Field |
|---|---|---|
| 0x00 | u32 | sector id (renumbered by the loader if the record's id changes) |
| 0x04 | u32 | tile x |
| 0x08 | u32 | tile z |
| 0x0C | u32 | the area's index within its tile (0, 1, 2 ...) |
| 0x10 | f32 | entity radius |
| 0x14 | f32 | pitch |
| 0x18 | f32 | max height |
| 0x1C | f32 | min height |
| 0x20 | u32, u32 | A: count, offset |
| 0x28 | u32, u32 | B (portals in the sector): count, offset |
| 0x30 | u32, u32 | C (border portals): count, offset |
| 0x38 | u32, u32 | D (step lines): count, offset of an index table |
| 0x40 | u32, u32 | E (outline): count, offset of an index table |
| 0x48 | u32, u32 | F (step cells): count, offset of a row table |

Offsets count from the area's start; an empty list's offset is 0xFFFFFFFF in DICE's files (the suite keeps whatever a
file holds there). The data follows the header in the order B, C, D, E, F.

[code: rust/bc2-formats/src/kymesh.rs:202-230] [code: rust/bc2-formats/src/kymesh.rs:452-540]
[test: rust/bc2-formats/src/kymesh.rs::reads_the_record] [test: bfbc2-auto-navkit/tests/test_pathdata_native.py::main]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:484-495] [doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:1069-1084]

### KYM-AREA-2: A records are unused

Status: **verified**

The A count is 0 in every file; the suite's reader refuses an area with A records (their layout, 0x20-byte records with
an offset at +0x18, is known only from the generator).

[code: rust/bc2-formats/src/kymesh.rs:226-228] [test: rust/bc2-formats/src/kymesh.rs::refuses_what_the_reference_raises_on]
[data: A count 0 in 171,354 of 171,354 area objects, KYNAPSE_SPEC 4.5]

### KYM-AREA-3: Area height range

Status: **observed**

An area's min height is its lowest ground and its max height its highest ground + 0.8 x EntityHeight (infantry +1.52
m, vehicle +3.2 m). Its outline vertices lie at ground + EntityHeight / 2 (infantry +0.95 m, vehicle +2.0 m). A range
reaching too low lets a lower floor's area claim points of the floor above.

[data: sp_004_b normal-pitch areas: median +0.000 / +1.520 against the outline heights minus 0.95; vehicle files: lowest ground .. highest + 3.2 m]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:686-692]

### KYM-AREA-4: Areas are single-layer and closed

Status: **verified**

An area never holds two surfaces of the same x, z column (a point-in-area test needs one surface per column), and its
boundary is closed: its outline segments (E), portals (B) and border portals (C), taken as segments, meet in pairs at
every end point. Coincident segment pairs (cracks between two unlinked cells of one area) occur and cancel in an
inside test.

[test: bfbc2-auto-navkit/tools/hpdcheck.py::check] [data: 0 open areas in DICE's 15 files checked by hpdcheck; 458 of 2,381 sp_004_b areas hold a repeated segment]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:296-342]

### KYM-B-1: Portals between areas of one sector (B)

Status: **verified**

0x20 bytes each: two 3-D points (a segment on the shared border, 24 bytes), at +0x18 the offset of the target area
(always another area of the same record) and at +0x1C **the target area's size in bytes**. Both sides of a portal are
stored (each area lists its portal to the other).

[code: rust/bc2-formats/src/kymesh.rs:242-249] [code: rust/bc2-formats/src/kymesh.rs:626-634]
[test: rust/bc2-formats/src/kymesh.rs::writes_what_it_reads]
[data: +0x1C equals the target's size in 37,132 of 37,132 portals; sp_004_b: 771 of 771 portals have one back]

### KYM-C-1: Border portals (C)

Status: **verified**

0x2C bytes each: two 3-D points (a segment on the sector's outer margin), +0x18 and +0x1C 0xFFFFFFFF in files (set by
the runtime when the neighbouring sector is loaded), then at +0x20 a unit direction out of the sector (+-x, +-z or a
diagonal). Some have direction "up" (0, 1, 0), where the outline meets the border.

[code: rust/bc2-formats/src/kymesh.rs:250-259] [code: rust/bc2-formats/src/kymesh.rs:470-480]
[test: rust/bc2-formats/src/kymesh.rs::reads_the_record]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:819-824]

### KYM-E-1: Outline (E) and step lines (D)

Status: **verified**

D and E share one layout: an index table of count + 1 u32 offsets (relative to the area), then the polylines' points
(12 bytes each, x, y, z) back to back; polyline *i* runs from entry *i* to entry *i* + 1. E is the area's outline:
segments on the pitch grid at (k + 0.5) x pitch, axis-aligned or exactly 45 degrees, the area on the right, including
closed outlines of holes. D lines are step edges inside the area (KYM-DF-1); each has 2 or more points (98 % exactly 2).

[code: rust/bc2-formats/src/kymesh.rs:260-273] [code: rust/bc2-formats/src/kymesh.rs:484-502]
[test: rust/bc2-formats/src/kymesh.rs::writes_what_it_reads] [test: bfbc2-auto-navkit/tools/hpdcheck.py::check]

### KYM-F-1: Step cells (F)

Status: **verified**

A table of count + 1 rows of 16 bytes {u32 offset of the polygon's points, 3 x f32 direction}, then the points. Row
*i*'s polygon runs to row *i* + 1's offset; the last row's offset is the data end and its direction (0, 0, 0) in
every file. Each polygon is closed (first point repeated) with a horizontal unit direction: in infantry files +-x or
+-z with zero components stored as -0.0; vehicle files also use the four diagonals.

[code: rust/bc2-formats/src/kymesh.rs:274-294] [code: rust/bc2-formats/src/kymesh.rs:506-521]
[test: bfbc2-auto-navkit/tools/hpdcheck.py::check] [test: rust/bc2-formats/src/kymesh.rs::writes_what_it_reads]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:951-963]

### KYM-DF-1: What D and F record

Status: **observed**

A side between two straight-neighbour cells of the same area whose ground heights differ by more than the link step
(0.6 m at pitch 0.8, on terrain and objects alike) is **broken**. A cell whose two sides along one axis are both broken
is an F cell of that axis; F cells form 8-connected polygons stored twice, once per direction (-axis then +axis), each
corner at the mean height of the cells on that side; per area all -x records, then +x, -z, +z. A broken side that is not
an F side is a D side, stored once per owning cell (two facing lines), its owner on the right, collinear runs merged,
vertices at the mean height of the owner's cells + 0.95 m. Outline never runs along a broken side. Between two
terrain cells DICE's D sides follow the terrain's slope across the side (above 0.77) rather than the centres' step,
while its F cells follow the steps (KYG-DF-1, round NKG3). At pitch 0.2 only terrain-terrain sides break (from about
0.17 m). Vehicle files do not follow the infantry rule (open).

The runtime uses D and E as obstacle edges and F with its heights in its straight-move test within an area; path
search itself (areas, portals, route graph) does not need them, so a file with no D or F routes.

[data: sp_004_b normal areas: 70 % of sides with D or F step 0.61 m or more; 14 of 199,324 sides without D or F do; F twins at the same place facing opposite ways in 926 of 934 ONS_MP_002 cells]
[exe: 0x1072A60 walks D (+0x38 / +0x3C), E (+0x40 / +0x44) and F (+0x48) of an area in its segment test; flag bits 2 = D, 1 = E, 4 = F; reached from game AI code via 0x58EF90]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:917-1014]

### KYM-DF-2: F polygons where DICE's data decides nothing (NavKit's generator)

Status: **verified**

DICE's F records never have these shapes. On the 18 DICE infantry files, every one of the 195,813 normal-pitch F
vertices is a cell corner, never a cell centre. Every one of the 44,380 F records is a simple loop, counter-clockwise
and closed. No two records of one direction in an area share a vertex, so DICE has no holes, no pinches and no
touching loops. NavKit's F groups (8-connected, with 45-degree bridges through the empty cells beside a diagonal
touch) can make both shapes, and the generator decides them by a rule of its own, never by an iteration order:

- **A loop that starts at a pinch.** A pinch is where two boundary edges leave the loop's start vertex: a second
  piece or a hole of the group. The loop leaves along the edge whose end vertex is least (x, then z). At a pinch on
  the way, it takes the sharpest right turn.
- **A vertex with no area cell on the record's side and no F cell touching it.** This is the centre of an empty cell
  where 45-degree bridges meet. It takes the mean height of the F cells nearest to it (half-unit |dx| + |dz|) that
  lie furthest toward the record's side. This follows DICE's convention that a record's corner heights come from
  the cells on its side (KYM-DF-1).

The rule decides only cases DICE never produces. Until round NK3 the Python generator left both to its sets'
iteration order. On the 15 regression levels the first never occurs; the second occurs at 17 vertices and changed
their heights by 0.01-0.69 m, 13 of them in the files (the other 4 lie in sectors a detailed zone replaces;
`rust/BENCHMARKS.md` "NK3").

[code: rust/bc2-navbuild/src/detail.rs:529-585] [code: rust/bc2-navbuild/src/detail.rs:653-715]
[test: rust/bc2-navbuild/src/detail_tests.rs::a_loop_starting_at_a_pinch_takes_the_least_edge]
[test: rust/bc2-navbuild/src/detail_tests.rs::nearest_f_cells_toward_the_side]
[data: 18 DICE infantry files: 195,813 of 195,813 normal-pitch F vertices at cell corners; 44,380 of 44,380 F records closed, counter-clockwise, no repeated vertex, no two records of one direction in an area sharing a vertex]
[data: NavKit's 15 regression builds, 27,193 detail calls: 0 loops starting at a pinch; 17 vertices with tied nearest F cells of different heights]

### KYM-LINK-1: Neighbour links

Status: **verified**

The link table holds 12-byte entries {u32 neighbour sector id, u32 count, u32 offset of a list}; a list holds *count*
u32 offsets of link objects. A link object is {u32 neighbour tile x, u32 neighbour tile z, u32 area index in that tile,
u32 count *m*} and *m* u32 offsets of this sector's C records (inside its areas) that lead there.

[code: rust/bc2-formats/src/kymesh.rs:327-358] [code: rust/bc2-formats/src/kymesh.rs:635-663]
[test: rust/bc2-formats/src/kymesh.rs::writes_what_it_reads]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:497-499] [doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:1083]

### KYM-REC-2: Trailer

Status: **verified**

The record's last 8 bytes are {u32 its own offset (record length - 8), u32 0}.

[code: rust/bc2-formats/src/kymesh.rs:667-670] [test: rust/bc2-formats/src/kymesh.rs::writes_what_it_reads]
[data: 3,120 of 3,120 sectors]

### KYM-ORD-1: DICE's layout order

Status: **verified**

DICE's tool (and the suite's writer) lays a record out as: the header and tile table; for each tile in table order the
tile, then every area it names for the first time; the 4-byte end slot; the neighbour-link table, then per entry its
list of link-object offsets followed by those link objects; the trailer. A record written this way from its parsed
form is byte-identical to DICE's.

[code: rust/bc2-formats/src/kymesh.rs:558-670] [test: rust/bc2-formats/src/kymesh.rs::writes_what_it_reads]
[test: bfbc2-auto-navkit/tests/test_pathdata_native.py::main]
[data: 7,944 of 7,944 meshes written back byte-identical (hpdtool.py meshtrip, base game and Napisal's files)]

## 5. What the runtime needs: parity targets

NavKit's parity measures (`navparity`, `navregress`, `ordercheck`, `navdf`) compare a NavKit build with DICE's file on
the same level: ground coverage and extra ground, area boundaries, portals, route-graph connectivity and islands, node
proximity to AI order positions, D / F precision and recall. The rules below are what the game needs beyond the byte
layout; the rest of DICE's generator choices (area division, midline cuts, node placement) affect only how well the
bots move.

### HPD-RT-1: Points resolve to near route nodes

Status: **observed**

A go-to request resolves each point to a route node: the runtime gathers nodes within R (15 m) of the point and takes
the first that passes its checks, including a straight-line test on the mesh from the point to the node. The request
then still failed in the second search stage when that node was 3.6-6.3 m from the point (DICE's: 0.2 m); a route node
near each AI order position fixed it. So route nodes must lie near the points the AI is ordered to.

[exe: 0x0105ED80 point -> route node; 0x010795E0 second search stage (code 2: no node reached); 0x0107F620 the go-to computation]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:582-595]

### HPD-RT-2: Path data describes the intact level

Status: **inferred**

Objects with `DisablePathfinding = PdtWhenAlive` (doors, gates, scripted walls) and dynamic props are absent from the
data, ladders are not in it, and soldiers are not obstacles; destruction is handled at run time by switching path data
off on destroyed objects (`PdtWhenDestroyed`).

[inferred: DICE's files route through PdtWhenAlive doors (sp_009, sp_003_b) and have islands reached only by ladder; the level settings DestructibleLocalPathfindingParts / MaxLocalPathfindingObjects point to run-time handling]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:356-361]

## Open questions

Round NKS turned this section's questions into tracked items, each answered with evidence or kept open with what would
settle it: `formats/kynapse.md` section 6 (OQ1-OQ9). Answered there: the header words and the resolution field
(KYN-FILE-1, KYN-TILE-1), the tile size (KYN-TILE-1), the link object's header (KYN-LINK-1), A records and the end
slot (KYRT-IGN-1, KYRT-LOAD-3). Still open:

- the lookup lists' tie-break (HPD-NODE-6; KYG-NODE-3 narrows it, KYRT-NODE-2 shows the runtime reads it);
- DICE's area division, the vehicle D / F rule and the detailed sectors (KYG-AREA-2, KYG-DF-2, KYG-DET-1);
- the ONS_MP_002 sector that resolved at offsets 0, 0 (HPD-USE-2; KYRT-SEC-1 gives the lookup);
- the generator's rectangle (KYG-RECT-1).
