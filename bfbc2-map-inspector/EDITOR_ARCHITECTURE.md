# Map Editor: architecture plan (planning, 2026-11-16)

Planning only: nothing here is built or scheduled. This plan grows the Map Inspector into a map editor for BFBC2,
Vietnam and Pacific War (WW2) maps. It is based on a survey of the suite and of Project OPFOR BFBC2's WW2 level
pipeline (2026-11-16). Items marked *(inferred)* were not checked in code.

## 1. Rulings (the user, 2026-11-16)

- **Stage 1 edits existing maps.** Move, add and remove objects and gameplay entities on stock, Vietnam and WW2
  maps, then build, install and play them. This sets the baseline.
- **Stage 2 makes new maps.** New terrain, imported or randomized, populated with existing game assets.
- **Audience: public modders too.** A guided GUI with validation and plain-language errors, in line with the
  suite's beginner QUICKSTART and GUI plans. A power user can still drive every step from the command line.
- **Home: the Map Inspector gets an edit mode,** as the Navigation Studio did, on the shared core. It is not a
  separate application.
- **WW2 mode.** The editor makes Pacific War maps under an explicit WW2 content profile. WW2 kits, ww2_common and
  Package-only levels are used there, and modern kits are never placed on a WW2 map. This is the same rule as the
  planned WW2 mode of the AI tools.

## 2. What already exists to build on

| Need | Existing piece | Gap |
|---|---|---|
| Load a level: entities, transforms, blueprints, teams, areas, routes | `core/bfbc2core/level.py` (`load`, `Entity`, `resolve_blueprints`, `assign_teams`, `shapes_of`) | Loads only a NavKit-prepared level (`work\levels\<lv>\dist`); no save path |
| 3D view, camera, picking | `core/bfbc2core/viewer.py` (moderngl + GLFW + imgui-bundle; `Viewer.pick`, `project`) | Geometry is uploaded once (`_init`); no moving, adding or removing objects; no gizmo |
| Edit log, undo, apply, install | Navigation Studio: `navtune.py` side file (operations, `by`/`when`, base sha1, undo/redo), `editing.py` (save, then `navkit apply`, then install/restore) | Built for navigation operations only |
| Add an entity to a level partition | `bfbc2-stockmp-ai/author.py` + `dbxbin.py`: clones a prototype, UUIDv5 guids, transforms, `EventConnections`, `LevelData.Objects` | One of three DBX mutation codebases (also DbxEdit and `core/dbx.py`'s writer) |
| Validate a built level offline | `bfbc2-stockmp-ai/validate.py` (9 checks); `culltree.py verify`; `renderqa.py`; the harness; OPFOR's `qa_checks.py` and `wake_install --plan` checks | Spread across tools, not one gate |
| Build and register a level | OPFOR `src/tools/ww2/wake_install.py`: closure, shader entries, bundles, `package.mft` / `bundleManifest` last, description, LevelListQuery, localization, pictures, backups and revert | WW2-specific and very large; not a library |
| Shader entries for placed meshes | `bfbc2-shader-tools/shaderdb.py` (merge, clone, add parts) | — |
| Cull tree for new placements | `bfbc2-culltree/culltree.py add` (blueprints with an existing cull shape) | A blueprint with no stock cull shape anywhere |
| AI on the edited map | NavKit (path data from collision), `bfbc2-stockmp-ai` (spawners, orders) | WW2 AI: planning only |
| Terrain | `terrain.py` reads the heightfield; `console-convert/terrain.py` writes converted terrain | No writer from new data (stage 2) |
| Collision | `havokport.py` ports existing Havok; `mopp.py` decodes and checks MOPP | No shape / MOPP builder for new geometry (stage 3) |

## 3. Architecture

```
 Map Inspector + edit mode (GUI)            mapedit CLI (same operations, scriptable)
        |                                            |
        +------------- map project -----------------+
                 <name>.mapedit.json  (operation log on a base level)
                        |
                 apply: level writer --> build --> validate --> stage
                        |                                         |
                 core: levelwrite.py, content profiles      install / revert (registration last)
```

### 3.1 The map project and its operation log

A map project is a folder holding `<name>.mapedit.json`. Game files and stock bundles are never edited in place.
The project is data only and shareable (section 6, ruling 2): it names game assets and never copies them.
The log follows `navtune.json`'s proven pattern:

- **The base:** the level it edits, the base's sha1, and the content profile (BFBC2, Vietnam, WW2).
- **Operations, each with `by` and `when`:** move, rotate, delete, duplicate, place blueprint, set field, and the
  gameplay operations in 3.4. An operation names an existing entity by its guid, and a new one by the UUIDv5 guid
  it was given when made.
- **Undo and redo** as a list plus a redo stack, as in `navtune.py`.
- **Base changed** (a new game patch or a rebuilt WW2 level): operations whose target guid is gone are reported,
  never dropped silently.
- **New maps (stage 2)** use the same file, with a generated base in place of a stock level.

### 3.2 The core additions

1. **Dynamic scene** (`viewer.py`): per-object transforms, so an object moves without uploading the level again,
   and add/remove of a batch. This is the largest change to the core.
2. **Gizmo:** translate, rotate, snap to the terrain and to a grid, and local or world axes.
3. **Selection:** box and polygon selection and a command log. These are planned in DESIGN.md 3.3 and today exist
   only in the Navigation Studio.
4. **`levelwrite.py`:** one DBX mutation API for the level partition. It clones a prototype, makes UUIDv5 guids,
   sets transforms and typed fields from `dbxcatalogue`'s schema, and wires `LevelData.Objects` and
   `EventConnections`. It would consolidate `author.py` / `dbxbin.py` and the needs of DbxEdit's
   `graft` / `setfield`, and would be fixed by tests against DbxEdit's round trip. Fields are always typed from the
   schema, never guessed (DbxEdit guesses an unknown new field as a float). Values are scanned in binary, not
   through XML (the denormal trap).
5. **Content profiles:** per mode, the asset roots allowed, the kits, the common bundles, MatchmakingModID, the
   description donor and the game-mode set. The WW2 profile refuses modern kits and BFBC2/Vietnam-only assets that
   need a licence.

### 3.3 The asset browser

The browser is a catalogue of placeable blueprints from the stock bundles, Vietnam and ww2_common, filtered by the
content profile. Thumbnails come from `meshview` / `renderqa`, renders made once and cached. Each asset is marked
as safe to place:

- it has a shader entry source to clone from;
- it has a cull shape somewhere (otherwise `culltree add` cannot place it, and an object at slot 0 draws
  see-through and cannot be destroyed);
- it has collision;
- its partition's references resolve. A blueprint whose references resolve nowhere crashes the level checksum at
  0x6EFCA4.

### 3.4 Gameplay editing

This covers editing flags and capture points, spawn points, vehicle spawners, objectives (M-COMs), combat-area
and team-area polygons, ceilings, spectator cameras and markers. The WW2 pipeline already authors these in code
(`wake_level.py`); the editor moves that authoring into `levelwrite.py` and gives each a panel. Per mode, the editor
shows what the mode needs (Conquest flags, Rush stages, Air Superiority). The needs come from data-driven mode
definitions, so custom modes work the same way as built-in ones (section 6, ruling 4).

### 3.4a The AI workspace (the user, 2026-11-16)

The editor has a switchable **AI workspace** for the bot side of a map. It works on any map, whether being built
or only viewed: stock, Vietnam, WW2 and custom. It brings the Navigation Studio's editing and the stockmp-ai layer
into the editor's own project, undo and apply, rather than being a separate tool:

- **AI spawners and orders:** place, move and delete AI spawners and AI orders (area, patrol, defend, objective),
  and edit their event wiring, teams, squads, kits and HasAI / PathfindingTypes / SectorOffsets. The stockmp-ai
  layer is proposed from the map's objectives and spawns as a starting point.
- **Routes and connections:** patrol routes and waypoints, and links between areas: jump, climb and door
  traversals, and hand-made links. The cross-domain routes are edited here too, from NavKit's
  `CROSS_DOMAIN_ROUTES.md`:
  - boat legs and shore links;
  - vehicle enter and exit points and seat claims;
  - for WW2, carrier spawns, LCVP boarding and the drop, and beach exits (OPFOR
    `research/WW2_BOT_ROUTES_PLANNING_2026-11-16.md`).
- **Navmesh coverage:** the navigation layers with their islands, sectors and coverage gaps, drawn over the map.
  Coverage can be edited with walkable and unwalkable regions and seeds, NavKit regenerated from the editor, and the
  result compared with DICE's data where a stock map has it. Navigation Studio's anomaly overlay and path test
  are available in the workspace.
- **Checks in the workspace:** spawns with no path to an objective, islands off the main network, orders outside
  the path data, routes crossing unwalkable ground, and a vehicle spawn with no route for its domain.

These edits go in the same operation log as object edits, so one project carries the map and its bot side. On
apply, AI steps run after the level steps (3.5, step 6). Navigation edits stay stored by position, as in
`navtune.json`, so they survive a regenerated base.

**WW2 maps are fully editable,** both their objects and their bot side. The WW2 content profile and the WW2 AI mode
keep modern kits off them. The workspace's WW2 tools follow the WW2 bot planning:
- carrier decks as walkable ground;
- choosing among spawns;
- LCVP boarding and beach exits;
- for planes (single seaters, no transport), a damaged-plane bail-out or a return to the repair pad, then the pilot
  continues as infantry.

Building them follows that planning, not ahead of it.

### 3.5 Apply: build, validate, stage

`mapedit apply <project>` is the same command from the GUI and the CLI. The editor then streams its log and report,
as the Navigation Studio does with `navkit apply`. The steps:

1. **Write the level partition(s)** from the base and the log (`levelwrite.py`).
2. **Closure:** every referenced partition and resource is found in the outputs or the stock bundles.
3. **Shader entries** for every drawn mesh, cloned with `shaderdb`.
4. **Cull tree:** `culltree add`, then `verify`.
5. **Bundles:** level, terrain, ingame and loader, packed with FbrbTool.
6. **AI:** NavKit regenerated from the changed collision, and the AI layer re-proposed by stockmp-ai. Under the WW2
   profile, the WW2 AI mode once it exists.
7. **Validation:** a single gate. It runs validate.py's 9 checks, DbxEdit check, the qa checks (shader entries,
   textures, references), renderqa over placed objects, and the harness's structure and reachability. Errors block
   staging; warnings are listed with their object, and the GUI jumps to it.
8. **Stage** into a work folder. Nothing in the game copy changes yet.

The step needing the most refactoring is the reusable level installer. `wake_install.py` proves the steps but is
WW2-specific: the editor needs them as a library **in the suite** (section 6, ruling 3), shared by OPFOR's WW2
install, stockmp-ai and the editor. Its hard rules carry over:

- registration (`package.mft` + `bundleManifest`) is written **last** and undone first;
- backups never hold an earlier install's content;
- the description, LevelListQuery and localization are written with the level.

The suite architecture audit (2026-11-09) should be read before this split.

### 3.6 Install, play-test, revert

The install and revert buttons follow `navkit install` / `restore`. Every install gets a new level name, its own
registration and a description. A stock map (BFBC2, Vietnam, WW2) is never overwritten. A custom map may be
replaced by a later build of its own project (section 6).

**Play-test** builds, installs and starts the level offline: OPFOR's `opfor.ini [dev] level`, then the crash reader
on a crash. The GUI asks first, because the game starts.

### 3.7 Live validation in the GUI

These warnings show while editing, before an apply:

- objects outside the combat area;
- spawns with no path to an objective (from the last NavKit build);
- an asset not safe to place (3.3);
- overlapping vehicle spawns;
- a flag with no spawn;
- the WW2 profile's kit and asset rules.

## 4. Stage 2: new maps from imported or randomized terrain

- **Terrain sources:** an imported heightmap (an image, or real-world elevation data), or terrain generated from
  noise with erosion and a seed, so it can be made again.
- **Terrain writer:** the heightfield is decoded (DESIGN.md section 7, item 1). Its tail, the level-of-detail data,
  is not; `console-convert/terrain.py` already computes it for converted terrain. Still to do:
  - writing the heightfield from new heights;
  - the material map and its layers;
  - the mask textures and VisualTerrain;
  - water (WaterMesh / VisualWater, decoded).

  The terrain-patch rule applies: the material/mask atlas layout must match the donor's, or tiled cells draw black.
- **A new-level shell:** the level-root files (shader database collection, emitter system, atlases), the material
  grid, weather and the description. These come from a donor under the new name, as the WW2 levels do from mp_005
  and NAM_MP_002CQ.
- **Populating:** existing assets only, so no new collision is needed. Optional scatter tools place vegetation and
  props by slope, height and density with a seed. Each scattered object still goes through the "safe to place" rules.
- **Terrain collision:** *(inferred)* the game builds terrain collision from the heightfield itself, which needs
  checking before stage 2 starts.
- **Stage 3, new models:** a Havok shape and MOPP builder for new geometry, plus cull shapes for blueprints that
  have none. It is needed only for custom models, not for terrain or existing assets.

## 5. Order of work (rough)

| Step | Delivers |
|---|---|
| E1 | Core: dynamic scene, gizmo, selection, command log; `levelwrite.py` with tests (round trip against DbxEdit) |
| E2 | Map project and operation log; edit mode in the Map Inspector: move, rotate, delete, duplicate; save |
| E3 | Apply for an edited stock map: partition, closure, shader entries, cull tree, bundles, validation gate, stage, install / revert; the level installer split out of `wake_install` |
| E4 | Asset browser with the safe-to-place rules; placing new objects; gameplay panels (flags, spawns, vehicles, combat areas); the mode definition format with the built-in modes (Conquest, Rush, Squad Rush, Squad Deathmatch, Onslaught, Air Superiority) |
| E5 | NavKit and AI regenerated on apply; play-test button; content profiles incl. WW2; custom game modes from mode definitions; export of a built map |
| E5a | The AI workspace (3.4a): spawners and orders, routes and connections, navmesh coverage and its regions, the workspace checks; Navigation Studio's editing and stockmp-ai brought into the editor's project. WW2's bot tools (carriers, LCVPs, spawn choice) when the WW2 bot planning becomes a build |
| E6 | Modder polish: QUICKSTART, guided first map, plain-language errors, a sample project |
| S2 | New maps: terrain import / generation, terrain writer, new-level shell, scatter |
| S3 | Custom models: collision and cull-shape generation |

## 6. Rulings on the open questions (the user, 2026-11-16)

1. **Stock maps are never overwritten.** An edited map whose base is one of OPFOR's stock maps (BFBC2, Vietnam or
   WW2/Pacific War) is always installed under a new name, with its own registration and description. The editor
   refuses to install over a stock level. A **custom** map (one the editor made and installed) may be overwritten
   by a later build of its own project. The install records which levels are custom, so the overwrite check knows
   a custom level from a stock one.
2. **A project is data-driven and shareable without content.** Until a map is built and installed or exported, a
   project holds only data: the base level's name, the content profile, the operation log, generator settings and
   seeds, and references to game assets by name. It holds no EA content, no copied partitions, meshes or textures.
   It stays small (kilobytes) and rebuilds on another machine from that person's own game copy. Anything the editor
   needs from the game is derived again at build time, in a cache outside the project. The built or exported map is
   the only output that holds game content.
3. **The level installer lives in the suite,** shared by every tool: the editor, stockmp-ai, and OPFOR's WW2 install,
   which would call it rather than keep its own copy.
4. **Every game mode, including Air Superiority and custom modes.** The editor authors maps for Conquest, Rush, Squad
   Rush, Squad Deathmatch, Onslaught and Pacific War's Air Superiority, and for **custom game modes**. A mode is data
   to the editor: a mode definition says which gameplay entities it needs and their rules (flags, stages, M-COMs,
   capture areas in the air, ticket rules, spawn rules), its GameModeData (ModeID, HUD layers, matchmaking id and
   bundles, as OPFOR's AirSuperiority is built), and the validation that applies. Built-in modes and custom modes
   use the same definition format, so a modder can add a mode without changing the editor. One project can carry
   several mode layouts, as DICE's mode-specific levels do (MP_005GR, MP_005SR).

   Two lessons from Air Superiority (2026-11-14 to 16) carry over:
   - a custom mode that shares a matchmaking key with another mode must not break the menus (the blank browser
     rows, fixed in OPFOR's DLL);
   - its description's GameMode must resolve at load, or the level crashes (0x74EAB3).

## 7. Still open

1. **The WW2 AI mode's timing:** WW2 bots are still being planned (OPFOR
   `research/WW2_BOT_ROUTES_PLANNING_2026-11-16.md`). WW2 maps are fully editable, including their bot side (3.4a),
   so the editor is where that work will be done. Bots play on them once the WW2 bot runtime (the DLL side) exists.
2. **Custom modes needing engine work:** some mode rules may need OPFOR's DLL (as Air Superiority's mode tests did).
   The mode definition format needs a way to say so, and the editor a way to report it.

## 8. Known risks (from OPFOR's records)

The validation gate checks each of these, or the design avoids it.

| Risk | Source |
|---|---|
| A level registered while its bundles are missing, or a new name not registered: the level checksum crashes at start-up | memory note new-level-registration; `wake_install.register` |
| A placement whose blueprint references resolve nowhere: null blueprint in the checksum (0x6EFCA4) | `wake_install.py` |
| A model-group member's physics part count differs from the group's (0xAB4E58) | OPFOR DEVELOPMENT_NOTES.md |
| Streaming wave data placed in a level bundle (0x49EDF4); waves hidden by deleted markers (0xFF2AC2) | `wake_install.py` |
| A description whose GameMode does not resolve (0x74EAB3, Coral Sea) | `wake_install.py` |
| A mesh part with no shader vertex layout; console-only shader entries (0x98BD6E) | `wake_install.py`, memory note console-resource-formats |
| A level missing from LevelListQuery: in no menu; a `_NAME` prefix mismatch: a black loading screen; a key shared by two modes: blank browser rows (2026-11-16) | OPFOR research notes; commit 6f809e1 |
| An object at cull slot 0: draws see-through and cannot be destroyed | `bfbc2-culltree` |
| A terrain mask atlas that differs from the donor's: black terrain cells | memory note terrain-patch-donor-rules |
