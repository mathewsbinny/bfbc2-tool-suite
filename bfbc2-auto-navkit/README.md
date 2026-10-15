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

## Layout

```
research\            specification and notes (KYNAPSE_SPEC.md, raw notes)
src\                 NavKit sources (tools and libraries)
tests\               format round-trip tests against the user's own game files
```

## Plan

1. `.hpd` format: header, sector table, per-sector runtime mesh, graphs, cross-sector links –
   verified by a parser that reads all 25 campaign files and Napisal's files without loss.
2. Runtime expectations: coordinate system and units, sector grid and offsets
   (`PathdataSectorOffsets`), infantry vs vehicle data, what the game checks on load.
3. Builder behaviour: sectoring, seeding, walkable-surface extraction (collision queries), graph
   building and linking – as a behavioural description.
4. Writer: re-emit a shipped `.hpd` from the parsed model byte-for-byte (proves the format is fully
   understood).
5. Generator: level collision geometry → walkable mesh (recastnavigation) → Kynapse-format sectors
   and graphs → `pathdata.hpd`; validated in game on a campaign level against the shipped data, then
   used for multiplayer maps.

Credits: see Project OPFOR's `CREDITS.md`. Kynapse is a trademark of Autodesk; Battlefield: Bad
Company 2 is © Electronic Arts / DICE.
