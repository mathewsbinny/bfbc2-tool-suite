# bfbc2-navigation-studio

The Navigation Studio's window was retired in round GR2 (2026-11-20): a level's navigation data, its colourings,
path test, anomaly and compare overlays and its edits are the map editor's AI workspace now (`bin\bfbc2-editor.exe
<level> --navmesh`; `bfbc2-map-inspector/EDITOR_ARCHITECTURE.md` 9.2).

What stays here is `overlay.py`, two headless exports as JSON:

```
python overlay.py <target level> --reference <source> [--candidate <pathdata.hpd>] [--out <file.json>]
python overlay.py <target level> --anomalies [--candidate <pathdata.hpd>] [--out <file.json>]
```

**`--reference`: another level's AI layer against this level's NavKit build** (read-only). A source is
`level:<lv>` (a level prepared in Project OPFOR BFBC2: campaign, Napisal's ONS levels, stock MP; its path data is the
level's installed original), `xbox:<lv>` (DICE's Xbox 360 Onslaught levels unpacked into `work\xbox_dlc`) or the path
of a level `.xml`. The AI layer read from it: HasAI, soldier spawners (PlayerType, Enabled, Team, SoldierSquadId),
AreaOrders (with AreaType - they carry no team or squad), patrol orders (their AreaGeometry waypoint chains),
objective objects (`mpobjective`, `communicationstation`) and the level's SectorOffsets ("none" when the level has no
such asset - never inferred). Per spawner, order, patrol waypoint and objective the export gives its position and
fields, whether NavKit's build has walkable ground there, the nearest same-floor route node within 15 m (the game's
gather radius) and whether it is on the main network. Default output: `work\navstudio\<level>_vs_<source>.json`.

**`--anomalies`: the anomalies of a NavKit build** against the level's own AI layer: islands under 25 route nodes,
soldier spawners whose nearest node is on one, AI orders and patrol waypoints with no node within 15 m on their
floor, sliver areas (under 5 cells, no portal or border portal), lonely route nodes (no other node within 12 m on the
same floor). Read the counts against DICE's own file (`--candidate <DICE's pathdata.hpd>`), not as absolutes: AreaOrder
positions are area volumes, not always walkable points. All 15 levels with DICE path data, NavKit's builds of
2026-10-22 (DICE / NavKit): spawns on small islands 147 / 72, orders without a node 768 / 686, sliver areas 77,996 /
62,708, lonely nodes 5,081 / 10,011, islands under 25 nodes 5,684 / 6,390. Default output:
`work\navstudio\<level>_anomalies.json`.

The data comes from `core/bfbc2core/navdata.py` (NavKit's byte-exact reader). The map editor reads the same layers
and lookups in Rust (`rust/bc2-editor/src/navdata.rs`), proven identical on recorded answers
(`rust/bc2-editor/tests/nav_parity.rs`).

Test: `tests/test_overlay_offline.py` (no game data).
