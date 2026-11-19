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
  173): DbxEdit.exe and FbrbTool.exe are retired, their answers recorded (`core/tests/*_golden*.json`); DbxEditGUI
  stays, built on DbxEdit.cs, until this editor replaces it (`rust/BENCHMARKS.md`, "Retired C# tools").
- The suite's GUIs (Map Inspector, Navigation Studio, NavKit's window, DbxEditGUI, the launcher) are rebuilt in Rust
  (`egui`, the shared design language, `DESIGN.md`) inside the map editor application, as its views and workspaces.
  Each Python GUI stays until its replacement works. **Status (G2, 2026-11-18):** the editor application's shell and
  its read-only map view (`rust/bc2-editor`, the "View a map" of 3.9; round G1), with 3.10's decisions on it (round
  G2): faction team labels, the 3D view's own tokens and the third neutral, the Help menu, and levels prepared from
  the editor; the Map Inspector stays until it is replaced.

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
- **View a map:** open any map read-only, in the Map Inspector as today. "Edit a copy" turns it into a new project.
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
- **Deep duplicate**: the sub-instances an object owns (reached through its references, named by nothing outside
  the set but `LevelData.Objects` and event connections: a flag's capture area points, map marker, its own
  spawners) copied with new guids, references among them remapped, their connections copied.
- **Viewport modes** (the editor view): lit, unlit, wireframe, collision, navmesh (Kynapse path data), cull tree
  (each object's slot: none, shared, its own; the leaves' spheres). **Polygon selection** (P). **Several objects'
  fields** in Properties, "Mixed" where they differ, one operation for all.
- Static group members stay read-only (queued). Placed and copied objects keep their template's `Enumeration` (cull
  slot) until the build round runs `culltree add` for them.

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
2. **Phase 2, the engine's values (one or two rounds, with the capture):** the sun direction, shadow cascades (the
   depth-only programs, three cascades), `EnvironmentMain` (sky, haze, fog), and DICE's own post-process
   (`psDownsampleHalfGrayscaleLog`, `psExposure`, `psFilmicTonemap*`, bloom, HBAO from `startup-00.fbrb`) replacing
   the harness's tonemap; the terrain stream checked against the captured vertex buffers; water (`WaterOcean` on the
   `VisualWater` mesh); static model groups (`*_HavokAsset` members), undergrowth, road strips, trees' engine
   constants; the `IrradianceVolume`. Each proven against the capture's own draws and its final picture.
3. **Phase 3, the editor (one or two rounds):** the shared texture (no copy) and the tonemap on the GPU; culling and
   LODs (the cull tree, `LodSettings`), the instanced programs (`StreamInstancingFloat`); edits shown live (a moved
   object drawn at its new transform); picking and the overlays on the game frame's depth; every prepared level drawn
   with no harness flag; Vietnam and the Pacific War levels (their installed Dx11 databases, BF1943's translated
   shaders included, draw the same way).

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
| E3 | Apply for an edited stock map: partition, closure, shader entries, cull tree, bundles, validation gate, stage, install / revert; the level installer split out of `wake_install` |
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
| A mesh part with no shader vertex layout; console-only shader entries (0x98BD6E) | `wake_install.py`, memory note console-resource-formats |
| A level missing from LevelListQuery: in no menu; a `_NAME` prefix mismatch: a black loading screen; a key shared by two modes: blank browser rows (2026-11-16) | OPFOR research notes; commit 6f809e1 |
| An object at cull slot 0: draws see-through and cannot be destroyed | `bfbc2-culltree` |
| A terrain mask atlas that differs from the donor's: black terrain cells | memory note terrain-patch-donor-rules |
