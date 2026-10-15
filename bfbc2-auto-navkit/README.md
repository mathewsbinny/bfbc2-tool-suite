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
src\, tests\         (reserved for the generator proper and its tests)
```

## Plan and status (2026-10-14)

1. **`.hpd` format – done.** Header, sector table, per-sector graph and runtime mesh fully decoded
   (spec §4); `hpdtool.py` parses every campaign file.
2. **Byte-exact rewrite – done.** `hpdtool.py roundtrip` / `meshtrip` re-emit every shipped file and
   all 7,944 sector meshes identically.
3. **Builder behaviour – done.** Spec §3: stages, the sampling grid, area cuts, graph rules,
   auto-seeding on sector perimeters, and the walkability rules (§3.7).
4. **Generator – in progress.**
   - Walkable surfaces: done for two sp_01 test blocks (97.9 % agreement within 0.8 m in a base
     area, 95.7 % on steep terrain).
   - Next: outlines (contours halfway between samples, axis-aligned + 45°), tiles and 25-cell area
     cuts, portals, the graph, then `pathdata.hpd` via `kymesh.write_mesh` / `hpdtool.write`;
     compare with DICE's data, validate in game on a campaign level, then multiplayer maps.
   - Later: a standalone mode that reads Havok collision directly instead of sampling in game.

Credits: see Project OPFOR's `CREDITS.md`. Kynapse is a trademark of Autodesk; Battlefield: Bad
Company 2 is © Electronic Arts / DICE.
