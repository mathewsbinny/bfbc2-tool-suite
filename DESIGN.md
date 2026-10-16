# BFBC2 suite: design

Five tools for building and debugging AI on Battlefield: Bad Company 2 maps. They share one core: level
loading, a 3D view, picking and the game's data formats. Each tool is a mode on top of that core. This
document fixes what each tool does, what they share, the file formats they write, and the order they are
built in. It is a draft for review, and it changes as the tools are built.

## 1. The tools

| Tool | For | In short |
|---|---|---|
| **bfbc2-map-inspector** | any BC2 modder | Browse a level in 3D: geometry, collision, spawns, objectives, destructibles, every placed entity. Click anything to see where it comes from and its properties. |
| **bfbc2-navigation-studio** | any BC2 modder, Project OPFOR | View and edit the AI's navigation data (Kynapse `pathdata.hpd`, made by NavKit). Force ground walkable or unwalkable, link areas and routes, inspect disconnected regions, annotate traversal links, test paths. Export a finished file or a side file. |
| **bfbc2-tactical-map-authoring-tool** | Project OPFOR bots | A layer above the navigation data: objectives, cover candidates, firing positions, rally points, vehicle approaches, squad routes, strategic regions. Generated as candidates, corrected by hand. |
| **bfbc2-bot-ai-debugger** | Project OPFOR bots | Live view of the AI while the game runs: positions, orders, targets, path requests, tactical states, line of sight, threats, squads. |
| **bfbc2-automated-map-and-bot-test-harness** | Project OPFOR | Repeatable scenarios across maps, with results saved per build so changes can be compared: reachability, path tests, bot traversal, stuck detection, squad coordination, vehicle use. |
| **bfbc2-asset-pipeline** (in the core) | any BC2 modder | Batch extraction into common formats: geometry and collision to OBJ/glTF, entities and transforms, spawns and objectives to JSON, textures and materials later, dependency reports. It is the same readers the viewer uses. |
| **bfbc2-diff-workbench** | research | Binary and asset diffing: ImHex pattern files for BC2 formats, plus a report of which structures are understood and what changed, with reproducible extraction logs and hashes. |
| **bfbc2-ai-behavior-simulator** | Project OPFOR bots | Runs OPFOR's bot decision code (behaviour trees or state machines, threat scoring, squad orders, objective priorities) on simplified maps, without the game. It finds logic flaws cheaply, though not problems with controls, physics or animation. |
| **bfbc2-asset-builder** (asset builder & validator, in the OPFOR repo) | Project OPFOR | Turns editor outputs into versioned runtime packages for the standalone client, with schema and dependency checks and a report of missing or incompatible data. |

## 2. Rules

- **Game data comes from the user's own install** and is read where it is. Nothing EA-owned is shipped:
  no geometry, no level data, and no DICE path data.
- **NavKit stays clean-room.** We learn only from DICE's output files and the generator's boundary with
  DICE code. The licensed Kynapse generator is never run or bypassed.
- **Edits never count toward DICE parity.** NavKit's parity score measures the generator alone. The
  studio's edits are for maps without DICE data, and for places shown to be beyond any rule (hand edits,
  geometry missing from the shipped game).
- **Everything the game reads stays in the game's formats.** Path data is Kynapse `pathdata.hpd`, written
  by NavKit's writer. Anything the format cannot carry goes into side files that OPFOR reads: ladder and
  jump links, tactical annotations.
- **No network traffic.** The live debugger connects to OPFOR on the same PC (127.0.0.1), which OPFOR's
  network guard allows.

## 3. The shared core (`core/`)

### 3.1 Data layer

| Source | How it is read | Status |
|---|---|---|
| `.fbrb` bundles | FbrbTool unpack, cached per bundle date | works |
| binary `.dbx` (entities, transforms, properties) | a Python reader written from `bfbc2-dbx-tools/FORMAT.md` (no exe calls, fast) | to write; the format is fully specified |
| object collision (`*_Havok.res`) | NavKit's `hkpack.py` | works for the shapes NavKit uses; other shape types to add |
| static model groups (instance transforms) | `*_HavokAsset` partitions | works in NavKit |
| terrain heights | `Terrains/<level>/<level>.Heightfield-000.res` | header read: extent (-1024..1024 m on both axes), height range (0..1000 m), a 2048 grid, then about 4.3 M 16-bit heights; to decode fully |
| water | `<level>.Water.res`, `.Water.Mesh.res` | to research |
| navigation data | NavKit's `hpdtool.py` and `kymesh.py` (byte-exact reader and writer) | works |
| the level's AI orders, spawns, objectives | level partition entities (as NavKit's `ordercheck` and `levelobjects` read them) | works |

### 3.2 Scene model

A `Level` holds:

- **entities:** GUID, type, name, partition, transform, blueprint, and the properties from the DBX tree;
- **meshes:** collision triangles per entity and per static-group member, terrain and water;
- **navigation layers:** areas, outlines, portals, border portals, route graph, islands, detailed zones;
- **annotation layers:** from the tactical tool.

Every drawable item keeps a link to its source (file, partition, instance GUID, field path), so picking
can always say where something comes from.

### 3.3 View and interaction

- **Camera:** orbit, pan and zoom around a point, plus a free-fly mode (WASD and mouse), with a 360° view
  and bookmarked views per level.
- **Picking:** ray against a bounding-volume hierarchy of triangles, then entity bounds and navigation
  geometry. Clicking selects; hovering shows a tooltip.
- **Layers:** each can be shown or hidden, with its own colour scheme (areas by island, by sector, or by
  difference against a reference file) and transparency.
- **Selection and editing:** box, polygon prism (a drawn outline with a height range) and single item;
  undo and redo; a command log (every edit as a reproducible command, like DbxEdit's edit scripts).
- **Panels:** properties of the selection, search (GUID, type, name, value), layers, edit history,
  messages.

### 3.4 Stack

- Python 3.10+.
- `moderngl` with `glfw` for the GPU view: millions of triangles at interactive rates.
- Dear ImGui (`imgui-bundle`, MIT licence) for panels.
- `numpy`.

NavKit's Python code (reader, writer, sector build, checks) is used directly. That is the main reason for
Python: "force walkable, then rebuild the affected sectors" runs in-process, with no port of the
generator. Every library is under a permissive licence (MIT, zlib, BSD).

## 4. Tool scopes

### 4.1 bfbc2-map-inspector (first)

- **Load:** a level from the game copy, by name.
- **Draw:**
  - terrain, collision meshes (static groups and placed objects) and water;
  - spawn points (per team, human or AI);
  - objectives (Rush M-COMs, Conquest flags, Onslaught sectors);
  - destructible objects, vehicles and vehicle spawns;
  - AI orders and patrol routes (the level's own AreaOrder and PatrolOrder data).
- **Click anything** to see its type, GUID, partition, blueprint, transform and full property tree, with
  references you can follow to other entities.
- **Search, filter by type, measure** distances and heights.

### 4.2 bfbc2-navigation-studio

- **Generate:** run NavKit on a level, with its sampling settings, from the studio.
- **View:**
  - areas coloured by island, sector or pitch; outlines, portals, border portals and the route graph;
  - detailed zones;
  - a difference view against a reference file (DICE's, or an earlier build), from NavKit's `navdiff`.
- **Edit:** each edit is recorded as an operation (§5.1).
  - force a region walkable or unwalkable, then rebuild the affected sectors;
  - add or remove seeds, to keep or drop an island;
  - link two areas, add or remove route edges;
  - annotate traversal links (ladders, jumps, vaults, drops);
  - mark regions as AI-only or vehicle-only.
- **Inspect:** disconnected regions (islands sorted by size, with the nearest connection to the main
  network), unreachable spawns and objectives.
- **Path test:** pick two points and see the route over the route graph and areas, its cost, or why
  there is none (which island each point is on, and where they are closest).
- **Output:**
  - a finished `pathdata.hpd` (written by NavKit's writer and checked by reading it back), for your own
    installs and for maps without DICE data;
  - or a side file (§5.1), applied by `navkit apply` (§5.2) when a level is built or installed. This is
    the shareable form, and the only form for edits to DICE's files.

### 4.3 bfbc2-tactical-map-authoring-tool (when OPFOR's bot layer takes shape)

- **Layers:**
  - objectives and their approaches;
  - cover candidates (positions with protection from one or more directions);
  - firing positions (with fields of view);
  - rally points, vehicle approach routes, squad routes;
  - strategic regions (sectors of a map with a role: base, front, flank).
- **Generators propose candidates** from geometry and the navigation data: cover from collision near
  walkable ground, sight lines by ray casts, approaches by path search. People correct them.
- **Output:** a per-level annotation file (§5.3) that OPFOR's bot layer reads.

### 4.4 bfbc2-bot-ai-debugger (as soon as AI runs in OPFOR; first for the game's own AI)

- **OPFOR side:** a telemetry module in OPFOR's DLL streams AI state over 127.0.0.1 (§5.4), a few times a
  second: soldier positions and orientation, team, current order and goal, target, path request and
  route, tactical state, perceived threats, squad membership. It starts with what the game's own AI
  exposes, then adds OPFOR's bot layer as it is built.
- **Suite side:** the same 3D view, with the live soldiers drawn over the level and navigation data:
  - click a soldier for its state;
  - its current route, line of sight and threat lines;
  - a timeline to scrub back through the last minutes;
  - recording to a file for later.

### 4.5 bfbc2-automated-map-and-bot-test-harness (grows from now on)

- **Offline tests** (no game):
  - spawn-to-objective reachability for every team and spawn;
  - unreachable-region detection;
  - path-test batteries;
  - NavKit parity (`navparity`).
- **In-game tests** (through OPFOR's `testrun.bat` and the debugger's telemetry):
  - a level loads and reaches the match;
  - bot traversal (bots reach given points within a time);
  - stuck detection (no progress for N seconds);
  - squad coordination and vehicle use.
- **Results:** saved per named run, as NavKit's parity runs are, and compared between runs to show
  whether a change improved things.

### 4.6 bfbc2-asset-pipeline (in the core; geometry and entities in M1)

- **Command line:** `bfbc2-extract <level> --geometry obj|gltf --entities json --deps report.json`, and
  the same for any bundle.
- **One code path:** it uses the core's data layer, so whatever the viewer can show, the pipeline can
  export.
- **Dependency report:** the partitions and resources a level or bundle needs, and where each is found
  (which bundle), with missing ones flagged. This doubles as the game-wide search index ("where is X
  defined, who uses it").
- **Textures, materials and render meshes:** later, when the inspector needs them. Existing community
  converters are called as external tools where their licence does not allow reusing their code.
- **Every run writes a log:** inputs with SHA-1, tool versions, outputs with SHA-1.

### 4.7 bfbc2-diff-workbench (alongside research, from M2)

- **ImHex pattern files** (`patterns/*.hexpat`) for BC2's formats, so a file can be viewed as structure
  and two files diffed by field in ImHex: `pathdata.hpd`, `.fbrb`, the binary DBX header and tree,
  terrain heightfields, Havok packfiles, then others as they are researched.
- **`bfbc2-coverage <file>`:** which byte ranges of a file our readers understand, and which are still
  unknown. It tracks research progress per format.
- **`bfbc2-bindiff <a> <b>`:** for known formats, changed fields by structure, using LevelDiff, `navdiff`
  and DbxEdit's parser; for unknown ones, changed byte ranges with their offsets.
- **Extraction logs and hashes**, so any finding can be reproduced from the same inputs.

### 4.8 bfbc2-ai-behavior-simulator (with OPFOR's bot layer)

- **The rule this brings forward:** OPFOR's bot decision logic is written as an engine-independent
  library. It reaches the world only through an interface: positions, line of sight, paths, objective
  states, squad messages. OPFOR's DLL connects that interface to the game; the simulator connects it to a
  simplified map. Both run the same decision code, so a flaw found in simulation is a flaw in the bots.
- **The simplified map** comes from NavKit's navigation data (route graph, areas), the tactical
  annotations, and line of sight from collision geometry.
- **Scenarios:** teams, spawns and objectives from the level; scripted or random enemy behaviour; many
  runs faster than real time; results into the test harness.
- **Out of scope:** controls, physics, animation, weapon ballistics. Those stay with in-game tests.

### 4.9 bfbc2-asset-builder: asset builder & validator (in the Project OPFOR repo)

- **Inputs:** NavKit output and side files, tactical annotations, level ports and recipes (such as the
  console Onslaught builds), menu patches.
- **Output:** versioned runtime packages for OPFOR's standalone client: bundles registered in
  `package.mft`, `bundleManifest` and the startup bundle; side files in place; backups kept for clean
  uninstall.
- **Checks before packaging:** schemas of the side files, `DbxEdit check` on every level, references
  resolved across bundles, and NavKit parity for campaign levels.
- **Report:** missing, incompatible or outdated data.
- **It absorbs** the import scripts and the bundle manager (OPFOR backlog item 8). The installer builds
  on it.

## 5. File formats

### 5.1 Navigation side file: `<level>.navtune.json`

```json
{
  "format": "bfbc2-navtune", "version": 1,
  "level": "Levels/MP_005", "base": {"kind": "navkit", "sha1": "..."},
  "operations": [
    {"op": "walkable",   "region": {"polygon": [[x, z], ...], "y": [low, high]}, "value": true},
    {"op": "unwalkable", "region": {"box": [x0, y0, z0, x1, y1, z1]}},
    {"op": "seed",       "at": [x, y, z]},
    {"op": "unseed",     "at": [x, y, z]},
    {"op": "link-areas", "a": [x, y, z], "b": [x, y, z]},
    {"op": "route-edge", "a": [x, y, z], "b": [x, y, z], "action": "add"},
    {"op": "traversal",  "kind": "ladder", "bottom": [x, y, z], "top": [x, y, z], "two-way": true},
    {"op": "note",       "at": [x, y, z], "text": "..."}
  ]
}
```

Operations are recorded by position, not by area index or node id, so they still apply after the level
is regenerated. The order matters, and each operation records who made it and when.

### 5.2 `navkit apply <level> <navtune>`

Applies a side file:

- **Walkability operations** feed into NavKit's build, which then rebuilds the affected sectors.
- **Link and edge operations** are applied to the written data.
- **Traversal links** the format cannot carry go into `<level>.traversal.json` for OPFOR.

The result is checked by reading it back, and reported. The same command is used by OPFOR's import
steps and by the installer.

### 5.3 Tactical annotations: `<level>.tactical.json`

Typed annotations, each with a position, orientation, extent, team relevance, game-mode relevance and
source (generated or hand-made). The schema is fixed when the bot layer's needs are known.

### 5.4 Telemetry stream

JSON lines over 127.0.0.1 TCP, from OPFOR to the debugger, one frame per update:
`{"t": ..., "soldiers": [...], "squads": [...], "events": [...]}`. The schema is fixed with the first
OPFOR telemetry module, and is versioned.

## 6. Order of work

| Milestone | Delivers | Also gives |
|---|---|---|
| M1 | the core: data layer (Python DBX reader, collision, terrain heightfield), scene, view, camera, picking, panels | |
| M2 | Map Inspector: entities, spawns, objectives, destructibles, properties, search | |
| M3 | Navigation Studio, viewing: navigation layers, islands, detailed zones, difference view against DICE | the 3D view for NavKit's parity work |
| M4 | Navigation Studio, editing: regions, seeds, links, traversal annotations, path test, finished file and side file, `navkit apply` | |
| M5 | Test harness, offline: reachability, path batteries, parity, saved runs | |
| M6 | OPFOR telemetry module and Bot AI Debugger, on the game's own AI first | the harness's in-game tests |
| M7 | Tactical authoring tool, once the bot layer's needs are known | |
| alongside | asset pipeline (geometry and entities with M1; dependency report with M2; textures later), diff workbench patterns (with each format researched) | |
| with Goal 3 | the bot decision library (engine-independent) and the AI behaviour simulator; bfbc2-asset-builder with the installer | |

## 7. Open questions

1. **Terrain heightfield:** the full layout (the 2048 grid plus about 270 KB more: probably patches or
   levels of detail), and how heights map to metres.
2. **Water:** surface heights and extents (relevant to NavKit's extra-ground gap).
3. **Traversal links in Kynapse data:** whether the format can carry a ladder or jump link the game's AI
   would use, to be tested in game. Until then, those links go to OPFOR's side file.
4. **The path test's fidelity:** routing over the graph matches what the game plans, but not its path
   smoothing. OPFOR's DLL could answer path queries from the real engine (`navquery`) for a
   check.
5. **Performance:** a whole level's collision (millions of triangles) in one view, with level-of-detail
   or culling if needed.
