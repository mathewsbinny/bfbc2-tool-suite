# BC2 NavKit

Standalone tooling to create AI navigation data (`pathdata.hpd`) for Battlefield: Bad Company 2
levels – including multiplayer maps that never shipped with any – so the game's own AI can move on
them. Part of Project OPFOR, kept self-contained so it can be published separately for other
modders.

## Why

BC2's AI uses Autodesk Kynapse. The game ships the Kynapse *runtime* (which loads `pathdata.hpd`)
under DICE's runtime licence, but the *generator* that DICE used to create that data is a separately
licensed developer tool: the copy that is still inside the game executable stops with a FLEXnet
licence error. NavKit replaces it with independent code.

## Rules (clean-room approach)

1. **Research** (`research\`): the Kynapse map builder in BFBC2Game.exe and the `.hpd` files are
   reverse engineered for interoperability. Results are written up as a *specification* – file
   formats, data structures, units, what each build stage produces. Licence checks are only located
   and documented as the boundary; they are not analysed in order to defeat them and nothing in this
   project bypasses them.
2. **Implementation** (`src\`): written from the specification with our own code and open-source
   libraries (e.g. recastnavigation, zlib licence). No Kynapse code is copied or translated
   line-by-line, no Kynapse binaries are shipped, and the tool never runs Kynapse code.
3. Game data (levels, collision meshes) always comes from the user's own installation.
4. Geometry is sampled through the **game engine's own physics queries** (DICE code, via Project
   OPFOR's `[dev] navsample` option) or read from the level's own data files – never through Kynapse.
   Observations of the original generator are limited to its boundary with DICE's code (the point
   queries it makes, the files it writes) and its output data.
5. The private folder `research\kynapse mapbuilder research\` is git-ignored and never published.

## Layout

```
research\            specification (KYNAPSE_SPEC.md) and images (img\)
tools\               NavKit tools (Python 3.13 + numpy / matplotlib)
  hpdtool.py         pathdata.hpd: info / sector / roundtrip / coverage / meshtrip
  kymesh.py          per-sector runtime mesh: parse_mesh / write_mesh (byte-exact)
  hpdview.py         render a sector to PNG (outlines, portals, graph)
  navsample.py       read in-game collision samples; compare with a pathdata.hpd
  hkpack.py          Havok 5.5 binary packfile reader (the level's *_Havok.res collision assets)
  levelobjects.py    object placements, pathfinding flags, forbidden-object triangles of a level
  navwalk.py         walkability rules + scoring against existing path data
  navbuild.py        the generator: samples + level objects -> pathdata.hpd
src\, tests\         (reserved for the generator proper and its tests)
```

## Plan and status (2026-10-15)

1. **`.hpd` format – done.** Header, sector table, per-sector graph and runtime mesh fully decoded
   (spec §4); `hpdtool.py` parses every campaign file.
2. **Byte-exact rewrite – done.** `hpdtool.py roundtrip` / `meshtrip` re-emit every shipped file and
   all 7,944 sector meshes identically.
3. **Builder behaviour – done.** Spec §3: stages, the sampling grid, area cuts, graph rules,
   auto-seeding, the walkability rules (§3.7), structures and doors (§3.8), and the rules found on the
   other campaign levels (§3.9).
4. **Generator – runs on all 13 campaign levels (`tools
avbuild.py`).**
   - Full pipeline: walkable surfaces from two in-game sampling runs (normal and high capsule) plus the
     level's objects (`levelobjects.py`), 100 m sectors (level `SectorOffsets`), 3 × 3 tiles of 50
     samples, single-layer 8-connected areas bisected above 20 m, outlines with 45° bridges, portals (B),
     border portals (C), neighbour links, route network (area, portal and shared border nodes; walkable
     edges up to 15 m; groups joined through string-pulled waypoints; no lone nodes), written as
     `pathdata.hpd`. Seeds: sector perimeter lines and soldier spawn points.
   - sp_01's file **loads in game** and the AI walks on it (play test 2026-10-15).
   - Accuracy against DICE's files (`ordercheck.py`: AI order / patrol positions on the areas, and the
     pairwise route check), 2026-10-15:

| Level | DICE-covered order positions also on NavKit's areas | Pairwise route check | Main route network (NavKit / DICE) |
|---|---|---|---|
| sp_01 | 618 / 633 (97.6 %) | 89.5 % | 639 / 607 |
| sp_02 | 241 / 260 (92.7 %) | 84.2 % | 313 / 362 |
| sp_03 | 343 / 353 (97.2 %) | 92.4 % | 374 / 232 |
| sp_04 | 500 / 526 (95.1 %) | 89.5 % | 582 / 593 |
| sp_003_b | 600 / 635 (94.5 %) | 87.3 % | 686 / 712 |
| sp_004_b | 169 / 181 (93.4 %) | 86.4 % | 185 / 192 |
| sp_005 | 304 / 309 (98.4 %) | 97.2 % | 308 / 312 |
| sp_005_b | 671 / 683 (98.2 %) | 94.8 % | 722 / 728 |
| sp_006 | 258 / 270 (95.6 %) | 92.7 % | 280 / 263 |
| sp_007 | 367 / 388 (94.6 %) | 88.6 % | 427 / 443 |
| sp_008 | 278 / 289 (96.2 %) | 83.4 % | 44 / 16 |
| sp_008_b | 110 / 113 (97.3 %) | 79.3 % | 107 / 58 |
| sp_009 | 68 / 72 (94.4 %) | 20.1 % | – (plane interior; measure unsuited) |
| **All 13** | **4,527 / 4,712 (96.1 %)** | | |

   - The pairwise check compares route-network layout; DICE's own network has gaps (spec §3.8), so it
     is a guide, not ground truth. Open leads: sp_008_b (sloped bridge foundation / fence passage DICE leaves
     out), sp_009 (plane aisles narrower than the capsule; DICE connects them), sp_03 / sp_008 (DICE network gaps).
   - Not generated yet: D lines, F cells, "up" border portals, diagonal border portals, detailed zones
     (pitch 0.2), vehicle data, ladder links (planned for MP bots, optional).
   - Next: play test of the current sp_01 build against DICE's file; then MP maps (self-check, no DICE
     data); later a standalone mode that reads Havok collision directly instead of sampling in game.

Credits: see Project OPFOR's `CREDITS.md`. Kynapse is a trademark of Autodesk; Battlefield: Bad
Company 2 is © Electronic Arts / DICE.
