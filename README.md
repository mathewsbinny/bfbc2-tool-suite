# BFBC2 Tool Suite

Tools for building and debugging AI on Battlefield: Bad Company 2 maps, in one repository. The 3D tools
share one core: level loading, a 3D view, picking and the game's data formats.

## Getting started

1. Install Python 3.12 or newer (`pyproject.toml` targets 3.12: bf1943-import uses its f-string syntax), then the
   libraries the tools use:
   `pip install numpy matplotlib moderngl glfw imgui-bundle pillow` (and `pefile capstone` for the crash
   reader's code view).
2. Get the suite's Rust core (one file, `core\bfbc2core\bc2_native.pyd`; texture conversion and NavKit's builds need
   it): in a command prompt in this folder, `python rust\build_native.py --download`. It fetches the prebuilt module
   made for this checkout's Rust sources, checks it and puts it in place; run it again after you update the suite
   (a tool that needs a newer one says so). No download for your checkout (local Rust changes, or not Windows):
   build it instead ("The Rust core" below).
3. Put this folder next to your `project-opfor-bfbc2` folder (or set `OPFOR_ROOT` to it): the tools read the game
   copy there.
4. Double-click **`BFBC2 Tool Suite.pyw`**. One window has every tool:
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

Every tool keeps a log in `%LOCALAPPDATA%\bfbc2-tool-suite\logs\` (one file per tool and day, a big day split
into numbered parts, each tool's 20 newest days kept within size caps: README "Caches"): what it did, warnings, and every error with its full details. When a tool crashes, a window says what
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
| `bfbc2-console-convert/` | **console resource converter**: Xbox 360 resources to PC - Havok physics (byte-swapped by the exe's own class layouts), textures (untiled, DXT1/3/5, BC5, DXT5A -> 8-bit, RGB565, RGBA16F, 8-bit L8 (format 10), cube maps; the header's flag bit 1 kept as the console's), MeshData (each part into a PC vertex layout; triangle strips unrolled) and mesh sets (rigid, composite, skinned, tree), terrain and water, soot meshes, sounds (EA-XMA -> EA-XAS); `FORMAT.md`; `texqa.py`: offline QA of converted textures against their Xbox sources and DICE's copies |
| `bfbc2-shader-tools/` | **shader databases** (`shaderdb.py`): read, check and write the levels' Dx9 / 10 / 11 shader databases (and the Xbox 360's); merge other levels' entries in, clone an entry under a new mesh name with its textures renamed, add mesh parts; `FORMAT.md`. Xbox 360 shaders on the PC: `xenos.py` (Xenos microcode), `xenos_hlsl.py` (HLSL for Dx10/11), `pcbuild.py`; `XENOS.md`; `renderqa.py`: an offline Direct3D 11 render harness that draws converted meshes, terrain strips and patches with the game's own Dx10/11 programs and flags black, NaN, misplaced and foreign-textured draws |
| `bfbc2-culltree/` | **static cull trees** (`culltree.py`): read, verify and extend a level's cull tree (places for objects that have none) |
| `bfbc2-mesh-viewer/` | **mesh viewer** (`meshview.py`): any model of the game with its colour textures, as a browser page (orbit, parts on / off) and a picture |
| `bfbc2-model-tools/` | **model tools**: edit skinned MeshData (replace or add a part's geometry in its own vertex layout; buffers and shared windows kept) and build simple solids with texture-atlas coordinates (`skinmesh.py`, `shapes.py`); PC textures from pictures (`texwrite.py`: DXT1 with 1-bit alpha, normal maps, DXT5, 8-bit atlas levels); fewer triangles (`decimate.py`) |
| `bfbc2-anim-tools/` | **Granny tools** (`granny.py`): the soldier skeleton and animations - bones, tracks and curves decoded; a bone's position track moved (constant, quantized, re-quantized ramp) with the Granny CRC recomputed; orientation curves written (`set_orientations`); tracks added from a donor (`add_tracks`); Xbox 360 resources read and converted to the PC's format (`topc`), DICE's bone table renumbered onto another skeleton; reading and `to_pc` in the Rust core when it is built, `to_pc_many` / `roots` for many files on all cores; generated aim poses (`aimanim.py`) |
| `bfbc2-bf1943-import/` | **Battlefield 1943 import**: the user's BF1943 Xbox 360 resources into BFBC2's PC formats, from the user's own copy - animations, skeletons and textures (`bf1943import.py`, a record per resource in `import.json`; the 3p undeploys get the tracks DICE adds), models (`meshport.py`), physics (`havokport.py`, Havok 4.6.1 -> 5.5.0; `mopp.py` decodes and checks the MOPP trees), DBX partitions (`dbxport.py` on BFBC2's own schema from `dbxcatalogue.py`; `dbxparity.py` / `portparity.py` against DICE's own conversions), effects (`effectport.py`) and merged emitter atlases (`emitteratlas.py`) |
| `core/` | the shared core: Python readers for `.dbx` (and a writer), `.fbrb` bundles (read only), terrain height fields and water; the level loader (entities, teams, collision, areas and routes); the 3D view with picking; `paths.py`, the one place that finds Project OPFOR BFBC2, its game copy, the built tools and the sibling projects (every tool's own locator wraps it) |
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

`texqa`, `renderqa`, dbxport's emitter-gradient converter and `mesh.relayout`, on branches on 2026-11-10, are on
`main` (checked 2026-11-15: no branch holds an unmerged commit).

How the code is laid out: every tool, module and test finds the suite root from its own file and names the folders it
imports from with `suite_paths.use(...)` (`suite_paths.py`, the one `sys.path` bootstrap); paths to OPFOR, the game
copy, the tools and the sibling projects come from `core/bfbc2core/paths.py` (DESIGN.md section 3.5).

## The 3D tools and your GPU

The Map Inspector and the Navigation Studio draw with OpenGL 3.3. Each prints the GPU it uses when it starts
(and shows it in its status line). On a laptop with two GPUs, Windows may give Python the slower integrated
one: set `python.exe` to "High performance" in Settings > System > Display > Graphics. On the development
PC that took the Navigation Studio on SP_01 (10.4 million triangles) from the AMD Radeon integrated GPU to
the GeForce RTX 4060, at 144 frames a second.

## The Rust core (required for texture conversion and NavKit's builds)

`rust/` is a Cargo workspace: the start of the shared Rust core the map editor is built on
(`bfbc2-map-inspector/EDITOR_ARCHITECTURE.md` 3.8). Today it holds the binary DBX reader, the FBRB bundle
reader, the terrain, water and Havok collision readers, the shader database reader / writer, the cull tree tool's element reader and level search, and the level loader (a Rust scene), ported from
`core/bfbc2core/dbx.py`, `fbrb.py`, `terrain.py`, `water.py`, `level.py` and NavKit's `levelobjects.py` / `hkpack.py`
and proven identical to them on every corpus file, every bundle of the game copy and, entity by entity, on 18
levels; and two pieces of ported logic whose Python was retired: texture conversion and NavKit's grid walks.

**What needs it** (since 2026-11-17): **texture conversion** (`bfbc2-console-convert/texture.py`'s `convert`: the
console converter, the BF1943 importer's textures, meshes and effects, OPFOR's WW2 and Onslaught pipelines) and
**NavKit's grid walks** (`navbuild.py` / `navwalk.py`: every NavKit build, apply and the Navigation Studio's
rebuilds). Their Python twins were retired after the full test tier had passed on the Rust (140 of 140,
2026-11-17), as `EDITOR_ARCHITECTURE.md` 3.8 rules for ported logic; without the module these stop at once with
an error saying how to get it. Everything else runs without it, on the Python readers (the format readers and
writers keep their Python for good: it is the reference the Rust is proven against).

**Get it** - either way puts `bc2_native.pyd` and `bc2_native.json` in `core\bfbc2core\`:

- **Download** (no Rust needed): `python rust\build_native.py --download` fetches the module made for this
  checkout's Rust sources from the suite's GitHub Releases (64-bit Windows, Python 3.12 or later), checks its
  SHA-256 and its fingerprint, and puts it in place. Each push to `main` that changes `rust\` publishes one
  (`.github/workflows/native.yml`: built with the pinned toolchain, the Rust checks and the Python tests without
  game data run first, release `native-<12 hex digits of the fingerprint>`). A checkout with Rust changes of its
  own has no download: build it.
- **Build** (once, and after changing anything in `rust/`; the quick test tier rebuilds it when it is stale):
  1. Install Rust from rustup.rs (stable, MSVC; `rust/rust-toolchain.toml` pins 1.100.0 and rustup fetches it) and
     Visual Studio's C++ build tools.
  2. `pip install maturin`
  3. `python rust\build_native.py` builds `bc2-py` (release) with maturin and puts the module in
     `core\bfbc2core\`. `--if-stale` builds only when the module is missing or was made from other sources (and,
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
| `rust/bc2-formats` | the game's formats in pure Rust, no Python types: `dbx` (parse, XML, values, writer; `parse_tree`: a compact read-only tree borrowing the file, a few allocations per file; `parse_all` / `parse_tree_all` / `to_xml_all` on all cores); `fbrb` (entries, any entry's data, whole bundles; `read_many` / `extract_many` on all cores; `cache`, the bundle cache: headers and decompressed payloads kept in the process and on disk; packing, byte for byte as FbrbTool's `pack`: `pack_path` / `pack_dir` / `pack_many`, `pack` from entries in memory; unpacking as FbrbTool's `unpack`: `unpack_path`, plain bundles, the rest left to FbrbTool.exe); `zlib` (zlib's deflate, levels 4-9, ported byte for byte: what FbrbTool's gzip streams hold); `terrain` (height fields: heights bit for bit, exact height at a point, a mesh, the tail's known layout); `water` (water meshes, the shader per triangle); `havok` (packfiles and a blueprint's collision triangles; a small test writer). `shaderdb` (shader databases, PC Dx9 / Dx10 / Dx11 and Xbox 360: every section as slices of the file, a writer); `culltree` (the element streams of cull trees and level partitions as `culltree.py` reads them; the reference levels' search on all cores); `dbxedit` (DbxEdit's commands in process - `xml`, `edit` with every edit command and `@file`, `import` with `--like` / `--types`, `check`, `roundtrip`, `fixslots` - a port of `DbxEdit.cs` with its own model of a file that keeps what an edit does not touch byte for byte: DbxEdit's files, messages and exit codes; `run_many` on all cores) |
| `rust/bc2-scene` | a level as the tools see it: `collision` (which packfile each blueprint uses: NavKit's index and mesh aliases in one parallel walk, each alias read from the binary partition - `dbx::field_ref`, the XML's answer without the text - and the converted XML only where the binary cannot answer; `index_cached` keeps each answer in a cache keyed by the files' sizes and times, so an unchanged level is indexed without opening its partitions), `scene` (`level.load`'s entities, blueprints, teams, outlines, collision, terrain and water on all cores; editable entities for the map editor) |
| `rust/bc2-convert` | the converter pipeline's hot spots: `texture` (console textures to the PC's: the Xenos untiling, the byte swap, DXT5A / RGB565 / A8R8G8B8 decoded; `bfbc2-console-convert/texture.py`'s `convert`, byte-identical on every BF1943 and Onslaught console texture; the Python converter retired 2026-11-17); `audio` (EA-XAS v1, the PC's in-memory sound codec: `bfbc2-console-convert/audio.py`'s encoder and decoder, the same bytes on every BF1943 Wave and every in-memory BFBC2 Xbox Wave; the encoder's search makes the reference's choice with less than half its work, each sound's groups on all cores, `encode_all` many sounds at once; round E3a, its NumPy code kept as the fallback); `mesh` (`bfbc2-console-convert/mesh.py`'s `convert` vertex loop: element by element, the packed normals and colour bytes to halves as NumPy rounds them; round E3g) and `havok` (`havok.py`'s `Packfile.convert`, the field walk over the game's class layouts; round E3g) - both hand anything the reference would not convert back to it, their Python kept as the fallback |

| `rust/bc2-formats` | the game's formats in pure Rust, no Python types: `dbx` (parse, XML, values, writer; `parse_tree`: a compact read-only tree borrowing the file, a few allocations per file; `parse_all` / `parse_tree_all` / `to_xml_all` on all cores); `fbrb` (entries, any entry's data, whole bundles; `read_many` / `extract_many` on all cores; packing, byte for byte as FbrbTool's `pack`: `pack_path` / `pack_dir` / `pack_many`, `pack` from entries in memory); `zlib` (zlib's deflate, levels 4-9, ported byte for byte: what FbrbTool's gzip streams hold); `terrain` (height fields: heights bit for bit, exact height at a point, a mesh, the tail's known layout); `water` (water meshes, the shader per triangle); `havok` (packfiles and a blueprint's collision triangles; a small test writer). `shaderdb` (shader databases, PC Dx9 / Dx10 / Dx11 and Xbox 360: every section as slices of the file, a writer); `culltree` (the element streams of cull trees and level partitions as `culltree.py` reads them; the reference levels' search on all cores); `granny` (Granny skeletons and animations, PC and Xbox 360: the tables, the object tree as `granny.py`'s `root()` has it, DICE's bone table and its renumbering); `dbxedit` (DbxEdit's commands in process - `xml`, `edit` with every edit command and `@file`, `import` with `--like` / `--types`, `check`, `roundtrip`, `fixslots` - a port of `DbxEdit.cs` with its own model of a file that keeps what an edit does not touch byte for byte: DbxEdit's files, messages and exit codes; `run_many` on all cores) |
| `rust/bc2-scene` | a level as the tools see it: `collision` (which packfile each blueprint uses: NavKit's index and mesh aliases in one parallel walk, each alias read from the binary partition - `dbx::field_ref`, the XML's answer without the text - and the converted XML only where the binary cannot answer; `index_cached` keeps each answer in a cache keyed by the files' sizes and times, so an unchanged level is indexed without opening its partitions), `scene` (`level.load`'s entities, blueprints, teams, outlines, collision, terrain and water on all cores; editable entities for the map editor) |
| `rust/bc2-convert` | the converter pipeline's hot spots: `texture` (console textures to the PC's: the Xenos untiling, the byte swap, DXT5A / RGB565 / A8R8G8B8 decoded; `bfbc2-console-convert/texture.py`'s `convert`, byte-identical on every BF1943 and Onslaught console texture; the Python converter retired 2026-11-17); `audio` (EA-XAS v1, the PC's in-memory sound codec: `bfbc2-console-convert/audio.py`'s encoder and decoder, the same bytes on every BF1943 Wave and every in-memory BFBC2 Xbox Wave; the encoder's search makes the reference's choice with less than half its work, each sound's groups on all cores, `encode_all` many sounds at once; round E3a, its NumPy code kept as the fallback); `granny` (Xbox 360 Granny resources to the PC's, `granny.py`'s `to_pc` byte for byte, a bone map renumbering DICE's table; `to_pc_all` on all cores; round E3i) |
| `rust/bc2-navbuild` | NavKit's generator walks: `Level` (which kept surface of a neighbouring column links to a surface), areas (`components`), traced sides, walkable lines, the route-group search, `walkable`'s flood and `islands_of`, `walkable`'s rules (`headroom`, `stair_slope`, `narrow_lips`, landings, `near_objects` / `on_objects`), `build_areas` and `build_graph`'s cell picking, and (E3c) `trace_edges` / `trace_emit` with `bridge_union`, `roof_islands`' rule, `add_cover_nodes`, `gabriel_prune`, `build_graph`'s walkable pairs and group joining, `near_characters`, `walkable`'s seed picks and `navsample.load`'s version 1 / 2 records - sectors, nodes and pairs in parallel where the order of results allows (`navbuild.py` / `navwalk.py` / `navsample.py`; the same path data byte for byte). The walks' Python (E1d: everything before the rules, plus `matched_flags`) retired 2026-11-17; E2a's and E3c's pieces keep theirs (`BFBC2_PURE_PYTHON`, `navbuild.use_native`) |
| `rust/bc2-shader` | the Xbox 360 shader translator (round E3f): `xenos` (the XDK shader container, its constant table, literals and header, and the Xenos microcode - control flow, ALU and fetch instructions - decoded: `bfbc2-shader-tools/xenos.py`'s `parse_shader`, the same tree attribute for attribute) and `hlsl` (a decoded program as HLSL for the Dx10 / Dx11 renderers: `xenos_hlsl.py`'s `translate`, the same text byte for byte and the same `TranslateError`s); `translate_all` / `translate_blobs` many programs on all cores. Proven on every program of BF1943's and BFBC2's Xbox shader databases (8,320 distinct, 63,305 with copies; 22,863 translations). The Python stays the reference and the fallback (`xenos.use_native`, `BFBC2_PURE_PYTHON`) |

| `rust/bc2-convert` | the converter pipeline's hot spots: `texture` (console textures to the PC's: the Xenos untiling, the byte swap, DXT5A / RGB565 / A8R8G8B8 decoded; `bfbc2-console-convert/texture.py`'s `convert`, byte-identical on every BF1943 and Onslaught console texture; the Python converter retired 2026-11-17); `audio` (EA-XAS v1, the PC's in-memory sound codec: `bfbc2-console-convert/audio.py`'s encoder and decoder, the same bytes on every BF1943 Wave and every in-memory BFBC2 Xbox Wave; the encoder's search makes the reference's choice with less than half its work, each sound's groups on all cores, `encode_all` many sounds at once; round E3a, its NumPy code kept as the fallback) |
| `rust/bc2-navbuild` | NavKit's generator walks: `Level` (which kept surface of a neighbouring column links to a surface), areas (`components`), traced sides, walkable lines, the route-group search, `walkable`'s flood and `islands_of`, `walkable`'s rules (`headroom`, `stair_slope`, `narrow_lips`, landings, `near_objects` / `on_objects`), `build_areas` and `build_graph`'s cell picking, and (E3c) `trace_edges` / `trace_emit` with `bridge_union`, `roof_islands`' rule, `add_cover_nodes`, `gabriel_prune`, `build_graph`'s walkable pairs and group joining, `near_characters`, `walkable`'s seed picks and `navsample.load`'s version 1 / 2 records, and (E3h) `build_graph` whole (`sector`: every sector of a stage at once) with a stage's areas and edge rows kept in the core (`NavAreas`: `drop_one_way`, the detail pass's candidate areas, `area_of`), the floods and islands from a parallel union-find, `walkable`'s unseeds - sectors, nodes and pairs in parallel where the order of results allows (`navbuild.py` / `navwalk.py` / `navsample.py`; the same path data byte for byte). The walks' Python (E1d: everything before the rules, plus `matched_flags`) retired 2026-11-17; E2a's, E3c's and E3h's pieces keep theirs (`BFBC2_PURE_PYTHON`, `navbuild.use_native`) |
| `rust/bc2-py` | the Python bindings (PyO3), module `bc2_native` |

**How the fallback works** (the pieces with a Python twin): `bfbc2core.dbx` imports `bc2_native` (from `core\bfbc2core\`, else an installed one).
When it is there, `dbx.parse`, `to_xml`, `parse_many` and `to_xml_many` run in Rust; when it is not, the same
functions run the Python reader, with the same results and the same errors (`ValueError`, `DeletedEntry`).
`BFBC2_PURE_PYTHON=1` forces the Python reader (for comparisons, or to rule the Rust core out of a problem);
`dbx.use_native(False)` does the same in code and `dbx.backend()` says which one runs. The Python reader stays the
reference: `core/tests/test_dbx.py` always tests it. A tool that loads many files at once should call
`dbx.parse_many` (all cores, and the cycle collector paused while the trees are built). `bfbc2core.fbrb` works the
same way (`entries`, `names`, `read` and the new `read_many`, one bundle per core; `fbrb.use_native`,
`fbrb.backend()`; a damaged bundle raises `ValueError` in both readers, a missing entry `KeyError`). So do
`bfbc2core.terrain` (`parse`, `read`), `bfbc2core.water` (`parse_mesh`, `parse_visual`, `read`), the new
`bfbc2core.collision` (`index`, `havok_index`, `triangles`, `triangles_many`, `blueprint_triangles`; NavKit's
Python functions as the fallback; NavKit's `detailed`, `ladders`, `navsample_offline` and `navwater_mesh` read their
collision through it) and
`bfbc2core.level` (`load` through the Rust scene; the Python loader as the fallback): each has `use_native` and
`backend()`, and `BFBC2_PURE_PYTHON=1` turns them all to Python. The Rust collision index (and so `level.load`)
keeps its mesh-alias cache in `%LOCALAPPDATA%\bfbc2-tool-suite\cache\collision` (`collision.use_cache`, or the
`BFBC2_COLLISION_CACHE` folder; `0` turns it off): it never changes a result, only skips partitions whose files
have the size and time they had when they were read.
**The bundle cache** (round E3e, `bc2_formats::fbrb::cache`): a zipped bundle's payload is one gzip stream, so
reading an entry means decompressing the payload up to it, and a tool reading entries one call at a time from the
same bundle decompressed it again for every call (OPFOR's WW2 level build: 726 reads, 132 s). With the Rust core,
`fbrb.read` / `read_many` now keep each bundle's header and the payload decompressed so far in the process
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
The writers too: `dbx.write` runs in Rust (the
Python writer's bytes; a tree the Rust writer does not take, such as one with an empty tag, goes to the Python
writer, so the result is always the Python writer's), and the new `fbrb.pack(folder, out)` / `fbrb.pack_many` pack
an unpacked folder as FbrbTool's `pack` does, the same bytes on every bundle of the game copy, with FbrbTool.exe as
the fallback (`bfbc2-stockmp-ai` packs through it). **Unpacking** (round E3j): `fbrb.unpack(bundle, out)` is
FbrbTool's `unpack` - the same files, names and `fbrb.manifest` - in the Rust core (`bc2_formats::fbrb::unpack_path`,
entries written on all cores) for the bundles every reader agrees on (one plain gzip member for header and payload,
plain ASCII names, no two entries on one file, paths under .NET's length limits); any other bundle, and any error in
process, goes to `FbrbTool.exe`, the reference (`core/tests/test_fbrb_unpack_native.py`: built, odd and damaged
bundles and the game copy both ways). `bfbc2-stockmp-ai` unpacks through it.
Outside `core`, `bfbc2-shader-tools/shaderdb.py` (`ShaderDb.parse` and `to_bytes`; a record of a kind only Python writes, such as a `bytearray`, falls back to the Python writer) and `bfbc2-culltree/culltree.py` (the element reader of `Doc`; `level_partitions`, the `--ref-dir` search) work the same way, each with `use_native` and `backend()`.
NavKit's walkability rules (`headroom`, `stair_slope`, `narrow_lips`, landings, `near_objects` / `on_objects`),
`build_areas` and `build_graph`'s cell picking (round E2a) work the same way too (`navbuild.use_native`, which turns
navwalk's rules off as well): their Python stays until they pass a full test tier.
So does `bfbc2-console-convert/audio.py`'s EA-XAS codec (round E3a: `xas_encode_frames`, `xas_decode_frames`,
`encode_xas` and the new `encode_xas_many`, all its sounds encoded at once; `audio.use_native`, `audio.backend()`;
`xas_encode_frames_python` / `xas_decode_frames_python` are the reference). The new `audio.convert_many` converts
many Waves side by side (ffmpeg's EA-XMA decodes in threads, the encoder without the interpreter).
So does `bfbc2-shader-tools`' shader translator (round E3f): `xenos.parse_shader` decodes in Rust (its tree's microcode attributes made on their first read), `xenos_hlsl.translate` translates such a tree in Rust while its microcode is unread, and the new `xenos.parse_many` / `xenos_hlsl.translate_many` take many programs at once on all cores; `parse_shader_python` and `translate_python` are the reference (`xenos.use_native`, `xenos.backend()`).

Round E3g (`rust/BENCHMARKS.md`, "E3g"): `mesh.convert`'s vertex loop and `havok.convert`'s packfile walk run in
Rust (`mesh.use_native` / `havok.use_native`, `backend()`; where the Rust stops - a refusal, a read past the data -
the Python runs, so results and errors are the Python's); meshport's layout search keeps BFBC2's part signatures,
donor lists and fallback choices (`meshport.REFERENCE` / `BFBC2_MESHPORT_REFERENCE=1`: the old scan); effectport's
template search reads BFBC2's emitter documents' instance types once (`EmitterIndex`, all cores) and keeps them in
`%LOCALAPPDATA%\bfbc2-tool-suite\cache\effectport` while the game copy's bundles are unchanged
(`BFBC2_EFFECTPORT_CACHE`: a folder, or `0`; `effectport.REFERENCE` / `BFBC2_EFFECTPORT_REFERENCE=1`: the old scan);
`meshview.Index.read_many` reads many entries a bundle at a time on all cores.

So does `bfbc2-anim-tools/granny.py` (round E3i): `Granny(b)` reads its tables and `root()` its object tree in
Rust (the same dicts, lists, tuples, numbers and strings, key order included), and `to_pc` converts in Rust (the
same bytes); a Granny written to (`put`, `to_bytes`, so `shift` and `set_orientations`) reads in Python from then
on, and what the Rust core refuses goes to the Python code, which raises what it raises (`granny.use_native`,
`granny.backend()`). New: `to_pc_many(items, errors='raise' | 'return')` and `roots(blobs)` (many files on all
cores, the interpreter released); `meshview.Index.read_many(names)` reads each bundle once for many names (a
bundle is one gzip stream: `read` of each name decompresses it from the start again).

Round E3m (`rust/BENCHMARKS.md`, "E3m: the sweep") profiled the tools no round had (`rust\bench_sweep.py`) and ranked
what is left. Changed, by measurement, in Python: the cull tree tool keeps each reference level's rows in
`%LOCALAPPDATA%\bfbc2-tool-suite\cache\culltree` (`BFBC2_CULLTREE_CACHE`: a folder, or `0`;
`BFBC2_CULLTREE_REFERENCE=1`: every reference read, as before) - console_cull's add 3x and 2.5 GB -> 0.2 GB, verify
5.8x; the test harness's opfor.log reader and the crash reader's look at the log search the text instead of every
line (`ingame.sessions_reference`, `crashreader.log_before_crash_reference` are the references).
Round E3d's two targets stayed in Python, by measurement (`rust/BENCHMARKS.md`, "E3d"): `pcbuild.compile_hlsl`
keeps what it compiled (in memory and in `%LOCALAPPDATA%\bfbc2-tool-suite\cache\hlsl`, keyed by the compiler DLL's
hash; `BFBC2_HLSL_CACHE` another folder or `0`) and `compile_many` / `prefetch` compile on all cores - ctypes
already releases the interpreter around Microsoft's compiler, so Rust could not make it faster; dbxport's template
ranking runs from an index (`_Ranking`, with the full scan `_best` as its reference, `BFBC2_DBXPORT_REFERENCE=1`).
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
command line that starts with `DbxEdit.exe`'s path and `subprocess.run`'s keyword arguments, so a caller switches
with one line. Writes go through `bc2-formats`' `atomic::replace` (a moment's lock on the target is waited out).
Each run is logged in DbxEdit's own log (`dbxedit-<date>.log`, "started (in process)"). `BFBC2_DBXEDIT_EXE=1` runs
`DbxEdit.exe` instead (for comparison), as does a checkout without the Rust core (warned once). `DbxEdit.exe`
stays the reference: `core/tests/test_dbxedit.py` runs DbxEdit's unit-test cases both ways (quick) and a corpus
sample, DbxEdit's edit fingerprints and a replay of a day's invocations (`--game`, `rust/bench_dbxedit.py`); the
suite's NavKit unpack and `bfbc2-stockmp-ai`'s validation use it.
**Without a fallback** (round E1d, Python retired 2026-11-17): `texture.convert` / `convert_many` and NavKit's grid
walks (`navbuild.Level`'s neighbour linking, `components`, `trace_edges`' side neighbours, `line_walkable`,
`build_graph`'s route-group search; `navwalk.walkable`'s flood, `islands_of`, `matched_flags`) always run in Rust:
`BFBC2_PURE_PYTHON` and `use_native` do not turn them off, and the tools call `texture.require()` /
`navbuild.require_native()` before they start work. Their tests check them against results recorded from the
Python before it was deleted (`bfbc2-console-convert/tests/texture_golden.json`,
`bfbc2-auto-navkit/tests/navbuild_golden.json`); NavKit's regression (`navregress`) stays the independent check
against DICE's path data.

**Develop:** in `rust\`, `cargo test` (unit, property and doc tests), `cargo clippy --all-targets -- -D warnings`,
`cargo fmt`; `python tests\test_rust.py` runs all three. Coverage: `cargo llvm-cov -p bc2-formats -p bc2-scene -p bc2-convert -p bc2-navbuild` (after
`cargo install cargo-llvm-cov` and `rustup component add llvm-tools-preview`). Timings: `rust\BENCHMARKS.md`.

## Caches

The tools keep what is slow to make again on disk. Every cache is only a cache: a file that is missing, damaged or
from other inputs (other code, another compiler, a changed bundle) is made again, never an error, so any of them can
be deleted at any time, also while tools run. Round E3p (2026-11-17) gave the shared ones under
`%LOCALAPPDATA%\bfbc2-tool-suite\cache` a size cap each, the least recently used files deleted first, and one
command to see and clear them:

```
cd core
python -m bfbc2core.caches                    each cache: size, files, cap and folder (also --list [name...])
python -m bfbc2core.caches --prune [name...]  delete the least recently used files beyond each cap now
                                              (every cache, project cache and the logs when no name is given)
python -m bfbc2core.caches --clear [name...]  delete the cached files (every shared cache when no name is given;
                                              the project caches navstudio, harness, navwater and logs by name only)
python -m bfbc2core.caches --opfor <folder>   Project OPFOR BFBC2's folder for the project caches (default: as the
                                              tools find it - OPFOR_ROOT, else walking up from the suite)
```

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
| `culltree` | `<sha1 of the level file's path>.pkl` | the code (`CACHE_VERSION`, a hash of `culltree.py`), the level file's size and time; each tree's inside | `culltree` `ref_rows` (E3m) | `BFBC2_CULLTREE_CACHE` | `BFBC2_CULLTREE_CACHE_MB`, default 2048; LRU (E3p) | one file per reference level partition (none yet on this PC) |

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
E3q there was no size cap (607 MB on the development PC; single NavKit and Studio days of 50-130 MB). DbxEdit.exe and
FbrbTool.exe (C#, `ToolLog`) still write one file per day; their logs are pruned with the rest. Also under
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

**Other project caches** (in Project OPFOR BFBC2's `work\` folder or the suite's `build\`; each is per level or per
game copy, replaced when its inputs change, and deleted by deleting the file or folder; the command leaves them alone):

| What | Where | Key | Written by | Eviction | Typical size |
|---|---|---|---|---|---|
| mesh viewer index | `work\tools_out\meshview_index.pkl` | the game copy's bundles (signature), `CACHE_VERSION` | `meshview.Index` (built under a lock) | one file, rebuilt when stale | 33 MB |
| DBX catalogue | `work\tools_out\dbx_catalogue.pkl` | as the mesh viewer index | `dbxcatalogue` | one file, rebuilt when stale | 70 MB |
| renderqa | suite `build\meshview_index.pkl`, `build\renderqa_truth.pkl` | as the mesh viewer index | `renderqa` | one file each | 33 MB, 0.2 MB |
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
python run_tests.py quick      before every commit: no game data, about a minute
python run_tests.py game       before every push: uses Project OPFOR BFBC2's game copy, about 10 minutes
python run_tests.py full       after changing a format reader: every unpacked file and NavKit's regression on all 15 levels, about 8 hours
python run_tests.py --list     what each tier runs;   --only <name> runs the tests whose name contains it
python run_tests.py game -j 8  tests side by side, 8 at a time (the default: half the CPU threads, at most 8)
python run_tests.py game -j 1  one after another in the listed order, as before 2026-11-17
python run_tests.py game --plan   the schedule and expected wall-clock, from the last run's durations; runs nothing
```

**Side by side.** Tests run several at a time (`-j N`). Each test declares what it needs in `run_tests.py`'s `NEEDS`
table: the CPU threads it keeps busy (`cpu`: 1, its process pool's size, or `'all'` for Rust batch work and cargo),
an estimate of its peak memory in GB (`mem`), and the shared things it reads or writes (`res`, `'r'` or `'w'`):
OPFOR's `work\navkit\regress` (the regression test rewrites it; the apply, links, Studio, viewer and harness tests
read it), every `pathdata.hpd` under `work\` (hpdtool reads them, the vehicle test rewrites one), the Navigation
Studio cache, `work\navkit\sp_004_b`, the suite's `build\` folder (shaderview.exe, renderqa's pickles), a window on
the desktop, `DbxTests.exe` and `rust\target`. A writer never runs beside a reader or another writer of the same
thing, and tests that share a thing keep their listed order, so each sees it as a one-at-a-time run did. The build
steps (Rust core, DbxEdit, FbrbTool, LevelDiff) run first, alone, before any test, since nearly every test loads
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
seeds a PC that has none). `tests/test_runner_parallel.py` (quick) checks the scheduler itself.

The quick tier has 83 tests (2026-11-17). It includes the Rust core's checks (skipped without Rust), ruff and mypy over every folder (`tests/test_lint.py`,
`tests/test_types.py`; settings in `pyproject.toml`), the path finder on a stand-in tree, the converter units, the
dbxport rules and the emitter atlas. A test that needs game data the machine lacks is skipped and says so. The table
lists a selection; `--list` gives every test.

| Test | Tier | Checks |
|---|---|---|
| `tests/test_rust.py` | quick, game | the Rust core: `cargo fmt --check`, clippy with warnings as errors, every crate's unit, property and doc tests; with `--corpus` (game) every unpacked `.dbx` read in Rust and its XML against DbxEdit's 31,404 fingerprints, and every bundle of the game copy extracted; skipped without Rust |
| `core/tests/test_dbx_native.py` | quick, game | the Rust DBX reader against the Python reader: the same XML, trees and refusals on built files, thousands of damaged and random ones, and 200 corpus files (quick) or every corpus file (game: 48,120 distinct, levels, NavKit's common data and the WW2 work); the dispatcher, the batch functions and `BFBC2_PURE_PYTHON`; skipped when the module is not built |
| `bfbc2-console-convert/tests/test_texture_native.py` | quick, game | the texture converter (Rust; its Python retired 2026-11-17) against the results recorded from the Python (`texture_golden.json`): 1,362 seeded built and damaged textures (every format, 2D and cube, packed tails, every mip count; the output's SHA-1 or the refusal's message), the batch function (quick); every XenonTexture of BF1943's source and OPFOR's Onslaught builds, input and output SHA-1 (game); needs the module; `--record` rewrites the recording |
| `bfbc2-auto-navkit/tests/test_navbuild_native.py` | quick, game | NavKit's grid walks (Rust; their Python retired 2026-11-17) against the answers recorded from the Python (`navbuild_golden.json`): every walk on 40 seeded random multi-layer levels (float32 and float64 gradients, stairs, terrain, D sides, every link rule), `walkable`, `islands_of`, `matched_flags`, arrays of other types cast; E2a's rules, `build_areas` and cell picking still call by call against their Python (quick); sp_009 and sp_004_b built, the path data's SHA-1 as recorded with the build clock fixed, the inputs' SHA-1 checked first (game; `--levels` for others: sp_008_b and ons_mp_004 are recorded too); needs the module; `--record` rewrites the recording |
| `tests/test_native_required.py` | quick | on a copy of the suite: with no module and with a stale one, texture conversion and NavKit's builds stop at once with `NativeMissing` saying how to get it (the DBX reader falls back to Python, warning once when stale); the matching module runs them under `BFBC2_PURE_PYTHON=1`; the source fingerprint ignores line endings and follows every source and crate |
| `core/tests/test_fbrb_native.py` | quick, game | the Rust FBRB reader against the Python reader: the same entries, bytes and refusals on built and hand-made bundles (gzip header fields, members, junk), about 1,700 damaged and 600 random ones, 8 game bundles (quick) or every bundle of the game copy and `work\ww2` (game); FbrbTool's unpack against the Rust reader; the dispatcher, `read_many` and `BFBC2_PURE_PYTHON`; skipped when the module is not built |
| `tests/test_runner_parallel.py` | quick | the test runner side by side: stand-in tests run through `run_tests.main` with `-j 4` and `-j 1` - a writer of a shared thing never overlaps its readers and they keep their listed order, an undeclared or all-core test runs beside nothing, a failure gives exit code 1 and its own output, one result line per test with its own summary; the scheduler's reservation for a held-back heavy test; the log reader; `atomicio.locked` across three processes |
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
| `bfbc2-anim-tools/tests/test_granny_native.py` | quick, game | the Rust Granny reader and `to_pc` against `granny.py`'s Python: resources built in both byte orders with every member type, 2,500 damaged copies, the batch functions, a written Granny; with game data every Granny resource of your Xbox `mp_common` / `sp_common`, the game copy and Battlefield 1943's bundles read and converted both ways |
| `bfbc2-anim-tools/tests/test_granny_port.py` | quick, game | DICE's bone tables read and renumbered (synthetic); with game data: all 2,654 Granny resources of your Xbox `mp_common` / `sp_common` with a PC twin convert to it; 31 BF1943 animations convert to exactly BFBC2's copy |
| `bfbc2-console-convert/tests/test_mesh_decode.py` | game | mesh parts decoded from your Xbox Valparaiso and the PC copy alike (103 parts); console layout names clean; Battlefield 1943's rifle parts decode in range |
| `bfbc2-bf1943-import/tests/test_bf1943import.py` | quick, game | output paths kept inside the output folder, every type routed or explained, refused imports write nothing; with BF1943: every animation and skeleton converts and reads back, the Garand bayonet imported and read back, `import.json` merged, the source unchanged; the 3p undeploys' added tracks against DICE's 22 |
| `core/tests/test_paths.py` | quick | `bfbc2core.paths` on a stand-in tree: OPFOR beside, inside and above the suite, the given folder before `OPFOR_ROOT` before the search, missing game copy and siblings, junctions, importable with only `core` |
| `tests/test_locators.py` | game | every tool's locator gives the same answers as a reference checkout of `main` |
| `tests/test_converter_units.py` | quick | havokport's w lanes, the MOPP byte code, L8 textures and flag bit 1, meshport's rename, the texture-coordinate element |
| `bfbc2-bf1943-import/tests/test_dbxport_rules.py` | quick | the 19 edge cases of the dbxport deep analysis and a test per rule: templates, references, schema, value passes |
| `bfbc2-bf1943-import/tests/test_dbxport_fair.py` | quick (`--quick`: 7 partitions), game | every partition both games ship ported on a fair pool and checked against DICE's; skipped without game data |
| `bfbc2-bf1943-import/tests/test_dbxport_ranking.py` | quick, game | the template ranking's index against the full scan: 20,000 random rankings and `Templates.choose`; `--game`: the four WW2 level ports against a third of BFBC2's level partitions, every choice and the bytes |
| `bfbc2-shader-tools/tests/test_pcbuild_batch.py` | quick, game | `pcbuild`'s compile cache, `compile_many` and `prefetch` against plain compiles (the disk cache's damaged entries, keys); `--game`: Wake's 815 translated pixel shaders compiled both ways |
| `bfbc2-bf1943-import/tests/test_emitteratlas.py` | quick | a merged emitter atlas from synthetic atlases, the emitters re-pointed, the proof |
| `bfbc2-bf1943-import/tests/test_havokport.py` | game | the Kurogane oracle, the re-exported Zero and Corsair, the WW2 physics; model-group w lanes bit-identical on mp_004 / mp_005; MOPP codes complete and rays as brute force |
