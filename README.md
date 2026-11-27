# BFBC2 Tool Suite

Tools for building and debugging AI on Battlefield: Bad Company 2 maps, in one repository. The 3D tools
share one core: level loading, a 3D view, picking and the game's data formats.

Version: see `VERSION` and `CHANGELOG.md`. What is being worked on and what comes next: `ROADMAP.md`. How changes
are merged and versions released: `RELEASING.md`. Which Python is left and what retires it, file by file:
`PYTHON_STATUS.md` (ROADMAP.md, "Python left").

**Licence.** The suite is under the MIT licence (`LICENSE`), with one exception: `rust/bc2-xma`, the XMA2 sound
decoder, is a translation of FFmpeg's decoder and is under FFmpeg's licence, the GNU Lesser General Public License
2.1 or later (`rust/bc2-xma/LICENSE`, its `README.md`; the user's choice, 2026-11-22). It is linked into the Rust
core (`bc2_native`) and `bfbc2.exe`; its source is that folder.

## Getting started

1. Install Python 3.12 or newer (`pyproject.toml` targets 3.12: bf1943-import uses its f-string syntax), then the
   libraries the tools use:
   `python -m pip install -r requirements.txt` (numpy, matplotlib, pillow, pefile and
   capstone for the crash reader's code view, maturin, ruff and mypy, pinned to the versions CI uses).
2. Get the suite's Rust core and its command-line binary: in a command prompt in this folder,
   `python rust\build_native.py --download`. It fetches the prebuilt module (`core\bfbc2core\bc2_native.pyd`: nearly
   every tool reads the game's files through it) and `bin\bfbc2.exe` (the tools' command lines in Rust: "bfbc2.exe"
   below) made for this checkout's Rust sources, checks them and puts them in place; run it again after you update
   the suite (a tool that needs a newer one says so). No download for your checkout (local Rust changes, or not
   Windows): build them instead ("The Rust core" below). The map editor has no download yet: it is built with Rust
   (`python rust\build_native.py --editor`, "The map editor" below).
3. Put this folder next to your `project-opfor-bfbc2` folder (or set `OPFOR_ROOT` to it): the tools read the game
   copy there.
4. Double-click **`BFBC2 Tool Suite.pyw`**. One window has every tool:
   - **Levels:** choose a level, then **Open in Map Editor** (or double-click the level) or **Open
     navigation** (the editor in its navmesh view: the AI's navigation data); **Prepare level** runs NavKit's
     prepare alone. A level is unpacked the first time (offline, about 20 s); the editor shows its progress and
     opens the level when it is loaded.
   - **Bundles, Level diff, Crash reader, Xbox files, Tests:** the command-line tools with file pickers and
     options; their output appears at the bottom. Bundles, the crash reader and the level prepare run through
     `bin\bfbc2.exe` when it is built from this checkout, else through the Python.
   - **More tools:** the map editor, its DBX inspector and NavKit's window.

   The test harness, the menu tools and the stock-MP AI tool have no tab yet: run them from a command prompt
   (their READMEs).

The map editor can also be started on its own: `bin\bfbc2-editor.exe` ("The map editor" below), or give the level
on the command line (`bin\bfbc2-editor.exe mp_005`; `--navmesh` opens its navigation). On a laptop with two GPUs,
see "The 3D tools and your GPU" below.

### When something goes wrong

Every tool keeps a log in `%LOCALAPPDATA%\bfbc2-tool-suite\logs\` (one file per tool and day, a big day split
into numbered parts, each tool's 20 newest days kept within size caps: README "Caches"): what it did, warnings, and every error with its full details. When a tool crashes, a window says what
happened, the likely cause and what to do - for example "a Python library the tool needs is not installed
(PIL) - pip install pillow", "the level is not prepared", "a file could not be written - close the game" - and
where the log is. The
suite window checks the libraries when it opens, and when a tool it runs fails it shows that tool's last lines.
The command-line tools - `bfbc2.exe` and the Python command lines it ports (FbrbTool `fbrbtool.py`, DbxEdit
`dbxedit.py`, NavKit's `navkit.py` and `tools\*.py`, the crash reader, the console readers, the menu tools, the test
harness, `leveldiff` ...), the test runner - log the same way and, on an error, print the cause, what to do and the
log's path (exit code 1); the map editor and NavKit's window show it in a window. If you report a problem, send the
log file.

**Exit codes** (round QW2, audit AU2 Q7). Every command line follows Python's conventions, and `bfbc2.exe` gives
the same codes as the Python command line it ports:

| Code | Meaning | Where |
|---|---|---|
| 0 | done; for a checking command, nothing found | every tool |
| 1 | an error (the cause, what to do and the log's path printed), or a check that found problems: `dbxedit check` / `roundtrip`, `aptcheck`, `texqa --baseline` regressions, `mapexport check` failing lines, `run_tests.py` failed tests, `gameledger original` with no original; a retired command line's refused input (README "bfbc2.exe") | every tool |
| 2 | wrong arguments: argparse's usage error (Python's and `bfbc2.exe`'s), `bfbc2-editor` options, `bfbc2 gameledger`, `bfbc2 install`, `bfbc2 mods` and `bfbc2 terrain` usage, a `bfbc2 mods` operation that changes other mods too (run again with `--yes`), `run_tests.py`'s unknown options, an `--only` that matches no test, `--no-game` outside the quick tier | every command line |
| 3 | `dbxedit import`: written, but with guessed field types (check the result); `bfbc2 install`: another WW2 rebuild or install holds the rebuild lock (`work\ww2\_rebuild\rebuild.lock`), nothing done; `bfbc2 mods`: another mods operation is running | `dbxedit`, `install`, `mods` |
| 9 | stopped on purpose by `BC2_LEDGER_CRASH_AT` / `BC2_INSTALL_CRASH_AT` / `BC2_MODS_CRASH_AT` (the install ledger's and the installer's crash tests only) | `bfbc2 gameledger`, `bfbc2 install`, ledger writes |

A Python tool that stops with an uncaught error also exits 1 (after the window or the printed cause). Project
OPFOR's scripts stop with `SystemExit(<message>)`, which is exit code 1 too.

**Ready to use**

| Folder | Tool |
|---|---|
| `rust/bc2-editor/` (`bin\bfbc2-editor.exe`) | **the map editor**: view any map of the game copy; edit a copy of it as a map project (objects, gameplay, layers, undo); export it as a new multiplayer level for any era and mode, and install or uninstall it through Project OPFOR ("The map editor" below) |
| `rust/bc2-cli/` (`bin\bfbc2.exe`) | **`bfbc2.exe`**: the tools' command lines below in one Rust binary, `bfbc2 <tool> ...` with the same arguments, output and files as `python <tool>.py ...` ("bfbc2.exe" below) |
| `bfbc2-auto-navkit/` | **BC2 NavKit**: generates AI navigation data (`pathdata.hpd`) for levels. Start with `QUICKSTART.md`, or `navkitGUI.pyw`. |
| `bfbc2-dbx-tools/` | **DbxEdit** (`bfbc2.exe dbxedit`, or `dbxedit.py`; the Rust core) and its window, the map editor's **DBX inspector** (`bfbc2-editor --dbx <file>`; DbxEditGUI, the C# window, was retired in round GR4): view, edit, check and repair binary `.dbx` files; XML export and import |
| `bfbc2-fbrb-tool/` | **FbrbTool** (`bfbc2.exe fbrbtool`, or `fbrbtool.py`; the Rust core): unpack and repack `.fbrb` bundles |
| `bfbc2-console-readers/` | read-only readers for Xbox 360 disc images and content packages |
| `bfbc2-automated-map-and-bot-test-harness/` | **test harness** (offline, M5): per level and path-data file (DICE's, NavKit's) structure, spawn-to-objective reachability, path-test batteries, orders near the route network, sector offsets, NavKit's DICE parity, and AI readiness from the game copy's bundles; saved runs compared between builds (`harness.py`); in-game tests on hold |
| `bfbc2-automated-map-and-bot-test-harness/crash-reader/` | **crash reader**: the game's `crashreport.xml` turned into names, code, null pointers and hints; `exeref.py`: where `BFBC2Game.exe` uses a string or an address, with a short code window |
| `bfbc2-menu-tools/` | **menu tools**: `aptcheck.py` checks menu patches against the rules that crash the menu loader (OPFOR's `src\menus\*.patch` too); `bfbc2 aptrefs` (library `aptrefslib.py`) cross-references the externs, FSCommands and engine events of every menu and HUD movie with the exe's name / id table; `bc2loc.py` looks up, searches and writes (`set`) the localized texts; `aptgraft.py` writes APT movies and grafts characters, mode cases and the Onslaught HUD timer from one movie into another |
| `bfbc2-console-convert/` | **console resource converter**: Xbox 360 resources to PC - Havok physics (byte-swapped by the exe's own class layouts), textures (untiled, DXT1/3/5, BC5, DXT5A -> 8-bit, RGB565, RGBA16F, 8-bit L8 (format 10), cube maps; the header's flag bit 1 kept as the console's), MeshData (each part into a PC vertex layout; triangle strips unrolled) and mesh sets (rigid, composite, skinned, tree), terrain and water, soot meshes, sounds (EA-XMA -> EA-XAS); `FORMAT.md`; `texqa.py`: offline QA of converted textures against their Xbox sources and DICE's copies |
| `bfbc2-shader-tools/` | **shader databases** (`shaderdb.py`): read, check and write the levels' Dx9 / 10 / 11 shader databases (and the Xbox 360's); merge other levels' entries in, clone an entry under a new mesh name with its textures renamed, add mesh parts; `FORMAT.md`. Xbox 360 shaders on the PC: `xenos.py` (Xenos microcode), `xenos_hlsl.py` (HLSL for Dx10/11), `pcbuild.py`; `XENOS.md`; `renderqa.py`: an offline Direct3D 11 render harness that draws converted meshes, terrain strips and patches with the game's own Dx10/11 programs and flags black, NaN, misplaced and foreign-textured draws |
| `bfbc2-culltree/` | **static cull trees** (`bfbc2 culltree`, library `culltreelib.py`): read, verify and extend a level's cull tree (places for objects that have none) |
| `bfbc2-mesh-viewer/` | **mesh viewer** (`meshview.py`): a picture of any model of the game with its colour textures (side, top, front), `find` for mesh names, and the index other tools read the game copy through. Its browser page went in round GR3: the map editor's **asset preview** shows a mesh (orbit, levels of detail, parts, destruction states, textures by channel) |
| `bfbc2-model-tools/` | **model tools**: edit skinned MeshData (replace or add a part's geometry in its own vertex layout; buffers and shared windows kept) and build simple solids with texture-atlas coordinates (`skinmesh.py`, `shapes.py`); PC textures from pictures (`texwrite.py`: DXT1 with 1-bit alpha, normal maps, DXT5, 8-bit atlas levels); fewer triangles (`decimate.py`) |
| `bfbc2-anim-tools/` | **Granny tools** (`granny.py`): the soldier skeleton and animations - bones, tracks and curves decoded; a bone's position track moved (constant, quantized, re-quantized ramp) with the Granny CRC recomputed; orientation curves written (`set_orientations`); tracks added from a donor (`add_tracks`); Xbox 360 resources read and converted to the PC's format (`topc`), DICE's bone table renumbered onto another skeleton; reading and `to_pc` in the Rust core when it is built, `to_pc_many` / `roots` for many files on all cores; generated aim poses (`aimanim.py`) |
| `bfbc2-bf1943-import/` | **Battlefield 1943 import**: the user's BF1943 Xbox 360 resources into BFBC2's PC formats, from the user's own copy - animations, skeletons and textures (`bf1943import.py`, a record per resource in `import.json`; the 3p undeploys get the tracks DICE adds), models (`meshport.py`), physics (`havokport.py`, Havok 4.6.1 -> 5.5.0; `mopp.py` decodes and checks the MOPP trees), DBX partitions (`dbxport.py` on BFBC2's own schema from `dbxcatalogue.py`; `bfbc2 dbxparity` / `portparity` against DICE's own conversions, `dbxparitylib.py` their library), effects (`effectport.py`) and merged emitter atlases (`emitteratlas.py`) |
| `core/` | the shared core (Python over the Rust core, `bc2_native`): `.dbx` (reader and writer), `.fbrb` bundles (read, pack, unpack), terrain height fields and water; the level loader (entities, teams, collision, areas and routes); the 3D view with picking; `paths.py`, the one place that finds Project OPFOR BFBC2, its game copy, the built tools and the sibling projects (every tool's own locator wraps it) |
| `bfbc2-map-editor/` | the map editor's README (build, run, what it does) and design document (`EDITOR_ARCHITECTURE.md`); its code is in `rust/bc2-editor` and the crates it names. The Map Inspector it grew from was retired in round GR1 (2026-11-20; `RETIRED.md`) |
| `bfbc2-navigation-studio/` | `overlay.py`: a reference level's AI layer against a level's NavKit build, and the anomalies of a NavKit build, as JSON (headless). The Navigation Studio's window was retired in round GR2 (2026-11-20): its views and edits are the map editor's AI workspace ("The map editor") |
| `bfbc2-stockmp-ai/` | **stock-MP AI layer**: gives a stock multiplayer level AI spawners, AI orders and their event wiring (HasAI, PathfindingTypes, SectorOffsets, AI template), proposed from the level's own objectives and spawns, built into its bundles and checked offline (`bfbc2 stockmp`; library `stockmplib.py`) |

**Planned** (see [DESIGN.md](DESIGN.md)). The user's plan of 2026-11-19 (`ROADMAP.md`): the bot AI debugger, the
tactical map authoring tool and the AI behaviour simulator become workspaces of the map editor rather than tools of
their own (their plans: `bfbc2-map-editor/EDITOR_ARCHITECTURE.md` section 10). Planned tools get no folders of their
own until they are built (`RETIRED.md`).

| Folder | Tool | Milestone |
|---|---|---|
| `rust/bc2-editor` (AI workspace) | M4 step 7: an in-game check that the game's AI uses a hand-made link (steps 1-6 - path test, editing, side files, `navkit apply`, install - are done) | M4 |
| `bfbc2-automated-map-and-bot-test-harness/` | in-game tests: `testrun.bat` per level (on hold: the user paused in-game tests on 2026-11-20), bot traversal, stuck detection, squads and vehicles from OPFOR's telemetry | M6, then growing |
| `rust/bc2-editor` (bot AI debugger workspace) | live view of the AI while the game runs (through Project OPFOR BFBC2): `bfbc2-map-editor/EDITOR_ARCHITECTURE.md` 10.2 | M6 |
| `rust/bc2-editor` (tactical authoring workspace) | tactical annotations above the navigation data for bots: `EDITOR_ARCHITECTURE.md` 10.1 | M7 |
| `bfbc2-diff-workbench/` | ImHex patterns for BC2 formats, format coverage and structure-aware diffs (`DESIGN.md` 4.7; LevelDiff became `bfbc2.exe leveldiff` and the map editor's Diff panel in round EX1) | alongside |
| `rust/bc2-editor` (AI behaviour simulator workspace) | runs OPFOR's bot decision code on simplified maps, without the game: `EDITOR_ARCHITECTURE.md` 10.3 | with Goal 3 |

The asset builder & validator, `bfbc2-asset-builder`, is part of the Project OPFOR BFBC2 repository (DESIGN.md
section 4.9).

`texqa`, `renderqa`, dbxport's emitter-gradient converter and `mesh.relayout`, on branches on 2026-11-10, are on
`main` (checked 2026-11-15: no branch holds an unmerged commit).

How the code is laid out: every tool, module and test finds the suite root from its own file and names the folders it
imports from with `suite_paths.use(...)` (`suite_paths.py`, the one `sys.path` bootstrap); paths to OPFOR, the game
copy, the tools and the sibling projects come from `core/bfbc2core/paths.py` (DESIGN.md section 3.5).

## The 3D tools and your GPU

The map editor draws with wgpu, and its game-accurate view with Direct3D 11 ("The map editor"); its status line
shows the GPU it uses. On a laptop with two GPUs, Windows may give it the slower integrated one: set
`bfbc2-editor.exe` to "High performance" in Settings > System > Display > Graphics.

## The map editor (preview: view a map, edit a copy of it)

The map editor (`bin\bfbc2-editor.exe`; its code in `rust/bc2-editor`, `bc2-levelwrite`, `bc2-render` and
`bc2-scene`) has its own folder since round FM1: **[bfbc2-map-editor/README.md](bfbc2-map-editor/README.md)** (what
it does, how to build and run it, its command-line switches, `mapexport.exe`, its tests) and its design document,
[bfbc2-map-editor/EDITOR_ARCHITECTURE.md](bfbc2-map-editor/EDITOR_ARCHITECTURE.md).

In short: view any map of the game copy (BFBC1, BFBC2, Vietnam and WW2 levels, the Package-only ones too), read-only;
edit a copy of it as a **map project** (objects, gameplay, the AI workspace's navigation, layers, undo; the project
holds only the edits, never game files); see it as the game draws it (the game-accurate view, DICE's own Direct3D 11
shaders); and export it as a new multiplayer level for any era and mode, installed and uninstalled by the editor itself
(`bc2_install::mapedit`, round OR6; `bfbc2 install mapedit` and Project OPFOR's `mapedit_install.py` run the same). The exports are built and checked offline; none has been tested in the game yet (the
user paused in-game tests on 2026-11-20). The Map Inspector (GR1) and the Navigation Studio's window (GR2) were
folded into it and retired (`RETIRED.md`).

Run it (Rust as in "The Rust core" below; the first build takes a few minutes):

    python rust\build_native.py --editor
    bin\bfbc2-editor.exe [level | --project <name>.mapedit.json] [--opfor <folder>] [--theme light] [--navmesh]

(`--editor` builds `bfbc2-editor.exe` and `mapexport.exe` in this checkout's build folder and copies them to the
suite's `bin\`: "Where Rust builds", below.) Or from the hub: Levels > "Open in Map Editor" / "Open navigation", or
More tools > "Open Map Editor"; `--dbx <file.dbx>` (More tools > "DBX inspector") opens the DBX inspector,
DbxEditGUI's successor (round GR4). Without a display,
`bfbc2-editor.exe <level> --screenshot out.png [--size 1600x950] [--select <guid>] [--theme light]` draws the window
offscreen and saves it (`--demo` instead of a level: a small made-up scene). The editor keeps its layout, theme and
bookmarks in eframe's per-user storage under `%APPDATA%`.

Its tests (the quick tier's Rust entries, `tests\test_rust.py`; the whole-level ones in the game tier since round TS1;
`python tests\test_editor.py` runs them alone,
one at a time; each crate's integration tests are one binary since round QW1: `--test editor`, a file's tests by its
module, `g2::`) prove it headless: the theme generated from every token in both
themes; the outliner against the retired Python Map Inspector's lists for SP_01, MP_005, MP_009GR and ONS_MP_004
(recorded before its retirement by `rust\bc2-editor\tests\record_inspector.py`, entity by entity: order, GUID, category and layer, team, position); the
view drawn offscreen (not blank, the outline in the `selection` colour and nowhere else, picking, a click selecting,
the command palette opening a top view) and each of the four levels drawn with a flag selected; round GR1's property
tree, references and referrers against what the Map Inspector showed for 1,074 objects of the same four levels
(`tests\inspect_parity.rs`, its answers kept as digests) and its Links, All fields and Measure driven headless
(`tests\gr1.rs`). Round G2's
(`tests\g2.rs`, `tests\factions.rs`): every `view-*` / `scene-*` token found in a rendered frame, the overlays'
contrast measured on the frame's own pixels, panel contrast and the third neutral's hue distance, the faction labels
on BFBC2, Vietnam and WW2 levels (read only), Help generated from the registry (an action without a description
fails), and opening a level that needs preparing (a stand-in script: its steps, then the level; a failure in words).
`BC2_EDITOR_PREPARE=<level>` also runs the real preparation on a stand-in OPFOR folder in temp.
Round E2 adds levelwrite's proofs (`cargo test -p bc2-levelwrite`: the project log, its file, undo / redo, autosave and recovery,
the base-changed report; edits on MP_005, SP_01 and Guadalcanal copied to a temp folder: they parse, differ from the
base exactly where the operations say, round-trip through the Rust DBX writers, load in the scene with each object
where the log put it, and equal DbxEdit's `drop` / `setfield` and the Python `dbxbin` reference byte for byte), the
surface placement with sink depths on terrain and on a mesh, the gizmo's maths, and the editing interface driven
headless (`tests/editing.rs`: click, a 5 m drag on the grid, undo, redo, a 15-degree turn, box selection, duplicate,
delete, snap to ground sunk 0.5 m and kept on the next move, a layer, save and reopen).
The later rounds' tests sit beside them: `tests\e2b.rs` and `tests\asset_browser.rs` (E2b), `tests\g3.rs` and
`tests\g4.rs` (the start screens), `tests\game_view*.rs` (R5 - R5c), and in `rust\bc2-levelwrite\tests` the exports
(`export_sp01.rs`, `export_e3b.rs`, `export_e3c.rs`, `export_e3d.rs`); each round's figures are in its
`rust/BENCHMARKS.md` section.

## The Rust core (required for the core formats, the console converters and NavKit's builds)

`rust/` is a Cargo workspace: the start of the shared Rust core the map editor is built on
(`bfbc2-map-editor/EDITOR_ARCHITECTURE.md` 3.8). Today it holds the binary DBX reader and writer, the FBRB bundle
reader and packer, the terrain, water and Havok collision readers, the shader database reader / writer, the cull tree tool's element reader and level search, and the level loader (a Rust scene), ported from
`core/bfbc2core/dbx.py`, `fbrb.py`, `terrain.py`, `water.py`, `level.py` and NavKit's `levelobjects.py` / `hkpack.py`
and proven identical to them on every corpus file, every bundle of the game copy and, entity by entity, on 18
levels; and the pieces whose Python was retired: texture conversion and NavKit's grid walks (2026-11-17); the Xbox
360 shader translator's HLSL emission, the sound, mesh, physics and Granny conversions, the rest of NavKit's
generator, `navdata` and the path test (2026-11-18); and the core format readers and writers - DBX, bundles, terrain,
water, the collision index, the level loader, the cull tree element reader (2026-11-18, round F1a).
Round F0 (2026-11-18) added the file formats
that had no Rust yet, each beside its Python reference: path data (`hpdtool.py`) and the runtime-mesh writer
(`kymesh.write_mesh`), the localized texts (`bc2loc.py`) and BIGF archives (`menucommon.parse_bigf`, `apt.read_bigf`),
the Xbox 360 containers (`stfs.py`, `xisolib.py`), the Wave resources and EA stream framing (`audio.py`), the mesh
headers (`mesh.py`, `meshset.py`), `texture.py`'s layout helpers and PC texture reader and `havok.classes`
(`rust/BENCHMARKS.md`, "F0"). Round F2a (2026-11-18) ported the rest of the console converter (`terrain.py`,
`soot.py`, `mesh.py`'s `console_layouts` / `decode` / `relayout`, `havok.convert_wrapper`, `texture.check`), the model
tools (`texwrite.py`'s encoders, `skinmesh.py`'s vertex work) and Granny's writing side (`granny.py`'s `to_bytes`,
`add_tracks`, `shift_position`, `set_orientations`), each beside its Python reference (`*_reference`,
`use_native(False)`, `BFBC2_PURE_PYTHON=1`); `texwrite.py`'s principal axes are the core's too since round OR14
(`bc2_convert::eigh3`: `numpy.linalg.eigh`'s path through OpenBLAS's kernels written out, the same axes to the bit;
`rust\parity_texwrite_axes.py`). Round RT2 (2026-11-20) retired most of those references against their recorded
answers (`tests\replay.py`, the `*_replay*.json` files beside the tests): the console converter's (terrain, soot,
mesh, `meshset.parse`, `havok.classes` / `convert_wrapper`, texture levels / decode / check), `menucommon`'s BIGF
and PE readers, `navsample.load`, `hpdtool.mesh_layout` / `mesh_coverage`, `audio.py`'s EA framing, and round F2b's
`havokport`, `mopp`, `navtune`, `xmlinst`; those functions now need the Rust core and raise
`bfbc2core.native.Refused` for an input it does not read. The others stay with their reason in `rust/BENCHMARKS.md`,
"RT2".

**What needs it** (since 2026-11-17): **texture conversion** (`bfbc2-console-convert/texture.py`'s `convert`: the
console converter, the BF1943 importer's textures, meshes and effects, OPFOR's WW2 and Onslaught pipelines) and
**NavKit's grid walks** (`navwalk.py`, the build itself since NK5 - `navbuild.py` retired in NK5-rest: every NavKit build and apply, the AI workspace's
rebuilds included). Their Python twins were retired after the full test tier had passed on the Rust (140 of 140,
2026-11-17), as `EDITOR_ARCHITECTURE.md` 3.8 rules for ported logic. Since 2026-11-18 (the full tier 173 of 173)
also **the Xbox 360 shader translation** (`bfbc2-shader-tools/xenos_hlsl.py`'s `translate` / `translate_many`:
OPFOR's WW2 shader translation, `xdiff.py`), **the console sound, mesh, physics and Granny conversions**
(`audio.py`'s EA-XAS encoder and decoder - `audio.convert`, OPFOR's WW2 sounds -, `mesh.convert`'s vertex loop,
`havok.convert` / `havok.verify`'s packfile walk - the BF1943 importer's models and physics, OPFOR's Onslaught
builds - and `granny.to_pc`, animations and skeletons) and **the rest of NavKit's generator** (the walkability rules,
areas, tracing, route graphs and floods: every NavKit build), **`bfbc2core.navdata`** and **`bfbc2core.navpath`** (the
path data NavKit's parity measures and the harness read; the path test) and
**`navdiff.area_cells`** (round R1). And since round F1a (2026-11-18, `EDITOR_ARCHITECTURE.md` 3.8: format readers and
writers are ported, proven and retired too) **the core formats**: `bfbc2core.dbx` (`parse`, `to_xml`, `parse_many`,
`to_xml_many`, `write`), `bfbc2core.fbrb` (`entries`, `names`, `read`, `read_many`, the cache, `pack`, `pack_many`,
`unpack`), `bfbc2core.terrain`, `bfbc2core.water`, `bfbc2core.collision`, `bfbc2core.level.load` and
`bfbc2-culltree/culltreelib.py`'s `Doc` - nearly every tool of the suite and of OPFOR's pipeline reads through them.
Without the module these stop at once with an error saying how to get it. The retired Python's answers are kept as
recorded digests beside the tests (`dbx_golden.json`, `dbx_golden_corpus_*.json`, `fbrb_golden.json`,
`geometry_golden.json`, `scene_golden.json` in `core/tests/`, `culltree_golden.json`, NavKit's
`collision_golden.json`): the tests compare the Rust with them. The readers that still have a Python twin (the
shader database's queries, `xenos.py`'s decoder, Granny's, APT's, `kymesh`'s and `navsample`'s readers, round F0's formats
and round F2a's ports) run in Python without the module.

**Get it** - either way puts `bc2_native.pyd` and `bc2_native.json` in `core\bfbc2core\`:

- **Download** (no Rust needed): `python rust\build_native.py --download` fetches the module and `bin\bfbc2.exe`
  made for this checkout's Rust sources from the suite's GitHub Releases (64-bit Windows, Python 3.12 or later),
  checks their SHA-256 and their fingerprint, and puts them in place (a release made before round RT1 has the module
  only, and the download says so). The map editor and `mapexport.exe` are not in the release: build them
  (`--editor`). Each push to `main` that changes `rust\` publishes one
  (`.github/workflows/native.yml`: built with the pinned toolchain, the Rust checks and the Python tests without
  game data run first, release `native-<12 hex digits of the fingerprint>`). A checkout with Rust changes of its
  own has no download: build it.
- **Build** (once, and after changing anything in `rust/`; the quick test tier rebuilds it when it is stale):
  1. Install Rust from rustup.rs (stable, MSVC; `rust/rust-toolchain.toml` pins 1.100.0 and rustup fetches it) and
     Visual Studio's C++ build tools.
  2. `pip install maturin`
  3. `python rust\build_native.py` builds `bc2-py` (release) with maturin and puts the module in
     `core\bfbc2core\`, and builds `bfbc2.exe` into `bin\`; `--editor` builds the map editor and `mapexport.exe`
     into `bin\`; `--target-dir` prints this checkout's cargo build folder. `--if-stale` builds only when the module is missing or was made from other sources (and,
     without Rust, downloads it instead); `--remove` deletes it; `--fingerprint` prints the checkout's fingerprint.
     A module installed into Python instead (`pip install rust\bc2-py`, `maturin develop`) is used only when it was
     built with `BC2_SOURCE_HASH` set to that fingerprint.

**Which module is used.** The module carries the fingerprint of the Rust sources it was built from
(`bc2_native.__source_hash__`, next to `__version__`): a SHA-256 over the workspace and crate manifests, the lock and
toolchain files and every crate's `build.rs` and `src\**\*.rs`, each as its path, length and bytes with line endings
made LF (`core/bfbc2core/native.py`), so every checkout of a commit, with or without autocrlf, has the same one.
`bfbc2core.native` compares it with the checkout's on the first use in a process; `bc2_native.json` keeps the
checkout's fingerprint with each source file's size and time, so it is hashed again only after a file changed (a
few milliseconds). A module from other sources is never used silently: the pieces that need it raise
`bfbc2core.native.NativeMissing` naming the mismatch and the fix, the others warn once and run in Python.
`tests\test_native_required.py` (quick) checks all three cases on a copy of the suite.

| Crate | What |
|---|---|
| `rust/bc2-formats` | the game's formats in pure Rust, no Python types: `apt` (the menus' and HUDs' APT movies: constants, `apt.py`'s dump and instruction records, `decode_many` on all cores; round E3n); `aptmodel` (aptgraft's movie model: parse, layouts, writes, leftovers, roundtrip; round E4c) and `aptscan` (aptcheck's rules, aptrefs' scan; round E4c); `dbx` (parse, XML, values, writer; `parse_tree`: a compact read-only tree borrowing the file, a few allocations per file; `parse_all` / `parse_tree_all` / `to_xml_all` on all cores); `fbrb` (entries, any entry's data, whole bundles; `read_many` / `extract_many` on all cores; `cache`, the bundle cache: headers and decompressed payloads kept in the process and on disk; packing, byte for byte as FbrbTool's `pack`: `pack_path` / `pack_dir` / `pack_many`, `pack` from entries in memory; unpacking as FbrbTool's `unpack`: `unpack_path`, every bundle since round E3k, FbrbTool's command lines in `fbrb::tool`); `zlib` (zlib's deflate, levels 4-9, ported byte for byte: what FbrbTool's gzip streams hold); `terrain` (height fields: heights bit for bit, exact height at a point, a mesh, the tail's known layout); `water` (water meshes, the shader per triangle); `havok` (packfiles and a blueprint's collision triangles; a small test writer); `kymesh` (NavKit's runtime mesh record, `kymesh.py`'s `parse_mesh`; round E3o). `shaderdb` (shader databases, PC Dx9 / Dx10 / Dx11 and Xbox 360: every section as slices of the file, a writer; `shaderdb::query`, the logic: parameter blocks' textures / constants / engine textures, program indices, texture kinds, mesh-part meshes, the needed-name test, coverage, `check` with its MD5; round E4b; `shaderdb::open`, the opening and dispatch of a database, `pick_database`, `dx_from_name`; round SD1); `culltree` (the element streams of cull trees and level partitions as `culltree.py` reads them; the reference levels' search on all cores); `granny` (Granny skeletons and animations, PC and Xbox 360: the tables, the object tree as `granny.py`'s `root()` has it, DICE's bone table and its renumbering); `dbxedit` (DbxEdit's commands in process - `xml`, `edit` with every edit command and `@file`, `import` with `--like` / `--types`, `check`, `roundtrip`, `fixslots` - a port of `DbxEdit.cs` with its own model of a file that keeps what an edit does not touch byte for byte: DbxEdit's files, messages and exit codes; `run_many` on all cores); round F0's formats, each a port of a Python reader / writer kept as its reference: `hpd` (path data, `hpdtool.py`'s `parse` / `write`), `kymesh::write` (`kymesh.write_mesh`), `loc` (the localized texts and histograms, `bc2loc.py`'s `LocFile`, `decode`, `to_bytes`, `khash`), `stfs` / `xiso` (Xbox 360 content packages and game discs, `stfs.py` / `xiso.py`), `wave` (Frostbite Wave resources and EA's SPS / SNR / SNS stream framing, `audio.py`'s `read_wave`, `write_wave_pc`, `read_sps`, `read_snr`, `write_sps`), `meshdata` (the `MeshData` header and the mesh sets, `mesh.py` / `meshset.py`'s `parse` / `write`), `bigf` (BIGF archives, `menucommon.parse_bigf` / `apt.read_bigf`); `xmlinst` (round F2b: `bfbc2core.xmlinst`'s instance and field-reference scans of DbxEdit's XML); round F3, each beside its Python reference: `aimanim` (generated aim poses, `aimanim.py`'s `parse` / `write` / `to_pc`), `audioframe` (`audio.py`'s `xma_parts`, `xma2_riff` and the EA-XAS blocks of `_xas_stream`), `hpdlayout` (`hpdtool.mesh_layout` / `mesh_coverage`), `havok::Packfile::pointer_fixups` (for `hkpack.Packfile`); round QW2 moved in `guid` (`dbxbin.new_guid`'s UUID v5, from `bc2-stockmp`) and `pe` (the 32-bit PE reader, from `bc2-harness`), so the level writer no longer needs `bc2-stockmp` and the menu tools no longer need `bc2-harness` (audit AU2 A5) |
| `rust/bc2-stockmp` | the stock-MP AI layer of `bfbc2-stockmp-ai` (round E4a): `model` (`levelmodel.Model`), `author` (the authoring of the AI layer into a level partition, `author.py`, on `bc2_formats::dbx::edit::EditDbx`, the port of `dbxbin.py`), `checks` and `files` (`validate.py`'s readings: the structure, the changes to existing instances, the references into other partitions, the Rush stages, the folder index, the packed bundle against the built files), `guid` (`dbxbin.new_guid`'s UUIDv5); where the reference raises or formats a value the port does not, the port refuses and the Python runs, so a result is always the reference's |
| `rust/bc2-scene` | a level as the tools see it: `collision` (which packfile each blueprint uses: NavKit's index and mesh aliases in one parallel walk, each alias read from the binary partition - `dbx::field_ref`, the XML's answer without the text - and the converted XML only where the binary cannot answer; `index_cached` keeps each answer in a cache keyed by the files' sizes and times, so an unchanged level is indexed without opening its partitions), `scene` (`level.load`'s entities, blueprints, teams, outlines, collision, terrain and water on all cores; editable entities for the map editor) |
| `rust/bc2-convert` | the converter pipeline's hot spots: `texture` (console textures to the PC's: the Xenos untiling, the byte swap, DXT5A / RGB565 / A8R8G8B8 decoded; `bfbc2-console-convert/texture.py`'s `convert`, byte-identical on every BF1943 and Onslaught console texture; the Python converter retired 2026-11-17); `audio` (EA-XAS v1, the PC's in-memory sound codec: `bfbc2-console-convert/audio.py`'s encoder and decoder, the same bytes on every BF1943 Wave and every in-memory BFBC2 Xbox Wave; the encoder's search makes the reference's choice with less than half its work, each sound's groups on all cores, `encode_all` many sounds at once; round E3a; its NumPy code retired 2026-11-18); `mesh` (`bfbc2-console-convert/mesh.py`'s `convert` vertex loop: element by element, the packed normals and colour bytes to halves as NumPy rounds them; round E3g) and `havok` (`havok.py`'s `Packfile.convert`, the field walk over the game's class layouts, round E3g; `verify_packfile`, `havok.verify`'s record of each byte's owner, round E3o) - their Python retired 2026-11-18: what they do not convert raises `ConvertError` saying why; `granny` (Xbox 360 Granny resources to the PC's, `granny.py`'s `to_pc` byte for byte, a bone map renumbering DICE's table; `to_pc_all` on all cores; round E3i; its Python retired 2026-11-18, the reader's stays); `dbxport` (BF1943's DBX partitions in BFBC2's schema, `bfbc2-bf1943-import/dbxport.py`'s `port` rule for rule: the template choice with E3d's ranking index, the alignment, the reference resolver, the converters, the value passes and the gate, over trees kept on the Rust side - BFBC2's template partitions and the catalogue's tables parsed or copied once; the same bytes, report lines, records and errors as the Python reference on every real port; round E4d); `mopp`, `havokport`, `emitteratlas`, `dbxcatalogue`, `meshport` (round F2b: `bfbc2-bf1943-import`'s `mopp.py`, `havokport.py`, `emitteratlas.py`, `dbxcatalogue.build` and meshport's layout search, each its reference's answers or a refusal that runs the Python; `pyfmt`: Python's `round`, ties-to-even fixed formatting, compensated float `sum`, a dict's order); `texcheck` (round F0: a PC texture's levels and a level decoded to pixels - DXT1 / DXT3 / DXT5 / BC5 / L8 - `texture.py`'s `levels` / `decode`, whose NumPy stays the reference; `texture.py`'s `tiled` / `packed` / `layout` are bound to `texture`'s own) and `hkclasses` (round F0: `havok.classes`, the Havok class layouts of BFBC2Game.exe); round F2a: `terrain` (`terrain.py`'s console terrain, water, material map, visual terrain / water and their PC readers; `soot.py`), `meshconv` (`mesh.py`'s `console_layouts`, `decode`, `relayout`), `havok::convert_wrapper`, `texcheck::check`, `texwrite` (`bfbc2-model-tools/texwrite.py`: levels, DXT1 with and without 1-bit alpha, DXT5; NumPy's `eigh` gives the axes), `skinmesh` (`skinmesh.py`'s vertex work) and `granny_edit` (`granny.py`'s CRC, `add_tracks`, the `shift_position` / `set_orientations` values), each beside its Python reference |
| `rust/bc2-navbuild` | NavKit's generator: `Level` (which kept surface of a neighbouring column links to a surface), areas (`components`), traced sides, walkable lines, the route-group search, `walkable`'s flood and `islands_of` (E1d); `walkable`'s rules (`headroom`, `stair_slope`, `narrow_lips`, landings, `near_objects` / `on_objects`), `build_areas` and the cell picking (E2a); `trace_edges` / `trace_emit` with the bridges' union, `roof_islands`' rule, the cover nodes, Gabriel pruning, the walkable pairs and group joining, `near_characters`, `walkable`'s seed picks and `navsample.load`'s version 1 / 2 records (E3c); `build_graph` whole (`sector`: every sector of a stage at once) with a stage's areas and edge rows kept in the core (`NavAreas`: `drop_one_way`, the detail pass's candidate areas, `area_of`), the floods and islands from a parallel union-find, `walkable`'s unseeds, `navdiff.area_cells` (E3h); `navdata`'s areas, cells and union-finds and `astar`, `navpath.route`'s search (E3o) - sectors, nodes and pairs in parallel where the order of results allows (`navbuild.py` / `navwalk.py` / `navsample.py`; the same path data byte for byte). Their Python is retired (E1d 2026-11-17, the rest 2026-11-18: round R1) but `navsample.load`'s record reader, a format reader kept for good (`navsample.use_native`); round E4e's tools (`vehicle`: navvehicle's obstacle distance; `domains`: navdomains' Dijkstra; `levelscan`: levelobjects' blueprint scan; `measures`: navdiff's sector comparison, navdf's figures and ordercheck's areas; `offline`: the offline sampler's terrain test), their Python kept as reference and fallback (`navnative`); `navtune` (round F2b: `bfbc2core.navtune`'s operation checks, side-file text and summaries); round F3: `navsample`'s version 3 (`navsample.load`'s NumPy pass, the arrays filled in Python's buffers) and its writer (`navsample_offline.file_bytes`), their Python kept as reference |
| `rust/bc2-shader` | the Xbox 360 shader translator (round E3f): `xenos` (the XDK shader container, its constant table, literals and header, and the Xenos microcode - control flow, ALU and fetch instructions - decoded: `bfbc2-shader-tools/xenos.py`'s `parse_shader`, the same tree attribute for attribute) and `hlsl` (a decoded program as HLSL for the Dx10 / Dx11 renderers: `xenos_hlsl.py`'s `translate`, the same text byte for byte and the same `TranslateError`s); `translate_all` / `translate_blobs` many programs on all cores. Proven on every program of BF1943's and BFBC2's Xbox shader databases (8,320 distinct, 63,305 with copies; 22,863 translations). The decoder's Python stays the reference (a format reader: `xenos.use_native`, `BFBC2_PURE_PYTHON`); the translation's Python was retired 2026-11-18 (any tree handed over: `xenos_tree_program`); `disasm` (round F3: `xenos.disasm`'s listing of a decoded program, line for line; the header comments stay Python's, `disasm_reference` the reference) |
| `rust/bc2-harness` | the map and bot test harness and the crash-reader tools (round E4f, `rust/BENCHMARKS.md` "E4f"): `oplog` (opfor.log sessions as `testharness.ingame.sessions`, the file read and split on all cores; the crash reader's lines before a crash), `pe` (the exe mapped as pefile maps it), `x86` (32-bit decoding with capstone 5's text and operands on iced-x86: every byte offset of BFBC2Game.exe's code, 16,786,165 starts, decoded identically; an encoding outside that proof is refused and capstone decodes it), `exe` (the crash reader's function starts, code view and call before a return address; exeref's strings, references, data references and annotated window), `graph` (the reachability pair bits, the ground cell index). The Python stays the reference and the fallback (`crash-reader/harnesscore.py`, `BFBC2_HARNESS_PYTHON=1`); `pe::raw` (round F2b: `menucommon.PEImage`'s section table, strings and searches, by file offset) |
| `rust/bc2-py` | the Python bindings (PyO3), module `bc2_native` |
| `rust/bc2-cli` | the command-line binary `bfbc2.exe` (rounds O1 - O5): the tools' command lines as subcommands, each its Python command line's port ("bfbc2.exe" below) |
| `rust/bc2-pyrt` | what the Python command lines show of the Python runtime, for `bc2-cli`'s ports: exceptions and their texts, `repr` / format of values, `ntpath` (round O3) |
| `rust/bc2-menu` | the menu tools' library (`menucommon`'s pieces) for `bc2-cli` (round O3) |
| `rust/bc2-editor` | the map editor application, binary `bfbc2-editor.exe` (rounds G1 - G4, E2b, GR1; "The map editor" above); not part of `bc2_native` |
| `rust/bc2-levelwrite` | the map project's operation log and its edits on a level partition (round E2), the export (`export`, `mpconvert`, `modelayer`, `profile`, `culltree`, `awards`; rounds E3 - E3d) and the binary `mapexport.exe` |
| `rust/bc2-render` | the editor's game-accurate view: DICE's Direct3D 11 shaders over the level (rounds R5 - R5c) |
| `rust/bc2-install` | the game-copy installer on the install ledger (round OR1, "OPFOR in Rust" stage 1): Project OPFOR's WW2 install after its build staged the bundles - the backup guard, the levels' bundles and records, the DLL's markers, the shared bundles and ini files through the ledger (`gameledger::Ledger::install_keep` / `restate`, the records OPFOR's Python wrote), the registration last, the verification, the opt-out, `--apply-staged`'s record and input fingerprint, the WW2 rebuild's lock; the registration and ini rules (`text`) as one implementation; `bfbc2 install` runs it; since round MM1 the mod manager (`mods`: enable / disable / uninstall / restore-original / verify, the migration of the markers from before the ledger), run by `bfbc2 mods` and the hub's Mods tab; since round OR7 the WW2 rebuild as a step graph (`rebuild`, feature `rebuild`: OPFOR's `rebuild_all` steps side by side within a memory budget, the step records and their decisions, `bc2_native.rebuild_run`) |
| `rust/bc2-terragen` | a new terrain from a seed (round TG1, `spec/transforms/terrain_gen.md`): the main height field shaped to one of DICE's five biomes (spectral synthesis, slope matching to the biome's slope distribution, hydraulic erosion, the border, the sea floor, objective pads, the water level), the material map painted from slope and height, the water mesh; round TG2: the sub-terrain ring stitched as DICE's, temperate's escarpment, and the visual side from a donor level (visual terrain, atlas, visual water, colour / occlusion map, the shader databases' cloned entries: `visual::build`); written through `bc2_formats::terrain_codec`; the library the editor's Create Level and sculpt tools call; `bfbc2 terrain generate` runs it |

**The core formats** (round F1a: their Python retired, the module required): `dbx.parse`, `to_xml`, `parse_many` and
`to_xml_many` run in Rust and raise `ValueError` (`DeletedEntry` for an empty file) for a file that is not a binary
`.dbx` or is damaged. A tool that loads many files at once should call `dbx.parse_many` (all cores, and the cycle
collector paused while the trees are built). `bfbc2core.fbrb`: `entries`, `names`, `read` and `read_many` (one
bundle per core; a damaged bundle raises `ValueError`, a missing entry `KeyError`). `bfbc2core.terrain` (`parse`,
`read`, `tail`), `bfbc2core.water` (`parse_mesh`, `parse_visual`, `shaders_of`, `read`), `bfbc2core.collision`
(`index`, `havok_index`, `triangles`, `triangles_many`, `blueprint_triangles`; NavKit's `levelobjects`, `detailed`,
`ladders`, `navsample_offline` and `navwater_mesh` read their collision through it) and `bfbc2core.level` (`load`
through the Rust scene). `BFBC2_PURE_PYTHON` and `use_native` no longer exist for them. The collision index (and so
`level.load`) keeps its mesh-alias cache in `%LOCALAPPDATA%\bfbc2-tool-suite\cache\collision`
(`collision.use_cache`, or the `BFBC2_COLLISION_CACHE` folder; `0` turns it off): it never changes a result, only
skips partitions whose files have the size and time they had when they were read.
**The bundle cache** (round E3e, `bc2_formats::fbrb::cache`): a zipped bundle's payload is one gzip stream, so
reading an entry means decompressing the payload up to it, and a tool reading entries one call at a time from the
same bundle decompressed it again for every call (OPFOR's WW2 level build: 726 reads, 132 s). `fbrb.read` /
`read_many` now keep each bundle's header and the payload decompressed so far in the process
(`BFBC2_FBRB_MEMORY_MB`, default 128; a later read inside it is a copy, one past it carries on), and a large bundle
(from `BFBC2_FBRB_CACHE_MIN_MB` of compressed payload, default 8) read a second time, by this process or any other,
is decompressed once in full into **`%LOCALAPPDATA%\bfbc2-tool-suite\cache\fbrb`** and read with a seek from then on,
by every process. `BFBC2_FBRB_CACHE` names another folder (`0` turns the disk cache off); `BFBC2_FBRB_CACHE_GB` caps
it (default 16; the least recently used payloads are deleted first; delete the folder any time to free the space).
Everything is keyed by the bundle's path, size, modification and creation times, so a changed bundle is read
afresh, and a cache file repeats that key in its own header; files are written beside their name and moved over it,
so processes side by side see whole files only. The results and errors are the reader's without the cache
(`core/tests/test_fbrb_cache.py`; `fbrb.read_uncached` reads without it; `fbrb.cache_stats()` says what it did).
Stored (unzipped) payloads are read where each entry is instead of from the start. `run_tests.py` runs the tests
with the disk cache off (`BFBC2_FBRB_CACHE=0`, unless set): a sweep of the game copy would fill it.
The writers too: `dbx.write` runs in Rust - the retired Python writer's bytes on every tree it wrote the same way;
it takes duck-typed elements (children as any iterable, raw bytes as anything `bytearray.extend` takes, any nesting
depth) and raises for what cannot be written (an empty tag, which reads back as an end mark: the Python wrote it;
more than 15 attributes; a kind outside 0-15; an attribute that is not a pair; a tag, attribute or text that is not a
`str`). `fbrb.pack(folder, out)` / `fbrb.pack_many` pack an unpacked folder as FbrbTool's `pack` does, the same bytes
on every bundle of the game copy (`bfbc2-stockmp-ai` packs through it; FbrbTool.exe is no longer a fallback).
**Unpacking** (round E3j): `fbrb.unpack(bundle, out)` is FbrbTool's `unpack` - the same files, names and
`fbrb.manifest` - in the Rust core (`bc2_formats::fbrb::unpack_path`, entries written on all cores); since round E3k
it is the engine of FbrbTool's command line in process (below), so every bundle is unpacked as FbrbTool unpacks it -
odd names, clashes, extra gzip bytes and damage included, a failure with FbrbTool's error and the files FbrbTool
leaves (`core/tests/test_fbrb_unpack_native.py`: built, odd and damaged bundles and the game copy, against FbrbTool.exe's
recorded unpacks).
`bfbc2-stockmp-ai` unpacks through it.
Outside `core`, `bfbc2-shader-tools/shaderdb.py` (`ShaderDb.parse` and `to_bytes`; a record of a kind only Python
writes, such as a `bytearray`, falls back to the Python writer) works with a Python twin (`use_native`, `backend()`,
`BFBC2_PURE_PYTHON=1`); `bfbc2-culltree/culltreelib.py` (the element reader of `Doc`; `level_partitions`, the
`--ref-dir` search) needs the module since round F1a: a stream that does not read raises `CullTreeError` saying why.

**The shader database in the Rust core** (round E4b, `rust/BENCHMARKS.md` "E4b"): `ShaderDb.parse` keeps the
file's bytes in a `bc2_native.ShaderDbHandle` with the places of its records, and makes a section's Python list
(`params`, `entries`, `vs` ...: the same objects as before) only when the attribute is first read; a pickle or copy
holds plain lists. The logic answers from the handle: `entry_textures`, the new `entry_signatures` (OPFOR's
`wake_shaders.signatures` in one call) and `entry_layouts`, `coverage`, `ShaderDb.mesh_names`, `ShaderDb.check`,
the merge's needed names and the writer read a section from the file while it was never made and from the
caller's list once it was (a list changed in place or replaced is read again: the entries' and groups' lookups are
kept between calls and checked by identity each call); `block_textures`, `block_constants`,
`block_engine_textures` and `texture_kind` read in Rust. Where the plain case does not hold (an item of another
type, a name that is not latin-1, an index past its section) the Python runs: the `*_reference` functions
(`entry_textures_reference`, `check_reference` ...) are the reference, `use_native(False)` / `BFBC2_PURE_PYTHON=1`
turn the handle off (`tests/test_shaderdb_queries.py`). The in-process merge is the Rust core's since round OR3a
(`shaderdb.merger`: `bc2_native.ShaderMerger` - `add_donor`, `clone_entry`, `add_part`, `finish`; the Python `Merger`
retired, `tests/test_shaderdb_merge.py`); `retexture` stays Python for OPFOR's own block edits. Since round SD1 (2026-11-20) the opening of a
database too - `ShaderDb.parse`'s choice of byte order and layout and its messages, `pick_database`, `dx_from_name`
- is one Rust module (`bc2_formats::shaderdb::open`) shared by `shaderdb.py` and `bfbc2.exe shaderdb`, the Python
kept as the reference and the fallback (`tests\sd1_golden_*.json`).
NavKit's walkability rules (`headroom`, `stair_slope`, `narrow_lips`, landings, `near_objects` / `on_objects`),
`build_areas` and `build_graph`'s cell picking (round E2a) run in Rust; their Python was retired in round R1
(2026-11-18, "Without a fallback" below).

`bfbc2-console-convert/audio.py`'s EA-XAS codec runs in Rust (round E3a: `xas_encode_frames`, `xas_decode_frames`,
`encode_xas` and the new `encode_xas_many`, all its sounds encoded at once; its NumPy reference retired 2026-11-18,
see "Without a fallback" below). The new `audio.convert_many` converts many Waves side by side. Since round F4 (2026-11-20)
`audio.convert` / `decode` run in Rust whole (`convert_many` without the interpreter's lock), the Python kept as the
fallback; since round PR1b-1c EA-XMA is decoded in process (`rust/bc2-xma`, FFmpeg's xma2 decoder ported, under
FFmpeg's LGPL), so no ffmpeg is needed.
So does `bfbc2-shader-tools`' shader translator (round E3f): `xenos.parse_shader` decodes in Rust (its tree's microcode attributes made on their first read), `xenos_hlsl.translate` translates in Rust (the program kept from the decode while the tree's microcode is unread, else the tree handed over), and the new `xenos.parse_many` / `xenos_hlsl.translate_many` take many programs at once on all cores; `parse_shader_python` is the decoder's reference (`xenos.use_native`, `xenos.backend()`); the translation has no Python since 2026-11-18.

**Round F4** (2026-11-20, `rust/BENCHMARKS.md` "F4": F3's leftovers, each identical to its Python, which stays the
fallback): `xenos.disasm`'s header lines, `hkpack.Packfile` as a handle (its dicts built only when read;
`read_object` in Rust), `navsample_offline`'s collision hulls (`asset_bodies`), `meshview`'s part reader, `texqa`'s
texture readers (WW2 texture QA 212 s -> 134 s) and the cull tree tool's string table.

Round E3g (`rust/BENCHMARKS.md`, "E3g"): `mesh.convert`'s vertex loop and `havok.convert`'s packfile walk run in
Rust (their Python retired 2026-11-18: what the Rust does not convert raises `ConvertError` saying why); meshport's layout search keeps BFBC2's part signatures,
donor lists and fallback choices (`meshport.REFERENCE` / `BFBC2_MESHPORT_REFERENCE=1`: the old scan); effectport's
template search reads BFBC2's emitter documents' instance types once (`EmitterIndex`, all cores) and keeps them in
`%LOCALAPPDATA%\bfbc2-tool-suite\cache\effectport` while the game copy's bundles are unchanged
(`BFBC2_EFFECTPORT_CACHE`: a folder, or `0`; `effectport.REFERENCE` / `BFBC2_EFFECTPORT_REFERENCE=1`: the old scan);
`meshview.Index.read_many` reads many entries a bundle at a time on all cores.

So does `bfbc2-anim-tools/granny.py` (round E3i): `Granny(b)` reads its tables and `root()` its object tree in
Rust (the same dicts, lists, tuples, numbers and strings, key order included), and `to_pc` converts in Rust (the
same bytes; its Python retired 2026-11-18, a refusal is a `GrannyError` saying why); a Granny written to (`put`,
`to_bytes`, so `shift` and `set_orientations`) reads in Python from then on, and what the Rust reader refuses goes to
the Python reader, which raises what it raises (`granny.use_native`, `granny.backend()`: the reader stays the
format's reference). New: `to_pc_many(items, errors='raise' | 'return')` and `roots(blobs)` (many files on all
cores, the interpreter released); `meshview.Index.read_many(names)` reads each bundle once for many names (a
bundle is one gzip stream: `read` of each name decompresses it from the start again).

Round E3m (`rust/BENCHMARKS.md`, "E3m: the sweep") profiled the tools no round had (`rust\bench_sweep.py`) and ranked
what is left. Changed, by measurement, in Python: the cull tree tool keeps each reference level's rows in
`%LOCALAPPDATA%\bfbc2-tool-suite\cache\culltree` (`BFBC2_CULLTREE_CACHE`: a folder, or `0`;
`BFBC2_CULLTREE_REFERENCE=1`: every reference read, as before) - console_cull's add 3x and 2.5 GB -> 0.2 GB, verify
5.8x; the test harness's opfor.log reader and the crash reader's look at the log search the text instead of every
line (`ingame.sessions_reference`, `crashreader.log_before_crash_reference` are the references).
Round E3n (`rust/BENCHMARKS.md`, "E3n"): the menu tools' APT code. `bfbc2-console-readers/apt/apt.py` decodes in
Rust (`bc2_formats::apt`): a movie's constants, `dump()` and `instructions()` (the same lines and records - types,
order, values - on every movie of the game copy, the pristine menus and the Xbox HUD copies; the dump's text is kept
in Rust until `dump_lines` is read); a movie the Rust core refuses (a read outside the file, deep nesting) and a
Movie with a `sink` decode in Python, so the answer is always the reference's (`apt.use_native`, `apt.backend()`,
`dump_python` / `instructions_python`). New: `apt.decoded(movies)` decodes many movies at once on all cores (aptrefs'
scan and `aptcheck check` with several movies use it). aptgraft's writer places each code node once (its size and
its bytes from one placement), assembles from an opcode table and, in `roundtrip`, shares one plan per layout (the
reachable nodes, offsets and node bytes) among the writes of the unchanged model; its parser decodes from the same
table (`aptgraft.REFERENCE` / `BFBC2_APTGRAFT_REFERENCE=1`: the writer and parser as before, byte-identical on every
movie; `Movie.plan` / `write(plan=...)` for callers that write one unchanged model several times).
Round E4c (`rust/BENCHMARKS.md`, "E4c"): aptgraft's movie model in Rust (`bc2_formats::aptmodel`, bound as
`bc2_native.AptModel`). With the Rust core `aptgraft.Movie(...)` is a `NativeMovie`: the parse, the layouts, every
write (either byte order, `keep` / `pack`, dead data), `reachable()`, `leftovers()` and `roundtrip` run in Rust, and no
Python object is made for the data until a caller reaches it - then a node is a shell of its class, filled from Rust on
its first read or write and a plain `Words` / `Code` / ... from then on; the filled and new nodes are sent back before
each write. Callers (the grafting code, OPFOR's menu tools) use the same API unchanged. Wherever the Rust refuses (a
movie the parse would raise on, a value a write cannot pack, a read that parses more of the file) the movie turns into
the Python model in place and the Python code answers, so results and errors are the reference's (`aptgraft.use_native`,
`aptgraft.backend()`, `BFBC2_PURE_PYTHON=1`; `REFERENCE` / `_py_movie` / `roundtrip_python` are the Python).
`aptgraft roundtrip` runs many movies at once on all cores (`roundtrip_many`). aptcheck's rules (`check_many`, used
by `check`) and aptrefs' scan (`refs_many`) run in Rust over the decoder's records, many movies on all cores
(`bc2_formats::aptscan`; `check_many_python` / `refs_many_python` are the references).


Round E3o (`rust/BENCHMARKS.md`, "E3o") took E3m's next items. NavKit's `kymesh.parse_mesh` reads the runtime mesh
in Rust (`bc2_formats::kymesh`; a record the Python would raise on, or one holding a NaN, is read by
`parse_mesh_reference`, so the error stays the Python's; `kymesh.use_native`: a format reader, kept);
`bfbc2core.navdata.load` takes a whole file's areas and cells from the core as flat arrays and its route-graph
union-finds step for step - an uncached load 5.9x faster; `navpath.route` searches in Rust (`PathGraph`: the same
route, cost and node count expanded; its estimate the fused `np.linalg.norm` of this PC's NumPy, now on every PC)
and `closest_between` is kept per island pair (navdata's and navpath's Python twins retired 2026-11-18, round R1:
an unreadable mesh, a value that is not finite or a graph the core cannot search raises `ValueError`); the console
converter's `havok.verify` answers from the Rust walk's record of each byte's owner (`havok.verify_packfile`; its
Python reference retired 2026-11-18); `texqa`
analyses textures in worker processes (`--workers N`, `BFBC2_TEXQA_WORKERS`; the same rows, report and cache as
`--workers 1`).
Round E3d's two targets stayed in Python, by measurement (`rust/BENCHMARKS.md`, "E3d"): `pcbuild.compile_hlsl`
keeps what it compiled (in memory and in `%LOCALAPPDATA%\bfbc2-tool-suite\cache\hlsl`, keyed by the compiler DLL's
hash; `BFBC2_HLSL_CACHE` another folder or `0`) and `compile_many` / `prefetch` compile on all cores - ctypes
already releases the interpreter around Microsoft's compiler, so Rust could not make it faster; dbxport's template
ranking runs from an index (`_Ranking`, with the full scan `_best` as its reference, `BFBC2_DBXPORT_REFERENCE=1`).
Round E3r (`rust/BENCHMARKS.md`, "E3r"): the rest of the BF1943 importer stayed Python, by measurement - its time
went to reading and to algorithms, not to rules a Rust port would speed up. `bf1943import.Source` no longer holds
BF1943's bundles whole (~1.2 GB): it keeps their entries (checked once per bundle with `fbrb1943` and kept in the
`bf1943source` cache, so an open takes 0.05 s instead of 3 s) and reads an entry from a compressed payload through
restart points (the inflater's state every 1 MiB, made on a bundle's first read) and the decoded spans of the last
reads (`BF1943_SOURCE_MEMORY_MB`, default 128); `bf1943import.WholeSource` (`BF1943_SOURCE_WHOLE=1`) is the reference.
`emitteratlas.dedupe` makes the scan's choices with array operations (`dedupe_reference`,
`BFBC2_EMITTERATLAS_REFERENCE=1`); `bf1943import.check` compares trees without copying them (`same_clean`);
`Importer` reads each game resource once; `dbxport.guids_of` remembers each partition's guids;
`meshview.Index.read_many(names, whole_bundles=False)` reads only the partitions named, the bundles on all cores.
Round E4d (`rust/BENCHMARKS.md`, "E4d") ported what E3r left: `dbxport.port` runs in the Rust core
(`bc2_convert::dbxport`) - the same signature and `Report`, its callers unchanged. `dbxport.Templates` parses its
partitions on the Rust side and builds the Python tables (`candidates`, `by_guid`, ...) only when something asks for
them; the catalogue's tables are copied there once per catalogue; `guids_of` reads guids without a Python tree.
`dbxport.port_python` is the reference and the fallback: it ports when the Rust core is missing, when
`BFBC2_DBXPORT_PYTHON=1` (or `dbxport.NATIVE_PORT = False`), when a caller changed one of the rules the Rust port has
built in (converters, guards, patterns, constants, functions; its data tables - `ENUM_NUMBERS` and the rest - it reads
from the caller's process on every port, as Project OPFOR changes them), and for a partition the Rust side says it does not reproduce (`dbxport.STATS` counts them; none on the
real corpora). `BFBC2_DBXPORT_DUAL=1` runs both on every port and raises `DualMismatch` on any difference;
`BFBC2_DBXPORT_STATS=1` prints the counts at exit.
Round F2b (`rust/BENCHMARKS.md`, "F2b") ported the BF1943 importer's remaining Python and three small modules, each
with its Python kept as the reference and the fallback (`mopp`, `havokport`, `PEImage`, `xmlinst` and `navtune`'s
were retired in round RT2 against their recorded answers): `mopp.py` (the MOPP queries, completeness and ray casts),
`havokport.py` (the 4.6.1 -> 5.5.0 restructure, the material table's swap, the shape subparts and their w lanes, the
physics materials' readers and writers), `emitteratlas.py` (`build`, `prove`, `same_but_uv`: the same atlases,
partitions and proof rows, NumPy's float decisions taken on exact integer statistics, a decision too close to call
left to the Python), `dbxcatalogue.build` (bundles read and partitions parsed on all cores, every table in the
reference's order: 173 -> 24 s on the game copy), meshport's layout search; `menucommon.PEImage`'s header, strings and
searches; `bfbc2core.xmlinst` (NavKit's instance scans, 40 -> 1.8 s over OPFOR's 791 level XML files) and
`bfbc2core.navtune`'s validation, side-file text and summaries. The importer's switch is
`bfbc2-bf1943-import/importcore.py` (`BFBC2_IMPORTER_PYTHON=1` runs the Python; `BFBC2_PURE_PYTHON=1` everything's).
Round E3l (`rust/BENCHMARKS.md`, "E3l"): the shader tools' remaining Python stayed Python, by measurement - the
render harness and the differential test waited on `shaderview.exe` (a Direct3D device created per job, ~55 ms on
WARP and ~340 ms on the GPU, and a virus scan of every input file it opened). `shaderview -serve` keeps one device
for many jobs (`renderqa.run_job`, thread-safe; `BFBC2_SHADERVIEW_SERVE=0` runs one process per job, the reference),
`pack` puts a job's inputs in one file, and `xdiff.py` runs its shaders on worker threads: every Wake pixel shader in
21 s instead of 168 s, the same verdicts and files; `xenos_exec.py`'s interpreter is 4.5 % of what is left.

**DbxEdit in process** (round E3b, 2026-11-17). `bfbc2core.dbxedit` runs DbxEdit's command lines through the Rust
core instead of starting `DbxEdit.exe` (a .NET start and a fresh parse for every call): `dbxedit.run(args)` and
`run_many(list of args)` (all cores) return a `subprocess.CompletedProcess` with DbxEdit's exit code and output;
`edit`, `import_xml`, `xml`, `check`, `roundtrip` and `fixslots` name the commands; `run_cmd(cmd, **kw)` takes a
command line whose program is DbxEdit (`'DbxEdit'`, or `DbxEdit.exe` by any path) and `subprocess.run`'s keyword
arguments, so a caller switches with one line; `python bfbc2-dbx-tools\dbxedit.py <command> ...` is the command line
by hand. Writes go through `bc2-formats`' `atomic::replace` (a moment's lock on the target is waited out).
Each run is logged in DbxEdit's own log (`dbxedit-<date>.log`, "started (in process)"). **DbxEdit.exe was retired**
(round R4, 2026-11-18, after the full tier): the Rust core is required (`NativeMissing` says how to get it) and
`BFBC2_DBXEDIT_EXE` is gone; `core/tests/test_dbxedit.py` checks the commands against the exe's recorded answers
(`core/tests/dbxedit_golden.json`: DbxEdit's unit-test cases; `dbxedit_golden_game_0.json / _1.json`: a 1,592-file corpus
sample through every command, 12,464 runs) and DbxEdit's edit fingerprints. DbxEdit.cs stayed as the DBX classes
of DbxEditGUI until round GR4 retired both (the window is the map editor's DBX inspector). The suite's NavKit unpack and
`bfbc2-stockmp-ai`'s validation use it.
**FbrbTool in process** (round E3k, 2026-11-17). `bfbc2core.fbrbtool` does the same for `FbrbTool.exe`'s `unpack`,
`pack`, `list` and `big` (`rust/bc2-formats/src/fbrb/tool.rs`): `fbrbtool.run(args)`, `run_many`, `unpack`, `pack`,
`list_entries`, `big`, and `run_cmd(cmd, **kw)` as a drop-in for `subprocess.run(['FbrbTool', ...], ...)`
(`capture_output` defaults to False as `subprocess.run`'s does; `stdout=` / `stderr=` PIPE, DEVNULL, STDOUT or a
file are honoured; with `cwd=` the command's relative paths are taken from that folder);
`python bfbc2-fbrb-tool\fbrbtool.py <command> ...` is the command line by hand. The same files byte for byte, the same messages and exit codes,
the same failures on damaged bundles: a bundle's gzip streams are read by .NET's own rules (`fbrb::netgzip`, a port of
.NET Framework's managed inflater and `GZipStream.CopyTo`; zlib reads every stream it takes whole, the port decides
the rest), entry paths are normalised as .NET's `GetFullPath` does. A zipped payload is decompressed as the file is
read and a stored one is never loaded (entries are read where they are). Entries, the manifest and packed bundles are
written beside their name and moved over it (`atomic::replace`). Logged in `fbrbtool-<date>.log` ("started (in
process)"). **FbrbTool.exe was retired** with DbxEdit.exe (FbrbTool.cs deleted; `BFBC2_FBRBTOOL_EXE` gone; the Rust
core required): `core/tests/test_fbrbtool.py` checks against the exe's recorded answers (`fbrbtool_golden.json`:
228 command lines, 600 damaged bundles, .NET's gzip reader on 3,000 streams; `fbrbtool_golden_game.json`: every
bundle of the game copy up to 300 MiB unpacked, listed and packed again). NavKit's unpack, `level.heightfield_of`'s
terrain unpack, the hub's Bundles tab and `bfbc2-stockmp-ai` use it.
**The command lines in Rust** (rounds O1 - O5, 2026-11-18 / 20, crate `rust/bc2-cli`, binary `bin\bfbc2.exe` since
round D1): `bfbc2 fbrbtool <command> ...` and `bfbc2 dbxedit <command> ...` were the first (O1), `fbrbtool.py` /
`dbxedit.py` without Python: the same arguments, files, exit codes, log lines and printed bytes (stdout and stderr in
the code page and order Python's stdio gives them, alone or on one pipe), in about 20 ms instead of about 300 ms per
call. Every subcommand since, and how to use them: "bfbc2.exe" below. `core/tests/test_cli_o1.py` checks it against
the Python command lines' recorded answers (`cli_o1_golden.json`; `--live` both ways, `--game` OPFOR's real calls);
`rust/BENCHMARKS.md`, "O1".
**The stock-MP AI layer** (round E4a, 2026-11-18, crate `rust/bc2-stockmp`). `bfbc2-stockmp-ai`'s logic runs in Rust
with its data kept there: `bc2_formats::dbx::edit::EditDbx` is the port of `dbxbin.py` (a level partition as an arena
of elements named by handles, edited and written back byte for byte, the string table kept until a string is added);
on it, `levelmodel.Model` (`levelmodel.model_of`), `author.Author` (`author.author_native`: the same edits in the same
order, so the same bytes), and the readings behind `validate.py`'s structure, changes, external-reference and stage
checks, its partition index and its packed-bundle comparison (on all cores); `dbxbin.round_trips` checks many files
on all cores. No element tree is built in Python on these paths (a level partition is about 400,000 elements). Each
Rust function returns `None` where the port leaves the input to the Python (whatever the Python raises - every
`AuthorError`, a `KeyError` - or formats itself, such as a Python float as text), so results and error messages stay
the Python's. Python keeps every piece as the reference and the fallback: `BFBC2_STOCKMP_PYTHON=1` (or
`BFBC2_PURE_PYTHON=1`, `stockcore.use_native(False)`) runs it. Validation also runs DbxEdit's read-only checks and
round trips of every variant at once (`dbxedit.run_many`). Proof: `bfbc2-stockmp-ai/tests/test_native_levels.py`
(game: all 52 multiplayer level partitions of the game copy, model, round trip, 30 authored, validation readings);
whole work folders identical old against new (`rust/BENCHMARKS.md`, "E4a").
**NavKit's tools around the build** (round E4e, 2026-11-18, `rust/BENCHMARKS.md` "E4e"; `bc2-navbuild`'s `vehicle`,
`domains`, `levelscan`, `measures`). What the driver and its tools spent their time in, in Rust with the data kept
there: `navvehicle`'s obstacle distance (85 of ONS_MP_002's 90 s), `navdomains`' searches (`Domain.graph()`, a
`bc2_native.DomainGraph` per domain; `World.tables` on all cores), `levelobjects`' reading of the level's and the common
bundles' converted blueprints for their pathfinding flags (`levelobjects.scan`: one walk, every XML file read once on
all cores, where `blueprint_flags` read them one at a time - `navkit.py prepare`; the havok index and mesh aliases are
`bfbc2core.collision`'s since round F1a), and the DICE-parity measures behind
`navregress` / `navparity` (`navdiff.diff`'s per-sector comparison, `navdf`'s D lines and F cells,
`ordercheck.PathData`'s route groups and `on_area` - also the harness's and the stock-MP AI's): the meshes parsed,
sampled and compared in Rust (`bc2_native.navkit_diff`, `navkit_df`, `OrderAreas`), and each path-data file parsed once
for all of a level's measures (`hpdtool.read` inside `hpdtool.shared_reads()`); and the offline sampler's terrain test
(`navsample_offline.terrain_blocks`, `offline`). DbxEdit's `xml` command converts a folder's files on all cores (the
same files, messages and counts in order): with the scan, `navkit.py prepare` of a level went from 7-9 minutes to
under one. Each
piece keeps its Python as the reference and the fallback this round (`bfbc2-auto-navkit/tools/navnative.py`:
`BFBC2_NAVTOOLS_PYTHON=1` for these pieces only, `BFBC2_PURE_PYTHON=1`, `navnative.use_native(False)`); where the core
refuses an input (a value `round` cannot take, a mesh it does not read) the Python runs and raises what it raises.
Proof: `bfbc2-auto-navkit/tests/test_navtools_native.py` (quick: every piece against its Python on random inputs;
`--game`: the tools' outputs on the game copy against `navtools_golden.json`, recorded from the Python before the port).
**Without a fallback** (round E1d, Python retired 2026-11-17): `texture.convert` / `convert_many` and NavKit's grid
walks (`navbuild.Level`'s neighbour linking, `components`, `trace_edges`' side neighbours, `line_walkable`,
`build_graph`'s route-group search; `navwalk.walkable`'s flood, `islands_of`, `matched_flags`) always run in Rust:
`BFBC2_PURE_PYTHON` and `use_native` do not turn them off, and the tools call `texture.require()` /
`navbuild.require_native()` before they start work. Their tests check them against results recorded from the
Python before it was deleted (`bfbc2-console-convert/tests/texture_golden.json`,
`bfbc2-auto-navkit/tests/navbuild_golden.json`); NavKit's regression (`navregress`) stays the independent check
against DICE's path data. Likewise (2026-11-18) `xenos_hlsl.translate` / `translate_many`: every tree is translated
in Rust - `xenos.parse_shader`'s program kept in Rust, any other tree (the Python decoder's, one whose microcode a
caller read or edited, copies, trees built by hand) handed over attribute by attribute (`xenos_tree_program`); what
it cannot take raises `ValueError` saying what. Its tests check the results recorded from `translate_python` before
it was deleted (`bfbc2-shader-tools/tests/xenos_golden.json`, `xenos_golden_game.json`); `xdiff.py` (the
interpreter) stays the independent check. The decoder `xenos.py` is a format reader and keeps its Python.
Since 2026-11-18 (`rust/BENCHMARKS.md`, "Retired Python"), likewise: `audio.py`'s EA-XAS
encoder and decoder, `mesh.convert`'s vertex loop, `havok.convert_packfile` / `verify_packfile` and `granny.to_pc` /
`to_pc_many` (`audio.require()`, `mesh.require()`, `havok.require()`, `granny.require()`). An input the NumPy or
Python code took and the Rust does not (none on the game's files) is now an error saying why: samples beyond int16
or not integers, a vertex element past its vertex or the data, a packfile that reads past its data. Their tests
check the recordings `bfbc2-console-convert/tests/audio_golden.json`, `bfbc2-bf1943-import/tests/ports_golden.json`,
`tests/havok_verify_golden.json` and `bfbc2-anim-tools/tests/granny_golden.json` (each with a "now" section: the
cases answered otherwise on purpose).
And (round R1, 2026-11-18) the rest of NavKit's generator (`navbuild.py`, `navwalk.py`: the walkability rules, the
areas, the tracing, the route graphs, the floods), `navdiff.area_cells`, `bfbc2core.navdata.load` and
`navpath.route`: no `use_native`, `BFBC2_PURE_PYTHON` does not turn them off; an unreadable mesh, a value that is
not finite or a route graph the core cannot search raises `ValueError`. Their tests check the answers recorded from
the Python (`navbuild_golden.json`, `tests/navdata_golden.json`).

**Develop:** `python rust\build_native.py --cargo test` (unit, property and doc tests; the whole-level tests marked
`#[ignore = "game tier: ..."]` need `-- --ignored`), `--cargo clippy --all-targets -- -D warnings`, `--cargo fmt`;
`python tests\test_rust.py` runs fmt, clippy, the build and the quick tests in turn, `tests\test_rust.py run --game`
the whole-level ones, `tests\test_rust.py list` names every test with the tier that runs it.

**A compile cache** (round TS1, off by default). With [sccache](https://github.com/mozilla/sccache) installed (`cargo
install sccache --locked --no-default-features`, about 3 minutes) and `BFBC2_COMPILE_CACHE=1`, every cargo the suite
runs goes through it (`bfbc2core.native.cargo_env`: `RUSTC_WRAPPER`, its folder
`%LOCALAPPDATA%\bfbc2-tool-suite\sccache`, capped at `BFBC2_COMPILE_CACHE_GB`, default 6, and `SCCACHE_BASEDIRS` set to
the build folders); `run_tests.py` starts its server before any test, so no test's job object ends it. It does not
share dependencies between worktrees' build folders: cargo gives every crate with a build script an `OUT_DIR` inside
its build folder, sccache 0.19 keeps that path in the key, and every crate built on such a crate misses in turn (2
of 208 compiles hit on a second folder; `rust/BENCHMARKS.md` TS1). It serves a rebuild of the same folder. One
`CARGO_TARGET_DIR` for several worktrees is not the answer either: cargo's lock keeps it consistent, but the release
binaries (`bfbc2.exe` ...) have one path per folder, so one worktree's build could be copied into another's `bin\`.
Each checkout keeps its own build folder.

**Where Rust builds** (round D1, 2026-11-19). cargo's build folder is not `rust\target` but
`C:\Programming\Projects\rust-targets\<checkout folder name>` (`rust-targets\bfbc2-tool-suite` for the main checkout,
`rust-targets\bfbc2-tool-suite-e2b` for a worktree): one folder per checkout, beside the checkouts, so worktrees never
share a cargo lock and none hides a build inside its own folder. `bfbc2core.native.target_dir()` says where:
`CARGO_TARGET_DIR` when it is set, else `BFBC2_RUST_TARGETS` (another root) or `rust-targets`, then the checkout's
folder name. Every suite tool that runs cargo sets it (`build_native.py`, `tests\test_rust.py`, `tests\test_editor.py`,
`run_tests.py`); a bare `cargo` typed in `rust\` would still use `rust\target`, so run cargo through
`python rust\build_native.py --cargo <arguments>` (or set `CARGO_TARGET_DIR` to `python rust\build_native.py
--target-dir`). The binaries the tools run are copied to the suite's `bin\` (gitignored, built outputs only):
`bin\bfbc2.exe` (every build), `bin\bfbc2-editor.exe` and `bin\mapexport.exe` (`--editor`); the tools look there first
(`native.find_binary`), then in the build folder.

Removing a worktree: `git worktree remove <folder>`, then `python -m bfbc2core.caches --prune-targets` (from `core\`)
lists every folder of the build root whose checkout no longer exists beside it, and `--prune-targets --yes` deletes
them. `--report` shows each checkout's build folder with its size.

Moving an existing `rust\target` (a checkout built before round D1) when nothing is building:
`move <checkout>\rust\target C:\Programming\Projects\rust-targets\<checkout folder name>` (a rename on the same
drive, instant; cargo goes on using what it built, a few build-script crates may build again). Or delete it:
everything is built again on the next build. Coverage: `cargo llvm-cov -p bc2-formats -p bc2-scene -p bc2-convert -p bc2-navbuild` (after
`cargo install cargo-llvm-cov` and `rustup component add llvm-tools-preview`). Timings: `rust\BENCHMARKS.md`.

## bfbc2.exe

`bin\bfbc2.exe` (crate `rust/bc2-cli`, rounds O1 - O5, 2026-11-18 to 11-20) runs the tools' command lines in Rust:
`bfbc2 <tool> <arguments>` takes the same arguments and gives the same printed output, files, log lines and exit
codes as `python <the tool's .py> <arguments>`, without starting Python (tens of milliseconds instead of hundreds
per call; `rust/BENCHMARKS.md`, "O1" - "O5"). `python rust\build_native.py --download` fetches it with the module
(since round RT1); `python rust\build_native.py` builds both. `bfbc2 --version` prints the suite's version,
`bfbc2 --source-hash` the Rust sources it was built from; `bfbc2` alone prints the list below.

| Subcommand | The Python command line it ports | Still runs in Python (the binary hands the line to it) |
|---|---|---|
| `fbrbtool`, `dbxedit` | `bfbc2-fbrb-tool\fbrbtool.py`, `bfbc2-dbx-tools\dbxedit.py` | nothing |
| `aptcheck`, `bc2loc`, `aptgraft` | `bfbc2-menu-tools\<tool>.py` | damaged movies |
| `apt`, `stfs` | `bfbc2-console-readers\<tool>\<tool>.py` | damaged files |
| `exeref` | `bfbc2-automated-map-and-bot-test-harness\crash-reader\exeref.py` | another exe's code the Rust decoder does not take (capstone answers) |
| `convert`, `audio`, `texqa` | `bfbc2-console-convert\<tool>.py` | damaged input; texqa's contact sheets (PIL) |
| `aimanim` | `bfbc2-anim-tools\aimanim.py` | damaged files |
| `hpdtool`, `hkpack`, `levelobjects`, `ladders`, `ordercheck`, `navcompare`, `hpdcheck`, `detailed` | `bfbc2-auto-navkit\tools\<tool>.py` | refused inputs; `detailed --score` |
| `navkit` | `bfbc2-auto-navkit\navkit.py` | `build`, `apply`, `all` and `sample` (`prepare`, `check`, `compare`, `status` are Rust) |
| `navparity`, `navregress`, `hpdview`, `sample_all` | `bfbc2-auto-navkit\tools\<tool>.py` | their real work (the regression builds, matplotlib drawings, starting the game); Rust checks the arguments |
| `graphstats`, `prepare_level` | `graphstats.py`, `rust\bc2-editor\prepare_level.py` (both **retired**, round RT1) | nothing: a line the port does not read ends with an error naming the retirement (exit code 1) |
| `dbxparity`, `portparity` | `bfbc2-bf1943-import\dbxparity.py`, `portparity.py` (both **retired**, round PR1b-1d; the library is `dbxparitylib.py`) | nothing: their runs are Rust (`bc2_convert::meshport::Porter`, `dbxport`, the catalogue built in process), their reports the Python's byte for byte |
| `aptrefs`, `xiso`, `culltree`, `stockmp`, `crashreader` | `bfbc2-menu-tools\aptrefs.py`, `bfbc2-console-readers\xiso\xiso.py`, `bfbc2-culltree\culltree.py`, `bfbc2-stockmp-ai\stockmp.py`, `bfbc2-automated-map-and-bot-test-harness\crash-reader\crashreader.py` (all **retired**, round RT3; their libraries are `<tool>lib.py` beside them) | nothing: a damaged or unusual input (a report that is not XML, a cut disc root, a hand-written configuration shape the port leaves, another exe's code) ends with the error naming the retirement (exit code 1) |
| `harness` | `bfbc2-automated-map-and-bot-test-harness\harness.py` | `run` (NavKit builds, NumPy drawings) |
| `caches` | `core\bfbc2core\caches.py` (`python -m bfbc2core.caches`) | nothing on real use |
| `meshview`, `dbxcatalogue`, `bf1943import` | `bfbc2-mesh-viewer\meshview.py`, `bfbc2-bf1943-import\<tool>.py` | the drawings, the catalogue's cache build, `bf1943import import` / `check` (Rust answers help, `list`, `find` and a current cache) |
| `mods` | none: new in round MM1 - Project OPFOR's mod manager on the install ledger (`list`, `status`, `enable`, `disable`, `install`, `uninstall`, `restore-original`, `verify`, `baseline`, `migrate`, `resume`; `bfbc2 mods` alone prints its usage; spec `transforms/install.md` section 5) | `install onslaught:<level>` runs OPFOR's `console_port.py` |
| `terrain` | none: new in round TG1 - `bfbc2 terrain generate --biome <b> --size 1024\|2048 --seed <n> --out <folder>` writes a generated terrain's main height field, ring (round TG2; `--no-ring`), material map and water mesh and a `terrain.json` with the seed, settings, generator version and its statistics against the biome's DICE medians; `--donor <level>` (round TG2) also its visual side from that DICE level (visual terrain, atlas, visual water, `level\` colour / occlusion map and Dx9 / 10 / 11 databases), the water at the donor's level unless `--water-level`; `--rough` the escarpment | nothing |
| `install` | Project OPFOR's `wake_install.py` / `ww2_level_install.py` install side (round OR1: OPFOR's build writes the install plan, `bfbc2 install plan / apply / revert / marks` does the rest; `crate bc2-install`) | nothing: OPFOR's Python builds the staging and calls it |
| `shaderdb`, `renderqa`, `xdiff` | `bfbc2-shader-tools\<tool>.py` | renderqa's and xdiff's drawing (Direct3D 11, NumPy, Pillow; Rust checks the arguments); `shaderdb` is Rust end to end, `merge` included |

The `--help` screens and argument errors of `caches`, `convert`, `audio`, `shaderdb`, `renderqa`, `xdiff`,
`dbxparity`, `portparity` (and `texqa --help`) are laid out at the console's width (or `COLUMNS`) by `bfbc2.exe`
itself, as argparse does (round RT2). The full inventory: `rust/BENCHMARKS.md`, "RT1" and "RT2".

**Which one runs.** The suite's tools, the hub and Project OPFOR's scripts ask `bfbc2core.native.cli_command(tool)`:
`[bin\bfbc2.exe, tool]` while the binary was built from this checkout's Rust sources (its `--source-hash` is the
checkout's fingerprint), else `[python, the tool's .py]`. `BFBC2_CLI_PYTHON=1` asks for the Python every time, except
for the retired command lines (`bfbc2core.native.RETIRED_CLI`: `graphstats`, `prepare_level`; since round RT3
`aptrefs`, `xiso`, `culltree`, `stockmp`, `crashreader`; `leveldiff`, which never had one), which have only the
binary: without it `cli_command` raises `CliMissing` saying to run `python rust\build_native.py --download` or build
it. The `.py` files of the other tools stay: most are also libraries other modules import.

**When the Rust port cannot answer** (round RT3, audit AU2 A17), one rule: while a tool's Python is alive, the
Python answers - `bfbc2.exe` hands the whole command line to it (the same output and exit code), and a module's
binding gives way to its Python reference; once the Python is retired, a plain error and never a silent
fallback - `bfbc2.exe` ends the line with the error naming the round that retired it (exit code 1, `BFBC2_CLI_TRACE`
records `retired`), a binding's function raises `bfbc2core.native.Refused`. `rust\bc2-cli\src\pyproc.rs`
(`RETIRED`) and `bfbc2core.native.RETIRED_ROUND` list the retired command lines; `tests\test_native_required.py`
checks they agree.

## The application icon

The suite's programs show one icon: `bfbc2.exe`, `bfbc2-editor.exe` and `mapexport.exe` carry it as a Windows
resource (what Explorer, the taskbar and shortcuts show), the map editor's window shows it in its title bar, the
taskbar and Alt+Tab, and so does the hub's window (`BFBC2 Tool Suite.pyw`). The icon is an M67 grenade, the author's
own render (`design\icons\source\m67_render.png`; `design\icons\README.md` has the files and how they are made).

What it is: one multi-size Windows `.ico` (256, 128, 64, 48, 32, 24 and 16 pixels; the editor's window takes the
largest PNG-compressed picture in it, the form current tools write). Where it is looked for, in order:

1. the environment variable `BFBC2_APP_ICON` - the full path of an `.ico`;
2. `design\icons\local\app.ico` - a personal override. `design\icons\local\` is gitignored: git never sees it;
3. `design\icons\app.ico` - the committed icon;
4. none - everything builds and runs without an icon.

The Rust binaries take the icon when they are built (`rust\app_icon.rs`, run by the build scripts of `bc2-cli`,
`bc2-editor` and `bc2-levelwrite`; it needs `rc.exe` of the Windows SDK, which the Visual Studio C++ tools bring).
Cargo builds them again when `BFBC2_APP_ICON` or anything under `design\icons` changes, so after adding, changing
or removing the icon, `python rust\build_native.py` and `python rust\build_native.py --editor` copy binaries with the
new icon to `bin\`. Without an icon, or without `rc.exe` (a build warning), the binaries have none. The editor and
the hub read the file when they start.

## Caches

The tools keep what is slow to make again on disk. Every cache is only a cache: a file that is missing, damaged or
from other inputs (other code, another compiler, a changed bundle) is made again, never an error, so any of them can
be deleted at any time, also while tools run. Round E3p (2026-11-17) gave the shared ones under
`%LOCALAPPDATA%\bfbc2-tool-suite\cache` a size cap each, the least recently used files deleted first, and one
command to see and clear them (`bin\bfbc2.exe caches ...` takes the same arguments without Python, round O3):

```
cd core
python -m bfbc2core.caches                    each cache: size, files, cap and folder (also --list [name...])
python -m bfbc2core.caches --prune [name...]  delete the least recently used files beyond each cap now
                                              (every cache, project cache and the logs when no name is given)
python -m bfbc2core.caches --clear [name...]  delete the cached files (every shared cache when no name is given;
                                              the project caches navstudio, harness, navwater, editor and logs by
                                              name only)
python -m bfbc2core.caches --opfor <folder>   Project OPFOR BFBC2's folder for the project caches (default: as the
                                              tools find it - OPFOR_ROOT, else walking up from the suite)
python -m bfbc2core.caches --report           every cache with its size and cap, the overall budget, the logs, every
                                              checkout's Rust build folder, Project OPFOR's work\ by kind (input,
                                              output, cache, scratch, backup; --no-work leaves that scan out)
python -m bfbc2core.caches --prune-targets [--yes]   the Rust build folders whose checkout is gone (deleted with --yes)
```

**One budget for all of them** (round D1, 2026-11-19): besides each cache's own cap, every cache together - the
shared ones (the fbrb payloads included) and the project caches - stays under `BFBC2_CACHE_TOTAL_GB` (default 20
GiB), the least recently used entries of any cache deleted first (`caches.prune_total`). It runs when tools write,
as the per-cache caps do (after a cache's own prune, and after `fbrb.read`), at most every 15 minutes across
processes; `--prune` without names prunes to it too. Nothing is deleted at import time. The logs keep their own caps.

(`python core\bfbc2core\caches.py ...` works from anywhere.) A cap is checked by the code that writes the cache, at
most every 15 minutes per folder (`bfbc2core.caches.maybe_prune`; fbrb: the Rust core after each payload it writes),
and a hit marks its file used (its modification time), so the clock the pruning goes by is the last use. The
pruning never deletes a writer's temporary file (`*.tmp`, `*.tmp-*`) younger than an hour, an entry another process
has open without delete sharing, or one rewritten or used since it looked; it runs under a lock between processes
(the folder's `.prune.lock`), and deletes only the cache's own kind of file (by suffix), so a cache variable pointed
at the wrong folder costs nothing else.

**Shared caches** (`%LOCALAPPDATA%\bfbc2-tool-suite\cache\<name>`; the command manages these; sizes measured on the
development PC on 2026-11-17, mid-way through a WW2 rebuild):

| Name | Files | Key (a change makes a new entry) | Written by | Folder variable (`0`: off) | Cap, eviction | Typical size |
|---|---|---|---|---|---|---|
| `hlsl` | `<compiler>\<2 hex>\<sha256>.bin`, a folder per `d3dcompiler_47.dll` | SHA-256 of the source, entry, profile, defines, name, flags and the compiler DLL's hash | `pcbuild.compile_hlsl` / `compile_many` (E3d) | `BFBC2_HLSL_CACHE` (none without `%LOCALAPPDATA%`) | `BFBC2_HLSL_CACHE_MB`, default 1024; LRU (E3p; before: never evicted) | 28 MB, 2,370 files |
| `fbrb` | `<bundle id>.payload`, `.seen` markers | the bundle's canonical path, size, modification and creation times | `fbrb.read` / `read_many` with the Rust core (E3e) | `BFBC2_FBRB_CACHE` (`BFBC2_FBRB_CACHE_MIN_MB` 8: smaller bundles never go to disk) | `BFBC2_FBRB_CACHE_GB`, default 16; LRU by the Rust core after each write; markers after 30 days | 12 GB, 117 payloads |
| `collision` | `aliases-<hash of the data folder>.bin` | one file per data folder; each XML file's and `.dbx`'s size and modification time inside | `collision.index`, `level.load` (Rust index, E1e) | `BFBC2_COLLISION_CACHE` | `BFBC2_COLLISION_CACHE_MB`, default 256; LRU (E3p; a reuse now marks the file) | 24 MB, 81 files (temp folders of test runs included) |
| `effectport` | `emitters-<24 hex>.pkl` | the emitter documents' places and their bundles' paths, sizes and times, `INDEX_VERSION` | `effectport.EmitterIndex` (E3g) | `BFBC2_EFFECTPORT_CACHE` | `BFBC2_EFFECTPORT_CACHE_MB`, default 256; LRU (E3p) | 3.8 MB, 1 file per game copy |
| `culltree` | `<sha1 of the level file's path>.pkl` | the code (`CACHE_VERSION`, a hash of `culltreelib.py`), the level file's size and time; each tree's inside | `culltree` `ref_rows` (E3m); `bfbc2.exe culltree` shares it (round CT1: either side's warmed cache serves both) | `BFBC2_CULLTREE_CACHE` | `BFBC2_CULLTREE_CACHE_MB`, default 2048; LRU (E3p) | one file per reference level partition |
| `bf1943source` | `<sha1 of the source folder>.pkl` | `INDEX_VERSION`, the BF1943 bundle reader's own source (`fbrb1943.py`'s SHA-1); per bundle its size and modification time | `bf1943import.Source` (E3r): which bundles `fbrb1943` takes, their entries and where each payload starts | `BF1943_SOURCE_INDEX` | `BF1943_SOURCE_INDEX_MB`, default 64; LRU | 3.2 MB, one file per BF1943 copy |
| `shadercover` | `<24 hex>.bin` per level bundle and Dx (`.pkl`: the retired Python index's, pruned) | the file named by the bundle's path and the Dx; its key the bundle's content key (length, head, gzip trailer), the Dx and `COVER_VERSION` (3) | `shaderdb.level_cover_prefixes` / `level_coverage` / `donor_levels` (S1; in Rust since OR3b, `bc2_convert::shaderlevels`): each level database's entry-name prefixes and their counts (`coverage` without loading the database) | `BFBC2_SHADERCOVER_CACHE` | `BFBC2_SHADERCOVER_CACHE_MB`, default 64; LRU | about 27 MB, one file per stock level's Dx9 |

The BF1943 importer also keeps restart points for the whole process (S1; in memory, not on disk):
`bf1943import.RESTARTS`, by the bundle's path, size and time, within `BF1943_RESTARTS_MB` (default 128; 0: none), so
a new `Source` does not decompress a bundle whole again (30 MB for every bundle of BF1943).

**Logs** (`logs` in the command; round E3q): `%LOCALAPPDATA%\bfbc2-tool-suite\logs\<tool>-<date>.log`
(`bfbc2core.errors`, DbxEdit in process). A day's file that reaches `BFBC2_LOG_FILE_MB` (default 20) goes on in
`<tool>-<date>.1.log`, `.2.log` ... (nothing renamed or truncated: no line of a session is lost, and a file another
process holds is never in the way). At each tool's start and each rollover the folder is pruned
(`bfbc2core.caches.prune_logs`): each tool's 20 newest days are kept, then the oldest files are deleted beyond
`BFBC2_LOG_TOOL_MB` per tool (default 100) and `BFBC2_LOGS_MB` in all (default 200) - never the running session's
own files, never (for the caps) a log written in the last 10 minutes (a running tool's), never one held open.
**Nothing is lost:** the logs a prune removes are first zipped into a new archive in `logs-archive\` beside the log
folder (`logs-<date>_<time>.zip`, written whole before anything is deleted - if it cannot be written, nothing is
deleted); the archives have their own cap, `BFBC2_LOG_ARCHIVE_MB` (default 1024), the oldest archive deleted first.
Before
E3q there was no size cap (607 MB on the development PC; single NavKit and Studio days of 50-130 MB). (DbxEditGUI's C#
`ToolLog`, which wrote one file per day, was retired in round GR4.) Also under
`%LOCALAPPDATA%\bfbc2-tool-suite`, not a cache: `test_times.json` (`run_tests.py`'s durations). In-memory caches
(`BFBC2_FBRB_MEMORY_MB`, `pcbuild.MEMORY_LIMIT`, `functools.lru_cache`s) end with the process.

**Project caches with a cap** (round E3q; in Project OPFOR BFBC2's `work\` folder by design, found as the tools find
it; listed, pruned and - by name - cleared by the command): `navdata.load(cache_dir=)`'s path-data entries
(`<key>.npz`, `<key>.navdata.pkl`; only these are pruned), checked after each entry it writes (at most every 15
minutes per folder), a hit marking its file used:

| Name | Folder | Cap variable, default | Size before (2026-11-17) |
|---|---|---|---|
| `navstudio` | `work\navstudio\cache` | `BFBC2_NAVSTUDIO_CACHE_MB`, 2048 | 1.3 GB |
| `harness` | `work\harness\cache` (entries in `navdata\`; `bundles\` not pruned) | `BFBC2_HARNESS_CACHE_MB`, 3072 (a full run over all levels reads about 2 GB) | 2.1 GB |
| `navwater` | `work\navwater\cache` | `BFBC2_NAVWATER_CACHE_MB`, 1024 | 274 MB |
| `editor` | `work\editor\levels\<level>\` (round G3: the map editor's copies of `game\Package` levels; `.dbx`, `.res`, `.xml` and each level's `editor-level.json`; a copy missing a file is made again when opened; the editor also drops the least recently opened levels beyond the cap) | `BFBC2_EDITOR_CACHE_MB`, 2048 | new (G3: 8 - 20 MB per level, 585 MB for all 45) |

**Other project caches** (in Project OPFOR BFBC2's `work\` folder or the suite's `build\`; each is per level or per
game copy, replaced when its inputs change, and deleted by deleting the file or folder; the command leaves them alone):

| What | Where | Key | Written by | Eviction | Typical size |
|---|---|---|---|---|---|
| mesh viewer index | `work\tools_out\meshview_index.bin` (PR1b-2-rest; was `.pkl`) | the game copy's bundles (signature), format version 4, a SHA-1 of the file | `bc2_gamefs::meshindex` (built under a lock; `meshview.Index`, `bfbc2.exe`, the editor) | one file, rebuilt when stale | 33 MB |
| DBX catalogue | `work\tools_out\dbx_catalogue.bin` (PR1b-1e; was `.pkl`) | the content of what it reads: each bundle's entry names and content key (length, header, gzip trailer), the executable's SHA-256; format version 1, a SHA-256 of the file | `bc2_convert::dbxcatalogue::cache` (built under a lock; `dbxcatalogue.load`, `bfbc2.exe dbxcatalogue`, `dbxparity`) | one file, rebuilt when stale | SIZE_TBD |
| renderqa | suite `build\meshview_index.bin`, `build\renderqa_truth.pkl` | as the mesh viewer index | `renderqa` | one file each | 33 MB, 0.2 MB |
| terrain unpack | `work\levels\<lv>\terrain\` (with its stamp) | the terrain bundle's size and time | `level.heightfield_of` (under a lock) | one per level, replaced when stale | 4-50 MB per level (37 levels here) |
| path data cells | `work\navstudio\cache`, `work\harness\cache\navdata`, `work\navwater\cache` (`<key>.npz`, `<key>.navdata.pkl`) | the file's path, size and time, the height source, `CACHE_VERSION` and the code | `navdata.load(cache_dir=)` | LRU to the caps above (E3q) | 1.3 GB, 2.1 GB (the whole harness cache), 274 MB |
| harness bundle entries | `work\harness\cache\bundles\<stat key>\` | the bundle's size and time | `testharness.levelinfo` | none | about 30 MB |
| stock MP unpacks | `work\stockmp\_cache\<bundle>` | the bundle's hash (stamp) | `bfbc2-stockmp-ai` `pipeline` | one per bundle, re-unpacked when it changed | empty here |
| detailed sectors | `work\navkit\<lv>\detailed.json` | the samples, their time and NavKit's settings | `navkit` | one per level | KB |
| texture QA rows | the `--cache` folder given (`rows_v<VERSION>.json`, name lists) | the textures' and sources' SHA-1 | `texqa --cache` | none | MB |

The project caches still without eviction (harness bundle entries, texture QA rows) grow by one entry per changed
input; they are small next to the path data caches capped above.

## Tests

```
python run_tests.py quick      before every commit: no whole levels (see "The Rust tiers" below)
python run_tests.py game       before every push: uses Project OPFOR BFBC2's game copy, the whole-level Rust tests too
python run_tests.py full       after changing a format reader: every unpacked file and NavKit's regression on all 15 levels, about 5 hours with -j 2
python run_tests.py --list     what each tier runs;   --only <name> runs the tests whose name contains it
python run_tests.py game -j 8  tests side by side, 8 at a time (the default: half the CPU threads, at most 8)
python run_tests.py game -j 1  one after another in the listed order, as before 2026-11-17
python run_tests.py game --plan   the schedule and expected wall-clock, from the last run's durations; runs nothing
python run_tests.py quick --no-game   only the entries that need nothing but the checkout (no game copy, OPFOR,
                                      BF1943 / Xbox copies or graphics card); the others are named with the reason
python run_tests.py quick --no-game --ci-steps   as CI runs it: also leaves out cargo, ruff and mypy (CI's own steps)
python run_tests.py game --timeout 3600   end (and fail) any test still running after an hour (default: 4 x its
                                      kept duration, at least 30 min / 3 h / 8 h by tier)
```

**CI.** `.github/workflows/checks.yml` runs on every push and pull request (Windows): the module's build, cargo fmt,
clippy `-D warnings`, the workspace's cargo tests, ruff, mypy and `quick --no-game --ci-steps`. Merge a branch into
`main` once its checks are green (`RELEASING.md`). The game tier still runs only where the game copy is.

**Side by side.** Tests run several at a time (`-j N`). Each test declares what it needs in `run_tests.py`'s `NEEDS`
table: the CPU threads it keeps busy (`cpu`: 1, its process pool's size, or `'all'` for Rust batch work and cargo),
an estimate of its peak memory in GB (`mem`), and the shared things it reads or writes (`res`, `'r'` or `'w'`):
OPFOR's `work\navkit\regress` (the regression test rewrites it; the apply, links and harness tests
read it), every `pathdata.hpd` under `work\` (hpdtool reads them, the vehicle test rewrites one), the navigation
data cache, `work\navkit\sp_004_b`, the suite's `build\` folder (shaderview.exe, renderqa's pickles), a window on
the desktop and the checkout's cargo build folder with `bin\` (`cargo`; `rust-targets\<checkout>`
since round D1). A writer never runs beside a reader or another writer of the same
thing, and tests that share a thing keep their listed order, so each sees it as a one-at-a-time run did. The build
step (the Rust core) runs first, alone, before any test, since nearly every test loads
what they build. The runner starts a test while a slot, the CPU threads (`--cpus`, default all) and the memory
(`--mem`, default three quarters of the PC's) it declares are free; the longest chains go first, and a test held
back for room keeps its place (shorter tests fill in only while they end in time). A test missing from `NEEDS` runs
alone, and the runner names it, so a new test is safe before it is declared. A level's terrain unpack and the
`work\tools_out` caches (meshview index, dbx catalogue) are built under a lock between processes, so tests and tools
side by side never build them twice or remove each other's files.

The output is the same in both modes: one line per test as it finishes, the failures' output at the end (in list
order), the total and exit code 1 if any failed. Every run keeps each passed test's duration and peak memory (all
its processes, measured with a Windows job object) in `%LOCALAPPDATA%\bfbc2-tool-suite\test_times.json`; the next
run starts the longest first, and `--plan` uses them (`--times <log or json>` for others; `tests\test_durations.json`
seeds a PC that has none). Both files, `NEEDS` and the CI skips are keyed by each entry's stable id (the first field
of `run_tests.tests()`, shown by `--list`), not its display name, so a test can be renamed without losing them.
`tests/test_runner_parallel.py` (quick) checks the scheduler itself. Each test's
processes are in a job object that ends them when the runner ends, however it ends (Ctrl+C, a crash, killed), and
the run's result lines and total go to the day log (`%LOCALAPPDATA%\bfbc2-tool-suite\logs\run-tests-<date>.log`).

**The Rust tiers** (round TS1). The Rust core's checks are four quick entries: the test build (`cargo test
--no-run` and the doc tests), then the quick tests beside clippy, fmt and the Python entries. A whole-level editor,
levelwrite or renderer test (whole levels prepared, opened, exported or drawn) is marked `#[ignore = "game tier:
<why>"]` and runs in the game tier's entry `Rust core: the game tests`; the quick tier keeps the synthetic tests and
one real level per crate (MP_005's game view and project, levelwrite's edits on three levels, the renderer's MP_005
world). A test ignored for any other reason fails the quick run: none runs in no tier (`python tests\test_rust.py
list` names every test and its tier). A crate whose fingerprint (its files, those of the workspace crates it uses,
the workspace's manifests and lock) is unchanged since its last green clippy or quick run on this PC is left out of
them, and said so (`%LOCALAPPDATA%\bfbc2-tool-suite\rust_green.json`); never on CI, in the full tier or with
`tests\test_rust.py ... --no-skip`. Every Rust test's own time is kept in `rust_test_times.json` beside
`test_times.json`, and each run prints its slowest.

The quick tier has 134 entries, the game tier those and 103 more, the full tier 5 more again (242; `--list`,
2026-11-21; round RT1 ran the full tier, then 232 entries, in 5 h with `-j 2`: 232 of 232 passed). It includes the Rust core's checks (skipped without Rust), ruff and mypy over every folder (`tests/test_lint.py`,
`tests/test_types.py`; settings in `pyproject.toml`), the path finder on a stand-in tree, the converter units, the
dbxport rules and the emitter atlas. A test that needs game data the machine lacks is skipped and says so. The table
lists a selection; `--list` gives every test.

| Test | Tier | Checks |
|---|---|---|
| `tests/test_rust.py` | quick, game | the Rust core: `fmt` (`cargo fmt --check`), `clippy` (warnings as errors), `build` (the test binaries and the doc tests), `run` (every crate's unit, property and integration tests but the whole-level ones), `run --game` (the whole-level ones, game tier); with `--corpus` (game) every unpacked `.dbx` read in Rust and its XML against DbxEdit's 31,404 fingerprints, and every bundle of the game copy extracted; skipped without Rust |
| `tests/test_rust_skip.py` | quick | test_rust.py's rules on a stand-in workspace: a crate's fingerprint follows its own files, its dependencies' and dev-dependencies', included files, the lock and the build flags, and no other crate; green records from three processes at once lose none; never left out on CI or with `--no-skip`; which "skipped" lines count |
| `core/tests/test_dbx_native.py` | quick, game | the DBX reader (Rust; its Python retired 2026-11-18, round F1a) against the answers recorded from the Python (`dbx_golden.json`, `dbx_golden_corpus_*.json`): the same XML, trees and refusals on 10 built files and 4,754 damaged and random ones, and 200 corpus files (quick) or every corpus file (game: 48,120 distinct, levels, NavKit's common data and the WW2 work; each file's XML, tree, and the writer's bytes as read and after seeded edits; a file changed since is counted, not compared); the batch functions; XML against DbxEdit's fingerprints; needs the module |
| `core/tests/test_dbx_write_native.py` | quick, game | the DBX writer (Rust; its Python retired in round F1a) against the bytes recorded from the Python writer: a built partition, 41 edge trees (5 deliberately otherwise: `DELIBERATE`), 2,000 random trees, 200 work\ww2 files written back and edited (quick) or every corpus file (game); DbxEdit's roundtrip of the written files |
| `bfbc2-console-convert/tests/test_texture_native.py` | quick, game | the texture converter (Rust; its Python retired 2026-11-17) against the results recorded from the Python (`texture_golden.json`): 1,362 seeded built and damaged textures (every format, 2D and cube, packed tails, every mip count; the output's SHA-1 or the refusal's message), the batch function (quick); every XenonTexture of BF1943's source and OPFOR's Onslaught builds, input and output SHA-1 (game); needs the module; `--record` rewrites the recording |
| `bfbc2-auto-navkit/tests/test_navbuild_native.py` | quick, game | NavKit's generator (Rust; its Python retired 2026-11-17 and 2026-11-18) against the answers recorded from the Python (`navbuild_golden.json`): every grid walk on 40 seeded random multi-layer levels (float32 and float64 gradients, stairs, terrain, D sides, every link rule), `walkable`, `islands_of`, `matched_flags`, arrays of other types cast; E2a's, E3c's and E3h's 826 recorded answers (the rules, `build_areas`, the tracing, sector graphs, roof islands, cover nodes, Gabriel, whole stages, unseeds, `navdiff.area_cells`); `navsample.load`'s records against its Python reader (quick); sp_009 and sp_004_b built, the path data's SHA-1 as recorded with the build clock fixed, the inputs' SHA-1 checked first (game; `--levels` for others: sp_008_b and ons_mp_004 are recorded too); needs the module; `--record` rewrites the recording |
| `bfbc2-auto-navkit/tests/test_navtools_native.py` | quick, game | Round E4e's NavKit tools in the Rust core against their Python (kept as reference and fallback, `navnative`): `navvehicle.obstacle_distance` on 24 random multi-layer runs, `navdomains`' searches on 30 random graphs, `levelobjects.scan` on a built folder tree, `navdiff.diff` / `navdf.compare_sectors` / `ordercheck.PathData`'s route groups and `on_area` on 40 random file pairs, `navsample_offline.terrain_blocks` on 24 random height fields (quick); the tools on the game copy against `navtools_golden.json`, recorded from the Python before the port: ONS_MP_002's vehicle measures and path data, ONS_MP_004's boat mesh, domains world and 40 plans, navparity's rows for sp_009 and sp_004_b, sp_009's offline samples (game; the inputs' SHA-1 checked first; `--record` rewrites the recording) |
| `tests/test_native_required.py` | quick | on a copy of the suite: with no module and with a stale one, texture conversion, NavKit's builds, the conversions and the core formats (DBX, bundles, terrain, water, collision) stop at once with `NativeMissing` saying how to get it (a stale module warns once, from a reader with a Python twin); the matching module runs them under `BFBC2_PURE_PYTHON=1`; the source fingerprint ignores line endings and follows every source and crate |
| `core/tests/test_fbrb_native.py` | quick, game | the FBRB reader (Rust; its Python retired 2026-11-18, round F1a) against the answers recorded from the Python (`fbrb_golden.json`): the same entries, bytes and refusals on 19 built and hand-made bundles (gzip header fields, members, junk), 1,742 damaged and 600 random ones, the messages and `read_many`, 8 game bundles (quick) or every bundle of the game copy and `work\ww2` (game: 1,123 bundles, 833,461 entries, 28.6 GB; a bundle changed since is counted, not compared); FbrbTool's unpack against the reader |
| `core/tests/test_geometry_native.py` | quick, game | the terrain, water and Havok readers and the collision index (Rust; their Python retired in round F1a) against the answers recorded from the Python (`geometry_golden.json`): 1,808 built and damaged height fields, water files and packfiles, the index of built folders (the mesh-alias cache off, cold and warm), 960 damaged copies of MP_005's packfiles (quick); every height field, water file and packfile of the game copy (186 bundles) and the collision index of every prepared level (game) |
| `core/tests/test_scene_native.py` | game | `level.load` (the Rust scene; the Python loader retired in round F1a) against the Levels recorded from the Python loader (`scene_golden.json`): SP_01, MP_005, MP_009GR and ONS_MP_004 with and without geometry, piece by piece |
| `bfbc2-culltree/tests/test_culltree_native.py` | quick, game | `culltreelib.py`'s element reader (Rust; its Python retired in round F1a) against the answers recorded from the Python (`culltree_golden.json`): 2,146 built, damaged and random files (a stream cut inside an element: `CullTreeError` where the Python raised `IndexError`; a varint past 64 bits, which the Python read as a big integer: `CullTreeError`), add on a built level, the folder listing against glob (quick); every cull tree and level partition of the game copy and work, and console_cull's add run on Onslaught MP_005 against the recorded Python run (game) |
| `tests/test_runner_parallel.py` | quick | the test runner side by side: stand-in tests run through `run_tests.main` with `-j 4` and `-j 1` - a writer of a shared thing never overlaps its readers and they keep their listed order, an undeclared or all-core test runs beside nothing, a failure gives exit code 1 and its own output, one result line per test with its own summary; the scheduler's reservation for a held-back heavy test; the log reader; `atomicio.locked` across three processes |
| `tests/test_repo.py` | quick | no game files or large files in the history, no co-author lines, no build output tracked, READMEs, LICENSE, every Python file compiles, every Rust test file a module of its binary, every `test_*.py` in a tier (or allowed out with its reason), no new runner entry named by a round code |
| `tests/test_review_fixes.py` | quick | three defects from a code review (2026-10-26): NavKit's apply builds with the normal build's inputs (detailed zones, ladders); the stock-MP installer reinstalls a rebuilt file and keeps the original as its backup; the navigation cache key follows the terrain's content |
| `tests/test_review_fixes2.py` | quick | defects from the follow-up review (2026-10-27): the harness records a missing path-data file per file key, refuses to overwrite its reference through a casing alias; NavKit's restore after a second install on a map without an original; the stock-MP installer's markers found case-insensitively; FbrbTool refuses an older manifest whose size2 differs |
| `bfbc2-culltree/tests/test_culltree.py` | quick | cull trees and levels built in the test: the DBX codec, sphere keys, the tree layout and checks, verify, adding places for slot-0 objects (leaves, spheres, ancestors, spare nodes), the command line |
| `bfbc2-shader-tools/tests/test_shaderdb.py` | quick, game | built Dx9 and Dx10/11 databases round trip and pass the checks; merges (needed meshes only, first donor wins, records shared, programs re-sorted, every entry drawing as in its source, Dx kinds and the program limit); with the game: mp_008cq's and ons_mp_008's databases |
| `bfbc2-menu-tools/tests/test_aptgraft.py` | quick, game | a movie built through the model written, read back, round-tripped packed and in the other byte order; a sprite grafted between byte orders; dead data kept; the command line; with the game: every SwfMovie round trips, the Onslaught timer grafted into a copy of the original HUD (no finding, no new extern reads) |
| `bfbc2-menu-tools/tests/test_apt_e3n.py` | quick, game | round E3n against its references: apt.py's decoder in Rust and in Python (constants, dump, records, the batch) and aptgraft's writer / parser against `REFERENCE` - built movies, 600 damaged copies, 3,000 pushed floats, grafts, crosshair-add; with the game: every movie of the game copy, the pristine menus and the Xbox HUD copies both ways, and aptrefs xref, aptcheck and bc2loc dump with either decoder |
| `bfbc2-menu-tools/tests/test_apt_e4c.py` | quick, game | round E4c: aptgraft's Rust model against its Python model (every write of the unchanged model, roundtrip --pack, grafts, label / mode aliases, mode cases, crosshair-add; the same bytes or the same exception), the native objects' rules (shells, edits before a read, new and shared nodes, values only Python takes), aptcheck's and aptrefs' Rust scans against Code / movie_refs - built movies and 300 damaged copies; with the game: the 133 movies of E3n |
| `bfbc2-menu-tools/tests/test_bc2loc_write.py` | game | every language written back byte-identical; `set` adds and changes texts, every other entry kept, the table sorted; non-ASCII refused |
| `bfbc2-console-convert/tests/test_havok.py` | game | class layouts from the exe; every Havok resource the console and PC share converts to the PC file (or differs only where FORMAT.md 1.3 says); every console Havok resource of the Onslaught builds converts |
| `bfbc2-console-convert/tests/test_texture.py` | game | console textures: every converted format matches its layout size; 180+ textures the console and PC share convert byte-identical or to the same picture; cube maps; unknown formats refused |
| `bfbc2-mesh-viewer/tests/test_meshview.py` | game | the Garand's first-person mesh read as DICE's parts with textures; an async-bundle weapon and a rigid projectile read; the page (deprecated, kept for OPFOR's WW2 previews) embeds every part; the picture shows the model |
| `bfbc2-model-tools/tests/test_models.py` | game | every Garand / Type 5 LOD and mesh set written back byte-identical; a part replaced and read back (other parts unchanged); shapes' winding and tangents |
| `bfbc2-anim-tools/tests/test_granny.py` | game | the soldier skeleton (147 bones, weapon bones); every WW2 rifle animation parsed with every curve format; `shift` on a constant, a quantized and a ramped track: keys where asked, other tracks unchanged, size kept, CRC right |
| `bfbc2-anim-tools/tests/test_granny_native.py` | quick, game | the Rust Granny reader against `granny.py`'s Python reader, and `to_pc` (Rust; its Python retired 2026-11-18) against the results recorded from the Python (`granny_golden.json`): resources built in both byte orders with every member type, 2,500 damaged copies, the batch functions, a written Granny; with game data every Granny resource of your Xbox `mp_common` / `sp_common`, the game copy and Battlefield 1943's bundles read both ways and converted as recorded; `--record` rewrites the recording's "now" answers |
| `bfbc2-console-convert/tests/test_audio_native.py` | quick, game | the EA-XAS encoder and decoder (Rust; their NumPy code retired 2026-11-18) against the results recorded from it (`audio_golden.json`): the shift rule, 1,657 seeded encodes, decodes and streams, the refusals (quick); every BF1943 Wave and in-memory BFBC2 Xbox Wave, input and output SHA-1 (game, `--every N`); `--record` rewrites the "now" answers |
| `bfbc2-bf1943-import/tests/test_ports_native.py` | quick, game | `mesh.convert`'s vertex loop and `havok.convert`'s walk (Rust; their Python retired 2026-11-18) against the results recorded from it (`ports_golden.json`): 400 seeded console meshes, every packed value, 3,600 packfiles; meshport's and effectport's indexes against their reference searches (quick); every BF1943 mesh and physics port, every Onslaught console mesh and physics file as recorded, the effect ports and the emitter index (game) |
| `bfbc2-anim-tools/tests/test_granny_port.py` | quick, game | DICE's bone tables read and renumbered (synthetic); with game data: all 2,654 Granny resources of your Xbox `mp_common` / `sp_common` with a PC twin convert to it; 31 BF1943 animations convert to exactly BFBC2's copy |
| `bfbc2-console-convert/tests/test_mesh_decode.py` | game | mesh parts decoded from your Xbox Valparaiso and the PC copy alike (103 parts); console layout names clean; Battlefield 1943's rifle parts decode in range |
| `bfbc2-bf1943-import/tests/test_bf1943import.py` | quick, game | output paths kept inside the output folder, every type routed or explained, refused imports write nothing; with BF1943: every animation and skeleton converts and reads back, the Garand bayonet imported and read back, `import.json` merged, the source unchanged; the 3p undeploys' added tracks against DICE's 22 |
| `core/tests/test_paths.py` | quick | `bfbc2core.paths` on a stand-in tree: OPFOR beside, inside and above the suite, the given folder before `OPFOR_ROOT` before the search, missing game copy and siblings, junctions, importable with only `core` |
| `tests/test_locators.py` | game | every tool's locator gives the same answers as a reference checkout of `main` |
| `tests/test_converter_units.py` | quick | havokport's w lanes, the MOPP byte code, L8 textures and flag bit 1, meshport's rename, the texture-coordinate element |
| `bfbc2-bf1943-import/tests/test_dbxport_rules.py` | quick | the 19 edge cases of the dbxport deep analysis and a test per rule: templates, references, schema, value passes |
| `bfbc2-bf1943-import/tests/test_dbxport_fair.py` | quick (`--quick`: 7 partitions), game | every partition both games ship ported on a fair pool and checked against DICE's; skipped without game data |
| `bfbc2-bf1943-import/tests/test_dbxport_ranking.py` | quick, game | the template ranking's index against the full scan: 20,000 random rankings and `Templates.choose`; `--game`: the four WW2 level ports against a third of BFBC2's level partitions, every choice and the bytes (the Rust port against the Python port with the scan) |
| `bfbc2-bf1943-import/tests/test_dbxport_native.py` | quick, game | the Rust `dbxport.port` against its Python reference on 600 seeded random partitions, pools, catalogues and options (bytes, report lines, records, exceptions); `--game`: every fair port, counterpart port and effect port of the user's copies both ways |
| `bfbc2-bf1943-import/tests/test_dbxport_tables.py` | quick | dbxport's conversion tables in the Rust port against the Python reference: every entry of every table, then the tables changed in place as Project OPFOR changes them (the planes' gear input), template roles under a change, a changed built-in rule left to Python |
| `bfbc2-bf1943-import/tests/test_dbxport_opfor.py` | game | Project OPFOR's BF1943 vehicle, weapon and object builds replayed against this checkout, every `dbxport.port` call both ways; outputs to a temporary folder, every write under OPFOR refused; skipped without OPFOR |
| `bfbc2-shader-tools/tests/test_pcbuild_batch.py` | quick, game | `pcbuild`'s compile cache, `compile_many` and `prefetch` against plain compiles (the disk cache's damaged entries, keys); `--game`: Wake's 815 translated pixel shaders compiled both ways |
| `bfbc2-bf1943-import/tests/test_emitteratlas.py` | quick | a merged emitter atlas from synthetic atlases, the emitters re-pointed, the proof |
| `bfbc2-bf1943-import/tests/test_havokport.py` | game | the Kurogane oracle, the re-exported Zero and Corsair, the WW2 physics; model-group w lanes bit-identical on mp_004 / mp_005; MOPP codes complete and rays as brute force |
