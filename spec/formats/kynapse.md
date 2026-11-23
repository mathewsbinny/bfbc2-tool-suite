# Kynapse navigation data: the data model

What BFBC2's Kynapse path data means, beyond the byte layout of `formats/hpd.md`. That file says where each field is;
this one says what the fields are for: the worlds per pathfinding type, sectors and their table, the runtime mesh
(areas, portals B and C, outlines E, step lines D, step cells F, links), the route graph, detailed and coarse
sectors, vehicle and infantry files, units, coordinate frames and the resolution field. How the game uses the data is
`engine/kynapse_runtime.md`; how DICE's generator made it is `transforms/kynapse_generation.md`.

**Corpus.** "DICE's 35 files" are every `Output/win32/ai/levels/<level>/{infantry,vehicle}/pathdata.hpd` in Project
OPFOR BFBC2's game copy: 18 infantry files (the 13 campaign levels and the console Onslaught builds ONS_MP_002 / 004 /
005 / 005GR / 008) and 17 vehicle files (sp_009 has none). Together 3,120 sectors (2,055 coarse infantry, 571
detailed infantry, 494 vehicle), 188,988 areas and 596,069 route nodes. Counts come from round NKS's probe
(`rust/bc2-formats/examples/kynapse_probe.rs`, run on 2026-11-21; section 9 records the run) and the corpus tests
(`rust/bc2-formats/tests/kynapse_corpus.rs`, game tier).

**Names.** Two kinds of route node are named here. An *ordinary* node has id bit 31 clear. A *border* node has id bit
31 set (HPD-NODE-4). "Across" means horizontal (x, z) distance. A *coarse* sector is an infantry sector at pitch 0.8.
A *detailed* sector is an infantry sector at pitch 0.2. A *vehicle* sector is at pitch 1.0.

## 1. Worlds per pathfinding type

### KYN-WORLD-1: One file and one world per pathfinding type

Status: **observed**

A level's path data is one file per `PathfindingType`: infantry (`OutputName` Infantry) and vehicle (`OutputName`
Vehicle). The runtime loads each into its own Kynapse world configuration (AI-START-3, AI-KY-1). The two files share
one format (`formats/hpd.md`). They hold different ground and graphs:

| | infantry file | vehicle file |
|---|---|---|
| sectors per file | 9-380 | 5-78 |
| pitch of a sector | 0.8 (coarse) or 0.2 (detailed) | 1.0 |
| entity radius (mesh +0x40) | 0.4 | 2.5 |
| DistEdgeMax (first node's value) | 15 | 50 |
| tile size (KYN-TILE-1) | 40 m coarse, 20 m detailed | 50 m |
| F directions | axes only, each polygon stored twice (KYN-DF-2) | axes and diagonals, not twinned |
| sector offsets | the level's `Precomputed/Infantry/SectorOffsets` | its own, `Precomputed/Vehicle/SectorOffsets` |

Vehicle ground lies on infantry ground (98.8-99.7 % of its cells on the three levels measured). A vehicle file is a
second build of the same level, not a layer of the infantry file. There is no air or water type (HPD-USE-1).

[data: DICE's 35 files: 18 infantry (2,626 sectors: 2,055 at pitch 0.8, 571 at 0.2), 17 vehicle (494 sectors, all at pitch 1.0); radius 0.4 / 2.5 and DistEdgeMax 15 / 50 on every sector of its type]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:76-98]

## 2. The file and its sectors

### KYN-FILE-1: The header words the loader reads

Status: **verified**

The loader compares the magic case-insensitively. It requires word 0 to be 2. The words it uses are:

| Word | DICE's value | Use |
|---|---|---|
| 0 | 2 | must be 2, else the load fails |
| 2 | sector count | the sector table's length |
| 3 | 1 | the duplicate mode: with 1, a sector whose six bounds equal an earlier sector's is marked a duplicate of it (KYN-SEC-2) |
| 8, 11, 14, 17 | 2, 0, 0, 0 | searched for the value 2; what the result controls was not traced |
| 32 | 36 | the stride of a route-node record |
| 33 | 12 | the stride of an edge record |
| 35 | 0 | the number of 8-byte dynamic-edge records stored after the sector table (KYRT-DYN-1) |

No read of words 1, 4-7 or 34 was found beyond an endian-swap pass that does nothing on the PC. Words 8-31 have the
shape of eight {u32, u32, u32} groups. So the constant tail `36, 12, 1, 0` holds the record sizes (words 32 and 33),
an unread word, and the dynamic-record count. A writer keeps every word as DICE's (HPD-HDR-2). It must leave word 35 at
0 unless it writes dynamic-edge records.

[test: rust/bc2-formats/tests/kynapse_corpus.rs::header_words_and_sector_kinds]
[exe: magic `_stricmp` 0x0108EC95-0x0108ECA1; `cmp [esi+0x3A8], 2` at 0x0108ECAE; words 1-35 read at 0x0108ECD9; duplicate mode `cmp [+0x3B4], 1` at 0x0109171B; strides used at 0x0108DB9E, 0x0108DC54, 0x0108DD28; word 35 count at 0x010918D3, records read at 0x01091980-0x010919A2; swap pass 0x0108EAE0]
[data: word 0 = 2, word 3 = 1, words 32 / 33 = 36 / 12 and word 35 = 0 in 35 of 35 files]

### KYN-FILE-2: The header as the generator writes it, and the node record's parts

Status: **verified**

The Kynapse generator inside `BFBC2Game.exe` (`graphgen\kympdbhpdlinker.cpp`) still holds the code that writes
this header (round NKS-deep A). It writes, after the magic, from its linker object:

| Words | Written from | DICE's value | Meaning |
|---|---|---|---|
| 0, 1 | one field, written twice | 2, 2 | the file version |
| 2 | the sector count | *n* | |
| 3 | the mode | 1 | KYN-FILE-1's duplicate mode |
| 4-7 | four floats, written only in mode 0 (zeros in mode 1) | 0 | a box or origin used by mode 0 |
| 8-19 | a table of four {u32, u32, u32} entries from three arrays | {2, 0, 8}, then zeros | data descriptors: id, offset, size |
| 20-31 | a second table of four such entries | zeros | |
| 32, 33 | two ints | 36, 12 | the node and edge record sizes |
| 34 | an int | 1 | not read by the runtime (KYRT-IGN-1) |
| 35 | the path-objects data's count (the writer refuses to write without it: "PathObjects data not set to hpd linker.") | 0 | the dynamic-edge records (KYRT-DYN-1) |

The runtime's search of words 8, 11, 14 and 17 for the value 2 (KYN-FILE-1) looks for descriptor 2 in the first
table. The node record is a 28-byte node (id, position, the +16 slot, the +20 float, the first-edge field *a*)
followed by descriptor 2's 8 bytes (the lookup-list fields *b*, *c*, HPD-NODE-6). Word 32 is 28 + descriptor 2's
size. A writer keeps the words as DICE's (HPD-HDR-2).

A third-party description of the same file layout in another Kynapse game (Divinity II,
https://github.com/OpenDraconis/divinity2-lib/blob/main/engine/kynapse.md, opened 2026-11-22) names these parts
`HPDHEADER_v2` (version 2, a "project type" 1, the node size 36, the edge size 12, four floats zero in every file),
the table entry's kind word "level", and the node `CONCRETENODE` {id, position, `abstract`, `nextCopy`, `firstOut`}
with "the additional data of descriptor id 2 (8 bytes)". Every one of those agrees with DICE's files and with the
writer above, except one: that page calls the first node's +20 float "the slot's longest edge". In DICE's files it is
DistEdgeMax (15 or 50 in every sector, HPD-NODE-1), and 2,605 of 3,120 sectors hold a longer edge, so the claim does
not hold for BFBC2 and is not used.

The tile table's end slot (KYM-TILE-1) holds left-over memory of the generator. 83 sectors hold 0xBAADF00D, the fill
of the Windows debug heap, and others hold values such as 1.0, 0.4 and 0.8 as floats. So DICE's generator ran on a
debug heap, and the slot carries nothing the data needs.

[test: rust/bc2-formats/tests/kynapse_corpus.rs::header_tables_and_record_layout]
[exe: the HPD writer 0x011C5140: magic 0x014C7180, words 0 and 1 from linker +0x50, word 2 from +0x10, word 3 from +0x4C, words 4-7 floats +0x18 / +0x1C / +0x20 / +0x24 only when +0x4C = 0 (0x011C52B7), words 8-19 from the arrays at +0x28 / +0x2C / +0x30 and words 20-31 from +0x34 / +0x38 / +0x3C (four entries each, 0x011C54C8-0x011C55C8), words 32-34 from +0x40 / +0x44 / +0x48, word 35 the count at +0x0C of the path-objects data at +0x54 (error string 0x014C72A4); runtime search for descriptor 2 at 0x0108E689-0x0108E69E]
[data: words 0 = 1 = 2, 3 = 1, 4-7 zero, 8-10 = 2 / 0 / 8, 11-31 zero, 32 = 28 + 8, 34 = 1 in 35 of 35 files; the first node's +20 float equals DistEdgeMax in 3,120 of 3,120 sectors and is shorter than the sector's longest edge in 2,605; 83 end slots 0xBAADF00D]

### KYN-SEC-1: The sector table's kind word and the sector id range

Status: **verified**

The table entry's word at +28 (`formats/hpd.md`, HPD-TAB-1: "reserved") is the **sector kind**. The loader counts kind
0 and non-zero sectors apart and keeps them in two containers. For a non-zero kind it reads the block from one byte
later and one byte shorter. Every DICE sector is kind 0; a writer must write 0.

A sector id is `x << 8 | z` with x and z each in 1..255. The runtime's point-to-sector lookup gives no sector outside
that range (KYRT-SEC-1), so a level's sector grid (after its offsets) must fit 255 x 255 sectors starting at 1.

[test: rust/bc2-formats/tests/kynapse_corpus.rs::header_words_and_sector_kinds]
[exe: kind counted 0x01091272-0x010912CA; containers 0x0108FAD7-0x0108FB04; block offset +1 / size -1 for kind != 0 at 0x010901AA-0x010901CA; id range check 0x0106D941-0x0106D960]
[data: kind 0 in 3,120 of 3,120 sectors; ids within 1..255 on both axes in 35 of 35 files]

### KYN-SEC-2: The table's bounds are the box of the sector's route nodes

Status: **verified**

The six bounds of a sector table entry (min x, max x, min y, max y, min z, max z) are exactly the bounding box of the
positions of the sector's route nodes, border nodes included. They are not the mesh rectangle and not the areas'
extent. Every DICE sector has at least one route node, and no two sectors of a file have equal bounds. With duplicate
mode 1 (KYN-FILE-1) the loader marks a sector whose bounds equal an earlier sector's as a duplicate and does not load it
as its own, so a writer must not give two sectors the same box.

[test: rust/bc2-formats/tests/kynapse_corpus.rs::table_bounds_are_the_route_nodes_box]
[exe: duplicate test over the six bounds 0x01091791-0x01091807, mode 1 only]
[data: bounds equal the node box bit for bit in 3,120 of 3,120 sectors; 0 sectors without a node; 0 files with two equal boxes]

### KYN-SEC-3: Sector ids and the file's offsets

Status: **verified**

A sector's id is its 100 m grid square minus the file's sector offsets:
`x = floor(centre x / 100) - XOffset`, `z = floor(centre z / 100) - ZOffset`, the centre being the mesh rectangle's.
Every sector of a file agrees on one offset pair, which the level's `SectorOffsets` asset must repeat (HPD-USE-2). A
level's infantry and vehicle files can have different offsets: they do on ONS_MP_005 / 005GR, ONS_MP_008, sp_005_b,
sp_008 and sp_008_b.

[test: rust/bc2-formats/tests/kynapse_corpus.rs::one_sector_offset_per_file]
[data: one offset pair per file in 35 of 35 files (3,120 of 3,120 sectors agree); the vehicle pair differs from the infantry pair on 6 of 17 levels]

### KYN-SEC-4: Coarse and detailed sectors

Status: **observed**

An infantry file mixes coarse sectors (pitch 0.8, 3 x 3 tiles of 40 m) and detailed sectors (pitch 0.2, 6 x 6 tiles of
20 m). A sector is one or the other: every area of a sector has the header's pitch and radius. The share of detailed
sectors varies by level, from 4 of 9 (sp_009) to 23 of 27 (sp_008_b). The vehicle file has no detailed sectors. Which
sectors are detailed is a generator decision (KYG-DET-1).

[data: pitch and radius of every area equal its sector header's in 188,988 of 188,988 areas; detailed sectors per infantry file 4-55 (571 of 2,626)]

## 3. The runtime mesh

### KYN-TILE-1: The resolution field is one over the tile size

Status: **verified**

The mesh header's "resolution" (+0x20) is the reciprocal of the tile edge in metres: 0.025 = 1/40 (coarse), 0.05 =
1/20 (detailed) and 0.02 = 1/50 (vehicle). The runtime finds a point's tile as
`trunc((p - rect min) x resolution)` on each axis, clamped to the tile count (KYRT-TILE-1). The header has no other
tile-size field. The tile counts are `ceil((rect max - rect min) x resolution)`, so the last tile on each axis is short
(coarse: 21.68 m of 40; vehicle: 2.1 m of 50).

[test: rust/bc2-formats/tests/kynapse_corpus.rs::resolution_is_one_over_the_tile_size]
[exe: tile range 0x01071BC0: `fsub [mesh+0x14]` (rect min x), `fmul` by [mesh+0x0C] (resolution), `cvttss2si`, clamp to [mesh+0x24] - 1 (tiles x), same for z with [mesh+0x1C] / [mesh+0x28]; mesh base = record + 0x14]
[data: resolution = 1/40, 1/20, 1/50 in 2,055, 571 and 494 sectors; tile counts = ceil(span x resolution) in 3,120 of 3,120]

### KYN-TILE-2: An area lies in the tile that holds its cells

Status: **verified**

An area is filed under tile (tx, tz) when the centres of its cells c satisfy `floor((c - rect min) x resolution) = t`
on both axes. Its outline vertices (cell corners) therefore lie within the tile's range widened by half a pitch. The
rule with the tile edge taken as the rectangle's span over the tile count instead fails for 1.8 million vertices.

[test: rust/bc2-formats/tests/kynapse_corpus.rs::resolution_is_one_over_the_tile_size]
[data: 6,279,328 of 6,279,328 outline vertices inside their tile's range; with span / tiles as the edge, 1,799,317 outside]

### KYN-AREA-1: Areas, their tile items and their index

Status: **observed**

An area is one walkable piece of one tile (KYM-AREA-1). Its header names its sector, tile x, tile z and its index in
the tile's item list. Its pitch and radius are the sector's. Its tile item holds the same heights as its header:
item +4 = header min height (+0x1C) and item +8 = header max height (+0x18). The runtime's point test reads the tile
item's pair, not the header's (KYRT-AREA-1).

[data: sector, tile and index equal the naming tile's in 188,988 of 188,988 areas; tile item heights equal the header's in 188,988 of 188,988]

### KYN-B-1: Portals between areas (B) are mostly two-way

Status: **observed**

A B portal names an area of the same sector across a shared side (KYM-B-1). Over the whole corpus, 96.5 % of B portals
have a portal back from their target. KYM-B-1's "both sides stored" holds on sp_004_b (771 of 771) but not everywhere.
Where B portals lie relative to tiles is KYG-AREA-2.

[data: 373,053 of 386,568 B portals have a portal back to their area; 13,515 do not]

### KYN-C-1: Border portals (C) sit on the first cell-corner line outside the rectangle

Status: **observed**

A C portal with an axis direction lies on the first cell-corner line, at (k + 0.5) x pitch, beyond the mesh rectangle
on the side its direction points to. That is half a pitch outside the minimum side, or 0.4 pitch past the maximum side
(the maximum side is 1.1 pitch beyond the square, KYM-HDR-2). The directions are axis (+-x, +-z), diagonal, or "up"
(0, 1, 0):

| | axis | diagonal | up |
|---|---:|---:|---:|
| coarse | 63,974 | 52,504 | 58,352 |
| detailed | 31,767 | 34,196 | 10,284 |
| vehicle | 5,677 | 2,615 | 1,490 |

The runtime fills a C record's +0x18 / +0x1C with the neighbouring area's address and size when the neighbour sector
loads (KYRT-LINK-1). It does not read the direction in any test traced.

The "diagonal" and "up" directions are not kinds of geometry. Consecutive C records along one straight run on one
corner line carry up, (0.924, 0.383, -0.383), (0.707, 0.707, -0.707) and (1, 1, -0) in turn. The y component is often
not 0, and 41 % of the coarse "diagonal" records lie on an axis line. Coarse C records are axis-line segments (63,366 axis, 21,737
"diagonal", 36,932 "up") or slanted 45-degree corner cuts (104 / 30,743 / 21,387). What the vector holds is not decoded.
A C record whose middle lies in a diagonal neighbour's square: coarse 23,510 of 174,830, detailed 5,892 of 76,247; one
that crosses a square line along it: coarse 30,036, detailed 11,566.

[data: axis C portals on that line: coarse 63,272 of 63,974, detailed 28,781 of 31,767, vehicle 5,667 of 5,677; of the rest, 76 lie on that line of the opposite side and 3,622 elsewhere]

### KYN-LINK-1: A link object names a tile item of an adjacent sector

Status: **verified**

A neighbour-link entry names an adjacent sector: one of the eight around it, diagonal neighbours included (6,263 of
16,450 entries). Each of its link objects names a tile and an item index of that sector:
{tile x, tile z, item index in that tile, count, offsets of this sector's C records}. The item index is the position in
the tile's item list, which is also the area's own index (KYN-AREA-1). Every link object of DICE's files resolves to an
existing item. The tile is the mirror of the C record's own tile across the shared side in 41,753 of 55,008 objects.
The rest are diagonal neighbours, or neighbours with another pitch: 9,396 objects link a coarse sector with a detailed
one. So the open question in `formats/hpd.md` ("83 % match the mirrored tile") is settled: the header is the
neighbour's own tile and item, in that neighbour's tile grid. Two link entries in the corpus name a sector the file
does not hold. The runtime skips them.

[test: rust/bc2-formats/tests/kynapse_corpus.rs::link_objects_name_a_tile_item_of_an_adjacent_sector]
[exe: link resolution 0x010A44D0: tile = neighbour.tiles[x x tilesZ + z], area = tile item [index] (0x010A44F4-0x010A4554)]
[data: 55,008 of 55,008 link objects resolved; 16,450 entries, all adjacent; 9,396 cross-pitch objects; 2 entries naming an absent sector]

### KYN-DF-1: D and F by pathfinding type

Status: **observed**

Infantry and vehicle files carry D lines and F cells by different rules (KYM-DF-1 is the infantry rule):

| | D lines | of them 2 points | diagonal segments | F records | F diagonal |
|---|---:|---:|---:|---:|---:|
| coarse | 431,972 | 98.1 % | 1.6 % | 40,908 | 0 |
| detailed | 81,158 | 98.8 % | 1.0 % | 3,472 | 0 |
| vehicle | 8,597 | 81.2 % | 26.1 % | 2,162 | 897 |

[data: probe counts of every D line and F record of DICE's 35 files (section 9)]

### KYN-DF-2: Infantry F records come in twins, ordered by direction

Status: **verified**

In an infantry area, F records are ordered by direction: all -x, then +x, then -z, then +z. The -x and +x records
are equal in number, and so are the -z and +z records: each polygon is stored once per direction (KYM-DF-1). Each
direction is an axis with the other components stored as -0.0. Each polygon is closed. Vehicle files break all three
of these: 311 of their 881 areas with F have balanced counts.

[test: rust/bc2-formats/tests/kynapse_corpus.rs::infantry_f_records_by_direction_and_twinned]
[data: 10,964 of 10,964 infantry areas with F ordered and twinned; vehicle areas with F 881, twinned 311]

## 4. The route graph

### KYN-NODE-1: Node ids are unique in the file, in one number space

Status: **verified**

An ordinary node id names one node in the whole file, not only in its sector. A border id names one node shared by two
to four sectors. Its low 31 bits never equal an ordinary id: the two kinds are numbered from one counter. The ids are
sparse. The largest ordinary id + 1 is 599,172 for 355,039 ordinary ids over the corpus, and per sector the ordinary
ids are 0..k-1 in only 68 of 3,120 sectors. The runtime compares ids with bit 31 masked (KYRT-PATH-2).

[test: rust/bc2-formats/tests/kynapse_corpus.rs::node_ids_are_unique_in_the_file]
[data: 0 ordinary ids in two sectors and 0 border ids whose low bits equal an ordinary id, in 35 of 35 files; border ids 113,874: 104,448 in 2 sectors, 5,570 in 3, 3,856 in 4]

### KYN-NODE-2: The node's flags word is a link slot

Status: **observed**

The node's +16 word, 0x80000000 in every file (HPD-NODE-1), is a slot the runtime uses to join a border node's copies.
Bit 31 set means "not resolved". Otherwise the word holds a pointer to the twin. At load the runtime looks the id up
in a neighbouring sector's graph and writes the result there. So 0x80000000 means "unresolved, id 0". A writer
writes 0x80000000.

[exe: 0x0108DE60 / 0x0108DEE0 test the word's sign, mask 0x7FFFFFFF and binary-search 0x0108D5E0; results written back at 0x0108E2D0-0x0108E2E8; reader 0x0105C400]
[data: 0x80000000 on 596,069 of 596,069 nodes]

### KYN-EDGE-1: An edge reaches DistEdgeMax across, twice that at a border

Status: **verified**

A route edge between two ordinary nodes is at most DistEdgeMax long across (15 m infantry, 50 m vehicle). With a
border node at either end it is at most twice that. Height is not limited: in 3-D, 7,186 ordinary-to-ordinary edges
are longer than DistEdgeMax, all of them on slopes or between floors.

[test: rust/bc2-formats/tests/kynapse_corpus.rs::edge_reach_is_dist_edge_max_across]
[data: 1,694,066 edges: 0 ordinary pairs over DistEdgeMax across; 29,210 border-to-border and 916 ordinary-to-border edges over DistEdgeMax across, 0 over twice it]

### KYN-EDGE-2: The cost word: a static flag and a dynamic index

Status: **observed**

Bit 31 of an edge's first word marks a **static** edge. With it clear, bits 16-30 index a dynamic-edge object (the
word-35 table, KYRT-DYN-1), and the search checks that object before it uses the edge. Every DICE edge is static and
carries `0x80000000 | floor(L x 16)` (HPD-EDGE-2). The search that was traced takes its step costs from node positions
through a cost object, not from this word (KYRT-PATH-1). The length L is the f32 3-D distance between the stored f32
positions: computed that way, all 1,694,066 DICE edges match. HPD-EDGE-2 counted 8 off by one unit with another
rounding.

[exe: `test dword [edge], 0x80000000; jne` 0x01079B3B; dynamic index `shr 16; and 0x7FFF` 0x0105CE67 -> 0x0108EDA0; registration of non-static edges 0x0108DAB0 -> 0x01089520]
[data: bit 31 set on 1,694,066 of 1,694,066 edges; cost exact with an f32 length on all of them]

### KYN-EDGE-3: One-way edges and the edge order within a node

Status: **observed**

Edges are sorted by source node (HPD-EDGE-1). Within one source they follow neither target order (656,207 steps up,
563,604 down) nor length order. 13.0 % of edges have no edge back in their sector. Joined by node id over the file,
99.1 % of infantry and 95.1 % of vehicle edges have one: the one-way edges are mostly edges into non-owner border
copies, whose edge back lives in the owner's sector (KYG-EDGE-4). By their ends: 205,190 from an owner border copy
into a non-owner copy and 5,683 from an ordinary node into one (96.1 %); the other 8,627 (ordinary to ordinary
2,858, between owned border copies 3,819, into an owned copy 1,581, owned to ordinary 369) are the few edges without
a reverse by node id.

[data: 219,500 of 1,694,066 edges one-way; within a source, target ascending at 656,207 steps and descending at 563,604; one-way edges by their ends (probe `e.one_way_*`)]

### KYN-EDGE-4: One edge per source and target

Status: **verified**

No sector holds two edges with the same source and target.

[test: rust/bc2-formats/tests/kynapse_corpus.rs::one_edge_per_source_and_target]
[data: 0 repeated (source, target) pairs in 3,120 of 3,120 sectors]

## 5. Units and coordinate frames

### KYN-UNIT-1: Units and frames

Status: **observed**

Positions (nodes, outline, portal and step points, rectangles, bounds) are world space in metres, Y up. They are not
relative to a sector. Heights are world Y. An edge's cost unit is 1/16 m. Pitch, radius, DistEdgeMax and the tile
edge (1 / resolution) are metres. Sector ids count 100 m squares (`SectorSize`, KYRT-CFG-1) from the file's offsets
(KYN-SEC-3). Tile and cell indices count from the mesh rectangle's minimum, which lies one finest pitch before the
sector square (zero at a grid edge, KYM-HDR-2). Outline vertices lie on cell corners at (k + 0.5) x pitch in world
space: 99.7 % of coarse vertices, 99.6 % of detailed and 98.6 % of vehicle. The rest are 45-degree points and similar.

[data: outline vertices on the half-pitch grid: coarse 3,688,293 of 3,699,138, detailed 2,354,344 of 2,364,824, vehicle 212,389 of 215,366; 101,816 nodes lie outside their sector's rectangle, 101,734 of them border copies]

## 6. Open questions, tracked

`formats/hpd.md`'s open questions as of round SP1, each answered with evidence or kept open with what would settle it:

| # | Question | State |
|---|---|---|
| OQ1 | The tie-break of the three lookup lists (HPD-NODE-6) | **open, narrowed** (KYG-NODE-3): none of index ascending / descending, the other axes, a stable sort or MSVC `qsort` reproduces DICE's ties. Most likely the generator sorted unrounded (pre-f32) coordinates. The runtime's reader is now known (KYRT-NODE-2, round NKS-rest): a box query that walks the list with the fewest candidates in its order, and the resolver takes the first candidate that passes, so equal keys' order can change a point's node. Settles it: the generator's sort |
| OQ2 | Border ids without an owner copy, or with two (HPD-NODE-5) | **answered** (KYRT-PATH-3, round NKS-rest): of 113,874 border ids, 107,437 have one owner (103,458 of them in the sector whose square holds the node), 3,623 none, 2,738 two, 76 three. The runtime drops edges into a node none of whose loaded copies is an owner and never resolves a point to a non-owner copy, so DICE's 3,623 ownerless ids are unreachable; ids with two or three owners are one node and keep their edges. Which copy DICE made the owner (KYG-NODE-2) stays a generator question |
| OQ3 | The header words besides the count, and the resolution field | **answered**: KYN-FILE-1 (words 0, 3, 8-17, 32, 33, 35 read; 1, 4-7, 34 not), KYN-TILE-1 (resolution = 1 / tile size) |
| OQ4 | How the runtime gets the tile size | **answered**: KYN-TILE-1, KYRT-TILE-1 |
| OQ5 | The link object's header | **answered**: KYN-LINK-1 (the neighbour's tile and item; every object resolves) |
| OQ6 | DICE's area division: the midline cut's trigger; the vehicle D / F rule; detailed sectors beyond `NeedsDetailedPathfinding` | **open, narrowed** (rounds NKS-rest, NKS-rest2): the per-cut features were fitted over all 35 files and none triggers the cut; neither does the number of times the ground crosses the midline (a tendency: 0.6 % of single crossings cut, 21 % of five or more) nor what lies between the crossings (KYG-AREA-2). Detailed sectors: no level-data rule (best 92.2 %; KYG-DET-1), so designer zones that are not shipped: DICE's list is the record for its levels. The vehicle D / F rule needs a vehicle-shaped sample run in game; none is on disk (KYG-DF-2). Settles it: the cut groups against the raw samples; an in-game vehicle sample run |
| OQ7 | Whether the runtime needs a file's ids to match `SectorOffsets`; why one ONS_MP_002 sector resolved at 0, 0 | **answered in part**: the point-to-sector lookup is `floor(x / 100) - offset` and the id must be in 1..255 (KYRT-SEC-1), so ids that do not match the asset's offsets name the wrong sector. With offsets 0, 0 on ONS_MP_002, the sectors that still resolved were reached through `SectorOverlap`'s extra ids or by a coincident id: **open**. Settles it: a runtime probe of 0x0106D970's ids at the failing points |
| OQ8 | The generator's rectangle (ground stops inside the sampled extent) | **answered** (KYG-RECT-1, round NKS-rest2, verified on the 13 campaign levels): the box of the level's soldier and vehicle spawns grown by 25 m; 45 of 52 infantry sides lie 0.11-0.91 m inside it and none passes it. Open: the Onslaught files, which the prepared partitions' spawns do not bound |
| OQ9 | A records, and whether the end slot matters | **answered**: A records are read by a nearest-segment search with B (KYRT-IGN-1), so a writer must keep their count 0; the end slot is relocated like a pointer but its value is not read (KYRT-LOAD-3); a tile's trailing word *is* read (KYRT-LINK-1) |

## 7. NavKit against this model (probe on NavKit's 15 regression builds, 2026-11-20 builds; rows updated by round NKG1)

The probe of section 9 run on `work\navkit\regress\<level>.hpd` (infantry only), against DICE's infantry files of the
same 15 levels:

| Rule | DICE | NavKit | Gap |
|---|---|---|---|
| KYN-SEC-2 table bounds | node box, 100 % | the node box since NKG1 (2,489 of 2,494; 5 without a node or with an earlier sector's box keep the rectangle) | none |
| KYN-TILE-1 / 2 | holds | holds (0 vertices outside) | none |
| KYN-C-1 directions | axis, "diagonal" and "up" values (not geometric) | axis only (85,098 since NKG1's cut runs) | the values are not decoded and not read in any test traced; not imitated |
| KYN-B-1 two-way | 96.5 % | 100 % | DICE keeps some one-way portals |
| KYN-NODE-1 ids | file-unique, sparse | file-unique, sparse | none |
| KYN-EDGE-1 reach | holds | holds | none |
| KYN-EDGE-3 one-way edges | 13.0 % (the border convention, KYG-EDGE-4) | 0 % | NavKit keeps border edges on non-owner copies too (KYG-NODE-2: its one row needs them) |
| KYN-LINK-1 diagonal-neighbour links | 5,682 of 14,562 entries (same 15 levels) | 5,922 of 14,904 since NKG1 (0 before) | DICE cuts its C records by its own tracing |
| KYN-EDGE-4 edges per (source, target) | one | one since NKG1 (6,916 duplicate pairs before) | none |
| border ids in 3-4 sectors | 9,426 | 288 | corner sharing far rarer |
| KYN-DF-2 | holds | holds | none |

The counts are section 9's.

## 8. How the rules here relate to `formats/hpd.md`

KYN-SEC-1 names HPD-TAB-1's reserved word. KYN-SEC-2 says what HPD-TAB-1's bounds are. KYN-TILE-1 answers the open
question under KYM-HDR-1. KYN-B-1 qualifies KYM-B-1 over the whole corpus. KYN-LINK-1 settles KYM-LINK-1's header.
KYN-EDGE-2 refines HPD-EDGE-2's rounding. KYN-DF-1 / 2 extend KYM-F-1 and KYM-DF-1 to every file.

## 9. The probe run (2026-11-21)

`cargo run --release -p bc2-formats --example kynapse_probe` on DICE's 35 files (2.4 s), and with NavKit's 15 regression
builds named on the line. Selected totals (DICE / NavKit):

| Measure | DICE (35 files) | NavKit (15 builds) |
|---|---:|---:|
| sectors (coarse / detailed / vehicle) | 2,055 / 571 / 494 | 1,784 / 710 / - |
| areas (coarse / detailed / vehicle) | 118,937 / 51,461 / 18,590 | 85,099 / 129,219 / - |
| route nodes (coarse / detailed / vehicle) | 360,694 / 216,101 / 19,274 | 249,059 / 423,734 / - |
| edges, one-way | 1,694,066, 219,500 | 1,900,962, 0 (6,916 duplicates) |
| nodes on the cell-centre grid | 22,375 (3.8 %) | 672,793 (100 %) |
| B portals on a tile line / midline / elsewhere, coarse | 118,108 / 16,159 / 11,401 | 75,436 / 1,748 / 64,370 |
| same, detailed | 123,484 / 550 / 23,573 | 155,194 / 2,404 / 237,972 |
| same, vehicle | 22,748 / 1,450 / 69,095 | - |
| border-to-border edges | 425,545 | 90,754 |
| D lines with 3+ points (coarse) | 8,148 | 396 |
| lookup lists equal to a stable sort by index (x / y / z sectors) | 1,225 / 2,328 / 1,089 of 3,120 | 2,480 / 2,487 / 2,470 of 2,494 |
| table bounds equal to the node box | 3,120 of 3,120 | 0 of 2,494 |

The DICE infantry files of the same 15 levels hold 502 detailed sectors; NavKit's builds hold 710.
