# Kynapse navigation data in Battlefield: Bad Company 2 – specification (work in progress)

Interoperability specification written from reverse engineering (see `..\README.md` for the
clean-room rules). Sources: the user's own game files (`Output\win32\ai\levels\<level>\{infantry,
vehicle}\pathdata.hpd`, 25 campaign files; byte-identical on Xbox 360), Napisal's MP files, and the
behaviour of the generator code in BFBC2Game.exe build 795745 (Project OPFOR
`research\BFBC2_DECOMP_RESEARCH.md` §9). Status: 2026-10-14, first survey.

## 1. Where the data is used

- Runtime: the level's `PathfindingTypes` (`AI/Pathfinding/Infantry`, `…/Vehicles`) select the
  data; the runtime world (Kynapse mode "Minimal") loads `…/<OutputName>/PathData.hpd`
  (`OutputName` from `PathfindingType`: "Infantry", "Vehicle"). Sector origin per level:
  `Levels/<lvl>/Precomputed/{Infantry,vehicle}/SectorOffsets.dbx` (`XOffset`, `ZOffset`).
- Coordinates: game world space, Y up (e.g. SP_009 spawns at y ≈ 480–585).

## 2. Build settings (inputs)

Shipped as `PathfindingSettings` / `PathfindingZoneSettings` / `PathfindingType` (values in
`BFBC2_DECOMP_RESEARCH.md` §9). Infantry: StepMax 0.28, StepLength 0.36, HoleMax 0.28, slope 40°,
Pitch 0.8, GraphAccuracy 3, NbSectors 32, EdgeRadius 0.41, EntityHeight 1.9, entity radius 0.4,
swept-sphere collision, detailed zones at Pitch 0.2. Vehicle: StepMax 0.4, HoleMax 0.75, slope
25–40°, Pitch 1, GraphAccuracy 15, NbSectors 64, EdgeRadius 2.51, EntityHeight 4, radius 2.5,
cylinder collision.

## 3. Build pipeline (behaviour of the original generator)

Stages: Dividing sectors → Generating seeds → Generating maps (per sector) → Linking meshes →
Linking graphs → Building hierarchical database. Intermediate files per sector id `<n>`
(decimal): `sector_<n>.autoseed`, `.aim`, `.udg`, `.pdl`, `.fdl`, `.fsg`; plus `Sectors.xml`
(sector definitions) and `BuildChainWorld.xml` (configuration). Level geometry is sampled from the
loaded physics world through point, ray, capsule (radius/height) and box queries.

## 4. `pathdata.hpd`

All values little-endian. **Verified** by `tools\hpdtool.py roundtrip`: all 74 `.hpd` files of the
base game and Napisal's install are parsed and written back byte for byte (2026-10-14).

### 4.1 File layout

| Offset | Size | Content |
|---|---|---|
| 0x00 | 12 | `"Kynogon HPD\0"` |
| 0x0C | 36 × u32 | header words: `2, 2, <sector count>, 1, 0,0,0,0, 2, 0, 8, 0 … 0, 36, 12, 1, 0` – identical in every file except the count |
| 0x9C | count × 48 | sector table |
| 0x9C + count×48 | Σ sizes | sector blocks, contiguous, in table order; they end exactly at the end of the file |

### 4.2 Sector table entry (48 bytes: `<I6f5I`)

| Field | Meaning |
|---|---|
| u32 id | sector grid coordinates `0xXXYY` (0x0101, 0x0102, 0x0201 … = decimal 257, 258, 513 – the original generator's `sector_<n>` file names) |
| 6 × f32 | bounds **minX, maxX, minY, maxY, minZ, maxZ** (world space, Y up) |
| u32 | 0 in every file |
| u32 size | block size in bytes |
| u32 offset | block offset from the start of the block area (running sum of the previous sizes) |
| u32 date | build date `YYMMDD` with **zero-based month**: campaign files `100007`–`100012` = January 2010; Napisal's Onslaught files `100330`/`100404`/`100407` = April–May 2010 (built by DICE before the June 2010 Onslaught release) |
| u32 time | build time `HHMMSScc` (e.g. `13305600` = 13:30:56.00) |

### 4.3 Sector block

| Part | Layout |
|---|---|
| counts | u32 node count, u32 edge count |
| nodes | node count × 36 bytes `<I3fIf3I>` – see 4.3.1 |
| edges | edge count × 12 bytes `<3I>`: cost, source node index, target node index – see 4.3.1 |
| runtime mesh | the rest of the block, starting with `"KynogonRuntimeMesh"` |

Sizes: SP_005 infantry has 269 sectors, 69 095 nodes, 199 572 edges and 18.4 MB of runtime mesh.

#### 4.3.1 High-level graph (verified statistically over all 2 776 campaign sectors)

| Node field | Meaning | Evidence |
|---|---|---|
| u32 id | node id, unique per sector; **high bit = border node shared with a neighbouring sector** | every high-bit node's id appears in a neighbouring sector at exactly the same position (84 688 / 84 688) |
| 3 × f32 | world position | inside the sector bounds |
| u32 flags | always `0x80000000` | 537 707 / 537 707 |
| f32 value | **DistEdgeMax** on the first node of each sector (infantry 15, vehicle 50 – the `PathfindingType` value), 0 elsewhere | 534 931 zeros, 2 391 × 15, 385 × 50 |
| u32 a | index of the node's first outgoing edge **+ 1** | 2 776 / 2 776 sectors |
| u32 b | low u16: k-th entry of the node indices **sorted by x**; high u16: sorted by **y** (spatial lookup lists stored interleaved, one entry per node slot) | 93 % / 98.7 % exact, the rest are ties |
| u32 c | low u16: k-th entry of the node indices **sorted by z**; **bit 16 set on border nodes** | 92.8 % exact (ties); bit 16 on exactly the 30 351 border nodes of the sample |

Edges: **sorted by source node**; cost = `0x80000000 | floor(3-D distance × 16)` (1 521 636 of
1 521 644 edges exact; the rest differ by float rounding); pairs usually exist in both directions.
Open for exactness: the tie-break order of the three sorted lists.

### 4.4 Runtime mesh record – structure (complete)

**Verified:** `tools\hpdtool.py coverage` explains every byte of all 7 944 runtime meshes in the base
game and Napisal's files (no gaps, no overlaps). The record is a relocatable memory image: all
offsets are relative to the start of the record (the marker), and **-1 means null**. The game's
loader turns offsets into pointers in place (the relocation walk defines which fields are offsets).

**Header (0x50 bytes):**

| Off | Type | Content |
|---|---|---|
| 0x00 | char[20] | `"KynogonRuntimeMesh"` + 2 zero bytes |
| 0x14 | u16 | version 2 |
| 0x16 | u16 | flags (0 in files; the loader sets bit 0) |
| 0x18 | u32 | 0 in files (the loader stores the record address) |
| 0x1C | u32 | sector id |
| 0x20 | f32 | resolution: 0.02 vehicle, 0.025 infantry, 0.05 detailed zone |
| 0x24 | f32 | **Pitch** (1.0 vehicle, 0.8 infantry, 0.2 detailed zone) |
| 0x28 | 4 × f32 | sector grid cell **minX, maxX, minZ, maxZ** – cells are 100 m; the far sides extend by 1.1 × pitch, inner near sides start one pitch early (e.g. infantry 0x0102: z −0.8 … 100.88) |
| 0x38 | u32 | tiles across X (3; 6 in detailed zones) |
| 0x3C | u32 | tiles across Z (3; 6) |
| 0x40 | f32 | entity radius (2.5 vehicle, 0.4 infantry) |
| 0x44 | u32 | offset of the tile table (always 0x50) |
| 0x48 | u32 | number of neighbour-link entries |
| 0x4C | u32 | offset of the neighbour-link table (−1 if none) |

**Tile table** at 0x50: tilesX × tilesZ + 1 offsets. The last entry points at a 4-byte slot that the
loader never reads (DICE's tool left uninitialised memory there – keep for exact copies, write 0).

**Tile:** u32 item count n; if n > 0: n × {u32 object offset, f32, f32 (min/max height of the
object)} followed by u32 end offset (end of this tile's data). An empty tile is just the count.

**Mesh object** (referenced from tiles): 0x50-byte header – u32 sector id (renumbered by the loader
if the record's id changes), …, f32 entity radius (+0x10), f32 pitch (+0x14), f32 max height
(+0x18), f32 min height (+0x1C), then pairs {u32 count, u32 offset (relative to the object, −1 if
none)} at +0x20 (A), +0x28 (B), +0x30 (C), +0x38 (D), +0x40 (E), +0x48 (F). Data follows the
header contiguously:
- A: count × 0x20-byte records (offset field at +0x18)
- B: count × 0x20-byte records – two 3-D points (a segment) and an offset at +0x18
- C: count × 0x2C-byte records (offset field at +0x18)
- D, E: an index table of count + 1 u32 (offsets relative to the object) then variable-length data
  (E holds point lists – polylines)
- F: (count + 1) × 16-byte table then variable-length data
Semantics of A–F: in progress.

**Neighbour links:** table of 12-byte entries {u32 neighbour sector id, u32 count, u32 offset of a
list}; each list holds `count` offsets of link objects; a link object has u32 count at +0x0C followed
by that many u32 offsets at +0x10.

**Trailer:** the last 8 bytes are {u32 own offset, u32 0}.

### 4.5 Semantics (from statistics over all files and plots made with `tools\hpdview.py`)

![SP_02 infantry, sector 0x0104](img/sp02_infantry_0104.png)

- **Mesh object = one walkable area inside one tile.** Object header +0x04/+0x08 = the tile's
  x/z index in the sector (0–2, or 0–5 in detailed zones), +0x0C = 0 (possibly a layer index),
  +0x10 entity radius, +0x14 pitch, +0x18/+0x1C max/min height of the area.
- **A:** never used (count 0 in all 171 354 objects).
- **E: the area outline** – line segments (each list holds 2 points = 24 bytes in the samples) on the
  pitch grid, giving stair-stepped contours, including closed outlines of holes (obstacles). The
  original generator samples the ground on a regular grid with spacing = pitch and traces contours.
- **Areas are a decomposition of the walkable ground** into pieces of roughly 20 m (split along tile
  boundaries and further straight cut lines); black outlines in the plot enclose the non-walkable
  parts (obstacles), the open space is walkable.

![SP_02 infantry, sector 0x0705](img/sp02_infantry_0705.png)

- **B: portals between areas of the same sector** – a segment (two 3-D points) on the shared
  border; +0x18 = offset of the area object on the other side (always another object of the same
  record); **+0x1C = size in bytes of that target object** (37 132 / 37 132).
- **C: portals on the sector border** – a segment on the sector's outer edge (on the 1.1 × pitch
  margin), +0x18 / +0x1C null in files (set at runtime when the neighbouring sector is loaded),
  then a unit **direction vector** (+0x20) pointing out of the sector (±x, ±z, diagonals); some have
  direction "up" (0, 1, 0) – these sit where the outline meets the border, meaning open.
- **D:** short polylines (mostly 2 points, up to 6) at obstacle edges; present in 21 % of areas.
- **F: special grid cells** – closed polygons (5–8 points) of pitch-sized cells with a height per
  corner; each row holds a horizontal direction (±x, ±z, diagonals); at obstacle edges, present in
  6 % of areas – likely ledges / height breaks with their direction.
- D and F are detail features: a generator can start without them (counts 0) and still produce
  valid data.
- **Graph:** see 4.3.1.

Open: exact meaning of D, F and the "up" C portals; object +0x0C.

## 5. Licence boundary

The original generator's map-building step (`Kaim::CMapBuilder`) requests a FLEXnet licence for the
feature "CMapBuilder" (registry `HKLM\SOFTWARE\FLEXlm License Manager`, licence path `.\*.lic`). This
is documented only as the boundary; NavKit does not use or bypass it and runs no Kynapse code.
