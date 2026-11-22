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
  separate tool. Under the performance ruling (3.8), the Map Inspector itself moves onto the Rust core and
  application, and the edit mode is part of it there.
- **Very fast, with parallelism:** a hybrid Rust core (3.8).
- **WW2 mode.** The editor makes Pacific War maps under an explicit WW2 content profile. WW2 kits, ww2_common and
  Package-only levels are used there, and modern kits are never placed on a WW2 map. This is the same rule as the
  planned WW2 mode of the AI tools.

## 2. What already exists to build on

| Need | Existing piece | Gap |
|---|---|---|
| Load a level: entities, transforms, blueprints, teams, areas, routes | `core/bfbc2core/level.py` (`load`, `Entity`, `resolve_blueprints`, `assign_teams`, `shapes_of`) | Loads only a NavKit-prepared level (`work\levels\<lv>\dist`); no save path |
| 3D view, camera, picking | `core/bfbc2core/viewer.py` (moderngl + GLFW + imgui-bundle; `Viewer.pick`, `project`; retired in round RT3, `RETIRED.md`: the editor's own view replaced it) | Geometry is uploaded once (`_init`); no moving, adding or removing objects; no gizmo |
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
- **Undo and redo** as a list plus a redo stack, as in `navtune.py`. The editor replays from checkpoints (round
  XF1): the state after each mode choice and after every 16 operations is kept (at most 8), so an undo applies the
  operations after the newest one the log still passes through, not the whole log.
- **Base changed** (a new game patch or a rebuilt WW2 level): operations whose target guid is gone are reported,
  never dropped silently.
- **New maps (stage 2)** use the same file, with a generated base in place of a stock level.
- **Operations on any file of the level** (round OP1, audit AU1 R18): each operation names its *targets*
  (`bc2_levelwrite::files::Target`): the level partition, a static group file (`<Name>_HavokAsset.dbx`), a height
  field, a paint layer, the navigation side file, or the project itself (layers, folders, the export's choices). The
  editor's document holds one writer per file (`LevelFiles`), each read from the level as loaded the first time an
  operation names it; its sha1 then goes into the project's `base.files` for the base-changed check. The `files`
  operation holds edits of several files (partition operations, group members' transforms, deletes and adds) and
  applies them on copies: all or nothing, one undo step, the scene following only
  a whole apply. The export applies the log to all files at once and writes the edited group files in place of the
  base's; a target it does not write yet (terrain before E5) refuses the export in words.
- **Brush strokes** (round OP1, audit AU1 R24 / D19): a `stroke` operation names the 64 x 64 tiles of a raster
  (height field, paint layer) it changed, by the sha1s of their files before and after; the tiles are kept once each
  in `<project>.mapedit.json.tiles` (`bc2_levelwrite::delta`). Rasters share unchanged tiles with the checkpoints,
  so a stroke and its undo cost the tiles touched. E5's brushes call `Doc::stroke(target, after, label)` with a copy
  of `Doc::raster(target)`; the terrain view and the export of rasters are E5's.
- **Autosave** (round OP1, audit AU1 R17): an append-only journal (`bc2_levelwrite::journal`), one checksummed line
  per change after a first line holding the project; crash-safe (a torn last line is left out), resumable and
  compacted. The organisation follows each new operation instead of replaying the log.
- **Static group members** (round SG1; spec XLOG-MEM-1 - 3, LEVEL-MG-5 - 8, HAVOK-RUN-3): a model group is three
  files - the partition's group entity and member types (`InstanceCount`, `SpawnDatas` with each instance's cull
  leaf, cumulative physics-part and network-id numbers whose "count" fields are ends), the group file (`Assets`,
  `Transforms`, `Enums` in member-type runs) and the physics resource (collision per member type in its own space;
  per instance part only a leaf word and a material byte). Members are named by identity: `<instance>#<k>` (k the
  base's place) for the whole project, `#+<n>` for added ones; `LevelFiles` keeps each edited instance's current
  order. The views' operations are made member edits by `bc2_editor::memberedit::lower` (an operation naming no
  member is written as before); a delete or add changes the group file and its member type in DICE's layout (no
  tombstones), the last of a kind is refused in words before it enters the log (`Doc::refusal`), an add copies a kind
  the group holds (the asset browser's **Place into** list; another kind is placed loose), and members are not
  scaled (DICE never does; one collision shape per kind). The export (`bc2_levelwrite::groupexport`) gives the
  leaves: a moved member keeps its client-tree leaf, reseated at its new place; an added one takes a deleted
  member's leaf or a new client leaf (then the exclusive tree's `EnumOffset` and every placed slot move up by as
  many); each edited group's physics is laid out again. `mapexport check` holds every group to DICE's rules.
- **Versions** (project format 7): `version` is the writer's, `min_reader` the lowest version that reads the file
  (its newest operation kind; 7 for a member delete or add); each version step has a migration
  (`project::migrate`), and the recorded projects of versions 1 - 6 (`rust/bc2-levelwrite/tests/data/projects`) must
  load (1 - 5 replay to the same partition, 6 its member move to the same member).

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

**What waits for what (round DF1, audit AU4).** The editor counts a *project generation*: a new number each time a
level or project is shown or the map is closed (`EditorApp::generation`). The state made for one project carries
it and goes with it: the Export window binds to the generation it first draws in with its project and is unbound
(closed, its form and target dropped) when the generation changes, and it never writes into a project it was not
opened for; a preview being opened is dropped; the new-project form's era / mode target goes only to the project
that Create makes. Work that writes shared files waits, with the reason in words: Export while an install or
uninstall runs (it would rebuild the stage being copied), Install now / Uninstall now while an export runs, and
navigation Install, Restore, Build and Close map while a navkit command runs. Install now installs exactly the stage
its plan was shown for and refuses it when `stage.json` changed since. A worker that ends without an answer is
reported, never polled forever. Closing the window while any of this runs asks first (Wait / Close anyway); a navkit
command still running then is stopped and its half-written output removed, and the install script's output goes to
a log file rather than the editor's pipes.

**The states (round OP1).** DF1's side fields are explicit states now: the close question (`app::Closing`: none,
busy - Wait / Close anyway -, unsaved - Save / Don't save / Cancel - or closing now, for the window or the map), the
Export window's project (`export_ui::ExportFor`: none, opened for a project file, bound to its generation), the
preview (`diffui::PreviewState`: none, opening, shown; an opening preview is dropped with its generation, so no
generation travels with it), preparing a level with what follows (`Mode::Preparing { job, then }`), the new
project's era and mode inside the Create that asked for it (`Then::Create(name, folder, target)`: a Create that fails
leaves none behind), the install script's run (`InstallRun`), a navkit job's end (`nav::Job::running` /
`outcome`) and the game-accurate view (`gameview::GameSlot`: off, loading on a worker, ready). Errors are typed
(`error::EditorError`, `bc2_render::RenderError`) and the Problems panel's entries are `error::Problem`s (severity,
code, operation, objects). The two state structs are split into focused parts (`ViewState`: `browser`, `menus`,
`outline`, `measures`, `inspect`, `props`, `pointer`, `game_view`; `EditorApp`: `command`, `new_project`,
`start_ui`, `catalog_state`); a new workspace or tool adds its own part rather than fields of `ViewState`.

### 3.7 Live validation in the GUI

These warnings show while editing, before an apply:

- objects outside the combat area;
- spawns with no path to an objective (from the last NavKit build);
- an asset not safe to place (3.3);
- overlapping vehicle spawns;
- a flag with no spawn;
- the WW2 profile's kit and asset rules.

### 3.8 Performance: a hybrid Rust core (the user, 2026-11-16)

The editor must be very fast and use parallelism. The user chose a **hybrid Rust core**:

**Rust is the preferred language** (the user, 2026-11-17; this widens the hybrid plan below):
- New code in the suite and in Project OPFOR's pipeline (`src\tools`: the WW2 and Onslaught converters and
  installers) is written in Rust. Existing code is ported wherever Rust is measurably faster, however slightly, not
  only the hot spots; profiles set the order, the largest gain first.
- The gate is unchanged: a port is used only once it is proven identical to the code it replaces on real data (for
  OPFOR, the staged bundles and installs byte for byte), with timings before and after.
- **Everything is ported, then the Python retired** (the user, 2026-11-18, superseding "format readers stay"): every
  Python module - format readers and writers included, the suite's and OPFOR's (`src\rust\opfor-pipeline`) - is
  ported, proven a perfect port on real data, and its Python retired once its Rust passes a full test tier. The
  **recorded reference data** taken from the Python before it is deleted (goldens, digests) is kept for good, as
  documentation of the behaviour and as what future changes are compared against. A retired piece needs the Rust
  module: people get it prebuilt (`rust\build_native.py --download`, a release per Rust fingerprint) or build it.
  Retired so far: texture conversion and NavKit's grid walks (2026-11-17, full tier 140 of 140); the converters,
  the translator's emission and all of NavKit's ported Python (2026-11-18, full tier 173 of 173); the core format
  readers and writers - DBX, FBRB, terrain, water, the collision index, the level loader, the cull tree element
  reader (2026-11-18, round F1a) - README.md, "The Rust core"; `rust/BENCHMARKS.md`, "Retired Python". The tests themselves move to Rust once every Python tool is
  retired (the user, 2026-11-18).
  The C# tools whose commands were ported went the same way (round R4, 2026-11-18, after the full tier's 173 of
  173): DbxEdit.exe and FbrbTool.exe are retired, their answers recorded (`core/tests/*_golden*.json`); DbxEditGUI,
  built on DbxEdit.cs, was folded into this editor as the DBX inspector in round GR4 and retired with it (9.4).
- The suite's GUIs (Map Inspector, Navigation Studio, NavKit's window, DbxEditGUI, the launcher) are rebuilt in Rust
  (`egui`, the shared design language, `DESIGN.md`) inside the map editor application, as its views and workspaces.
  Each Python GUI stays until its replacement works. **Status (G2, 2026-11-18):** the editor application's shell and
  its read-only map view (`rust/bc2-editor`, the "View a map" of 3.9; round G1), with 3.10's decisions on it (round
  G2): faction team labels, the 3D view's own tokens and the third neutral, the Help menu, and levels prepared from
  the editor; the Map Inspector was replaced and retired in round GR1 (section 9, with the other GUIs' plan).

- **In Rust:**
  - the editor application: window, 3D view, gizmos, panels;
  - a shared core library: the formats (DBX read and write, FBRB read and pack, terrain, water, shader databases,
    cull trees), the scene, `levelwrite`, and the multithreaded build steps.
  - Candidate stack *(to confirm when E1 starts)*: `wgpu` for rendering, `egui` for the interface, `rayon` for data
    parallelism, background worker threads for loading and builds.
- **Exposed to Python through PyO3.** The existing Python tools call the same Rust core, so every format has one
  implementation and Python gets its speed. This covers the WW2 converters, NavKit, stockmp-ai, the harness and the
  Map Inspector's Python side while it remains.
- **The converter pipeline stays in Python** for now. Its hot spots are ported to the core one at a time, where a
  profile shows the time goes. These include DBX parsing, bundle packing, texture encoding, the shader database
  merge and level builds (a WW2 level build takes about 28 minutes today).
- **Parallelism:**
  - Loading: partitions, meshes and textures load in parallel; the window is usable while a level streams in.
  - Building: the apply steps run as a dependency graph, independent steps at once: per-level builds, per-texture
    encoding, shader entries, cull tree, bundle packing.
  - The interface never waits on a build: builds run on workers and report progress.
- **Correctness first:** a Rust format replaces the Python one only when it passes the same test corpora. These
  include the DBX XML being identical to DbxEdit's on 31,404 files, the FBRB round trips, terrain against the
  in-game samples, and the level loader against its snapshot. The Python implementation stays as the reference
  until then.
- **Measured, not assumed:** each milestone records load and build times before and after.
- **Toolchain** (E0, 2026-11-16): Rust 1.100.0 (pinned in `rust/rust-toolchain.toml`), the Cargo workspace `rust/`
  (`bc2-formats`, `bc2-py`), maturin for the Python module; `run_tests.py` runs `cargo fmt`, clippy and the tests
  (quick) and the corpus checks (game), and skips them on a machine without Rust (README.md, "The Rust core").
- **Two DBX trees** (E0b, 2026-11-16; measured in `rust/BENCHMARKS.md`):
  - `dbx::DbxTree<'a>` (`parse_tree`), **read-only**: every element of a file in one vector in file order (each
    knows where its subtree ends), every attribute in another, strings and values borrowed from the file's bytes.
    A few allocations per file however large; what a reader of many files keeps (loading a level, the corpus
    checks, the Python bindings, which build their Python trees from it).
  - `dbx::DbxFile` / `Element` (`parse`), **owned and editable**: what `dbx::write` takes and what E1's editing
    path (the scene's changes, `levelwrite`) builds on. `parse` is `parse_tree` then `DbxTree::to_file`: one
    reader, so the two cannot drift apart; `Node::to_element` copies one subtree out.
  - Trade-offs for E1: a `DbxTree` borrows its file's bytes, so whatever holds trees must hold the bytes too, and
    longer (Rust does not let one struct hold the bytes and a tree borrowing them, so the scene keeps the bytes in
    its own store and the trees beside it, or re-parses). It cannot be edited: an edit converts the partitions it
    touches to `DbxFile` (about the old parse's cost per partition) and writes those; untouched partitions stay
    compact and can be written back as their original bytes. Positions are 32-bit (files under 4 GiB;
    `DbxError::TooLarge` beyond).
- **The scene** (E1a, 2026-11-16; `rust/bc2-scene`, measured in `rust/BENCHMARKS.md`):
  - **What a level load costs was measured first.** With the Rust DBX reader a whole `level.load` took 6-15 s, of
    which 85-95 % was NavKit's collision index: `mesh_aliases` reads and regex-scans every converted XML file of the
    level and the shared data (17,589 files, 205 MB on MP_005) and `havok_index` globs the same folders again. The
    height field, water and Havok triangles were 0.1-0.2 s together. So the port order was the index, then the
    readers; the index is one walk per folder with the subfolders and files on all cores.
  - **`bc2_scene::scene::load`** builds what `level.py` builds (entities, blueprint types, categories, teams,
    outlines, collision, terrain, water) on the compact `DbxTree`, the entity pipeline and the geometry at once.
    The caller finds the files and prepares / unpacks (those run tools and write); `LoadOptions` names them. Python's
    `level.load` keeps its own element trees for `Entity.element` (the scene's `Source` says where each entity's
    element is) and does the world-space transform in NumPy, so its results stay bit for bit the Python loader's.
  - **Editing:** entities are owned and mutable (`set_transform`, `changed`); collision is kept once per blueprint
    in local space and placed by each entity's transform (`world_triangles`), the instancing the editor's view
    needs (a move changes one matrix, no triangles are copied); the partitions' bytes are kept (`file`, `tree`), so
    the trees are re-read on demand rather than held (the "Two DBX trees" trade-off: no self-borrowing struct), and
    a partition that is edited converts to `DbxFile` for `levelwrite`.
  - **The alias table** (E1e, 2026-11-16): each blueprint's `Mesh` is read from its binary `.dbx`
    (`dbx::field_ref`: what `xmlinst.field_ref` finds in the XML, without building the text), the converted XML only
    where the binary cannot answer (an empty *deleted* marker whose XML was left over - MP_005 has one with a
    `Mesh` -, a text or damaged file). The XML files still decide which partitions count and in which order, as
    NavKit's reference does, so both backends give the same table (all 37 prepared levels). Measured, the binary
    alone did not make the index faster: opening 17,600 files costs about the same whatever their size
    (`rust/BENCHMARKS.md`, "The alias table"). So each answer is also **cached** per data folder, keyed by the size
    and modification time of the XML file and of its `.dbx` (from the folder listing the index makes anyway): an
    unchanged level is indexed without opening a partition (MP_005 0.36 -> 0.11 s), a changed file is read again,
    a damaged or foreign cache file is ignored and replaced, a write is a temporary file renamed over the old one,
    and an error writing is ignored. The cache lives outside the game data (`%LOCALAPPDATA%\bfbc2-tool-suite\cache\
    collision`, or `BFBC2_COLLISION_CACHE`; `0` turns it off); the index is the same with or without it.
  - **NavKit's own collision readers** (`detailed`, `ladders`, `navsample_offline`, `navwater_mesh`) read through
    `bfbc2core.collision` (`havok_index`, `index`, `blueprint_triangles`), so they run on the Rust core too, with
    their output unchanged (byte for byte on four levels, `rust/BENCHMARKS.md`). `levelobjects.py` (NavKit's
    prepare step and the Python reference) is unchanged.
  - **Still open for E1:** new / deleted entities and field edits (with `levelwrite`), undo; a level that is not
    prepared still needs NavKit's prepare (unpack and DbxEdit's XML) before it loads.
- **The writers** (E1b, 2026-11-16; measured in `rust/BENCHMARKS.md`): `dbx.write` runs in Rust
  (`dbx::Writer`, element by element, driven straight from the Python tree: the Python writer's bytes on every
  corpus file, as read and edited) and bundles are packed in Rust (`fbrb::pack_path`, `bfbc2core.fbrb.pack`:
  FbrbTool's bytes on all 1,086 bundles of the game copy, unpacked and packed again). FbrbTool's gzip is .NET's
  `GZipStream`, which is zlib at level 6, so `bc2_formats::zlib` is a byte-exact port of zlib's lazy deflate (the
  Rust deflaters choose other matches). `levelwrite` builds on both; unpacking stays with FbrbTool.
- **Shader databases and cull trees** (E1c, 2026-11-16; measured in `rust/BENCHMARKS.md`):
  - `bc2_formats::shaderdb` reads and writes every database the suite reads (PC Dx9 / Dx10 / Dx11, Xbox 360 v120
    and BF1943's v117), records as slices of the file; `shaderdb.py`'s `ShaderDb.parse` / `to_bytes` use it with the
    Python code as the fallback. Identical on all 745 databases of the game copy, OPFOR's Xbox and BF1943 levels and
    work folders, and on the four WW2 levels' merges.
  - The profile put the merge's time in an algorithm (every name against every needed path), fixed in Python
    (`needed_test`); the merge stays Python, as the WW2 tools drive `Merger` directly. A level's shaders-step merge:
    40 -> 26 s, of which 20 s is now reading the databases out of the bundles (three reads per level where one would
    do: a caller change).
  - `bc2_formats::culltree` reads the element stream `culltree.py`'s `Doc` reads and finds the `--ref-dir` levels on
    all cores (an add run 190 -> 61 s). The tree builder and writer stay Python (seconds per run). For the editor,
    cull trees are DBX partitions: E1's `levelwrite` writes them with the other partitions.
- **The converter pipeline and NavKit** (E1d, 2026-11-16; `rust/BENCHMARKS.md`, "The converter pipeline and
  NavKit"):
  - **Profiled first.** A WW2 level build (coral_sea) spent half its time in `fbrb.read`: the converters read
    BFBC2's partitions one at a time through `meshview.Index.read`, and a bundle is one gzip stream, so each read
    decompressed its bundle up to the entry. Not a format to port but a read pattern: `meshview.Partitions` reads a
    bundle's partitions in one pass and keeps them (coral_sea's grid step 777 -> 101 s, the build 1,293 -> 667 s).
    Texture, mesh and Havok conversion were under 4 s of a level build; texture conversion is ported anyway
    (`bc2-convert`), as Wake's steps convert all 704 BF1943 textures (48 s -> 0.4 s).
  - **NavKit:** 62 % of a build was one question asked 142 million times (which kept surface of a neighbouring column
    links to this one); the four walks that ask it, and the floods of the walkability rules, are ported whole
    (`bc2-navbuild`), so the calls stay in Rust. The floats are computed as the reference computes them (NumPy 2's
    float32 / float64 promotion, Python's rounding), so the path data is byte-identical.
  - **Still in Python, by the profile:** the HLSL compiles of the shader translation (9 % of a level build; the
    compiler, not Python, takes the time - running compiles on several threads is the open gain), the DBX trees
    Python builds (10 %), the shader-database parsing (the shader databases are E1c's), OPFOR's own donor scoring
    (`wake_shaders.role_score`, 8 %), NavKit's remaining walkability rules (`stair_slope`, `narrow_lips`,
    `headroom`: 1-5 % each).

### 3.9 UX and workflow (draft, 2026-11-16)

The editor is for a modder who has never opened a DBX file. Every step is visible, safe to try, and undoable.
Everything the GUI does is an operation in the project log, so the CLI does it the same way.

**Start screen**
- **New map from an existing map.** Pick a stock, Vietnam, WW2 or custom map, then give the new map its name. For a
  stock base this happens at once, because stock maps are never overwritten (6.1). The content profile follows the
  base (WW2 for a Pacific War map).
- **Open a project, recent projects, open a shared project.** For a shared project, the editor checks that this
  game copy has its base map and assets, and names anything missing in plain words. It never asks for game files from
  another person.
- **View a map:** open any map read-only (the editor's View workspace since G1). "Edit a copy" makes a new project.
- **Later (S2):** new map from terrain (import or generate).

**Main window.** The panels are dockable (the user, 2026-11-16). A default layout ships as described below, and
the user can drag, dock, tab, float and reset panels. The layout is saved per user, and tutorials say "Reset layout"
first.
- **Centre:** the 3D view. Orbit, fly (WASD) and focus (F) as today, plus camera bookmarks and a top-down map view
  drawn like the in-game map.
- **Top bar:**
  - the workspace switcher: Objects, Gameplay, AI, and later Terrain;
  - the mode layout (Conquest, Rush, Air Superiority, a custom mode ...) the Gameplay and AI workspaces show;
  - the content profile badge (BFBC2, Vietnam, WW2);
  - Build, Install and Play-test.
- **Left:**
  - the outliner: the map's contents by kind (gameplay, objects, vehicles, AI, areas), searchable, with hide and
    isolate;
  - the asset browser tab.
- **Right:** properties of the selection. Fields are typed from the schema; references are links; several selected
  objects can be edited at once.
- **Bottom:**
  - Problems: live validation, each with what is wrong, why it matters, how to fix it, a button to jump to the object,
    and a quick fix where one is safe ("snap to the ground", "add a spawn for this flag");
  - the build log;
  - History: the operation log with names, times and undo/redo to any point.

**Workspaces**
- **Objects:**
  - select (click, box, polygon);
  - move and rotate with the gizmo (snap to ground, surface normal, grid, angle);
  - scale only where proven safe (the user, 2026-11-16). An object type is allowed to scale only once tests and a
    play-test show the engine handles it: collision, destruction and cull data. The list starts empty, so move and
    rotate are the only transforms at first;
  - duplicate, delete, and place from the asset browser by dragging into the view.
  - Unsafe assets (3.3) are greyed out, with the reason shown.
- **Gameplay:** the active mode layout's entities: flags, spawns, vehicles, objectives, combat and team areas
  (polygons with vertex handles), ceilings. A checklist from the mode's definition shows what the mode still needs
  ("Conquest: 2 bases, at least 1 flag, a spawn per base").
- **AI (3.4a):**
  - overlays: the navmesh, coverage, islands, routes and links;
  - tools: spawners, orders, routes, links, walkable/unwalkable regions;
  - a path test (click A, then B);
  - "Regenerate navigation", which runs in the background and shows progress.

**Build, install, play-test**
- **Build** runs apply (3.5) in the background. The step graph shows as progress: which steps run in parallel, which
  are done, and which failed. Errors stop the build and land in Problems with a jump button. Warnings do not stop it.
- **Install** is a separate button. It installs the built map under its own name and keeps the revert. The game must
  be closed, and the editor says so instead of failing.
- **Play-test** builds if needed, installs and starts the map offline in the chosen mode layout, with bots where the
  map has AI. It asks first, because the game starts. A crash comes back into the editor as the crash reader's
  summary, with the objects it names linked.
- **Export** (the user, 2026-11-16):
  - "Share project" writes the small data-only file (6.2). It is the only way a map is shared: the other person
    builds it from their own game copy.
  - "Export built map" packs the built map **for this machine only**. A built map holds EA content, so the editor
    offers no shareable built package.

**Guard rails**
- The base map is read-only; there is no "save over the original".
- The WW2 profile hides modern kits and assets that need a licence.
- An unsaved project is kept in an autosave log.
- One editor per project (round QW2, audit AU1 R4): an open project holds `<project>.mapedit.json.lock`; a
  second editor refuses it in words, a lock whose process is gone is taken over, and a save first checks that the
  file is still the one this editor read or wrote (modified time and length), else writes nothing and says so.
- Long work never freezes the window.

**Getting started**
- A guided first map: copy Valparaiso as a new name, move a flag, add a vehicle, build, play-test.
- A QUICKSTART, tooltips on every tool, and a sample project.

### 3.10 Parity with modern editors (the user, 2026-11-18)

The editor matches what modders expect from modern engine editors (Unreal, Unity, Frostbite-era tools). Core
features, beyond 3.1 - 3.9, each in the operation log and undoable like any edit:

1. **Prefabs and groups.** Save a selection as a prefab (a checkpoint, a fortified position) and place it anywhere;
   edits to the prefab can update its instances; group and ungroup in place.
2. **Layers and outliner control.** Layers with hide, isolate, lock and a colour; folders in the outliner; hidden
   objects stay in the build unless excluded.
3. **Snapping and placement.** To the grid, by angle, to vertices and edges, and **to surfaces**: an object dropped
   or dragged onto terrain or a mesh sits on it, optionally aligned to the surface normal. **Sinking below the
   surface** (the user, 2026-11-18): a sink depth (metres, or a fraction of the object's height) pushes the object
   partly underground along the surface normal or straight down - a rock half buried, a bunker sunk into a hill - and
   stays with it when it is moved again. Align and distribute tools; a ruler and distance read-out.
4. **Several viewports.** Top, side and front orthographic views beside the 3D view, each with its own camera.
5. **Viewport modes.** Lit with the game's own shaders (the shader translator and shaderview), unlit, wireframe,
   collision, navmesh and cull-tree views.
6. **Terrain sculpting and painting** as core tools, not only stage 2: raise, lower, smooth and flatten brushes;
   texture-layer painting under the terrain-patch rules (the mask atlas layout of the donor); a foliage and scatter
   brush whose objects pass the safe-to-place rules.
7. **Splines.** Roads, fences, rivers and AI patrol routes edited by control points.
8. **Search everywhere.** A command palette (Ctrl+Shift+P for actions, Ctrl+P for assets and objects).
9. **Autosave and crash recovery** of the project and its log.
10. **Copy and paste** between maps and projects, references kept or reported.
11. **Performance budgets.** Live counts against the engine's limits (draw calls, entities, memory, cull-tree
    slots), warnings in the Problems panel as a map nears them.
12. **Version control.** Project history with diffs between versions and branches, on top of the data-only project
    format (which is already friendly to `git`).

**Decisions on G1's first version** (the user, 2026-11-18):
- Team labels name the side's faction per level and content profile (US / RU, US / NVA, US / JP ...), not T1 / T2.
- The design tokens gain 3D-view variants of the team, neutral and status colours (readable on terrain in both
  themes) and a `scene-*` group (terrain, water, sky, object greys) where it helps.
- A third neutral colour for gameplay objects no team owns (objectives, combat areas), distinct from
  `team-neutral`, the selection orange, the accent and the status colours.
- Selection outlines show through walls; fly is right-button + WASD; F focuses.
- A Help menu in the top bar for new users: controls and shortcuts, a getting-started guide, what each workspace is
  for.
- Opening a level NavKit has not prepared prepares it from the editor (a Prepare step with progress, or done
  automatically on open), not by sending the user to another tool.
- **Live rendering is the goal for the 3D view:** terrain, water and objects drawn as the game draws them - its
  materials, textures, shaders (the shader translator, shaderview) and lighting - so a level looks in the editor as it
  does in game. The opaque water and plain terrain of G1 are a first step.

*Done in round G2 (2026-11-18; `rust/BENCHMARKS.md` "G2"):* the labels by faction (`LevelDescription`'s
`FactionTeam1` / `FactionTeam2` read through the content profile: `FactionRUS` is RU, NVA or JP; T1 / T2 where the
level names none, said in Properties); `view-*` and `scene-*` tokens and the third neutral `unowned` (violet; the
design README's Colour section); the Help menu (generated from the action registry); opening an unprepared level
prepares it in the editor with its progress (NavKit's prepare and the terrain unpack, run as the suite's Python).

*Done in round G3 (2026-11-18; `rust/BENCHMARKS.md` "G3"): the levels that live only in `game\Package`.* NavKit lists
only `game\Dist` levels, so Vietnam's (`NAM_*`), the Pacific War's and DICE's other Package-only levels (the Bad
Company 1 maps, mode layouts such as MP_005GR) are listed, prepared and opened by the editor itself
(`rust/bc2-editor/src/package.rs`):
- **Listed when installed:** the game's registry `game\Dist\win32\package.mft` names the level and every bundle its
  `level-00.fbrb` loads is present. The badge comes from the bundles it loads (`ww2_common`: Pacific War, `nam_common`:
  Vietnam; else by name, as G2). Not installed (Pacific War and Vietnam are optional): not listed, no error.
- **Prepared in Rust, in memory:** the registry's stack in the game's order (`nam_mp_002cq`, `nam_common`,
  `mp_common`), the first bundle naming a file decides it, a `*deleted*` marker hiding later bundles' copies as in the
  game. Only what the scene opens is read, each bundle's payload streamed once per round: the level partition and
  static groups; the placed blueprints, their packfiles and the `ReferenceObjectData` blueprints; the second-level
  blueprints and the `Mesh` aliases' packfiles; the terrain's height field and water through the terrain stack (a
  Vietnam level's terrain is in `nam_common`'s terrain bundle). The same progress card, Cancel and plain failures as
  G2 (the job runs on a thread instead of a child process).
- **Where it goes: `work\editor\levels\<level>\`, never `work\levels`.** Several suite tests and tools enumerate every
  folder of `work\levels` against recorded corpora (the cull tree's, the geometry's, the shader goldens), so a new
  folder there would change them. The copy holds one folder per bundle of the stack (`00_<level>`, `01_nam_common`,
  `02_mp_common`: the scene's bases in the game's order), the placed blueprints' XML (the collision index counts a
  partition by its XML, as in NavKit's unpack) and `editor-level.json` (the bundles' sizes and times, every file).
  It is a project cache (`bfbc2core.caches`, `editor`: listed, pruned, cleared; `BFBC2_EDITOR_CACHE_MB`, default
  2048), made again when a bundle changed or a file is missing; the editor drops the least recently opened copies
  beyond the cap.
- **Projects:** "New map from an existing map" offers every level, Package levels included (one not prepared is
  prepared first); a project on a Package level takes its profile (Vietnam, WW2) from the same recognition.
- **Choose mode (the user's design, 2026-11-18):** the start screen opens on four cards (BFBC1, BFBC2, Vietnam, WW2:
  each set's installed map count and its own picture, the game's loading screen read at run time - DICE's
  `bc1_oasis`, `mp_005`, `nam_mp_002`, OPFOR's converted `wake_island_s`; a plain tile without it, greyed "Not
  installed" without maps), then a set's map list (thumbnail, name, game mode, prepared, Open / New project); shown on
  every start. Sets from the data: `bc1_` Package maps, `nam_common`, `ww2_common`, the rest BFBC2. The pictures are
  decoded into `work\editor\art` (`rust/bc2-editor/src/catalog.rs`), never into the repository.

*Done in round E2b (2026-11-19; `rust/BENCHMARKS.md` "E2b"): the user's context menus, edit modes and sink by mouse.*
- **Workspaces are editing methods** (`rust/bc2-editor/src/workspace.rs`): View, Objects (buildings, props,
  vehicles, effects, sounds ... edited alike; a category filter decided from blueprint data limits what can be
  selected and what the content browser shows), Terrain and Vegetation (layouts and brush actions registered,
  greyed "Coming in the terrain round"), Gameplay (only the mode's entities select; Set team) and AI (orders, AI
  spawns, area points; the navmesh view; the path test registered for E5a).
- **Context menus** from the action registry, per workspace, on objects (views and outliner) and on the ground;
  disabled entries say why.
- **Sink by mouse**: Alt+wheel (0.1 m a notch, 0.01 m with Shift) over a view sinks the selection or the object being
  placed; a popup beside the pointer (depth in metres and as a share of the height, a slider, straight down / along
  the normal / aligned to the normal). A wheel burst, a slider drag or a mode click is one operation
  (`MapProject::amend_last`).
- **Placing** (levelwrite `place`, "add instance"): the content browser (the level's placements, then its folders'
  and shared bundles' blueprints) by drag, or "Place object here..." with a searchable picker; Shift+wheel turns
  (15 degrees, 1 with Ctrl), a click places. A placement is a clone of a template (a plain placement of the same
  blueprint, else any placed object) with the blueprint, transform and the fields most of the level's placements
  of it share.
- **The asset browser** (the user's refinement): three modes. *Place Objects*: a tile for every placeable object of the
  content set (the level's placements, its folders', every bundle of the set by its header - read from its bundle
  when placed), category tabs, search; a type icon until rendered thumbnails exist (`work\editor\thumbs`, the hook
  for the renderer's rounds). *Spawners* (player spawn points, HQ / base spawns, AI spawners, a vehicle spawner per
  vehicle of the set) and *Objectives* (capture points with their capture area, M-COMs, combat areas, mission
  objectives, arm / disarm awards, life and ticket counters), drawn from the level's data, placed with the team
  picker's team and the sub-instances they own; their links (spawns to flags, Rush stages, tickets) are E4's.
- **Deep duplicate**: the sub-instances an object owns (reached through its references, named by nothing outside
  the set but `LevelData.Objects` and event connections: a flag's capture area points, map marker, its own
  spawners) copied with new guids, references among them remapped, their connections copied.
- **Viewport modes** (the editor view): lit, unlit, wireframe, collision, navmesh (Kynapse path data), cull tree
  (each object's slot: none, shared, its own; the leaves' spheres). **Polygon selection** (P). **Several objects'
  fields** in Properties, "Mixed" where they differ, one operation for all.
- Static group members are edited like objects (round SG1, 3.1 "Static group members"). Placed and copied objects keep their template's `Enumeration` (cull
  slot) in the project; the export gives each its own blueprint's shape (round XF1, 3.12). **Paste** is a deep
  duplicate (round XF1): a pasted flag has its own capture area and marker.

*Done in round G4 (2026-11-19; `rust/BENCHMARKS.md` "G4"): the start flow of the user's design.* The start screen
opens on three buttons (`rust/bc2-editor/src/app/modes.rs`, `create.rs`; Back and a trail of links lead up again):
- **Open project:** a `.mapedit.json` path (typed or pasted: no file-dialog dependency), the recent projects, and the
  projects found in the projects folder; an autosave newer than the file is offered first (crash recovery).
- **Edit existing level:** G3's four mode cards with their game art, each counting its **multiplayer** maps, then a
  mode's multiplayer maps (thumbnail, game mode, prepared; search; Open views read-only, New project starts a project
  on it in a dialog). WW2 lists OPFOR's Pacific War maps.
- **Create level:** *From existing levels* lists every level of every mode - multiplayer and campaign - grouped by
  mode with a search; New level opens the name and folder dialog (a campaign base's dialog says the export strips its
  campaign parts), and the project records the base's kind (`base.kind`: `multiplayer` / `campaign`) for the export.
  *From a height map* is a form: the file (a 16-bit greyscale PNG, or RAW 16-bit with its size and byte order chosen),
  the map size in metres, the height of sample values 0 and 65535, the water level, an optional colour map (PNG) and
  the mode whose content is used. The file is checked as it is typed: it reads, it is square, and it is 1024 or 2048
  samples a side - the main height field sizes the terrain reader knows (`bc2_formats::terrain::FIXED`) - or 1025 /
  2049 (the last row and column dropped). Create saves a project with `terrain_import` (the settings, the sample
  range) and the height map and colour map copied beside it with their sha1s; its base names no level. Such a
  project opens on its own page saying "Terrain generation arrives in a later round (E7)"; nothing is generated.
- **Multiplayer or campaign, from the data:** each level's description's `GameMode` (read by `catalog.rs`): a
  campaign mode (`gamemodes/singelplayer...`) is campaign, any other mode multiplayer; a level whose description is
  not read is listed under Create level only. On OPFOR's game copy: BFBC2 38 multiplayer + 13 campaign, BFBC1 8 + 0,
  Vietnam 20 + 0, WW2 4 + 0 (70 + 13, none unknown).
- **The top bar** keeps the project's badge and name in the room its buttons leave (the name is cut, never drawn
  under Save), the orthographic views in one Views menu, the theme button shortened to Light / Dark.

*Done in round SV1 (2026-11-20; `rust/BENCHMARKS.md` "SV1"): the simplified view draws render meshes.* The user: "in
our simplified view trees and objects sometimes look unrecognizable, like spike balls". The view drew each blueprint's
Havok collision; for vegetation that is the render mesh's leaf cards without their alpha-tested textures.

- **Render meshes** (`bc2_render::simple`, read beside the open view; collision is drawn until they arrive): the
  meshes the Game-accurate view draws, chosen by the same rule (`world::mesh_and_set`) and read by the same code
  (`material::prepare_untextured`), as plain geometry - positions, first texture coordinates, 32-bit indices, one
  part per draw - instanced per blueprint as collision was. Alpha-tested parts (`material::TESTED`) are cut by their
  diffuse texture's alpha (its level no larger than 64 texels, in one atlas); scene-lit objects take a muted tint of
  their texture's mean colour. Blended parts (glass, decals) are left out where a mesh has other parts.
- **Invisible objects**: blueprints that name no mesh, meshes of blended parts only (invisible walls, light shafts,
  vista smoke) and occluder meshes are the `invisible` group: Hidden (the default), Outline or Solid (the View
  dropdown).
- **Show as** (`display.rs`): the view's (View dropdown, `C`): Visual, Collision, Visual + collision (the collision's
  edges over the mesh, in its blueprint's hue), Wireframe, Bounds; an object's or a blueprint's (the object menu):
  Visual, Collision, Both, Bounds, Hidden. The Collision view mode still draws only collision.
- **Variants the data holds** (Properties' Display section): the mesh set's levels of detail (114 of SP_01's 205
  render meshes and 124 of MP_005's 146 have 2 - 4) and, for a destructible blueprint, its parts' health states
  (`PartComponentData.HealthStates[].MeshName`: `part_1`, `part_1_dest`, `ruin`, `ruin_downed`): Intact, Destroyed,
  All parts. The parts' states are not split in the collision the scene reads (one shape list per blueprint), so
  collision has no per-state variant yet (round VW1 split them: below).
- These are view settings: kept per level in the editor's settings (`Saved::displays`), never in the project's log;
  "Reset view overrides" clears the objects', blueprints', levels of detail and states.
- Picking reads the id of what is drawn (the render mesh); snapping, placing and the ruler still use collision.

*Done in round VW1 (2026-11-20; `rust/BENCHMARKS.md` "VW1"): the view toggles the user asked for.* Every size and
shape is read from the level's partitions, never made up (`rust/bc2-editor/src/overlays.rs`, `walk.rs`):

- **Overlays**, each a toggle in the View dropdown and the palette (`Show or hide ...`), with their own labels
  (`Overlay labels`); kept with the level's display settings (`Display::overlays`). Colours: the new tokens
  `view-fx-effect`, `view-fx-light`, `view-fx-sound` (each 3.5:1 on the lit terrain and the water, none orange) and
  the team tokens, `view-team-neutral` for what the data marks neutral, `view-unowned` (the third neutral) for what
  no team owns.
  - *Effects*: the placed effects (`GameEffectEntityData.Effect`, and placed objects whose blueprint is an effect),
    each emitter of the blueprint at its `Transform`, with its spawn volume where the emitter document gives one
    (`SpawnPositionData.Pre`: `SphereEvaluatorData` `Radius` x `Scale` per axis at `Pivot`, or `BoxEvaluatorData`
    `Dimensions`, drawn as extents from `Pivot`). Particles are not drawn.
  - *Lights*: BC2 has no light entities. Its dynamic lights are emitters whose template acts as a point light
    (`EmitterTemplateData.ActAsPointLight`): a sphere of `PointLightRadius` at the emitter plus `PointLightPivot`, the
    label's swatch `PointLightColor` (HDR, scaled to its largest channel). No spot lights exist in the data. MP_005
    places none (its three light emitters belong to weapons and the detonation pack); SP_01 places 15.
  - *Sounds*: `GameSoundEntityData.Sound` in the level and `SoundEntityData.SoundTemplate` in effects, with the
    template's minimum distance (the largest of `MasterMinDistance` and its positional emitters' `MinDistance`).
    BC2's sounds have **no maximum distance**: past the minimum they fall off logarithmically by `DistanceSlope`, so
    the label says the slope; the only `MaxDistance` fields are the BigWorld one-shot settings (where around the
    listener a distant sound is played), not an emitter's reach. Sound areas: their `Geometry` and `OuterGeometry`
    (the fade edge, `FadeWidth`).
  - *Combat areas*: `CombatAreaTriggerEntityData.Geometry` (the out-of-bounds edge) as a wall from the ground to its
    points' `Height`, or 20 m where the data has none (labelled unbounded: MP_005's has `Height` 0), and
    `SurroundingGeometry` as a line on the ground; the team's colour with `UseTeamRestriction`.
  - *Capture areas*: `CapturePointEntityData.Geometry` (a prism of its points' `Height`) and `CaptureRadius` above 0
    (a circle; a sphere with `IsCapturedInUpperSphere`), `InitialOwnerTeam`'s colour; `UnableToChangeTeam` is an HQ.
  - *Spawn protection*: `SoldierSpawnEntityData.SpawnEnemyRadius` around each of its `AlternativeSpawnPoints` (the
    spawner's own position without them): the radius a spawn checks for enemies. BC2's level data has no safe-zone
    volume; `ContinueEntityData.SoldierImmortalTimer` (the campaign) is a time, not a place.
  - The overlays replace the scene's outlines of what they cover (`Look::skip_shapes`), so a toggle off draws none.
- **Player's eye camera** (View dropdown, Alt+W; Esc returns to the camera before): the 3D view at the soldier's eye,
  walking on the surface with the CPU ray cast (W A S D, a drag looks, Shift sprints, C crouches). Eye height
  `CharacterPoseData.EyePosition` of `DefaultSoldierPhysics` (stand 1.6 m, crouch 1.0 m; BC2 has no prone pose),
  `StepHeight` 0.3 m, speeds `CharacterStatePoseInfo.Velocity` (4 / 2.5 m/s) and `SprintMultiplier` 1.5; field of
  view `LevelData.DefaultFOV` (55 on every level) x `InfantryFOVMultiplier` (1 multiplayer, 0.82 campaign), taken as
  vertical with the width following the window (Hor+), as the editor's cameras are - not proved against the
  executable in this round.
- **Navmesh and cull tree modes** checked on MP_005 and SP_01: both draw their data (`tests/view_toggles.rs`). The
  cull tree mode now colours all three slot kinds in the 3D view's status tokens (its own green, none yellow, shared
  red; the leaves' spheres blue) - its note had called the yellow orange, the selection's colour.
- **Collision per destruction state.** The scene loader keeps each blueprint's Havok parts beside its merged
  triangles (`Collision::parts`, `bc2_formats::havok::parts`): BC2's `*_Havok.res` starts with a header listing its
  parts (each name's hash, `h * 33 ^ byte` from 5381 over the lower-case `PartInfoData.Name`), and each
  `hkpStorageExtendedMeshShape` subpart record holds its part's index at +0x20. The merged list is unchanged (snapping,
  picking, NavKit); the view draws, per object, the parts its destruction state shows (`collision_part_shown`: a
  part a health state names when that state is its component's). The Display section's State gains the ranks between
  intact and destroyed (`Destruction::Stage`), up / down arrows and Page Up / Page Down, and says plainly when a
  blueprint's collision is one piece in its files.

**Later** (the user, 2026-11-18): scripting and plugins (an API for modders' automations and panels), customisable
hotkeys (shortcuts are fixed and shown until then), and the nearest thing to play-in-editor - a live connection to
the running game (the bot-AI debugger) to see it and jump back to objects. True play-in-editor is not possible: the
editor cannot host the game's engine; the fast build / install / launch of 3.6 stands in for it.

### 3.11 Game-accurate rendering (round R5, 2026-11-18: study and prototype)

The goal of 3.10 ("live rendering"): terrain, buildings and objects in the 3D view look as they do in BFBC2 on PC under
Direct3D 11. This section is what the game draws (read from its data), the two routes compared, the route chosen and
the prototype that proves it (`rust/bc2-render`, the 3D view's **Game-accurate** switch; `rust/BENCHMARKS.md` "R5").

#### What the game draws (from its data)

**The shader databases hold compiled DXBC only - no HLSL source.** Each level has `Levels/<LV>/Shaders/Dx11_Single.res`
(`bfbc2-shader-tools/FORMAT.md`). MP_005's: 1,714 entries, 12,850 programs, 1,527 vertex and 2,166 pixel shaders
(`vs_5_0` / `ps_5_0`, every one built by DICE's "Microsoft (R) HLSL Shader Compiler 9.27.952.3022"), 2,112 parameter
blocks, 114 vertex layouts, 31 different render-state words. What the file records beside the code:

- each shader's **own reflection** (`RDEF`): its `$Globals` variables by name, offset and size, its textures and
  samplers by name and register - so every constant a shader reads is named;
- **binding tables** (which engine value goes in which register: `viewProjMatrix`, `worldMatrix`, `cameraPos`,
  `outdoorLightDir`, `outdoorLightKeyColor` / `TopColor` / `BottomColor`, `outdoorLightShadowTransform0..2` (three
  shadow cascades), `boneMatrices`, `pointLight*`, `time`, `invScreenSize` ...) and **register tables** (the textures
  the engine binds: the shadow map, `outdoorLightReflectionEnvmap`);
- **parameter blocks**: a material's named constants with their registers and values, its textures by sampler, the
  textures the engine binds by name (`heightmap`, `maskTileMap`, `DepthBuffer` ...), and the pixel shader's sampler
  states as whole `D3D11_SAMPLER_DESC`s;
- **programs**: vertex and pixel shader, two parameter blocks, the render state (blend, depth, culling, alpha to
  coverage: FORMAT.md 6, read from the exe) and a key (pass: main / depth only; mesh kind: rigid, bones, skinned,
  destructible composite; instanced; point light; the vertex layout).

**Mesh materials.** A draw is the entry `<mesh>/<shader>` for each part of a mesh set's chosen shader set, its
main-pass program for the part's vertex layout. Texture kinds by DICE's names (MP_005's 587 textures): colour (`_C`,
`_D`, `_Df`), normal (`_Nm`, `_N`), specular (`_S`, `_Sp`), gloss (`_Gm`), baked ambient occlusion per mesh (`_Oc`,
`_OcRGB`, `_Occlusion`, `_AO`) and sky occlusion (`_SkyOc`), material masks (`_RGBmask`, `Msk_*`) with tiling detail
textures (`Dt_*`, `Nm_*`), the level's reflection cube (`texture_outdoorLightReflectionEnvmap`: the level's
`StaticEnvmapEntityData`, MP_005's `mp05_envmap`, radius 5,000 m). **No classic lightmaps**: buildings carry baked
occlusion textures and an `IrradianceVolume` (`<mesh>_Irradiance.res`, 19 in `mp_common`; not decoded yet), and the
databases hold Enlighten's entries (`Systems/Enlighten/DirectIllumination`, `DiffuseDuster`; MP_005's
`EnlightenSkyColorIntensity` is 0).

**Terrain.** `PatchMaskShaders/<layers>` entries (MP_005: 50, `Dunes_Asphalt_Destruction` ...) drawn on 16 x 16 m
patches. Read from the `VisualTerrain` (`<lv>.Visual.res`) and the shaders' disassembly:

- the patch map (MP_005: 128 x 128 cells, header `[66, 1, 15, 7, 128]`: tile size, border, tiles per row, rows, cells
  per side) gives each cell its patch shader and up to four mask tiles (`u16`) with flags (`u8`, 8 where tiled);
  rows run along z (sea-floor patches lie 37 m lower that way round) and a cell's tiled-mask count equals its non-zero
  flags on all 16,384 cells;
- the **mask-tile atlas** (`<lv>.MaskTileAtlas.res`, L8, 1024 x 512: 66-pixel tiles with a 1-pixel border) holds the
  tiled layers' masks; the other layers are procedural ("rule" layers: slope, height, noise in the shader);
- the vertex shader reads a grid point (`R16G16_SINT`) and two masks' atlas coordinates per `R16G16B16A16_SNORM`
  stream, samples the **height map** and places the vertex at `(x, h, z) * posScale + posOffset`; the pixel shader
  takes the world back through `heightmapScaleAndOffsetXZ`, its normal from neighbouring height samples, the level's
  **colour / occlusion map** (`mp05_AmbientOcclusion`, over the whole 2 km) and per layer a colour, normal and detail
  texture blended by DICE's per-shader formulas (literal weights compiled into each shader), then the dynamic
  (destruction) mask;
- road and decal strips are baked into the `VisualTerrain` (renderqa `strips`), undergrowth comes from the
  `UgMaskShaders` entries.

**Water.** `Shaders/WaterOcean` (6 programs: lerp-blended, `0x581`; a depth-only program reads `DepthBuffer`) on the
`VisualWater` mesh (`<lv>.Water.res`, per-triangle shaders: `bc2_formats::water`), with `WaterTile_t01_Nm` and the foam
texture, `time` for the waves.

**Sky, fog, light.** The weather's active state (`WeatherSettings.ActiveState` into `WeatherSystem.States`; MP_005:
`Default`, whose partitions are `Sunny`'s) names `Sun` (theta, phi), `OutdoorLight` (key, top and bottom colours with
intensities, specular scale, hemisphere), `Fog` (start, end, colour, density, height fall-off), `Sky` (horizon
coefficients, halo, haze, Mie / Rayleigh constants) and `PostProcess`. **No object shader reads a fog constant**: the
sky and the atmosphere are `Systems/Weather/Shaders/Surface/EnvironmentMain` (and its half-resolution and cube
variants), a pass that reads the **depth buffer** and the level's panoramic sky (`mp05_Skydome`, `_A`) with twelve
`g_*` constants (horizon coefficients, halo, haze, sun direction, Mie mu / sigma) - so haze and fog are laid over the
drawn scene by depth.

**Post-process and shadows: the engine's own shaders are in `async/startup-00.fbrb`** as standalone resources (112
`Dx11PixelShader`, 14 `Dx11VertexShader`): `Systems/PostProcess/ShaderContainer/psDownsampleHalfGrayscaleLog`,
`psExposure` (auto exposure), `psFilmicTonemap` and its variants with bloom blur, depth of field, grain and colour
effects, the separable bloom filters, `HorizonBasedAO` / `HorizonBasedAOBlur` (HBAO: the game's ambient occlusion,
its `Hsao*` settings in the exe), `psDrawParticleVertexShadows*` and `psDrawEnlightenShadows`. Shadows are three
cascades (`outdoorLightShadowTransform0..2`, a comparison sampler on `s0`), drawn with each entry's depth-only
programs (pass 5) - all in the databases.

#### Route A: DICE's own Direct3D 11 shaders, unchanged

A Direct3D 11 device in Rust (the `windows` crate, the version wgpu already brings in) drawing each program's DXBC
with its input layout, constant buffers filled by name from `RDEF`, its parameter blocks' constants, textures and
sampler states, and its render state - off screen, the frame then shown in egui. This is the suite's render harness
(`renderqa.py` / `shaderview.cpp`, which already drew DICE's meshes, terrain patches and strips this way) moved into
the editor.

- **Exact by construction** for everything the shaders compute: the result is DICE's code on DICE's data. What is
  not in the data are the **values the engine computes per frame**, each one a named constant now: `outdoorLightDir`
  from the sun's theta / phi (OPFOR's Wake formula does not carry over: it points below the horizon on MP_005), the
  shadow cascades' transforms, `EnvironmentMain`'s twelve `g_*` values from the Sky and Fog states, the exposure /
  tonemap constants, tree and wind vertex constants, the terrain's per-patch stream (the prototype's reading of the
  atlas tiles, above) and LOD choices. A single RenderDoc capture answers all of them, draw by draw (the capture asked
  for is in the R5 report).
- **Interop with wgpu.** Phase 1 copies: the float frame read back, tonemapped on the CPU, uploaded as an egui texture
  (costs in `rust/BENCHMARKS.md` "R5"). Without the copy: the frame in a shared texture (`D3D11_RESOURCE_MISC_SHARED_NTHANDLE`)
  opened by wgpu's Direct3D 12 backend (`ID3D12Device::OpenSharedHandle`, wgpu-hal's `texture_from_raw`) or by its
  Vulkan backend (`VK_KHR_external_memory_win32`, the D3D11 texture handle type) - both need wgpu-hal's unsafe
  interop, and the editor runs on Vulkan today.
- **Costs:** Windows only (the game and its users are); `unsafe` foreign calls, confined to `bc2-render`'s `d3d`
  module; two GPU APIs in one window.

#### Route B: DICE's material model again in WGSL on wgpu

Portable, but approximate by nature: MP_005 alone has 2,166 distinct compiled pixel shaders, each a shader graph with
its own blend formulas and literal weights (the terrain patch above: per-layer weights such as 0.7025 / 0.6755 / 0.648
compiled in). A hand-written "material model" reproduces none of them exactly; translating every DXBC program to WGSL
would mean a DXBC decompiler (naga reads no DXBC) and still a proof against route A's picture. Portability buys
nothing here: the game, its data and its users are on Windows.

#### Recommendation: route A

Evidence (this round):

- **The Rust port draws DICE's shaders exactly as the harness does:** MP_005's `IND_Warehouse` with its material-mask
  mesh, renderqa's three views at 512 x 512, its light and post: the Rust `bc2-render` picture and the C++
  `shaderview` picture are **identical, pixel for pixel, in all three views** (786,432 of 786,432 pixels;
  `bc2-render/examples/harness_views.rs`, `tests/warehouse.rs`). The harness had already validated 15 of 16 stock
  meshes against the game's look, 33 of 33 terrain patch entries of mp_009cq and DICE's road strips.
- **Terrain patches draw with their real layers**: MP_005's patch map, height field, mask atlas and patch shaders give
  continuous ground across patches (no seam where tiles meet), asphalt with its road markings where the harbour is,
  and the occlusion map's dark footprints under the buildings.
- **The editor shows it**: the 3D view's Game-accurate switch draws MP_005's placed objects and the patches around the
  camera with DICE's shaders (`tests/game_view.rs`; screenshots with the R5 report).

The editor keeps wgpu for its own drawing (the editor's view, the overlays, collision, navmesh and cull-tree modes of
3.10 item 5); the game-accurate frame is one more mode of the 3D view.

#### Phased plan

1. **Phase 1 (R5, done):** the study; `bc2-render` (files, DXBC reflection, materials, weather light, terrain
   patches, frame and draw order, the D3D11 device, the harness's tonemap); the 3D view's switch; parity with the
   harness; costs measured.
2. **Phase 2a (R5b, done: what the data alone determines, no capture):**
   - `bc2_render::world::World`: a level's placed objects and model-group members (`placement`: the partition's
     `MeshShaderSet` overrides, picked from the mesh set by the set's own `path/GUID` name; the groups' `MemberDatas`
     with each instance's own set - MP_005: 554 of 3,915 drawn with an override), the terrain the first
     `TerrainEntityData` names, the baked road and decal strips (`VisualTerrain`'s road blocks, the strip layout), the
     visual water (`water`: `.Water.res` ranges with their slots' entries; `g_depthScaleFactors` / `g_screenSizeInv`
     / `sunDir` written by name over their block placeholders), the weather the partition references.
   - Bone-transformed and skinned programs place a vertex by `boneMatrices` and `viewProjMatrix` alone, so each bone
     is now the world matrix times the bone at rest (the trees and other kind-1 meshes were drawn at the origin).
   - DICE's post shaders (`post`, from `Dist/win32/async/startup-00.fbrb`) as full-screen `vs2D` passes on the
     device: `psUnprojectDepthBuffer` before the first draw reading `DepthBuffer`; `psDownsampleHalfGrayscaleLog`
     and `psDownsampleHalf` to 1 x 1, `psExposure`, `psDownsampleQuarter`, `psSeperableFilter15H/V`,
     `psFilmicTonemap`. `post::PLACEHOLDERS` lists the values not in the data (exposure blend, kernel, colour
     matrix construction, vignette, flare, samplers) for the capture.
   - Proofs (`examples/world_view.rs --job`, `tests/world.rs`): the whole MP_005 frame (10,417 draws: objects,
     members, patches, strips, water) written as a `shaderview` job (`job`) and drawn by the C++ harness:
     921,600 of 921,600 pixels bit-identical (colour and draw class) at the warehouses; at the west shore 921,599
     (one far horizon pixel, 3.8e-4; 3 with the water left out - depth ties between distant draws, open). Getting
     there: engine textures get a `GenerateMips` chain as the harness's float textures do (the height map's implicit
     LOD reads differed). `psUnprojectDepthBuffer` against `n / (1 - z / q)` in singles: worst 3.8e-5 relative;
     `psFilmicTonemap` (bloom off) against the harness's CPU curve: worst 2.4e-7.
   - `examples/levels_check.rs`: all 37 prepared stock levels load and draw with DICE's post-process. Missing:
     `ONS_MP_005GR` (its partition names MP_005's terrain, whose visual terrain its terrain bundle lacks: SP_005's
     drawn, with no program for its patch, strip and water entries), two-layout gaps on ONS_MP_005GR / ONS_MP_008
     (radar dish, antenna, lamp metal parts), SP_01's `waterfall_drip_01` (no mesh data), SP_006 / SP_009 water with
     no range.
2. **Phase 2, the engine's values (one or two rounds, with the capture):** the sun direction, shadow cascades (the
   depth-only programs, three cascades), `EnvironmentMain` (sky, haze, fog), and DICE's own post-process
   (`psDownsampleHalfGrayscaleLog`, `psExposure`, `psFilmicTonemap*`, bloom, HBAO from `startup-00.fbrb`) replacing
   the harness's tonemap; the terrain stream checked against the captured vertex buffers; water (`WaterOcean` on the
   `VisualWater` mesh); static model groups (`*_HavokAsset` members), undergrowth, road strips, trees' engine
   constants; the `IrradianceVolume`. Each proven against the capture's own draws and its final picture.
3. **Phase 3a (R5c, done: interactive and editable, the picture unchanged):**
   - `bc2_render::live::Live`: the draw list built once. Each instance's draws are planned once (`live::plan`: the
     bytes `frame::cbuffer` writes, the camera's values - `viewProjMatrix`, `viewMatrix`, `cameraPos`,
     `worldViewProjMatrix`, `invScreenSize`, `g_screenSizeInv`, `g_depthScaleFactors` - left as holes filled per
     frame); the renderer's order kept as indices (only the blended draws sorted again per frame, farthest first, the
     given order among equals); the depth pass where the first reader of `DepthBuffer` is in the whole list.
   - **Culling** (`Options::cull`, on by default): a draw whose bounds (its part's positions, through its bones at
     rest, a tenth of its size and at least a metre of margin) lie wholly outside the frustum is left out; instances
     sit in a 64 m grid tested first. Exact: the rasterizer clips to the same planes. Strips and water have no
     bounds and are always drawn.
   - **The device** (`d3d/live.rs`): each (mesh, draw) resolved once into a material (shaders, layout, buffers,
     texture and sampler arrays, states); the frame's constants uploaded with one `Map` into a ring bound by offset
     (Direct3D 11.1 `VSSetConstantBuffers1`; `BC2_RENDER_NO_CB_OFFSETS` or a device without it: one update per
     draw); only the bindings that changed since the last draw are set. No state sorting: it would change the draw
     order, and with it depth ties and blending.
   - **Edits** (`World::set_placed`, `Live::set`): the instances whose key, mesh and transform are unchanged are kept
     as planned; moved, turned, added, removed and duplicated objects are planned again (the editor's `GameView::sync`
     follows the scene by a fingerprint of its placed things).
   - **No read back:** the finished frame is copied on the GPU into one of three `R8G8B8A8_UNORM` textures shared by
     NT handle, its alpha written as 1 (the tonemap's alpha is not coverage: R5b's upload showed it as holes), and
     opened on egui's Vulkan device (`VULKAN_EXTERNAL_MEMORY_WIN32`, wgpu-hal's `texture_from_d3d11_shared_handle`),
     an event query waited for before egui samples it. `BC2_GAMEVIEW_COPY=1` / `GameView::copy`: the read back and
     upload as before.
   - **Picking** replays the frame's draws into the stencil inside a one-pixel scissor, a byte of the object's
     number per replay (DICE's own shaders decide coverage, discards included; draws that write no depth leave it
     alone). **The selection** is replayed against the frame's depth into stencil bits (`EQUAL`: seen, `GREATER`:
     hidden) and outlined in the view's selection orange by a full-screen pass (our own HLSL, compiled at run time
     by `D3DCompile`): seen edges solid, hidden edges faint. The gizmo and the labels stay egui's overlays above it.
   - **Package levels** (Vietnam, the Pacific War, DICE's other Package-only levels): `World::load_package` reads
     the level's bundle stack from `package.mft` in place (`Files::bundles`: the first bundle that lists a name decides
     it, *deleted* markers hide it), in rounds - each round's missing files read with one streamed pass a bundle -
     until nothing more is asked for (Vietnam's NAM_MP_002CQ 11 rounds, 84 MB; Wake Island 12 rounds, 134 MB).
     Nothing is written to disk.
   - Proofs and costs: `rust/BENCHMARKS.md` "R5c".
4. **Phase 3b (R5d, done: the visual gaps; `rust/BENCHMARKS.md` "R5d", spec RNDR-TERR-5, RNDR-WATER-2,
   RNDR-SKY-1..3, RNDR-LOD-1, RNDR-VIEW-1..2, TERR-VT-7):**
   - **Terrain masks** (`terrain::Terrain::patch`): a mask tile spans `flag` x `flag` cells (8 or 4) aligned in the
     patch map; each cell reads its part of the tile. R5 to GW1 stretched the whole tile over each cell (blocky,
     repeating masks; the WW2 shores' ragged cell edges).
   - **Engine values the terrain and water read** (`terrain::engine_values`): `heightmapWorldScaleY` is the height
     field's range (the records' 500 is a placeholder: Wake's 1,000 m terrain drew its beach black);
     `heightmapWorldScaleXZ` / `OffsetXZ` and the height map bound for the water (`water::load` takes the terrain's
     values: DICE's Vietnam water and OPFOR's Wake water read them).
   - **The sky** (`sky`): `EnvironmentMain` on a camera-following dome (`live::Key::Sky`, `Dyn::World`,
     `Dyn::SkyDepth`), drawn first of the blended draws (after the depth pass, which it reads); values from the
     weather state's `Sky`, `Sun` and `Clouds` by name; the panoramic clouds off (`sky::CLOUDS`) until their
     composite is proven. `World::sky_pass` (off by default: the harness proofs draw no sky; the editor turns it on).
   - **Levels of detail** (`live::Options::lod`, `Options::exact` for the proofs): terrain patches by distance
     (`terrain::patch_lods`: one shared index buffer with every level, each level's outer edges whole, so no cracks;
     `Step::range`), objects by their mesh set's `MeshLodSettingsAsset` (`material::LodSettings`, one source per level
     with its distance band, `live::LodBand`); meshes with alpha-tested or blended parts keep level 0.
   - **Labels** over the game view: the frame's depth read back after each drawn frame (`Device::read_depth`, about
     2 ms at 920 x 660), `GameView::hidden` / `frame::occluded`.
   - **Package loads** (`Files::fetch`): a round's bundles read at once through the suite's bundle cache
     (`fbrb::read_many`: the in-process session and the shared disk cache), `BC2_RENDER_UNCACHED` for the old path.
   - Not done: the instanced programs (`StreamInstancingFloat`: a per-instance stream and program choice, the draw
     count is no longer the bottleneck after the LODs), the haze over the scene (`EnvironmentHalfResMain`, the fog:
     with the capture), `MinScreenArea` culling (its unit).
5. **Phase 3c (R5e, done: the WW2 open sea; `rust/BENCHMARKS.md` "R5e", spec RNDR-DRAW-8, RNDR-WATER-3):** a
   program's material goes to the registers its block header names (`query::block_registers`, `Draw::ps_window`,
   `frame::material_window`): the shader's `constants` array where it has one (every DICE program), else the header's
   window. OPFOR's translated BF1943 water reads its material as `waterMaterial`; before R5e its `g_depthScaleFactors`
   was never written, the water's alpha was 0 everywhere and the black sea floor showed to the horizon (R5d's
   "haze" reading of the deep sea was wrong: no fog is needed). DICE's levels draw bit for bit as before.

### 3.12 Exporting a level as a new multiplayer level (round E3, 2026-11-18: study and first export)

The user's goal: load Operation Aurora's map, edit it, save, export it, and find it in the level list on the next
launch. A campaign level is exported as a NEW multiplayer level; no original is ever written. Built in round E3:
`bc2_levelwrite::{export, mpconvert, culltree}`, the `mapexport` binary, the editor's Export window
(`bc2-editor/src/export_ui.rs`), and OPFOR's `src\tools\mapedit\mapedit_install.py`.

#### What a campaign level lacks for multiplayer (from the data)

The ground truth is DICE's own conversion of a campaign level: Heavy Metal's Conquest, MP_SP_005CQ, made from SP_005.
It was compared instance type by instance type with SP_005, SP_01, MP_005 and MP_009CQ (and Napisal's MP_SP_001CQ,
his Aurora Conquest).

- **The world stays.** DICE kept the objects (`ReferenceObjectData`, in fact more of them), terrain splines, model
  groups and their members, effects, sound areas, weather states, damage and clear areas, and predestruction.
- **The campaign's scripting goes.** Every instance of these types was removed:
  - AI orders (`AreaOrder`, `PatrolOrder`, `InterludeOrder`, `CancelOrder`, `ShootAtObjectOrder` ...);
  - cutscenes (`Cutscene*`, 170 `CutsceneSoldier` on SP_01) and `PlayVideo`;
  - mission objectives and saves (`MissionObjective`, `Save`, `Continue`);
  - the campaign's AI spawning (`SoldierSpawnGhost`, `SquadGhost`, `SquadEntity`, `SpawnPointManager`, 190 AI and
    player `SoldierSpawn`s on SP_01);
  - waypoints and targets (`AnimatedWaypoint`, `Waypoint`, `WaypointAnimation`, `TargetObject*`, `StaticPosition`);
  - scripted triggers (`VehicleStateTrigger`, `UnderFireTrigger`, `LookAtTrigger`, `CharacterStateTrigger` ...);
  - tooltips, debug menus and screen fades;
  - scripted demolitions (`ExplosionPack`, `ExplosiveObject`), artillery strikes and weapon pickups.

  `mpconvert::CAMPAIGN_SCRIPTING` has the list, computed from 13 campaign against 24 multiplayer levels (Onslaught's
  AI levels left out). These types are in no multiplayer level, or DICE removed every campaign instance of them.
- **The game-mode layer comes in.** A Conquest level's layer is the same set of types on every stock level:
  - `AutoTeam` and three `TeamEntityData`s, 12 `TeamFilter`s, two `TicketCounter`s, `PreRound`, `Spectate`;
  - `SquadData`s, 16 `Message`s, the announcer's `VoiceOver`s and `VoiceOverLineAsset`s;
  - an MP `HumanPlayer`, `Mission`, `Menu`s, `InputRestriction`;
  - the event glue (`EventSplitter`, `DelayTrigger`) wiring player enter, death, spectate, the win menu and the ticket
    messages;
  - the `CapturePoint`s: two team bases (`UnableToChangeTeam`, each with HQ `SoldierSpawn`s and `AlternateSpawn`s)
    and the flags (`objects/gameplay/flag_*`, `mp_flagpole`, a `MapMarker` each);
  - one `CombatAreaTrigger` with its polygon (`AreaGeometryEntityData` points linked by `Next`).

  Without such a layer, a campaign level hosted as a match stalls in session state 14 (OPFOR's SP_005 test,
  `research/BFBC2_NAPISAL_RESEARCH.md` 7).
- **The level's fields.**
  - Campaign values: `HasAI` true, `StartLevelFadedToBlack`, a `SubtitleAsset`, `WaypointAnimations`,
    `PathfindingTypes`, `SpawnPointManager`, `InfantryFOVMultiplier` 0.82.
  - Multiplayer values: false, null, empty, and 1. The `LevelDescription` gets the mode's `GameMode`
    (`gamemodes/conquest`), `FactionTeam1` / `2` US / RU, `WaitForButtonBeforeStarting` false, no achievements or
    collectibles, and the mode's `KitImages`.
  - DICE kept the base's minimap reference (`levels/sp_005/ui/minimaptexture`): the new level reads it from the
    original.
- **Bundles.**
  - MP_SP_005CQ's `package.mft` blocks target `mp_common` (plus the DLC's `mp2_common`, which holds SP_005's
    terrain); campaign levels target `sp_common`.
  - The engine finds some files by the level's name, so a new name needs its own copies: the shader database
    collection and its databases, the emitter system and atlases, the model groups (`*ModelGroupEntity*_HavokAsset`,
    `ForestEntity_HavokAsset`), the static cull trees and the animation sound map. Proof: DICE's MP_005GR holds all
    of them under its own name, and deleted markers hide MP_005's. The Onslaught ports copied them as well
    (`console_onslaught.rename_level_files`).
  - The level partition names only the cull trees, the material grid, the minimap and its ambient sounds by
    reference.
- **Menus.**
  - `Levels/<LV>_description.dbx` in `Package\async\startup-00.fbrb`; without it the load crashes in state 13.
  - Its two `LevelListQuery` items (`<lv>.dbx`, `<lv>_description.dbx`); without them the level is in no menu.
  - OPFOR's `maps.ini` section gives the launcher's and the server browser's row its name.
  - The description's `Name` loc key also names the loading picture (cut at `_NAME`).
- **Kits and unlocks** come with the mode: the MP soldier kits from `mp_common` and the description's `KitImages`.
  Nothing per level.
- **Voice-over bundles.** The stock levels' `vo/<lang>-00` bundles are registered in `package.mft` but absent from
  OPFOR's game copy. A new level ships and registers none (`export::KINDS`: level, terrain, ingame, loader).
  Round E3c changed this: each stock level's `vo/<lang>` block targets its set's common voice-over bundles, which
  are there, so a new level now registers the donor's blocks (see "Round E3c" below).

#### The new level's identity

- **Its id** is `Levels/<LV>` (letters, digits, `_`; for the first export `MAPEDIT_AURORA_CQ`). Its folder is
  `game\Package\levels\<lv>\`. The export refuses an id that is a level of the game copy or registered by another
  tool. Only an earlier install of an export may be replaced; it is marked by `opfor-mapedit-installed.json`
  (section 6, ruling 1).
- **The level partition** is the edited base partition renamed: a new partition guid (UUID v5 of its name, as the
  Onslaught copies), `LevelData.Name` and `id`, and the references to the copied root files re-pointed.
- **The root files** are copied under the new name, their own names re-pointed (`mpconvert::rename_strings`). 22
  files on SP_01, including 44.6 MB of shader databases; the level bundle is 11.7 MB.
- **The rest is read from the original's bundles.** The new level's `package.mft` level block targets its own
  bundle, then `package:` and `game:levels/sp_01/level-00`, then `mp_common`, then `sp_common`. This is DICE's
  pattern for mode levels (MP_005GR targets MP_005's bundle).
  - The new level's own bundle comes first. It also holds what the base's deleted markers hide: SP_01's bundle hides
    `mp_common`'s neutral flag and spawn camera, so `export::fill_hidden` copies those 12 entries from `mp_common`.
  - The terrain block targets SP_01's terrain bundles; the level's own terrain bundle holds SP_01's one Package
    entry.
  - The ingame and loader blocks target `mp_common`'s (the MP HUD and loading UI). The new level's ingame holds
    SP_01's minimap and its loader SP_01's loading music.

#### Building it

`mapexport build <project> <export.json> --game <game> --out <stage>` (or the editor's Export window):

1. The base partition is read from the game's bundles (`Package` before `Dist`) and checked against the project's
   sha1. When it differs, the base-changed report runs. Then `levelwrite::apply` replays the log.
2. `mpconvert::strip` removes the campaign's scripting and what the layer replaces, in one pass
   (`LevelWriter::delete_many`: event connections and references cut as DbxEdit's `drop` cuts them).
   - On SP_01: 1,785 instances, 1,185 event connections and 3,639 references.
   - One of the base's combat areas is kept (the spec names it).
3. `mpconvert::graft` copies the donor level's layer.
   - Donor: MP_009CQ, a stock Conquest level, chosen because its layer refers only to `mp_common`. MP_SP_005CQ's
     layer would need the DLC's `mp2_common`.
   - What it copies: the seed types, the chosen capture points, and what they refer to or their events reach within
     the layer's types. Each copy gets a new UUID v5 guid. Event connections, `Objects` items and the voice-over
     arrays come too.
   - Anything else is cut and listed: the donor's vehicle spawns, and the flag C not chosen.
   - On SP_01: 158 instances and 75 event connections.
4. `mpconvert::place` moves each base and flag, with its spawns, alternative spawn points, map marker and area, so
   its capture point stands at the spec's x, z.
   - Heights follow the ground: each instance keeps its height above the donor's terrain, over the base's terrain
     (both height fields read from the bundles).
   - Then `level_fields` sets the level's fields, and the donor's `LevelDescription` comes in with the base's name,
     minimap, music and rich presence.
5. `culltree::reslot`: a moved object, or a copy sharing its original's slot, gets new leaves in the exclusive cull
   tree.
   - This is `culltree.py add` ported, the shape taken from the object's own leaves in the base, moved with it.
   - Its old leaves stay unregistered, as a deleted object's do.
   - Otherwise a moved object is culled where it used to stand.
6. The root files are renamed, the description is built (MP_009CQ's, with new guids and the level's name, map and
   music), and the bundles are packed (the suite's packer). `stage.json`, `report.txt` and `registration.txt` (what
   an install adds) go beside them.

`mapexport check <stage> --game <game>` proves the stage offline:

- each bundle is the one recorded, opens, and every partition parses (the DBX reader and `EditDbx`);
- a `package.mft` block and a `bundleManifest` line per bundle, and every target bundle exists;
- the name is no level of the game copy;
- every local reference names an instance of its partition;
- every reference into another partition (8,578 into 777 partitions on SP_01) names a partition in a bundle the
  level loads, and an instance in it;
- references to partitions no bundle of the whole game copy has are allowed only when DICE's own source partition
  has the same reference. Two on SP_01: `animations/settings/quality/1p/zoomquality` and
  `objects/objects/shaders/flag_neutral`, both from stock MP data;
- `LevelData.Name` and the description's `LevelName`;
- the cull tree is one preorder tree, every slotted object has its own slot, and every slot is a leaf.

**Install and uninstall (Project OPFOR).** `mapedit_install.py --plan <stage>` runs the check, then builds the four
shared files into `work\mapedit\<lv>\shared` and prints every change. `--apply` installs in WW2's order, and every
check runs before the first write:

1. the level's bundles go into `Package\levels\<lv>\`, with `opfor-mapedit-installed.json` (their sha256);
2. `startup-00` gets each exported level's description and list items (`mapexport startup`: FbrbTool's unpack and
   pack, the list kept sorted as DICE's is);
3. `maps.ini` gets a section per exported level after the tool's marker;
4. last, `package.mft` and `bundleManifest`.

The backups and the revert:

- **Backups** follow `console_common`'s stage protocol: stage `mapedit`, `<file>.opfor-mapedit-orig`, the last-writer
  record. The file's earlier record is kept as `.opfor-mapedit-prevlast`.
- **The shared patch** is always built from that backup while the tool wrote the file last, so every exported level
  installed is in one patch.
- **`--revert <LV>`** checks the bundles' sha256 first and takes the registration out first.
  - The last level out: each file comes back from its backup, with its earlier last-writer record, so the WW2
    opt-out still recognises its own state.
  - One level among several: the patch is written again without it.
  - The level's folder goes last.
- **Refusals.** Both refuse while the game runs, and when another tool wrote a shared file after this one (revert
  that tool first, as the stages already do).

**Bots (later).** `HasAI` is false. Bots need NavKit path data for the new level, written under
`Output\win32\ai\levels\<lv>`, with `HasAI`, `PathfindingTypes` and the `SectorOffsets` partitions under the new
name. They also need the AI layer (stockmp-ai's spawners and orders) or the AI workspace (3.4a, E5 / E5a). SP_01's
own campaign path data covers the campaign's routes, and a NavKit build from the edited collision is the right
source. A NavKit build of sp_004_b takes 186 s.

**The editor.** The Export window (the "Export as a multiplayer level" action, also in the command palette):

- the form: map name (the `maps.ini` row), level id, game mode (Conquest; the other modes need E4's mode
  definitions), teams (the layer's), the bases' and flags' x / z, and the combat area kept;
- the form is kept as `<project>.export.json` beside the project (data only);
- the checks run as the form changes, and Export stays disabled with the reason while one fails: project saved,
  valid and free level id, a supported mode, two bases, at least one flag, a combat area, a game copy;
- Export builds and checks on a worker thread, then shows the report and the install command. The editor never
  writes the game copy.

E4's gameplay panels replace the x / z fields: placing flags, spawns and combat areas in the 3D view, as operations
in the project log.

#### First export: SP_01 as Conquest (offline)

`examples/aurora_export.rs` writes a project and an export file:

- the project moves the two slotted objects nearest the campaign start 10 m east and deletes the next two;
- the export file names `MAPEDIT_AURORA_CQ`: MP_009CQ's two bases and flags A and B placed between the beach and
  the inland end, and CombatArea6 kept.

The results:

- **The stage.** Level 11.7 MB, terrain 0.1 MB, ingame 1.6 MB, loader 2.0 MB. The build takes 26-45 s (debug, on
  a shared PC) and `mapexport check` passes every check (`tests/export_sp01.rs` repeats it).
- **The cull tree.** The two moved objects got slots 8,248 and 8,250 (leaves 4,612 -> 4,615); no slot is shared.
- **The game copy did not change.** All 1,582 files of `game\` have the same size and time after the build, check
  and plan, and 17 key files have the same sha256: SP_01's bundles, `mp_common`, `sp_common`, both startups,
  `package.mft`, `bundleManifest`, `maps.ini` and `opfor.ini`.
- **The install and revert.** OPFOR's test runs `--apply` and `--revert` on a scratch copy of the shared files.
  Every file comes back byte for byte, with its last-writer record.

**Open for the in-game test.**

- Placement: the bases' and flags' places are first guesses from the terrain. Spawns over water or inside rocks
  show only in game.
- Campaign set pieces: SP_01's 53 campaign vehicle spawns were removed (E4 places multiplayer vehicles).
- Loading picture: the level keeps SP_01's loc keys (`SP_01_NAME`), so the loading picture is looked up as SP_01's.
- WW2 profile: under the user's 2026-11-16 note, an Aurora Conquest is a Pacific War map (WW2 kits). This export
  uses the BFBC2 profile and modern kits.

#### Round E3b: profiles, the level's own name and picture, the campaign checklist (the user, 2026-11-19)

The user's answers to E3's open points: levels for BFBC1, BFBC2, Vietnam and WW2 by choice; each export with its own
in-game name and loading picture; the user chooses what a campaign base loses; placement stays E3's first guess,
editable, until E4's gameplay tools.

**Content profiles** (`bc2_levelwrite::profile`). Nothing is a hand-kept list; each profile is read from the stock
levels of its set in the game copy:

- **The set** of a level is what its bundles load (`Profile::of_stack`, the start screen's recognition from G3):
  a level block that targets `ww2_common` is WW2, `nam_common` Vietnam, a `bc1_` level of `game\Package` BFBC1,
  anything else BFBC2. Its multiplayer levels are those whose description names a multiplayer game mode
  (on OPFOR's copy: BFBC2 38, BFBC1 8, Vietnam 20, WW2 4).
- **The common bundles** of each kind are the `_common` targets every multiplayer level of the set loads, in order:
  BFBC2 `mp_common`; BFBC1 `mp2_common`, `mp_common`; Vietnam `nam_common`, `mp_common`; WW2 `ww2_common`,
  `mp_common` (terrain: `mp2_common` / `nam_common` only). The new level's `package.mft` blocks take them in place
  of E3's fixed `mp_common`.
- **The donors** are the set's levels of the mode (Conquest: `conquest`, and Vietnam's and WW2's `namconquest`).
  The donor's description brings the profile's own data: the game mode (its HUD, kit bundles), `FactionTeam1` / `2`,
  the kit pictures, `Licenses` (`nam` on Vietnam and WW2), `KickIfLicenseMissing`, `MatchmakingModID` (`BC2`,
  `VIETNAM`, `PACIFIC`), the start delay and the level's memory. The default donor is the first by name whose layer
  names nothing outside the set's commons and the startup bundles (BFBC2 `mp_001`, BFBC1 `bc1_harvest_day_cq`,
  Vietnam `nam_mp_002cq`); the form lists them all. WW2's donors all name their team kits and flags
  (`objects/teams/ww2_us`, `ww2_jp`, `flag_us_1943`, `flag_jpn`) in their own bundle, which the export copies.
- **The profile's level fields** are the `LevelData` fields every donor of each set has with one value and the sets
  do not share: on OPFOR's copy only `SoundStates` (`sound/nam_soundstates` on Vietnam). The export takes the donor's
  where the base's differs.
- **The team labels** are the donor's factions through the profile (`factions::faction_side`: `FactionRUS` is RU on
  BFBC2 and BFBC1, NVA on Vietnam, JP on WW2).
- **Hidden partitions.** `nam_common` and `ww2_common` carry deleted markers over modern content of `mp_common` and
  `sp_common`, among it partitions SP_01's own world uses (destruction effects, havok props: 466 entries on the
  Vietnam export). The export copies what no loaded bundle gives from the level block's own bundles (each read
  alone), then the donor's, into the new level's bundle; the check takes a copied partition's references as DICE's
  when the bundle it was copied from has them too. Only the shared bundles (`_common`, `async`) count for "a partition
  no bundle has": a level's own bundle is loaded by no other (OPFOR's `iwo_jima_s` holds a root `emitters/...` copy).

**The level's own name and picture** (`ExportSpec::identity`, `bc2_levelwrite::awards`; OPFOR's WW2 install's way):

- the description's and the level's `Name` / `Description` loc keys are `<LV>_NAME` / `<LV>_DESC`, with the
  export's title and description as their texts in every language of `Dist\win32\overlay-00.fbrb`
  (`language_<lang>_normal.bin`, plain ASCII). The server browser's and the launcher's row is `maps.ini`'s title;
- the loading screen cuts the name key at `_NAME`, so its picture is `Awards/images/loadingScreens/<lv>.res`; the main
  menu's map list shows `Awards/images/Levels/<lv>.res`; both go into `Package\async\ondemand_awards-00.fbrb`;
- the pictures are made from the user's PNG or JPEG (empty: the base's own loading picture), scaled to cover and cut
  at the centre (Lanczos-3), in the form of the donor's own pictures, read from them: DXT1 1280 x 720 one level for
  the loading screen; DXT5 512 x 256 for the map list, the picture in the box its alpha shows (295 x 196 top-left),
  white at alpha 0 elsewhere. The suite's texture writer (`texwrite`, F2a) encodes them, each block's axis from a
  3 x 3 Jacobi solve; each is decoded back and its PSNR against what was encoded reported (about 41 dB loading,
  40 dB map list on the test picture);
- `mapexport overlay` and `mapexport awards` build the two bundles from the install's backups (FbrbTool's unpack
  and pack, as `startup`): only the levels' keys and entries are added or taken out, every other entry and text
  is the source's (`tests/export_e3b.rs` compares them all).

**The campaign checklist** (`mpconvert::STRIP_GROUPS`, the editor's `strip_ui`): the 60 campaign types and the
campaign vehicles, grouped (AI orders and areas; squads and AI spawning; waypoints and targets; cutscenes and videos;
objectives, checkpoints and saves; scripted triggers; tooltips, debug menus and screen effects; demolitions, strikes
and pickups; campaign vehicles), each with the level's count; the ten types the mode's layer replaces are shown and
always go. Nothing kept is DICE's own choice and E3's result. The choice is an operation of the project's log
(`Op::SetStrip`, undone and redone like any edit; the export reads the last one). The checklist opens by itself when
a project is made or opened on a campaign level with no choice yet, and from the Export window.

**Placement** stays E3's: each base and flag keeps its x / z in the form, and "Use selection" puts it where the
selected object stands (move any object with the gizmo, then take its place).

**Proof (2026-11-19).** On the Aurora project of E3's example:

- BFBC2 profile, no choice in the log: the four bundles and the description are byte-identical to E3's `mapexport`
  (built from 05fc268) on the same project and export file;
- keeping the campaign vehicles keeps SP_01's 53 vehicle spawns and no other instance (`tests/export_e3b.rs`);
- Vietnam profile with its own name and picture: `mapexport check` passes every check, among them the profile's
  commons in every block, the game mode, factions and 8 kit pictures the donor's, `SoundStates` the set's, the name
  keys, and both pictures in their templates' form.

#### Round E3c: the announcer per profile (the user, 2026-11-19)

E3b's open defect: a WW2 export (Aurora, donor `guadal_canal`) failed one reference check. The grafted layer's
`guadal_canal_GameInfo` trigger set names `sound/vo/vo_common` lines that only the WW2 level's own copy of that bank
has. The user's ruling: the announcer follows the profile's own conventions (WW2 BF1943's voices as OPFOR's WW2
levels have them, Vietnam the DLC's, BFBC2 its own, BFBC1 the BFBC1 maps' own). Copying back the hidden modern
partitions the base's own objects need stays as it is.

**Each set's announcer, from the data** (`profile::Announcer`, read per donor from its own level bundles,
`package.mft` and `bundleManifest`; `examples/profiles.rs` prints it):

| Set | Line banks and trigger sets | `vo/<lang>` blocks (sources absent, targets there) | Own voice-over bundle |
| --- | --- | --- | --- |
| BFBC2 (`mp_009cq` ...) | `mp_common`'s (`sound/vo/vo_common`, `vo_en` ...; `sound/vo_triggers/mp/...`) | 7 languages -> `package:levels/mp1_common/vo/<lang>` | none |
| BFBC1 (`bc1_*`) | `mp2_common`'s own copies of the banks, before `mp_common`'s | 7 -> `mp2_common`'s, then `mp_common`'s | none |
| Vietnam (`nam_mp_*`) | `nam_common`'s own copies and `nam/` trigger sets, before `mp_common`'s | 7 -> `nam_common`'s, then `mp_common`'s | none |
| WW2 (OPFOR's 4) | the level's own: `vo_common`, `vo_en` (BF1943's lines merged in), `vo_triggers`, `<level>_GameInfo`, `VO_SP_JP_Generic` | 1 (`vo/en`) -> `mp1_common`'s | `Package\levels\<lv>\vo\en-00` (BF1943's waves) |

- Every stock level's own bundle hides other modes' trigger sets with deleted markers (Squad Deathmatch's on the
  Conquest levels), one of thousands of markers a level bundle carries. The export copies no donor marker (E3 never
  did), so this is not taken over.
- DICE's MP_SP_005CQ (a BFBC2 level made from the campaign with the DLC's `mp2_common`) targets `mp2_common`'s
  voice-over bundles, as the BFBC1 levels do. OPFOR's `mp_001` has two Squad Deathmatch trigger sets in its
  `Package` bundle. The export takes whatever donor is chosen.

**What the export does** (`export::build`, step "announcer"):

- the donor's own announcer partitions that the layer reaches (the level's references into `sound/vo/` and
  `sound/vo_triggers/`, followed through the donor's own copies) go into the new level's bundle, which comes first, so
  the triggers resolve as on the donor (WW2: 5 partitions; the other sets: none, their commons give them);
- the donor's `vo/<lang>` blocks under the new name (a target in the donor's own folder re-pointed to the new
  level's) and its `bundleManifest` vo lines are added to the registration. These are the same blocks the stock
  levels have: absent sources, present targets;
- the donor's own voice-over bundles are staged byte for byte as `vo/<lang>-00.fbrb`, installed into
  `Package\levels\<lv>\vo\` (WW2: `guadal_canal`'s `vo/en-00`, BF1943's waves);
- `stage.json`'s `announcer` lists the copied partitions; the check takes their stock references from the donor's
  own bundle.

**The check** (`mapexport check`, two new lines):

- *the voice-over registration is the donor's*: the vo blocks and manifest lines under the new name, its own bundles
  byte for byte;
- *`<Set>`'s voices*: every announcer partition the level names, followed through their references, resolves in the
  bundles the level loads to the same bytes the donor's own load gives. The set is named from the bundles they come
  from: a set's own commons or levels over the shared BFBC2 `mp_common`. It must be the profile's.

The editor's Export window shows the donor's announcer (a row under Kits).

**Proof (2026-11-19).** On the Aurora project of E3's example, the four profiles (donors `mp_009cq`, `nam_mp_002cq`,
`guadal_canal`, `bc1_harvest_day_cq`):

- `mapexport check` passes every check on all four, among them the two announcer lines:
  - BFBC2: 13 partitions, all from `mp_common`;
  - Vietnam: 13, all from `nam_common`;
  - WW2: 12, 5 from its own copy of `guadal_canal`'s and 7 from `ww2_common`;
  - BFBC1: 13, 10 from `mp2_common` and 3 from `mp_common`.

  Each one resolves to the same bytes as on its donor. E3b's binary failed the WW2 export on `guadal_canal_GameInfo`
  -> `vo_common/CE4CC304...` (`Guadalcanal_Win`);
- the BFBC2, Vietnam and BFBC1 exports' four bundles and description are byte-identical to E3b's. Their registration
  gains the donor's vo blocks (7 each) and its manifest lines (7 on `mp_009cq`; none on the Vietnam and BFBC1 donors,
  whose levels have no `bundleManifest` lines at all);
- WW2: the level bundle holds E3b's entries plus the three it lacked: the `vo_common` and `vo_en` banks and
  `VO_SP_JP_Generic` (1,391 entries). The copy-back of hidden partitions is otherwise unchanged (1,362 entries; E3b's
  1,364 counted `vo_triggers` and `guadal_canal_GameInfo`, which the announcer step now copies).
  `vo/en-00` is `guadal_canal`'s, byte for byte (476 entries);
- `game\`: the 1,582 files kept their size and write time, and 19 key files kept their sha256;
- OPFOR's `mapedit_install --plan` on the WW2 stage lists the new `vo\en-00.fbrb` and the vo block. Its apply /
  revert test on a scratch copy of the shared files, run with all four stages, gives every file back byte for byte;
- `tests/export_e3c.rs`: each set's announcer data, and the WW2 export with its check.

#### Round E3d: any map, any era, any mode (the user, 2026-11-19)

The user's goal: any map altered and exported for any multiplayer mode of the era the level is declared for
(BFBC1, BFBC2, Vietnam, WW2), then installed end to end - a stock BFBC2 map declared WW2, a WW2 Conquest level
declared a BFBC2 Rush level, Operation Aurora as a WW2 level with WW2 objects placed from WW2 bundles. The user's
rulings during the round: the era and the mode are **project settings**, and the mode layer is **editable project
data**, not a graft at export time; an era change gives the level the target era's kits, factions and teams; every
combination that cannot work fails in the check and the editor with a plain message; the proof is a list of named
**canary conversions**.

**Each era's modes, from the data** (`profile::modes`: every level's description's `GameMode`; `examples/modes.rs`
prints it):

| Era | Modes its stock levels have (stock levels) | Offered |
| --- | --- | --- |
| BFBC2 | Conquest (9), Rush (`goldrush`, 10), Squad Rush (7), Squad Deathmatch (`teamdeathmatch`, 8), Onslaught (4) | all but Onslaught |
| BFBC1 | Conquest, Rush, Squad Rush, Squad Deathmatch (2 each) | all |
| Vietnam | Conquest, Rush, Squad Rush, Squad Deathmatch (5 each) | all |
| WW2 | Conquest (3: `guadal_canal`, `iwo_jima_s`, `wake_island_s`), Air Superiority (1: `coral_sea`) | Conquest |

A mode is offered for an era only when a stock level of that era has it: WW2 has no Rush, so "WW2 Rush" is refused
in words, never invented from another era's layer. Onslaught is co-operative play against bots (it needs NavKit
path data and the AI layer: see "Bots" below); Air Superiority is BF1943's mode, still queued in OPFOR's Pacific
War work (`modelayer::not_offered` says both; the editor lists them greyed with the reason).

**The project's target** (`bc2_levelwrite::modelayer`, `Op::SetMode`): the era, the mode and a donor (a stock
level of them; the profile's default donor, or the user's choice) are an operation of the project's log. Choosing
them:

- takes the base's old layer out: a level with a mode layer (`AutoTeamEntityData`) loses its whole layer and the
  types `REPLACED_BY_LAYER` names; a campaign level the latter (its player, mission, menus, spawns, areas, markers
  and announcer: what E3's strip always took). Vehicle spawners stay for the campaign checklist (by default
  removed at export; their events to removed instances are cut);
- brings the donor's layer in as ordinary objects: each instance with a new guid (UUID v5 over the project, the
  operation and the donor's guid), its references and event connections re-pointed, its `Objects` items, the
  donor's voice-over arrays. The editor shows them at once: flags, bases, M-COMs, spawns and combat areas are
  selected, moved, deleted and edited like any object; one undo takes the whole choice back;
- the operation holds data only (guids, places): the donor's instances are read from the game copy when the log is
  applied (`write::apply_with`: a donor resolver; the editor's `Doc::set_game`).

**What a mode layer is, from the data** (`modelayer::layer`): E3's Conquest seeds plus the other modes' (Rush's
attacker lives `LifeCounterEntityData`, the M-COMs' `ArmDisarmAwardEntityData` and `MissionObjectiveEntityData`,
Squad Deathmatch's `KillCounterEntityData`, every capture point and combat area, WW2's end-of-round spawns) and the
**objectives**: the placed objects whose events reach the arm / defuse `GameInteractionEntityData`
(`objects/objects/mpobjective/mpobjective` on BFBC2 and BFBC1, `mp_objective_vietnam_01` on Vietnam), with the
objective props their events destroy. What they reach forward (references and events) among the gameplay types is
the layer. Measured: `mp_009cq` 238 instances, `mp_005gr` 821, `mp_005sr` 279, `mp_009sdm` 172, `guadal_canal` 277,
`nam_mp_002r` 507, `bc1_oasis_gr` 1,082. Left out, as in E3: the donor's vehicle spawns (E4 places vehicles), world
objects, effects and cutscenes.

- **Rush stages** (`modelayer::stages`): the M-COMs that trigger the same stage counters
  (`MultipleTriggerEntityData`, `onDestroyed`) are one stage: `mp_005gr` 4 stages of 2, `nam_mp_002r` 3 of 2,
  `bc1_oasis_gr` 5 of 2, `mp_005sr` and `bc1_oasis_sr` 2 of 1.
- **First-guess placement** (`modelayer::choose`): the whole layer is moved (scaled down when larger, never turned)
  so its combat areas' box lands centred in the base's largest combat area; each instance keeps its height above
  the donor's ground over the base's. "Use selection" (`export::move_anchor`) moves a flag, base or M-COM with what
  belongs to it (spawns, alternative spawns, markers and area geometry; an M-COM's interactions, markers, alarm sound
  and objective props) to the selected object, as an undoable move. E4's gameplay tools build on these hooks.

**The export** writes the project as it is: no graft, no placement. The campaign checklist strips the base's
campaign scripting but never what the log made; the level's fields and the description come from the donor
(the target era's game mode, kits, factions, licences, announcer: an era change gives the target era's kits and
loses the base's, as ruled). An export file of a project without a target still builds E3's Conquest graft.

**The closure** (`export::closure`, generalising E3b's copy-back): the level must resolve under the target
profile's bundles. From the level's own partitions, every partition named is looked up in the bundles the level
block loads; one in the level's own or its base's bundles is walked in turn (what it names must resolve too); one
in a shared bundle (`_common`, `async`) stops the walk. What does not resolve is copied into the level's own bundle
with its resources (`<stem>.res`, `<stem>_*.res`):

- hidden by a deleted marker of a bundle before it (the base's own bundle over `mp_common`; `ww2_common` over
  `mp_common` for modern content under WW2);
- in no bundle the level loads: another era's commons under the target's profile (a WW2 base exported as BFBC2 no
  longer loads `ww2_common`: the level block leaves out every other set's commons, `profile::set_commons`), an object
  placed from another level's or set's bundle (E2b's "+bundle"), a mode level's own data.

The sources, in order: the level block's bundles (each read alone), the donor's own bundle, the bundles the base
itself loads (a WW2 base's `ww2_common`, whose partitions OPFOR's `nam_common` also holds), then any level bundle of
the game copy (the shared ones first); a copy's own references are looked for first in the bundle it came from.
**Copy, not registration**: DICE's levels register no other level's bundle (mode levels only their base map's) and
registering another era's commons would load its deleted markers and make the level that era's (the start screen's
recognition); so the needed partitions are copied, as E3b did. `stage.json`'s `closure` and `closure.txt` list
each copy: the entry, its bundle, why, and the partition that named it. A partition only the base's own partitions
name and that the stock base's own bundles lack too is DICE's dangling reference (the stock level loads with it:
`mp_005`'s guard tower effects name emitters only OPFOR's `guadal_canal` holds): listed, never copied from an
unrelated level. A mode level's minimap is its map's (`MP_005GR` reads `levels/mp_005/ui/`): the ingame bundle
takes the base's UI from the base's ingame stack. The terrain block keeps every terrain bundle the base loads,
whatever the profile: the ground is the map's (a Vietnam map's height field is in `nam_common`'s terrain bundle).

**The announcer over the base.** OPFOR's WW2 levels carry their own `sound/vo/vo_common` and `vo_en` (BF1943's lines
merged in) in their own bundle, which the level block loads before the commons: a WW2 base exported as BFBC2 would
speak BF1943's lines. Every announcer partition the level names whose version in the bundles it loads is not the
donor's own load's goes into the level's bundle from the donor's load (`export::announcer_over_base`), so the
voices are the target era's (E3c's check found it: "WW2's voices" on the BFBC2 export of Wake Island).

**The check** (`mapexport check`, new lines): *terrain* - the map's height field is in a bundle the terrain or level
block loads; *closure* - every partition the level reaches through its own and
its base's bundles resolves in the bundles it loads (what the export copied, from where); *mode layer* - complete
for its mode against the donor's own layer, each missing piece in words (`modelayer::missing`: "Rush stage 2 has
one M-COM (the donor's has 2)", "Conquest: team 1 has no base", "Squad Deathmatch has 3 teams (the donor's 5: the
squads)"). The editor's Export window shows the same lines and keeps Export disabled while one fails.

**The editor.**

- The **New project** form has Era and Mode pickers (the modes listed from the data per era; a campaign base starts
  with Conquest of its own era, a multiplayer base with its own layer kept); the project's first operation is the
  target.
- The **Export** window shows the target (Era, Game mode, Donor level; "Set target" makes a changed one the
  project's), the layer's completeness, and each flag, base and M-COM with "Use selection".
- The panels follow the target: the asset browser lists the target era's objects (its content set's bundles);
  Spawners and Objectives are read from the edited partition (the target mode's layer); the team labels are the
  target era's (`FactionRUS` is RU, NVA or JP); Help names the target.
- **Install** (Project OPFOR): "Install..." runs `mapedit_install.py --plan <stage>` on a worker and shows what an
  install changes; "Install now" (the game closed) runs `--apply`; "Uninstall..." asks, then runs `--revert <LV>`.
  The install side was already mode-free (`maps.ini` gets `<Mode>=Levels/<LV>` with the stage's mode, now `Rush`,
  `Squad Rush`, `Squad Deathmatch` too; one exported level is one mode, as DICE's mode levels are); `--check-game`
  checks a stage against the real game copy while installing into a scratch copy.

**Compatibility rules found** (a later round makes them a validation layer):

1. A mode is offered for an era only when a stock level of the era has it (WW2: Conquest only).
2. Onslaught needs bots (path data, the AI layer); Air Superiority waits for OPFOR's Pacific War work: refused with
   the reason.
3. The target era's donor gives the kits, factions, team labels, licences, matchmaking id, sound states and the
   announcer; nothing of the base's era survives in them.
4. Another era's commons are never loaded; what the level needs from them is copied (and listed).
5. DICE's dangling references stay dangling; they are never filled from an unrelated level.
6. A deleted marker of the base's own bundle or the target's commons hides a partition the level needs: copied.
7. The donor's vehicle spawns are not brought in (E4 places vehicles); the base's follow the campaign checklist.
8. A mode layer must stay complete for its mode (the check's rules above); Conquest may lose flags, never a base.
9. The first guess never turns the layer and never scales it up; heights follow the ground.
10. A mode level's minimap is its map's: taken from the base's ingame stack.
11. The level id is no level of the game copy and no other tool's registration (E3).
12. A project with a target needs the game copy to open (its donor is read when the log is applied): without it the
    editor says so in the Problems panel.
13. The map's terrain bundles are kept whatever the era (the ground is the map's); the check finds the height field.
14. The announcer's banks are the target era's even where the base's own bundle shadows the commons' (WW2 bases).
15. A base whose description's name key names no loading picture (`WAKEISLAND2_NAME`) takes the picture of its level
    name (`wake_island_s.res`).

**Bots (later).** `HasAI` stays false. The path data hook is after the export's step 2: the level's edited
collision and the layer's spawns and combat areas are NavKit's input (`Output\win32\ai\levels\<lv>`), with `HasAI`,
`PathfindingTypes` and the `SectorOffsets` partitions under the new name; Onslaught needs it, and the start-up
checksum test comes with the in-game round.

#### Round EX1: every name, the preview, the Diff panel (the user, 2026-11-20)

The user's requests: every name of the level editable in the Export window and the project, validated together; the
staged export opened beside the original; a visual diff of a project against its base and of any two levels;
LevelDiff folded in (section 9.5).

**The level's names, from the data** (`bc2_levelwrite::names`; 83 level descriptions of OPFOR's game copy, its level
folders, `maps.ini`, OPFOR's `src\dll\server_browser.cpp`, the loader movie):

| Name | Where it is read | Rule |
|---|---|---|
| Level id `Levels/<ID>` | `maps.ini`'s `<Mode>=Levels/<ID>`, the description's `LevelName`, `Levels/<ID>.dbx` and `LevelData.Name`, the menus' list items `<id>.dbx` / `<id>_description.dbx`, the keys `<ID>_NAME` / `<ID>_DESC` | A-Z, digits, `_`, 3 - 40, a letter first; no stock level, no other project's install; no `_NAME` inside (the loading screen cuts the key at its first `_NAME`: a black screen); keys and loading picture the game does not have yet |
| Folder `Package\levels\<folder>\` | `package.mft`'s `BundleSource game:levels/<folder>/...`, `bundleManifest`'s `levels/<folder>/...` | the id in lower case (all 83: the game finds the bundles by the id); editing it renames the id |
| Server-browser name | `maps.ini`'s `name=` of the section `[<ID>]`: the launcher's row and the browser's `OPFOR - <name> (<mode>)` | plain ASCII, no `"`, no outer spaces, the row within 159 characters, no other map's name |
| On-screen name and description | the localisation's `<ID>_NAME` / `<ID>_DESC` in each of the 8 languages: the menus' map list, the loading screen's panel, the browser's map column | up to 64 / 400 characters; each language's text in that language's letters (its histogram's codes) |

DICE gives every mode of a map the map's keys (`MP_005GR` reads `MP_005_NAME`); an export has keys of its own, so its
picture is `loadingScreens/<id>` and nothing of a stock level is renamed.

- The Export window's **Names** section edits them (a per-language table under it), shows where each one is seen,
  and lists the broken rules at once; **Keep in project** (and Export) puts them in the project's log
  (`Op::SetNames`, project format 3: undoable, shared with the project); opening the window takes the log's last
  names.
- `ExportSpec` gains `folder`, `Identity.name` (the on-screen name; empty: the browser's) and
  `Identity.languages`; `stage.json` gains `strings_by_language`; `mapexport overlay` writes each language's own
  texts in its encoding (`export::patch_overlay_langs`, `bc2_formats::loc::encode`).
- `mapexport names <export.json>` checks the names; `mapexport build` refuses names that fail; `mapexport check` has
  two new lines: the names' rules on the stage, and *names agree* (the description, the menus' list items, every
  `package.mft` block and `bundleManifest` line name the same level, and the name key names the picture staged) -
  section 8's checksum crash and black loading screen.
- OPFOR's `mapedit_install.py` describes the per-language texts and the browser row; its install was already
  name-driven (`[<ID>]`, `Levels/<ID>`, `levels/<id>/`).

**Preview export.** After a build, the stage opens read only as a level (`compare::stage_level`: the stage's bundles in
front of the targets its registration names, prepared like a Package level into the editor's cache), beside the
original base level, with the project's camera; T switches between them (the same camera), Close preview gives the
project back. Both views draw the differences, and the Diff panel lists them: the mode layer in and out, the campaign
content stripped, the placed objects whose blueprint the closure copied from another set, the rest.

**The Diff panel** (`compare.rs`, `app/diffui.rs`): **Compare with base** (the project against its base, compared
again by itself after each edit), **Compare with level** (the open level against any other), and the preview's. Rows
added, removed, moved or changed, grouped (mode layer, from other sets, campaign content, world) and filtered (kind,
group, text, placed objects only); a click selects the object and flies to it (or to where it stands on the other
side); its fields as LevelDiff prints them, Inspect opens Properties > All fields. In the 3D view the side's
differing objects are tinted, a box marks where an object of the other side stands, a line joins a moved object's
places (`view-diff-*`, never orange).

**Proof** (`rust/BENCHMARKS.md`, "EX1"): `bfbc2 leveldiff` identical to LevelDiff.exe on 30 recorded cases;
`tests/export_ex1.rs` (the rules, every name in a staged canary, each language's text in the overlay); OPFOR's install
and revert of the renamed canary on a scratch copy of the shared files, back byte for byte; `tests/ex1.rs` (the Names
section, the preview of Atacama Rush as Conquest, the Diff panel on Aurora made WW2 Conquest).

#### Round XF1: the export fixes before E4 (the user, 2026-11-20; audit AU1)

**Shader entries for objects from other levels** (G-N1; spec SHDB-LVL-1, SHDB-LVL-2, RNDR-DRAW-6). The ground truth: a
level draws through its own `levels/<lv>/shaders/dx{9,10,11}_single.res` only (no `_common` bundle has a database),
and DICE puts the entries of every mesh a level draws in them, the commons' too (MP_005: 384 of `mp_common`'s 431
meshes it reaches). 0x98BD6E is the draw-record builder (0x98BCE0) reading a part's program through a null pointer:
a part with no program for its layout, which is what a mesh without entries is. The export now walks every mesh the
level reaches (its own partitions, through the bundles it loads), and for those its databases lack merges the
entries and mesh-part names from the database of the same kind of the first level that has them (the donor, the
base, the closure's sources, then the game copy's levels with DICE's first): `bc2_levelwrite::shaders`, on the
shader-database merge that moved from `bc2-cli` to `bc2_formats::shaderdb::merge`. Not only placed objects needed
it: every E3d canary lacked entries for the new mode's and era's meshes (Aurora as WW2: 91 of 359, the WW2 kits
among them; Arica as Vietnam SDM 91 of 319). `mapexport check` has a line per database.

**Each object's own cull shape** (G-N2; XCULL-RES-1, XCULL-RES-3). `reslot` gave an object its slot owner's leaves:
Aurora's placed radar bunker took a map prop's one leaf (the bunker has two on Guadalcanal), and Guadalcanal as Rush
gave 20 M-COMs grafted with Arica Harbor's `Enumeration` the shapes of Guadalcanal's objects with those slot
numbers. Now a moved base object carries its own leaves and every other object (copy, placement, grafted object)
takes its blueprint's shape: the base's first slotted placement of it, else another level's (donor, the levels that
hold the blueprint, then all, DICE's first); with no shape anywhere it gets slot 0, never another's slot. The check
reads each shape's source again and compares the leaves where the object stands, and refuses a new object at slot 0
whose blueprint some level slots.

**Undo without the whole log** (D1, D2, P1; XLOG-LOG-3). `Doc` keeps checkpoints (entities, outlines, collision
placements, deleted set, edited partition) after mode choices and every 16 operations, and reads a mode choice's
donor from the game copy once. SP_01 as WW2 Conquest with 40 moves, release: undo 9.1 - 10.9 s before, 78 - 98 ms
after; History jumps 1.4 s before, 73 - 98 ms after; every result equal to the log applied afresh.

**Paste as a deep duplicate** (G-N23; XLOG-APP-7) and **typed gameplay operations** (D18, R23; XLOG-APP-6):
`set-team`, `link-spawner`, `set-tickets`, `insert-point`, `remove-point` (project format 5, after VW2's 4; a version-4 file loads
as it is, a newer one is refused in words). An outline's points are a chain of `Next` and `Previous` and an area's
`Geometry` names one point per outline: inserting and removing keep both links and the area's point. E4's tools
build on these.

**Proof** (`rust/BENCHMARKS.md`, "XF1"): E3d's six canaries rebuilt, every `mapexport check` line passing (the new
shader and cull lines among them); `tests/export_xf1.rs` (DICE's own coverage; Aurora with Guadalcanal's bunker: its
entries and part names in each database equal to Guadalcanal's, its leaves Guadalcanal's shape); OPFOR's
`mapedit_install --apply` and `--revert` of that stage on a scratch copy of the six shared files with `--check-game`
the real game copy: back byte for byte, `game\` unchanged. Not yet proven in game (in-game tests are paused): that
the merged entries draw, and that the grafted objects' new shapes cull as on their own levels.
#### Round VW2: the mode view layer, weather in the views and in exports (the user, 2026-11-21)

The user's requests: show or hide objects by game mode and Rush stage; the weather states each level holds in both
views; and "a night, rainy version of a level" as an export. Research round WX0 found nothing baked depends on the
sun (per-asset occlusion, the per-level terrain art map and one reflection cube; light, shadows, fog, sky and
exposure are computed per frame from the weather state), so weather is data: no re-bake.

**The mode view layer** (`crate::modeview`, `modelayer::layer_view` / `rush_play`; a view setting, never in the log).
The Mode switch over the 3D view: every object, the mode layer only, or the level without it; and one Rush stage as
it is played (spec GAME-RUSH-9: L1's simulation of the stages' events from the instances' `Enabled`): the other
stages' M-COMs with what belongs to them (`modelayer::cluster`), and the bases, combat areas and spawns disabled
during the stage, are hidden - in the simplified view, the outliner, the overlays and the game-accurate view (which
also leaves out what the project deletes or hides). With a project it reads the edited partition (the project's
target layer), else the level as loaded; it is read again when the project's revision changes.

**Weather in both views** (`crate::weatherview`, `bc2_render::world::level_weather`, `World::set_state`). The
Weather switch lists every state of the level's system (WX0's kinds by DICE's names; the spawn switch and
`ActiveState` named). The game-accurate view draws a state's light, post-process and sky, and opens on the level's
own state: the spawn-time switch when it names one (RNDR-WTHR-6: in 45 of 70 levels with a switch it is another state
than `ActiveState`, whose placeholders are dark). The simplified view is tinted by the state's look from its values
(RNDR-WTHR-7).

**Weather in exports** (`bc2_levelwrite::weather`, `Op::SetWeather`, project format 4; the Export window's Weather
section, `crate::weather_ui`). Spec RNDR-WTHR-8, -9:

- **VW2a**: one of the level's states: the settings copied with `ActiveState` set, the spawn switch's `State` set to
  the same state, the weather entity re-pointed (a placed entity blueprint copied, its placement re-pointed);
- **VW2b**: another level's state appended to a copy of the system (the level keeps its sky), or the donor's whole
  system swapped in (its sky); renamed under the level's folder with new guids (no donor name can collide), the
  system's 4-byte `WeatherSystem` resource beside it;
- **VW2c**: night: `ons_mp_002_night` (MP_002's system, the Onslaught Package's) appended as the template, its
  `ReflectionEnvmapScale` x 0.1 (the cube is a capture of the day), its panoramic `Trancparency` x 0.25 (the level's
  own panorama darkened); lamps only with an editor tool (left: the cross-level emitter placement it needs);
- **VW2d** (left): rain and snow beyond the state's values (SP_03's `afterrain` exports as a state; the area effects
  and sound areas need the same emitter copy as the lamps);
- every new partition under `weather/<level>/` in the level's own bundle; the base's stock partitions never change;
- night and rain are off by default and marked "not yet seen in game": the build refuses them until "Build it anyway";
- `mapexport check`: the entity names the recorded system and settings, every state resolves with its components,
  `ActiveState` and the switch name the chosen state, a new system has its resource; the copies' sources are in
  `stage.json` for the dangling-reference rule (a Setup's static Sun / Sky data are in no bundle of DICE's either).

**Proof** (`rust/BENCHMARKS.md`, "VW2"): the stages against L1's on four levels; MP_009GR's filters in every view;
MP_002SR's seven states drawn and measured against their looks; six export canaries, each against its baseline stage
byte for byte but the intended changes, every check line passing; install and revert on a scratch copy exact.

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
| E0 | The Rust workspace (3.8): Rust toolchain, Cargo workspace in the suite, PyO3 bindings; the first core formats (DBX, FBRB) ported and proven on the existing corpora; baseline timings. **Status (2026-11-16, branch `rust-e0`):** workspace `rust/` (`bc2-formats`, `bc2-py` / `bc2_native`), the DBX reader ported and identical to the Python reader on all 48,120 distinct corpus files and to DbxEdit on its 31,404 fingerprints, `bfbc2core.dbx` on it with the Python fallback, timings in `rust/BENCHMARKS.md`. **E0b (branch `rust-e0b`):** the FBRB reader ported (`bc2_formats::fbrb`; `bfbc2core.fbrb` on it with the Python fallback), the same entries and bytes as the Python reader on every bundle of the game copy and `work\ww2`, and FbrbTool's unpack on the built and sampled bundles; level loading through `dbx.parse_many`. Bundle packing stays with FbrbTool (E1 / `levelwrite`). E0 done. |
| E1 | Core (Rust): dynamic scene, gizmo, selection, command log; `levelwrite` with tests (round trip against DbxEdit). **E1a (branch `rust-e1a`, 2026-11-16):** a level load profiled (the collision index was 85-95 % of it); terrain, water and Havok readers in `bc2-formats`, the collision index and the scene (`bc2-scene`) loading what `level.load` loads, identical to the Python loader entity by entity on 18 levels; `level.load` on it (6-15 s -> under a second). **E1b (branch `rust-e1b`):** the writers in Rust: `dbx.write` (the Python writer's bytes on all 48,119 corpus files) and bundle packing (`fbrb.pack` / `pack_many`, FbrbTool's bytes on all 1,086 bundles of the game copy, with a byte-exact port of zlib's deflate). **E1c (branch `rust-e1c`):** shader databases (reader / writer, all 745 corpus databases identical) and the cull tree tool's element reader and level search in `bc2-formats` (3.8). **E1e (branch `rust-e1e`):** the mesh alias table read from the binary partitions and cached by the files' sizes and times (identical on all 37 prepared levels; MP_005's index 0.36 -> 0.11 s warm), NavKit's `detailed`, `ladders`, `navsample_offline` and `navwater_mesh` on the Rust collision readers (their output byte for byte the same), a WW2 level (Guadalcanal) loaded and timed from a temp prepare. **E1d (branch `rust-e1d`, 2026-11-16):** a WW2 level build and a NavKit build profiled; console textures (`bc2-convert`) and NavKit's grid walks and floods (`bc2-navbuild`) in Rust, byte-identical to the Python code on every BF1943 / Onslaught texture and on whole NavKit builds; the converters' partition reads one pass per bundle (`meshview.Partitions`): coral_sea's build 1,293 -> 667 s, a NavKit build of sp_004_b 566 -> 186 s (sp_009 153 -> 19 s). **Integrated** on branch `rust-e1`. **G1 (branch `rust-g1`, 2026-11-18): the editor's view on the scene** - the application's shell in `rust/bc2-editor` (binary `bfbc2-editor`, egui 0.36 on wgpu 30 through eframe, docked panels with egui_dock): the theme generated from `design/project/tokens.json` (Dark and Light, IBM Plex), the start screen ("View a map": the game copy's levels, read-only), the 3D view on `bc2-scene` (terrain, water, collision instanced per blueprint, entities as markers in team colours with labels, outlines; orbit / fly / focus, GPU picking, the selection outlined in `selection`, camera bookmarks) plus top / front / side orthographic views with their own cameras, the Outliner / Properties / Problems panels, an action registry shared by the top bar, the shortcuts and a command palette (Ctrl+Shift+P), and a CPU ray-cast on the scene's surfaces (the base of 3.10's surface snapping); the hub starts it. Proven headless: every token reaches the theme in both themes; the outliner equals the Map Inspector's lists on SP_01, MP_005, MP_009GR and ONS_MP_004 entity by entity; offscreen frames, outline, picking and clicks (README.md, "The map editor"). Next: entity creation / deletion and field edits with `levelwrite` |
| E2 | Map project and operation log; edit mode in the Map Inspector: move, rotate, delete, duplicate; save; with 3.10: surface / grid / angle / vertex snapping incl. sinking below the surface, align and distribute, ruler; layers and outliner folders; groups; several viewports and the viewport modes; the command palette; autosave and crash recovery. **Status (2026-11-18, branch `rust-e2edit`, first part):** the map project and its log in Rust (`rust/bc2-levelwrite`: `<name>.mapedit.json` after navtune.json's pattern - base level, partition sha1, content profile, operations with `by` / `when`, UUIDv5 guids, undo / redo, base-changed report, autosave beside it and crash recovery); levelwrite on `EditDbx` (transform, delete = DbxEdit's `drop` plus the deleted object's event connections, duplicate wired into `LevelData.Objects` and `EventConnections`, typed field sets from the partitions' own data by `dbxcatalogue`'s rules, never guessed; a field-level diff), proven on MP_005, SP_01 and Guadalcanal (parse, exactly the expected changes, writer round trips, the scene loads them; DbxEdit `drop` / `setfield` and the Python `dbxbin` reference byte for byte); the Objects workspace in `bfbc2-editor` ("New map from an existing map", "Open a project"; select by click and box, the move / rotate gizmo in world or local axes, grid / angle / vertex and edge / surface snapping with a sink depth in metres or a share of the height, along the normal or straight down, kept with the object; delete, duplicate, align, distribute, ruler, layers with hide / isolate / lock / colour, outliner folders, groups, typed properties; every edit one undoable operation in the History panel; the actions in the command palette). Remaining for E2: the viewport modes (lit / unlit / wireframe / collision / navmesh / cull tree), polygon selection, editing static group members (their group files), deep duplicates (owned sub-instances such as a flag's capture area), multi-object property editing ("Mixed"), and the levelwrite proofs extended to every prepared level (rust/BENCHMARKS.md, "E2") |
| E3 | Apply for an edited stock map: partition, closure, shader entries, cull tree, bundles, validation gate, stage, install / revert; the level installer split out of `wake_install`. **Status (2026-11-18, branch `rust-e3`, 3.12):** export as a NEW multiplayer level, campaign levels included - `bc2_levelwrite::{export, mpconvert, culltree}` and `mapexport` (build, offline check, the startup patch), the editor's Export window, OPFOR's `mapedit_install.py` (plan / apply / revert on the stage protocol); SP_01 staged as `MAPEDIT_AURORA_CQ` and every offline check passed, the game copy unchanged. Next: the first in-game test (with the user), new shader entries for placed meshes (E4's placing), bots |
| E4 | Asset browser with the safe-to-place rules; placing new objects; gameplay panels (flags, spawns, vehicles, combat areas); the mode definition format with the built-in modes (Conquest, Rush, Squad Rush, Squad Deathmatch, Onslaught, Air Superiority); with 3.10: prefabs, copy and paste between maps, performance budgets, splines |
| E5 | NavKit and AI regenerated on apply; play-test button; content profiles incl. WW2; custom game modes from mode definitions; export of a built map |
| E5a | The AI workspace (3.4a): spawners and orders, routes and connections, navmesh coverage and its regions, the workspace checks; Navigation Studio's editing and stockmp-ai brought into the editor's project. WW2's bot tools (carriers, LCVPs, spawn choice) when the WW2 bot planning becomes a build |
| E6 | Modder polish: QUICKSTART, guided first map, plain-language errors, a sample project; with 3.10: version control (history, diffs, branches) |
| S2 | New maps: terrain import / generation, terrain writer, new-level shell, scatter; with 3.10: terrain sculpting and painting brushes and the foliage brush (the terrain writer is their prerequisite; they also work on existing maps' terrain) |
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
| A mesh part with no shader vertex layout; console-only shader entries; a mesh brought in without entries in the level's databases (0x98BD6E: a null program read in the draw-record builder 0x98BCE0) - the export adds them and `mapexport check` refuses a mesh without them (round XF1, 3.12; spec SHDB-LVL-1/2, RNDR-DRAW-6) | `wake_install.py`, memory note console-resource-formats |
| A level missing from LevelListQuery: in no menu; a `_NAME` prefix mismatch: a black loading screen; a key shared by two modes: blank browser rows (2026-11-16) - the names' rules and *names agree* of `mapexport check` (round EX1, 3.12) | OPFOR research notes; commit 6f809e1 |
| An object at cull slot 0: draws see-through and cannot be destroyed | `bfbc2-culltree` |
| A terrain mask atlas that differs from the donor's: black terrain cells | memory note terrain-patch-donor-rules |

## 9. Folding the older GUIs into the editor (rounds GR1 on, 2026-11-20)

The user's plan (2026-11-19): for each of the suite's older GUIs, list what it does that the editor
(`rust/bc2-editor`, `bfbc2-editor`) does not, add that to the editor, then retire the GUI - removed from the
repository and from the hub's launcher, its docs pointing at the editor. The hub (`BFBC2 Tool Suite.pyw`) stays as
the launcher. The planned AI behaviour simulator, bot AI debugger and tactical map tools become editor workspaces
later, not tools of their own (section 10; their placeholder folders were deleted in round FM1). This replaces 3.8's "each Python GUI stays until its replacement works" with an
order: the Map Inspector (GR1), the Navigation Studio (GR2), the mesh viewer's page (GR3), DbxEditGUI (GR4, the last C#);
LevelDiff, which has no window, became the editor's Diff panel and a `bfbc2.exe leveldiff` command in round EX1 (9.5).

What each does, whether the editor has it (round GR1's read of their code and READMEs), and the plan. "Partly"
says which part.

### 9.1 Map Inspector (`bfbc2-map-inspector/inspector.py`, a folder deleted in FM1): retired in GR1

| Feature | In the editor? | Plan |
|---|---|---|
| A level opened by name, prepared on first use; a picker without one | Yes (G1-G4: the start screen, `bfbc2-editor <level>`, Prepare with progress) | - |
| The 3D view: terrain (height field), water, collision by layer, markers for objects without collision, spawn facing lines, outlines as prisms | Yes (G1; R5's Game-accurate view besides) | - |
| Orbit, pan, zoom, fly (F), double-click flies to the object | Yes (fly on the right button; F focuses; double-click focuses) | - |
| Layers with colours and counts, shown and hidden; area points, effects and other hidden at start | Yes (the Outliner's groups, Hide / Show, `HIDDEN_AT_START`) | - |
| Search by GUID, type, id, name or blueprint | Yes (the Outliner's search, the same haystack; no shortcut: Ctrl+Shift+P is the command palette, P polygon select) | The type filter combo: the Outliner's groups are the types' categories; "Select all of type" covers the rest |
| Selection panel: type, full type, GUID, id, name, category, blueprint, its object type, source partition and group, team, player type, destructible, position; fly to; copy GUID | Yes (Properties, G1/G2) | - |
| **The full property tree**: every field nested, child counts, values as the XML shows them, references as buttons that follow them, *back* | **Added in GR1**: Properties, *All fields* (types too; long lists paged; references go to their object, Back in Links) | - |
| **Referenced by** (every entity whose fields name this one, `LevelData` left out) | **Added in GR1**: the *Links* panel | - |
| Links drawn over the scene: yellow to what it references, cyan from what references it | **Added in GR1**: `view-link` lines in every view, arrows by direction; K toggles | - |
| Event connections | Not in the Map Inspector; **added in GR1** to Links (sent and received, both ways) and drawn dashed | - |
| Measure (M): points on terrain or objects, last segment's length / horizontal / height / slope, path total, height above the terrain, water surface above, Esc clears | **Added in GR1**: Measure (M) in every workspace, the figures beside the last point (M again ends it); the Objects workspace's ruler besides | - |
| Hover tooltip: what is under the mouse and where; water's shader | Partly: labels at gameplay objects, the status bar's read-out of the last click | Hover read-out with the next view round, if wanted |
| Window layout kept between runs | Yes (eframe's storage: the dock, theme, bookmarks) | - |
| `--screenshot`, `--select`, `--frames`, `--measure` for tests | Yes (`--screenshot`, `--select`; the headless tests drive the rest) | - |

Its offline tests (`tests/test_inspector_offline.py`) and its part of `tests/test_viewers.py` went with it. What the
editor shows was compared with what it showed before it went: the outliner in G1 (`inspector_parity.rs`), the
property tree, references and referrers in GR1 (`inspect_parity.rs`: 1,074 objects of SP_01, MP_005, MP_009GR and
ONS_MP_004 - of every type the first, middle and last, every 50th group member, the most referencing and most
referenced - 102,243 fields, 35,263 references, 1,939 referrers, all identical; the inspector's answers kept as
digests in `rust/bc2-editor/tests/data/inspector_props_*.json.gz`). The folder kept this plan and a README saying
where each feature went (this table) until round FM1 moved the plan to `bfbc2-map-editor/` and deleted the folder
(`RETIRED.md`); nothing else in the suite or OPFOR used its code.

### 9.2 Navigation Studio (`bfbc2-navigation-studio`: `studio.py`, `editing.py`, `overlay.py`): retired in GR2

| Feature | In the editor? | Plan |
|---|---|---|
| NavKit's build and DICE's path data side by side over the level: walkable cells, outlines, portals, border portals, route graph per file | **Added in GR2**: the navmesh view mode and the AI workspace draw both files (`navdata.rs` reads them on a worker; the cells as meshes per colour, `navview.rs`), every layer switchable in the Navigation panel | - |
| Colour by island, sector, pitch, difference (NavKit vs DICE: same, extra, missing), anomaly | **Added in GR2**: the five colourings (actions, the panel's buttons), the `view-nav-*` tokens; DICE's ground NavKit lacks its own layer | - |
| Ladder ground and ladder links layers | **Added in GR2** | - |
| Path test (P): the cheapest route over each file's graph, cost, length, detour, NavKit against DICE, why there is none (islands, nearest approach) | **Added in GR2**: Path test (Shift+P, "Path test from here" on the ground), A* in Rust (`bc2_navbuild::astar`), the routes drawn, the figures and Fly to the gap in the panel | - |
| Edit mode: walkable / unwalkable regions, seed, unseed, link areas, add or remove a route edge, traversals (ladder, jump, vault, drop), notes; `navtune.json` with who and when; undo, redo, save, reload | **Added in GR2**: the Navigation edits panel's tools (and the AI ground menu's), each edit an operation of the project's log (`Op::NavAdd` / `NavRemove` / `NavLoad`, `bc2_levelwrite::navedit`): undo, redo, autosave and save with the project; the side file written and read in `navtune`'s form (the Rust core's `navtune::dumps` / `validate` / `summary`); `when` is UTC | Placing spawners and orders, routes and cross-domain links: E5a |
| Build with these edits (`navkit apply` in the background, its log), open the result, install into the game, restore DICE's original, the report | **Added in GR2**: `bfbc2.exe navkit apply / install / restore` (Python `navkit.py` without the binary); install and restore show their plan (what they write in the game copy's AI path data) in a confirm dialog first | - |
| DICE's data edited additively (links, edges, traversals) into a copy | **Added in GR2** (a DICE base: region tools disabled with the reason) | - |
| Compare with a reference AI layer (`level:`, `xbox:`, a level XML): spawners, orders, objectives, path outlines, filters, the nearest route node, export JSON | **Added in GR2**: the Navigation panel's Compare (read from the partitions in Rust: `level:`, `xbox:`, a `.dbx` or the `.xml` beside one), markers in `view-ref-*`, filters, a marker clicked shows its nearest NavKit node, Export JSON | `overlay.py --reference` stays a command |
| Anomaly overlay: spawns on small islands, orders without a node, sliver areas, lonely nodes; export JSON | **Added in GR2** (layers, counts, the anomaly colouring, Export JSON) | `overlay.py --anomalies` stays a command |
| Panels: per-file figures, islands largest first with fly-to, spawns off the main network, the selected area (sector, tile, layer, cells, island); hover shows the area | **Added in GR2**: Navigation (files, figures, selection), Islands and spawns; the hover's area beside the pointer | - |
| Camera, `--screenshot` for tests | Yes (`bfbc2-editor <level> --navmesh` opens the navigation) | - |

What the editor shows was compared with what the Studio showed before it went (`tests/nav_parity.rs` against
`tests/data/navstudio.json.gz`, digests recorded by the round's `record_navstudio.py`, removed with the Studio): on
SP_01, ONS_MP_004, ONS_MP_002 and SP_009, both files each - the summary, areas, cells and their heights, islands,
the five colourings' classes, the difference figures, the spawns' islands, the islands' gaps, the anomalies and 12
path tests - and the compare layers of ONS_MP_004 and MP_002 against `level:ons_mp_002`, SP_009 against `level:sp_01`
and the WW2 level GUADAL_CANAL's own layer: all identical. `tests/gr2.rs` drives the workspace headless and runs the
edit, build, install and restore round trip on a scratch OPFOR copy. The folder keeps `overlay.py` (its README says
only that); OPFOR's code never called the Studio.

### 9.3 Mesh viewer (`bfbc2-mesh-viewer/meshview.py`): its page retired in GR3

The mesh viewer's GUI was the page for a browser it wrote (`meshview.py <mesh>`: three.js, the models embedded). Round
GR3 read `meshview.py` and the page's script for every feature, added what the editor lacked as the **Asset preview**
panel (`src/preview.rs`, `src/app/previewui.rs`; a tab beside Properties, shown when an asset is picked) and the asset
browser's **Meshes** mode, then retired the page. The picture command line stays (`meshview.py <mesh> [--view]
[--size]`, `bfbc2.exe meshview`), and so does `meshview.py` as the library other tools read the game copy through.

| Feature | In the editor? | Plan |
|---|---|---|
| An index of the game copy's bundles and every level's Dx9 shader database, cached (`work\tools_out\meshview_index.pkl`) | **Added in GR3**: the same index, read from the same cache (`bc2_convert::meshview::cache`: the pickle read in Rust; a stale or missing one built with `bc2_formats::meshindex` and written byte for byte as the Python writes it, under the same lock) | - |
| `find <text>`: mesh names containing a text | **Added in GR3**: the asset browser's Meshes mode (the same list as `meshview.py find`, `Index::find`); click previews, Shift+click compares | `find` stays a command line too |
| A mesh's parts with their colour textures (`load_mesh`: layouts from the databases, the blueprint's surfaces, the colour texture by name) | **Added in GR3**: `bc2_convert::meshview::load_mesh`, identical to the Python on every recorded mesh (below) | - |
| Orbit, pan, zoom (three.js `OrbitControls`) | **Added in GR3**: drag turns, right or middle drag moves, the wheel zooms; Perspective or orthographic; Frame resets | - |
| A switch per part, with its triangle count and colour texture | **Added in GR3**: the parts list (switch, name, triangles, texture), and a click in the picture selects the part, drawn in `view-selection` | - |
| Wireframe | **Added in GR3**: Shading > Wireframe, and Edges over the shading | - |
| Several meshes side by side (`stack_models`) | **Added in GR3**: Shift+click in Place Objects or Meshes adds to the comparison (laid out by `stack_models`, identical offsets); Remove, Clear comparison | - |
| The level of detail (`--lod`) | **Added in GR3**: the LOD list of the mesh's levels | - |
| `--png` (side / top / front, size): a picture | Yes: the Side, Top and Front presets are `render`'s framing and drawing, pixel for pixel; Save picture writes `work\tools_out\meshview\<names>_<view>.png` (1200 px, the command line's grey) | The command line stays (the user's plan) |
| The page itself (a file holding the models and textures, three.js from a CDN) | Not carried over: the preview draws in the editor, nothing to share or to fetch | `meshview.page()` stays, deprecated, while Project OPFOR's WW2 tools (`attachments.py`, `scopes.py`) write their preview pages with it (`RETIRED.md`) |

Beyond the page (the round's brief): **destruction states** (a blueprint's health states pick its mesh parts, as the
simplified view does: Intact, each stage, Destroyed, Every part); **textures per channel** (each part's shader entry
lists its textures, sorted into colour, normal, specular, mask, occlusion, detail, environment; the picture draws the
chosen channel; Colour is the mesh viewer's choice); **normals** (lines from each vertex); **bounds** (a box) and
**measurements** (the bounds' size and centre in metres, triangles and vertices per part and per mesh).
Not carried over, and why: **a skeleton** (the mesh viewer has none - skinned meshes are drawn in their rest pose,
as there; bones belong to the animation tools' Granny readers) and **collision** (not the mesh viewer's either; the
level views' Collision mode draws each placed blueprint's Havok shapes). The index covers the stock game only, as
the mesh viewer's does: an OPFOR-built asset (a WW2 level's) says so.

How the preview draws: on the CPU, with the mesh viewer's own triangle loop ported (`bc2_convert::meshview::render`:
float32 screen positions, float64 weights and depths, NumPy 2's promotion rules), so the presets need no tolerance;
the orbit camera projects the vertices and runs the same loop. Proof (`tests/asset_preview_parity.rs`, quick tier, about 3 s): the
mesh viewer's answers recorded by `bfbc2-mesh-viewer/tests/record_preview_parity.py` (digests only,
`tests/data/meshview_parity.json.gz`) on 8 meshes and all their levels of detail - a rifle, a guard tower, a barrack,
a palm, the M1A2, a soldier, an ammo crate, a parrot: 18 levels of detail, 94 parts (names, surfaces, textures,
vertex and triangle counts, positions, normals, texture coordinates, indices, decoded textures), the layout offsets
and 55 pictures (side, top, front at 400 px, and two rifles stacked) - all identical; Save picture at 1200 px is
identical to `meshview.py --png --size 1200` on the stacked rifles. The panel test (the demo level) draws, picks and
hides a part; the screenshot test saves MP_005's guard tower and the rifles in Dark and Light.

### 9.4 DbxEditGUI (`bfbc2-dbx-tools/DbxEditGUI.cs`, the last C#): retired in GR4

The **DBX inspector** (round GR4): a window of its own (`dbxpanel.rs`, its model `dbxinspect.rs`; a thin hook in
`app.rs`), opened from the start screen ("Inspect a DBX file"), the command palette ("DBX inspector") and
`bfbc2-editor --dbx <file>` (the hub's More tools > DBX inspector). It needs no level. Every edit is
`bc2_formats::dbxedit`'s, the code `bfbc2.exe dbxedit` runs, so whatever an edit does not touch is written back byte
for byte.

| Feature | In the editor? | Plan |
|---|---|---|
| Open any binary `.dbx` (File > Open, drop) | **Added in GR4**: Open (a path), a file dropped on the editor, From bundle (an entry read in memory), Level file (the open level's own files), Project partition | - |
| The tree: instances (bold), fields, lists, values | **Added in GR4**: DbxEditGUI's labels, headings and value texts (identical on 19 real files); rows drawn only where seen (SP_01's 3 MB opens at once) | - |
| Edit a value or an attribute, Apply; Go to reference | **Added in GR4**: the attributes and the value (numbers separated by `/`) typed like the field as the XML import types it, a rounded decimal said; a denormal's stored decimal shown beside its printed bits; Revert. On the project's partition a field edit is an `Op::SetField` of the project (History, Ctrl+Z, saved with it) carrying exactly the bits the file would take | - |
| Instance menu: remove, graft from another file, copy a field, add a list entry | **Added in GR4** (and Copy GUID) | - |
| Edit menu: undo, find (Ctrl+F, F3), replace strings, remove a field everywhere, set a bool everywhere, the partition's GUID | **Added in GR4**: Undo (snapshots, as DbxEditGUI), Find / Next (F3), the five commands | Ctrl+Z / Ctrl+F stay the editor's own shortcuts: the inspector's are buttons |
| Level menu: check (F5), cull slots against another level, fix cull slots; results with go-to | **Added in GR4**: the output pane, a line's guid double-clicked goes to its instance | - |
| File: save keeping `.bak`, export / import XML, save the edits as a DbxEdit script | **Added in GR4**, and Save into bundle (a copy of the entry's bundle, FbrbTool's unpack and pack). The script's header names `bfbc2.exe dbxedit edit` (DbxEdit.exe is retired) | - |
| Help menu (FORMAT.md, README) | Not carried over: the editor's Help window and `bfbc2-dbx-tools/README.md` | - |
| Its log (`ToolLog`, a file a day) and crash window | Not carried over: the editor's own error handling; an error shows in the window | - |

What the inspector does was compared with what DbxEditGUI did before it went (`tests/dbxedit_parity.rs` against
`tests/data/dbxeditgui_answers.json.gz`, recorded by the round's recorder driving DbxEditGUI's window through reflection,
its prompts answered; digests only): on 19 real files (MP_005, MP_005SR and SP_01's level partitions, a description,
a terrain, and files with denormals, 8- and 2-byte numbers, bytes, lists) 1,232 steps - the tree, 618 Applies (513
changed, 41 unchanged, 64 refused with DbxEditGUI's message; 39 denormals typed as decimals, 125 rounded), Undo,
Save and the `.bak`, the edit script, 114 finds, 194 menu commands, 57 grafts, 57 field copies, the level check, the
check against MP_005SR's cull trees, fix slots, XML export and import of an edited export, Save as - every file's
bytes, line and message identical. `every_level_file_round_trips_through_the_inspector` writes back every unique
`.dbx` of the prepared levels (31,037) and imports each one's XML back, byte for byte. `tests/dbx_inspector_window.rs` drives the
window headless (Apply gives `dbxedit edit setfield=`'s bytes) and the project route on MP_005.

### 9.5 LevelDiff (`bfbc2-diff-workbench/level-diff/LevelDiff.cs`, a folder deleted in EX1): retired in EX1

| Feature | In the editor? | Plan |
|---|---|---|
| Two `.dbx` files compared instance by instance: added, removed, changed fields (`path: old -> new`), reference lists as sets, reorderings | **Added in EX1**: `bc2_formats::dbxedit::leveldiff`, the editor's Diff panel (3.12, "Round EX1") and `bfbc2.exe leveldiff`, proven identical to LevelDiff.exe's text, JSON and exit codes on 30 recorded cases (21 on real level pairs) | - |
| `--match-objects` (placed objects paired by blueprint and position), `--type`, `--ignore`, `--summary`, `--all`, `--json`; exit codes 0 / 1 / 2 / 3 | **Added in EX1**: `bfbc2.exe leveldiff` (the hub's Level diff tab, OPFOR's `opforcore.paths.leveldiff()`) | - |
| (no window) | **Added in EX1**: the Diff panel (Compare with base, Compare with level, the export's preview), the differences drawn in the 3D view | - |

## 10. Planned workspaces: tactical authoring, the bot AI debugger, the AI behaviour simulator (round FM1, 2026-11-20)

Three tools of `DESIGN.md`'s first plan (2026-10-16) were never built: the tactical map authoring tool, the bot AI
debugger and the AI behaviour simulator. Each had a folder holding only a README ("Planned - see ../DESIGN.md"). The
user ruled (2026-11-19) that they become workspaces of the editor, not tools of their own, and that planned tools get
no folders (2026-11-20); round FM1 deleted the three folders (`RETIRED.md`) and moved their plans here from
`DESIGN.md` 4.3, 4.4 and 4.8. They build on the AI workspace (3.4a), and the same project, undo and apply rules
hold. Nothing here is scheduled yet: each waits for Project OPFOR's bot layer (`ROADMAP.md`).

### 10.1 Tactical authoring (milestone M7, when OPFOR's bot layer takes shape)

A layer above the navigation data, for OPFOR's bots: generated as candidates, corrected by hand.

- **Layers:**
  - objectives and their approaches;
  - cover candidates (positions with protection from one or more directions);
  - firing positions (with fields of view);
  - rally points, vehicle approach routes, squad routes;
  - strategic regions (sectors of a map with a role: base, front, flank).
- **Generators propose candidates** from geometry and the navigation data: cover from collision near walkable
  ground, sight lines by ray casts (the editor's CPU ray-cast on the scene's surfaces, 3.10), approaches by path
  search (the AI workspace's A*, GR2). People correct them in the workspace, each correction an operation of the
  project's log.
- **Output:** a per-level annotation file, `<level>.tactical.json` (`DESIGN.md` 5.3: typed annotations, each with a
  position, orientation, extent, team relevance, game-mode relevance and source - generated or hand-made; the schema
  is fixed when the bot layer's needs are known), which OPFOR's bot layer reads. It is not stockmp-ai's layer:
  stockmp writes game data, this writes annotation side files.

### 10.2 The bot AI debugger (milestone M6, as soon as AI runs in OPFOR; first for the game's own AI)

A live view of the AI while the game runs: positions, orders, targets, path requests, tactical states, line of
sight, threats, squads.

- **OPFOR side:** a telemetry module in OPFOR's DLL streams AI state over 127.0.0.1 a few times a second (`DESIGN.md`
  5.4: JSON lines over TCP, one frame per update, `{"t": ..., "soldiers": [...], "squads": [...], "events": [...]}`,
  the schema fixed and versioned with the first module): soldier positions and orientation, team, current order and
  goal, target, path request and route, tactical state, perceived threats, squad membership. It starts with what the
  game's own AI exposes, then adds OPFOR's bot layer as it is built.
- **Editor side:** a workspace on the same 3D view, with the live soldiers drawn over the level and its navigation
  data:
  - click a soldier for its state;
  - its current route, line of sight and threat lines;
  - a timeline to scrub back through the last minutes;
  - recording to a file for later, and playing a recording back without the game.
- It also feeds the test harness's in-game tests (bot traversal, stuck detection, squads, vehicles), which are on
  hold with the user's pause of in-game tests (2026-11-20).

### 10.3 The AI behaviour simulator (with OPFOR's bot layer, Goal 3)

Runs OPFOR's bot decision code (behaviour trees or state machines, threat scoring, squad orders, objective
priorities) on simplified maps, without the game. It finds logic flaws cheaply, though not problems with controls,
physics or animation.

- **The rule this brings forward:** OPFOR's bot decision logic is written as an engine-independent library. It
  reaches the world only through an interface: positions, line of sight, paths, objective states, squad messages.
  OPFOR's DLL connects that interface to the game; the simulator connects it to a simplified map. Both run the same
  decision code, so a flaw found in simulation is a flaw in the bots.
- **The simplified map** comes from NavKit's navigation data (route graph, areas), the tactical annotations (10.1),
  and line of sight from collision geometry.
- **Scenarios:** teams, spawns and objectives from the level (or the project's mode layer); scripted or random enemy
  behaviour; many runs faster than real time; results into the test harness. The workspace draws a run as 10.2 draws
  a live game, so one view serves both.
- **Out of scope:** controls, physics, animation, weapon ballistics. Those stay with in-game tests.
