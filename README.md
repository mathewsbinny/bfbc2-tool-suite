# BFBC2 Tool Suite

Tools for building and debugging AI on Battlefield: Bad Company 2 maps, in one repository. The 3D tools
share one core: level loading, a 3D view, picking and the game's data formats.

## Getting started

1. Install Python 3.10 or newer, then the libraries the tools use:
   `pip install numpy matplotlib moderngl glfw imgui-bundle pillow` (and `pefile capstone` for the crash
   reader's code view).
2. Put this folder next to your `project-opfor-bfbc2` folder (or set `OPFOR_ROOT` to it): the tools read the game
   copy there.
3. Double-click **`BFBC2 Tool Suite.pyw`**. One window has every tool:
   - **Levels:** choose a level, then **Open in Map Inspector** or **Open in Navigation Studio** (or
     double-click the level). A level is unpacked the first time (offline, about 20 s); the tool's progress
     shows at the bottom and its 3D window opens when the level is loaded.
   - **Bundles, Level diff, Crash reader, Xbox files, Tests:** the command-line tools with file pickers and
     options; their output appears at the bottom.
   - **More tools:** DbxEditGUI and NavKit's window.

   The test harness, the menu tools and the stock-MP AI tool have no tab yet: run them from a command prompt
   (their READMEs).

The 3D tools can also be started on their own: double-click `bfbc2-map-inspector\inspector.py` or
`bfbc2-navigation-studio\studio.py` and a level picker opens, or give the level on the command line
(`python studio.py sp_01`). On a laptop with two GPUs, see "The 3D tools and your GPU" below.

### When something goes wrong

Every tool keeps a log in `%LOCALAPPDATA%\bfbc2-tool-suite\logs\` (one file per tool and day, the 20 newest
kept): what it did, warnings, and every error with its full details. When a tool crashes, a window says what
happened, the likely cause and what to do - for example "a Python library the tool needs is not installed
(imgui_bundle) - pip install imgui-bundle", "the 3D view could not start OpenGL 3.3 - update the graphics driver",
"the level is not prepared", "a file could not be written - close the game" - and where the log is. In the 3D
tools an error in one panel does not close the window: it is listed in a Problems window and logged once. The
suite window checks the libraries when it opens, and when a tool it runs fails it shows that tool's last lines.
The command-line tools - FbrbTool, DbxEdit, LevelDiff, NavKit's `navkit.py` and `tools\*.py`, the crash reader,
the console readers, the menu tools, the test harness, the test runner - log the same way and, on an error, print the cause, what to do and the
log's path (exit code 1); DbxEditGUI and NavKit's window show it in a window. If you report a problem, send the
log file.

**Ready to use**

| Folder | Tool |
|---|---|
| `bfbc2-auto-navkit/` | **BC2 NavKit**: generates AI navigation data (`pathdata.hpd`) for levels. Start with `QUICKSTART.md`, or `navkitGUI.pyw`. |
| `bfbc2-dbx-tools/` | **DbxEdit** and **DbxEditGUI**: view, edit, check and repair binary `.dbx` files; XML export and import |
| `bfbc2-fbrb-tool/` | **FbrbTool**: unpack and repack `.fbrb` bundles |
| `bfbc2-console-readers/` | read-only readers for Xbox 360 disc images and content packages |
| `bfbc2-diff-workbench/level-diff/` | **LevelDiff**: what differs between two `.dbx` files, instance by instance |
| `bfbc2-automated-map-and-bot-test-harness/` | **test harness** (offline, M5): per level and path-data file (DICE's, NavKit's) structure, spawn-to-objective reachability, path-test batteries, orders near the route network, sector offsets, NavKit's DICE parity, and AI readiness from the game copy's bundles; saved runs compared between builds (`harness.py`); in-game tests on hold |
| `bfbc2-automated-map-and-bot-test-harness/crash-reader/` | **crash reader**: the game's `crashreport.xml` turned into names, code, null pointers and hints; `exeref.py`: where `BFBC2Game.exe` uses a string or an address, with a short code window |
| `bfbc2-menu-tools/` | **menu tools**: `aptcheck.py` checks menu patches against the rules that crash the menu loader (OPFOR's `src\menus\*.patch` too); `aptrefs.py` cross-references the externs, FSCommands and engine events of every menu and HUD movie with the exe's name / id table; `bc2loc.py` looks up, searches and writes (`set`) the localized texts; `aptgraft.py` writes APT movies and grafts characters, mode cases and the Onslaught HUD timer from one movie into another |
| `bfbc2-console-convert/` | **console resource converter**: Xbox 360 resources to PC - Havok physics (byte-swapped by the exe's own class layouts), textures (untiled, DXT1/3/5 and BC5, cube maps), MeshData (each part into a PC vertex layout) and mesh sets (rigid, composite, skinned); `FORMAT.md` |
| `bfbc2-shader-tools/` | **shader databases** (`shaderdb.py`): read, check and write the levels' Dx9 / 10 / 11 shader databases; merge other levels' entries in, clone an entry under a new mesh name with its textures renamed, add mesh parts; `FORMAT.md` |
| `bfbc2-culltree/` | **static cull trees** (`culltree.py`): read, verify and extend a level's cull tree (places for objects that have none) |
| `bfbc2-mesh-viewer/` | **mesh viewer** (`meshview.py`): any model of the game with its colour textures, as a browser page (orbit, parts on / off) and a picture |
| `bfbc2-model-tools/` | **model tools**: edit skinned MeshData (replace or add a part's geometry in its own vertex layout; buffers and shared windows kept) and build simple solids with texture-atlas coordinates (`skinmesh.py`, `shapes.py`) |
| `bfbc2-anim-tools/` | **Granny tools** (`granny.py`): the soldier skeleton and animations - bones, tracks and curves decoded; a bone's position track moved (constant, quantized, re-quantized ramp) with the Granny CRC recomputed |
| `core/` | the shared core: Python readers for `.dbx`, `.fbrb` bundles (read only), terrain height fields and water; the level loader (entities, teams, collision, areas and routes); the 3D view with picking |
| `bfbc2-map-inspector/` | **Map Inspector**: browse a level in 3D and inspect every object's source, properties and links; measure |
| `bfbc2-navigation-studio/` | **Navigation Studio**: NavKit's and DICE's navigation data in 3D, by island, sector, pitch or difference; islands and spawns off the main network; the path test; edit mode (walkable / unwalkable regions, seeds, links, route edges, traversals) into a side file, built with `navkit apply` and installed; a reference level's AI layer drawn on another level (`--compare`) and an anomaly overlay of NavKit builds, both with JSON export (`overlay.py`) |
| `bfbc2-stockmp-ai/` | **stock-MP AI layer**: gives a stock multiplayer level AI spawners, AI orders and their event wiring (HasAI, PathfindingTypes, SectorOffsets, AI template), proposed from the level's own objectives and spawns, built into its bundles and checked offline (`stockmp.py`) |

**Planned** (see [DESIGN.md](DESIGN.md))

| Folder | Tool | Milestone |
|---|---|---|
| `bfbc2-navigation-studio/` | M4 step 7: an in-game check that the game's AI uses a hand-made link (steps 1-6 - path test, editing, side files, `navkit apply`, install - are done) | M4 |
| `bfbc2-automated-map-and-bot-test-harness/` | in-game tests: `testrun.bat` per level (on hold - needs playtest), bot traversal, stuck detection, squads and vehicles from OPFOR's telemetry | M6, then growing |
| `bfbc2-bot-ai-debugger/` | live view of the AI while the game runs (through Project OPFOR BFBC2) | M6 |
| `bfbc2-tactical-map-authoring-tool/` | tactical annotations above the navigation data for bots | M7 |
| `bfbc2-diff-workbench/` | ImHex patterns for BC2 formats, format coverage and structure-aware diffs (LevelDiff, in `level-diff/`, is ready) | alongside |
| `bfbc2-ai-behavior-simulator/` | runs OPFOR's bot decision code on simplified maps, without the game | with Goal 3 |

The asset builder & validator, `bfbc2-asset-builder`, is part of the Project OPFOR BFBC2 repository (DESIGN.md
section 4.9).

## The 3D tools and your GPU

The Map Inspector and the Navigation Studio draw with OpenGL 3.3. Each prints the GPU it uses when it starts
(and shows it in its status line). On a laptop with two GPUs, Windows may give Python the slower integrated
one: set `python.exe` to "High performance" in Settings > System > Display > Graphics. On the development
PC that took the Navigation Studio on SP_01 (10.4 million triangles) from the AMD Radeon integrated GPU to
the GeForce RTX 4060, at 144 frames a second.

## Tests

```
python run_tests.py quick      before every commit: no game data, about a minute
python run_tests.py game       before every push: uses Project OPFOR BFBC2's game copy, about 10 minutes
python run_tests.py full       after changing a format reader: every unpacked file and NavKit's regression on all 15 levels, about 8 hours
python run_tests.py --list     what each tier runs;   --only <name> runs the tests whose name contains it
```

| Test | Tier | Checks |
|---|---|---|
| `tests/test_repo.py` | quick | no game files or large files in the history, no co-author lines, no build output tracked, READMEs, LICENSE, every Python file compiles |
| `tests/test_review_fixes.py` | quick | four defects from a code review (2026-10-26): NavKit's apply builds with the normal build's inputs (detailed zones, ladders); the stock-MP installer reinstalls a rebuilt file and keeps the original as its backup; the Navigation Studio's `--edit` is a flag; the navigation cache key follows the terrain's content |
| `tests/test_review_fixes2.py` | quick | defects from the follow-up review (2026-10-27): the harness records a missing path-data file per file key, refuses to overwrite its reference through a casing alias; NavKit's restore after a second install on a map without an original; the stock-MP installer's markers found case-insensitively; FbrbTool refuses an older manifest whose size2 differs |
| `bfbc2-culltree/tests/test_culltree.py` | quick | cull trees and levels built in the test: the DBX codec, sphere keys, the tree layout and checks, verify, adding places for slot-0 objects (leaves, spheres, ancestors, spare nodes), the command line |
| `bfbc2-shader-tools/tests/test_shaderdb.py` | quick, game | built Dx9 and Dx10/11 databases round trip and pass the checks; merges (needed meshes only, first donor wins, records shared, programs re-sorted, every entry drawing as in its source, Dx kinds and the program limit); with the game: mp_008cq's and ons_mp_008's databases |
| `bfbc2-menu-tools/tests/test_aptgraft.py` | quick, game | a movie built through the model written, read back, round-tripped packed and in the other byte order; a sprite grafted between byte orders; dead data kept; the command line; with the game: every SwfMovie round trips, the Onslaught timer grafted into a copy of the original HUD (no finding, no new extern reads) |
| `bfbc2-menu-tools/tests/test_bc2loc_write.py` | game | every language written back byte-identical; `set` adds and changes texts, every other entry kept, the table sorted; non-ASCII refused |
| `bfbc2-console-convert/tests/test_havok.py` | game | class layouts from the exe; every Havok resource the console and PC share converts to the PC file (or differs only where FORMAT.md 1.3 says); every console Havok resource of the Onslaught builds converts |
| `bfbc2-console-convert/tests/test_texture.py` | game | console textures: every converted format matches its layout size; 180+ textures the console and PC share convert byte-identical or to the same picture; cube maps; unknown formats refused |
| `bfbc2-mesh-viewer/tests/test_meshview.py` | game | the Garand's first-person mesh read as DICE's parts with textures; an async-bundle weapon and a rigid projectile read; the page embeds every part; the picture shows the model |
| `bfbc2-model-tools/tests/test_models.py` | game | every Garand / Type 5 LOD and mesh set written back byte-identical; a part replaced and read back (other parts unchanged); shapes' winding and tangents |
| `bfbc2-anim-tools/tests/test_granny.py` | game | the soldier skeleton (147 bones, weapon bones); every WW2 rifle animation parsed with every curve format; `shift` on a constant, a quantized and a ramped track: keys where asked, other tracks unchanged, size kept, CRC right |
