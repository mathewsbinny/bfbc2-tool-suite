# The Kynapse runtime: how BFBC2 loads and uses path data

How `BFBC2Game.exe`'s Kynapse runtime configures a world for each pathfinding type, loads `pathdata.hpd`, resolves
points to sectors, tiles, areas and route nodes, searches the route graph, follows and smooths a path, links sectors,
and handles the dynamic parts. It also lists what the runtime does not read, and the crashes bad data gives. The data's
meaning is `formats/kynapse.md`, its bytes `formats/hpd.md`. The AI's start and the go-to request's outcomes are in
`engine/ai.md` (AI-START-3, AI-KY-1 to AI-KY-4).

**Sources.** Static reverse engineering of the PC retail executable, build 795745 (image base 0x400000), done read-only
with a small capstone disassembler in rounds NKS and NKS-rest (which added the class names from the RTTI and
the reflected field tables). It builds on Project OPFOR BFBC2's earlier runtime probes and targeted
disassembly (`private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md` section 9). An `[exe: ...]` item names the instructions read. A rule is
**observed** when those instructions were read, and **inferred** when the conclusion goes beyond them. In the
addresses, `mesh` is the runtime-mesh record + 0x14, the base the runtime works from after relocation. So
`[mesh+0x0C]` is the record's +0x20.

## 1. World configuration

### KYRT-CFG-1: Sector size, overlap and offsets are world settings from the level

Status: **observed** · Variants: PC

The path-data set-up gives each world four settings by name. `SectorOffsetX` and `SectorOffsetZ` are integers from the
level's `Precomputed/<kind>/SectorOffsets` asset (AI-KY-1). `SectorSize` and `SectorOverlap` are floats read from +0x40
and +0x20 of each per-type object the set-up walks. That object is the level's `PathfindingType` (`InfantryData`,
`VehicleData`): its reflected fields put `OutputName` at +0x30 (the set-up builds the file path from it),
`SubgraphBoxSize` at +0x40 and **`DistEdgeMax` at +0x20**. So `SectorSize` is `SubgraphBoxSize`, 100 for DICE's types
(the sector edge every file's ids match), and **`SectorOverlap` is `DistEdgeMax`: 15 m infantry, 50 m vehicle**, the
same value the file's first node stores (HPD-NODE-1). Kynapse's parser stores the four at fixed slots of the path-data
manager: +0x874C offset x, +0x8750 offset z, +0x8758 size, +0x8754 overlap.

[exe: set-up 0x00F20C0E-0x00F20CD9 (size from [obj+0x40], overlap from [obj+0x20], objects walked at 0x00F20800-0x00F2080F, `lea edi, [esi+0x30]` the name at 0x00F20829); parser 0x01090873-0x0109092E (atoi for the offsets, atof for size and overlap)]
[exe: `PathfindingType`'s reflected field table 0x01C39DB8-0x01C39EBF, 24-byte entries {name, type, offset}: OutputName +0x30, EntityRadius +0x2C, MaxGraphHandles +0x3C, SubgraphsToLoad +0x44, SubgraphBoxSize +0x40, DiagramHalfWidth +0x1C, MaxGaps +0x18, DistEdgeMax +0x20, CanGoMaxEntityRadius +0x28, UseLocalPathfinding +0x48; names at 0x014752CC-0x01475364]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:464-471]

## 2. Loading a file

### KYRT-LOAD-1: The file loader

Status: **observed** · Variants: PC

A load reads the 16-byte start, compares the magic case-insensitively and requires header word 0 to be 2
(KYN-FILE-1). It reads words 1-35, then the sector table (count x 48 bytes from 0x9C), then word 35's dynamic-edge
records (8 bytes each), which follow the table. A sector's block is read from
`0x9C + 48 x count + 8 x word35 + offset` (one byte more and one shorter for a non-zero kind, KYN-SEC-1). The reusable
block buffers are sized from the largest block of each kind, rounded up to 8. The loader does not check the node
flags, the first node's DistEdgeMax value or the edge costs. A block's last 8 bytes are the mesh trailer
{u32 back offset, u32 0}. The mesh record starts `back offset` bytes before the trailer (KYM-REC-2), and is relocated
from there.

[exe: from-file entry 0x01092270, from-memory 0x01092160, state machine 0x01092340; magic 0x0108EC95; word 0 0x0108ECAE; table 0x01091130 / 0x010911A6; word-35 records 0x01091980-0x010919A2; block offset 0x01090193-0x010901D9; buffer sizes 0x0106C770 / 0x0106C7B0; trailer 0x0108E237-0x0108E286]

### KYRT-LOAD-2: Duplicate sectors

Status: **observed** · Variants: PC

With header word 3 = 1 (every DICE file), the loader compares each kind-0 sector's six bounds with every later kind-0
sector's. A later sector with all six equal is marked a duplicate of the earlier one, and at most four duplicates are
recorded per sector. With word 3 = 0, a sector whose first three bounds are the integer 99999 is treated specially
instead. Since DICE's bounds are the route-node box (KYN-SEC-2), two sectors with identical node boxes would be merged.

[exe: 0x0109171B `cmp [+0x3B4], 1`; float compares 0x01091791-0x01091807, duplicate record at +0x444 (16 bytes per sector); mode 0 test 0x010916C9]

### KYRT-LOAD-3: Relocating the runtime mesh

Status: **observed** · Variants: PC

The relocation compares the 19-byte marker and requires version 2. Another version zeroes the field and the mesh is
not used. If +0x18 (loaded) is non-zero and is not the record's own address, the relocation fails. If it equals the
address, the record is already relocated: the C links are cleared and the call returns. Otherwise it sets bit 0 and
clears bit 1 of +0x16, stores the record's address at +0x18, and turns into pointers (0xFFFFFFFF becomes null):

- the tile-table and link-table offsets;
- all tiles x tiles + 1 tile-table entries, the end slot's included;
- in each tile, each item's area offset and the tile's trailing word;
- in each area, the target offset at +0x18 of each A and B record and of each C record. C's +0x1C is left alone.
- each link entry's list offset, the list's object offsets, and each object's C offsets.

The walk through an area's A, B, C, D, E and F data is sequential from the area's header and ignores the area's own
offset fields. **The area's list offsets (+0x24 ... +0x4C) stay relative**, and every reader adds the area's address.
A writer must therefore lay each area's data contiguously in the order A, B, C, D, E, F (KYM-AREA-1, KYM-ORD-1). The
relocation also renumbers every area's and link entry's sector id when the record's id differs from its first area's.

[exe: relocation 0x010A41D0 (marker 0x010A41E4, version 0x010A428E, flags 0x010A42A2-0x010A42A7, loaded 0x010A42B2, tables 0x010A42AD-0x010A42E1, tile table 0x010A42E3-0x010A4310); tiles 0x010A3BB0; areas 0x010A3A70-0x010A3BAC (A 0x010A3A87, B 0x010A3AB0, C 0x010A3AE0); links 0x010A4380, 0x010A43C4, 0x010A43F8; renumbering 0x010A3C10; relative use 0x01073B32-0x01073B3A, 0x01071D21]

## 3. Resolving a point

### KYRT-SEC-1: Point to sector

Status: **observed** · Variants: PC

A point's sector is `x = floor(px / SectorSize) - SectorOffsetX` and `z = floor(pz / SectorSize) - SectorOffsetZ`.
The id is `x << 8 | z` when both are in 1..255, else 0 (no sector). A second function returns up to five ids for a
point: its own, then those of the point moved by +`SectorOverlap` in x, in z, and by -`SectorOverlap` in x and z. It
does not remove duplicates. With `SectorOverlap` = DistEdgeMax (KYRT-CFG-1), a point within 15 m (infantry) of a
sector line also looks in the sector across it, which is about the width of DICE's border-node band (at most 15.7 m
from the line, KYG-NODE-2). A file whose ids were built on other offsets than the asset's puts its data under ids the
lookup never computes for the points inside it (HPD-USE-2).

[exe: point to sector 0x0106D8D0 (floor via 0x00C5286C, `sub [+0x874C]` / `sub [+0x8750]`, range checks 0x0106D941-0x0106D960); five ids 0x0106D970 (0x0106D995-0x0106DA85)]

### KYRT-TILE-1: Point to tiles

Status: **observed** · Variants: PC

Within a sector's mesh, a point is first tested against the rectangle widened by one pitch on every side:
`min - pitch <= x < max + pitch`, the same for z. Its tile is then `trunc((p - rect min) x resolution)` on each axis,
clamped to 0 .. tiles - 1 (KYN-TILE-1). The same function also returns the tiles of the points one pitch either side,
at most four (tx, tz) pairs, so a point near a tile line is looked up in both tiles. A tile's index in the tile table
is `tx x tilesZ + tz`.

[exe: 0x01071BC0 (rectangle test 0x01071430; tiles 0x01071BDF-0x01071C48; the pitch neighbourhood 0x01071C4C-0x01071D02); table index `imul edx, [esi+0x28]; add edx, tz` 0x01073ED0]

### KYRT-AREA-1: Point in area

Status: **observed** · Variants: PC

To find a point's area, the runtime walks the loaded sector meshes. In each mesh it takes the point's tiles
(KYRT-TILE-1) and, in each tile, the items whose height pair holds the point (`item +4 <= y <= item +8`). Each such
item's area gets an inside test, and the first area that passes is the answer. The inside test counts boundary
crossings in x / z over three sets of segments: the outline E, the B portals and the C portals. A point on or very
near a segment is accepted at once; the band uses a constant 1.1 x pitch. So:

- an area's horizontal extent is its E + B + C boundary. A boundary that is not closed, or a duplicated segment, flips
  the parity (KYM-AREA-4);
- its vertical extent is the **tile item's** heights. The area header's +0x18 / +0x1C are not read here;
- A, D and F play no part in point location.

[exe: 0x0107ADD0 -> 0x010706D0 (cached) / 0x010707A0 (every mesh) -> 0x01073EA0 (item heights 0x01073EF4-0x01073F0F) -> inside test 0x01073B00 (E, B, C; pre-test 0x010739D0 with 1.1 at 0x0143BD08; parity 0x01073740)]
[inferred: the exact maths of 0x01073740 (on-segment band versus crossing) was not followed instruction by instruction]

### KYRT-NODE-1: Point to route node

Status: **observed** · Variants: PC

A go-to request checks its two points with the point query (KYRT-AREA-1) and snaps a failing point. It then resolves
each point to a route node. The resolver gathers the nodes within R of the point (R = the largest DistEdgeMax over the
query's sector graphs: the first node's value, HPD-NODE-1) and takes the first that passes its checks. Those are: in
the query's result set, edge and link checks, and a straight-line test on the mesh from the point to the node
(KYRT-MOVE-1). HPD-RT-1 and AI-KY-3 give the outcomes. The gather is a box query on the sorted lookup lists
(KYRT-NODE-2), and the candidates are tested in the order it returns them: the first that passes wins, not the
nearest.

[exe: resolver 0x00F095C0 -> point query 0x00F09360 / 0x0107ADD0, snap 0x00F01250; node lookup 0x01086CE0 -> gather 0x0105ED80 (R from [status+0x38])]
[exe: the candidates walked one by one from the gather's list: cursor [list+8] incremented at 0x0105F278, end [list+0xC] at 0x0105F27F, each candidate read `mov ebx, [edi+edx*4+0x18]` at 0x0105EF5A; no sort between the gather 0x0105EEDF and the walk]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:429-435]

### KYRT-NODE-2: The box query reads the lookup lists, in list order

Status: **observed** · Variants: PC

The gather (0x0105ED80) asks each loaded sector graph for the nodes inside the box point +- R on all three axes
(0x0105ED10 -> 0x0105E270). Per sector graph, the query:

1. skips the sector when the box misses the range between the first and the last entry of a list (the nodes with the
   smallest and the largest coordinate), on any axis;
2. binary-searches each of the three lists (x: field b low half, y: b high half, z: c low half; HPD-NODE-6) for the
   first and last entry inside the box on its axis;
3. takes the axis with the fewest entries in range (x, then y, then z on a tie);
4. walks that list's range in list order, keeps the nodes whose other two coordinates lie inside the box,
   skips non-owner border copies (KYRT-PATH-3), and keeps one copy of each border id (masked id, at most 32,768 per
   query).

So the order of the candidates is the order of the chosen list: ascending coordinate on the axis with the fewest
candidates, sector by sector in the loaded-sector chain. **Equal coordinates keep their list order**, so the lists'
tie order (KYG-NODE-3) decides which of two tied nodes the resolver tests first, and therefore which one a point gets
when both pass its checks. Membership does not depend on the ties (the binary search compares values).

[exe: box query 0x0105E270: list fields read as `movzx [node+0x1C]` / `[node+0x1E]` / `[node+0x20]` (0x0105E339-0x0105E457), first / last entry rejects 0x0105E34B-0x0105E483, binary searches 0x0105E504-0x0105E623 (three axes, 2-byte list fields 0x1C..0x20), fewest candidates 0x0105E650-0x0105E687, walk 0x0105E6E0-0x0105E7F5 (other axes `fcomi` 0x0105E722 / 0x0105E768, owner test 0x0105E786, border dedup table 0x01B594B0 up to 0x8000 at 0x0105E7C2); the graph is picked by a search for the value 2 in four 12-byte groups at 0x0105ED27, the shape of KYN-FILE-1's words 8-17]

## 4. Searching and following

### KYRT-PATH-1: Graph search costs

Status: **observed** · Variants: PC

The search stage traced (0x010795E0) expands a node's out-edges. It starts at the node's first-edge field - 1
(HPD-NODE-3) and reads each edge's {cost word, source index, target index}. For a static edge (bit 31 set,
KYN-EDGE-2), the step cost is **computed** by a cost object's virtual call from the two nodes' stored positions. The
child's g is the parent's g plus that value. The stored cost word is not used as the step cost in this stage. For a
dynamic edge the cost comes from another virtual call after the dynamic object is checked (KYRT-DYN-1).

The search is an A* on Kynapse's (`Kaim`) interfaces:

- **The cost object** (`[search+0xC]`) is a `Kaim::IConstraint`. Its +0xC method takes the two nodes and returns
  false (the step is refused) or true with the cost. The path finder holds a `CConstraintShortestPath` of its own as
  the default. Its cost is the **3-D distance between the two nodes' stored positions, in f32**: the same length
  KYN-EDGE-2's cost word stores, but computed again. BFBC2's AI creates a `CSphereConstraint` and a
  `CConstraintShortestPath` for its path objects. The sphere constraint refuses a node outside its sphere (centre at
  +0x8, radius at +0x14), and every step while its radius is 0; otherwise it costs the same distance.
- **The heuristic** (`[search+0x10]`) is a `Kaim::IHeuristic`. With none set, h is the 3-D distance from the node to
  the goal; `CHeuristicEuclidianDistance` (+0x8) computes the same. A heuristic that answers false gives h = -1. The
  reflected classes also include `CZeroHeuristic` and `CPathCostHeuristic`. So with the default objects, the step
  cost and h are both straight-line 3-D distances, and h is admissible.
- **Time slicing.** One call expands at most 100 nodes. It then saves its state (context, start, goal, cost object,
  heuristic) and returns 3; the next call with the same five resumes.

[exe: out-edge walk 0x0105C930 (first edge - 1 at 0x0105C9B2-0x0105C9C6); static test 0x01079B3B; cost call `[edi+0xC]` vtable +0xC with node positions 0x01079F13-0x01079F3A, g sum 0x01079F51-0x01079F63; dynamic cost vtable +0x2C 0x01079C61-0x01079CD1]
[exe: RTTI vtables `Kaim::CConstraintShortestPath` 0x014C2D28 (+0xC = 0x01059480: dx, dy, dz of the positions at node +4 / +8 / +0xC, sum of squares, `sqrt` 0x00C521BC, stored as f32), `Kaim::CSphereConstraint` 0x014C2D44 (+0xC = 0x01059970: radius at +0x14 against 0.0, equal: false at 0x01059985; else a squared-distance test against the radius, then `jmp 0x01059480`), `Kaim::CHeuristicEuclidianDistance` 0x014C2D78 (+0x8 = 0x01059210, the same distance); the path finder's own constraint written at +0x43C, 0x010803D1; the AI's pair created at 0x00F0F122-0x00F0F192 (+0x94 sphere, +0x98 shortest path)]
[exe: heuristic call 0x01079320 (`cmp [edi+0x10], 0` at 0x01079375, no object: 3-D distance 0x00EFDB10; vtable +0x8 at 0x010794FA-0x0107951B; false: -1.0 from 0x014D9420), called at 0x01079746 and 0x0107A25E; 100 expansions `cmp [esp+0x2c], 0x64` at 0x0107A5CE, state saved 0x0107A5F8-0x0107A614 (globals 0x01B9B088-0x01B9B098), resume test 0x0107969A-0x010796C5, return 3 at 0x0107A621]
[inferred: whether another stage reads the cost word's low bits was not traced]

### KYRT-PATH-2: Border nodes in the search

Status: **observed** · Variants: PC

The search and the result-set checks compare node ids with bit 31 masked, so a border node's copies in different
sectors are one node. Their flags slots join them at load (KYN-NODE-2). Only the owning copy carries the node's edges
(HPD-NODE-5). With the owner bit (bit 16 of field c) set on every copy, searches failed to expand in play tests;
KYRT-PATH-3 gives the instructions that read the bit.

[exe: masked compares 0x0105C066-0x0105C085, 0x0105C148, 0x01079A90-0x01079A96]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:566-570]

### KYRT-PATH-3: Where the owner bit is read

Status: **observed** · Variants: PC

The runtime reads the owner bit as the byte at node +0x22 (bit 16 of field c; 1 = a non-owner copy). Three readers:

- **The out-edge walk** (0x0105C930, its 36-byte-node part from 0x0105CAB5). For a node it visits every loaded copy
  in turn (the copies are chained through node +0x14) and collects **each copy's** out-edges, keeping one edge per
  target id (masked). An edge is dropped when its target is a non-owner copy and no other loaded copy of that node is
  an owner (0x0105C8C0). The source side is not tested: edges stored on a non-owner copy are used like the owner's.
- **The box query** (KYRT-NODE-2) never returns a non-owner copy.
- **A random-node pick** (0x0105C2A0) draws again while it lands on a non-owner copy.

So a border id whose copies are all non-owners (3,623 of DICE's 113,874, KYG-NODE-2) is never a point's node, and
edges into it are dropped: the graph cannot reach it. With the bit on every copy, every edge into a border node is
dropped, which is the play test's "searches do not expand". A border id with two or three owners (2,814) is one
candidate and keeps its edges. A writer sets the bit on every copy but one, and puts the node's edges on that copy
(HPD-NODE-5); edges a writer stores on a non-owner copy are not ignored.

[exe: target filter 0x0105C8C0 (`cmp byte [ebp+0x22], 1` at 0x0105C8CC; the copies walked through [node+0x14] 0x0105C8D3-0x0105C918, an owner copy found: false 0x0105C91A; none: true 0x0105C923), called from the edge walks 0x0105CB5B (36-byte nodes; `sete cl` keeps the edge when false), 0x0105CD36 and 0x0105D43D; per-copy edge collection 0x0105CAD0-0x0105CC26 (first edge [node+0x18], target dedup table 0x01B58CB0); box query 0x0105E786 (`cmp byte [ecx+0x22], 0`); random pick 0x0105C2E8-0x0105C307 (`rand` % count, retried on `cmp byte [eax+0x22], 1`)]
[inferred: the chain through node +0x14 is written at load (the file holds DistEdgeMax or 0 there, HPD-NODE-1); the write was not found]

### KYRT-MOVE-1: Straight moves, path following and smoothing

Status: **observed** · Variants: PC

The test "can an agent move straight from A to B inside this area" walks the area's D lines and E outline as obstacle
segments, then its F cells with the segment's end heights. A hit fails the move. Flag bits skip each list (2 D, 1 E,
4 F). Game AI code reaches the test through the path follower. The point-to-node resolver uses a straight-line test
of the same kind. So D and F shape which straight shortcuts a bot takes, and which node a point gets. Areas, portals
and the route graph decide whether a path exists (KYM-DF-1).

[exe: segment test 0x01072A60 (D +0x38 / +0x3C, E +0x40 / +0x44, F +0x48; segment against polyline 0x01071900), variant 0x01072D00; reached from 0x0058EF90 via 0x00F2D170, 0x00F2CF90, 0x01070E60, 0x010756A0, 0x01074B30]
[doc: bfbc2-auto-navkit/research/KYNAPSE_SPEC.md:917-935]

### KYRT-LINK-1: Joining neighbouring sectors

Status: **observed** · Variants: PC

When two neighbouring sector meshes are both loaded, each one's link entries for the other are resolved. For each
link object, the area is the neighbour's tile `[tile x x tilesZ + tile z]`, item `[index]`. The area's size is the
next item's area address minus this one's, or, for a tile's last item, the tile's trailing word minus it. Every C
record the object lists gets +0x18 = that area and +0x1C = its size. Unloading a sector clears the C records pointing
into it. So a tile's trailing word must be the end of its last area's data, as DICE's tool writes it (KYM-TILE-1).

[exe: 0x010A44D0 (called both ways from 0x01070974 / 0x01070980; 0x010A44F4-0x010A4554); unlink 0x010A3CB0 from 0x01070917]

## 5. Dynamic parts

### KYRT-DYN-1: Dynamic edges, and destruction

Status: **observed** · Variants: PC

The runtime supports dynamic route edges. An edge whose cost word has bit 31 clear names, in bits 16-30, an entry of
the file's word-35 table, registered at load. During a search such an edge is used only when its object says it is
traversable. DICE's files have no dynamic edges (word 35 = 0, every edge static). Destruction in BFBC2 therefore does
not go through them. Objects with `DisablePathfinding` and the level's `DestructibleLocalPathfindingParts` /
`MaxLocalPathfindingObjects` point to the world's local-pathfinding service (`LpfManager`, with `GapManager`,
KYRT-DYN-2), but the reflected field names have no code reference and the service was not traced. HPD-RT-2 (the
data describes the intact level) stays inferred.

[exe: registration 0x0108DAB0 -> 0x01089520 (no table: false); dynamic test 0x0105CE67 -> 0x0108EDA0 -> 0x0105CF20 (0x01087EC0 / 0x01087F80); services registered in 0x00F1B980 (0x00F1BCA3, 0x00F1BD59, 0x00F1C83E, 0x00F1C915); LpfManager profile strings used in 0x010C5E50]
[data: word 35 = 0 in 35 of 35 files; bit 31 set on 1,694,066 of 1,694,066 edges]

### KYRT-DYN-2: Local pathfinding: an obstacle layer beside the path data

Status: **inferred** · Variants: PC

BFBC2 configures Kynapse's local pathfinding (`Mpd::CLpfManager`, "LPF") and its gap manager (`CGapManager`) as world
services, next to the path data. The set-up names their parts: an obstacle layer (`CObstaclesLayer`, configured as
`LpfLayer` with `MaxObstacles`, `MaxMeshCount`, `MaxLocalMap` and `MaxProjectionsPerFrame`) read through an
`ObstaclesAccessor`; a pre-merger that joins obstacle outlines on a grid (`CLpfPreMerger_Grid`, `MaxGridWidth`,
`MaxPrecision` 0.1); the manager's two profiled steps, `AreaComputation` and `PreAggregateComputation`; a goal refiner
that wants a point both on the AI mesh and LPF-compatible (`CRefineGoal_NearestAiMeshAndLpfCompatible`); and the
go-to's dynamic avoidance through gaps (`Goto_GapDynamicAvoidance_v2`). The path-finding type carries `MaxGaps` and
`UseLocalPathfinding` (KYRT-CFG-1's field table: infantry true, vehicle false).

None of these names a write to the loaded sectors, the route graph or the dynamic-edge table (KYRT-DYN-1). So
destruction most likely reaches the AI as **obstacles in the LPF layer**, avoided locally around the agent, while the
path data keeps describing the intact level (HPD-RT-2). What exactly an object with `DisablePathfinding =
PdtWhenDestroyed` adds or removes when it is destroyed was not traced.

[exe: service and layer names 0x014A9360 (`CGapManager`), 0x014A9530 (`Mpd::CLpfManager`), 0x014A95C4 (`CObstaclesLayer`), 0x014A95A4 (`MaxObstacles`), 0x014A94E8 (`CLpfPreMerger_Grid`), 0x014A9800 (the LPF goal refiner), 0x014A9A48 (`Mpd::CGoto_GapDynamicAvoidance_v2`); the obstacle layer configured at 0x00F1F584-0x00F1F600 (and again 0x00F1FA47-0x00F1FADE); the managers registered 0x00F1BCA3 / 0x00F1BD59, their profile names `LpfManager::AreaComputation` / `::PreAggregateComputation` at 0x00F1C915 / 0x00F1C83E and in the service 0x010C69E4-0x010C69F5; `PdtWhenDestroyed` 0x0145C728 and `DestructibleLocalPathfindingParts` 0x01460610 without a code reference found]
[inferred: from the configuration's class and parameter names only; no LPF method was disassembled and no live probe was run (in-game tests are paused)]

## 6. What the runtime does not read

### KYRT-IGN-1: Unread fields, and A records

Status: **observed** · Variants: PC

Fields the loader and the functions traced do not read:

- header words 1, 4-7 and 34;
- the area header's heights +0x18 / +0x1C in point location (the tile item's pair is used, KYRT-AREA-1);
- the area's list offsets as such (the relocation walks the data in order, KYRT-LOAD-3);
- the value at the tile table's end slot (relocated, never dereferenced in the functions read);
- C's direction in the tests traced.

The sector table's date and time words are passed on to the block read's callback; whether it uses them is not known.
**A records are read**: a nearest-segment search walks A and B as segments. So A is not ignored. A writer must keep
the A count at 0, as every DICE file does (KYM-AREA-2), unless it knows what A holds.

[exe: no reads of words 1, 4-7, 34 found outside the swap pass 0x0108EAE0; date / time passed to 0x010C8A30 / 0x010C8A50; A and B walked in 0x01071D10 with 0x010714C0]
[inferred: "not read" covers the functions traced (loader, relocation, point location, search, links); the code was not swept exhaustively]

## 7. Crashes

### KYRT-CRASH-1: What bad path data does to the runtime

Status: **observed** · Variants: PC

Recorded faults and refusals caused by the data, with the rule that keeps a writer clear of each:

| What | Effect | Rule |
|---|---|---|
| sector nodes not sorted by id | go-to ids missing from the result set; pure virtual call at exit (0x401090) | HPD-NODE-2, AI-CRASH-4 |
| a first-edge field set on a node without out-edges | another node's edges read; crash | HPD-NODE-3 |
| the owner bit on every border copy | every edge into a border node dropped: searches do not expand | HPD-NODE-5, KYRT-PATH-3 |
| file ids not on the `SectorOffsets` asset's grid | requests in those sectors fail | KYRT-SEC-1, HPD-USE-2 |
| a mesh version other than 2, or a bad marker | that sector's mesh is not used | KYRT-LOAD-3 |
| magic or header word 0 wrong | the file does not load | KYRT-LOAD-1 |
| a cull tree that does not describe the level | crash in the AI's sphere query at load | AI-CRASH-1 |

[exe: version refusal 0x010A428E; magic / word 0 0x0108EC95-0x0108ECAE]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:450-461]

## Open questions, and the reverse engineering left

- Answered in round NKS-rest: the cost object and the heuristic (KYRT-PATH-1), where the owner bit is read
  (KYRT-PATH-3), where the lookup lists are read and that their order matters (KYRT-NODE-2), the `SectorOverlap`
  value (KYRT-CFG-1).
- Destruction (KYRT-DYN-2): what the LPF layer receives when an object with `DisablePathfinding = PdtWhenDestroyed` is
  destroyed. Next step: disassemble `CLpfManager`'s update and the obstacle layer's add / remove methods (their
  vtables through the RTTI-less class names above), or a probe on them while a building is destroyed in a test level
  (in-game tests are paused).
- Where node +0x14 (the copies' chain, KYRT-PATH-3) is written at load.
- Whether the block read's callback uses the table's date / time words (KYRT-IGN-1): find the writer of the callback
  pointer at 0x01521F40.
- The exact on-segment rule of the inside test (0x01073740, KYRT-AREA-1).
