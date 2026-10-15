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

## 4. `pathdata.hpd` (first survey)

All values little-endian.

| Offset | Content |
|---|---|
| 0x00 | `"Kynogon HPD\0"` (12 bytes) |
| 0x0C | u32 2 (version?) |
| 0x10 | u32 2 |
| 0x14 | **u32 sector count** (sp_009 infantry 9, sp_008_b vehicle 5, sp_005 infantry 269) |
| 0x18 | u32 1 |
| … | zeros, then u32 2, 0, 8, zeros … |
| ~0x8C | `24 c 1 0` (meaning open), then the **sector table** |

Sector table entry (13 u32; observed):

| Word | Meaning (provisional) |
|---|---|
| 0 | sector id = grid coordinates (`0x0101`, `0x0102`, `0x0103`, `0x0201` … → decimal 257, 258, 259, 513 = the generator's `sector_<n>` names) |
| 1–6 | six floats – sector bounds (order to be determined) |
| 7 | 0 |
| 8 | block size in bytes |
| 9 | block offset – running sum of the previous sizes |
| 10 | constant per file (`0x186A8` infantry, `0x186A9` vehicle in the samples) |
| 11 | constant per file (`0x82AC58`, `0xCC63D4`) – possibly a checksum or build id |
| 12 | next entry's id follows (entry stride to be confirmed) |

Sector block: graph / link records (`8000xxxx` words followed by small index pairs) precede a
`"KynogonRuntimeMesh"` record, which starts with: u16 0, u32 2, …, sector id (bytes `01 01`), then
floats – infantry `0.025, 0.8, -200.0, …` (0.8 = Pitch), vehicle `0.02, 1.0, -400.0, …` (1.0 =
Pitch) – then counts (3, 3 / 6, 6 …).

Open: exact header layout up to the table, table stride, graph record format, runtime mesh layout
(vertices, polygons, edges, heights), cross-sector links, hierarchical database.

## 5. Licence boundary

The original generator's map-building step (`Kaim::CMapBuilder`) requests a FLEXnet licence for the
feature "CMapBuilder" (registry `HKLM\SOFTWARE\FLEXlm License Manager`, licence path `.\*.lic`). This
is documented only as the boundary; NavKit does not use or bypass it and runs no Kynapse code.
