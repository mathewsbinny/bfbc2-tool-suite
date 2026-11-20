# Changelog

What changed for the suite's users, version by version. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/): each version lists what was **Added**, **Changed**,
**Fixed** and **Removed**, newest first.

**Versions.** The suite uses [Semantic Versioning](https://semver.org/) and is before 1.0, so a version is
`0.MINOR.PATCH`:

- **MINOR** goes up for a batch of features a user can see (a new tool, command, editor workspace or format).
  Before 1.0 a minor version may also change how a tool is called or what it writes; such changes are listed under
  **Changed** or **Removed**.
- **PATCH** goes up for fixes only.
- 1.0 is the full suite release (`ROADMAP.md`, priority 8).

The version lives in one place, the `VERSION` file at the repository's root. `bfbc2core.__version__`,
`bfbc2.exe --version`, `bfbc2-editor --version`, the editor's Help window and the hub's title all read it. How to
release a version: `RELEASING.md`.

Versions 0.1.0 to 0.8.0 were assigned afterwards (round P1, 2026-11-19), from the commit history and
`rust/BENCHMARKS.md`; they were not tagged at the time. Each date is the day the last change of that version
landed on `main`.

## [Unreleased]

### Added
- **Map editor: the Map Inspector's views** (round GR1). Properties' **All fields**: every field of the selected
  object, nested, with its type (`int32`, `float32 x16`, `bool`, `text`, `reference`, `array [n]`, `struct`) and its
  value as DbxEdit's XML writes it; references lead to their object. A **Links** panel: what the object references,
  what references it, and the event connections it sends and receives (from the level's `EventConnections`); a row
  goes to that object, Back returns. The links are drawn in the views in the new `view-link` colour (references
  solid, event connections dashed, arrows by direction; K shows or hides them). **Measure** (M): clicks add points;
  the last segment's length, horizontal distance, height and slope, the path's total, the height above the terrain
  and below the water. A project's links and fields are read from its edited partition. The plan for the other
  GUIs is `bfbc2-map-inspector/EDITOR_ARCHITECTURE.md` section 9.
- **Specification:** the engine specification's first two layers, the file formats and the data model (`spec/`,
  439 rules with their evidence; `spec/trace.py` checks the index).

- **`bfbc2.exe`:** the shader tools (`shaderdb` with `merge`, `renderqa`, `xdiff`), the console readers (`apt`,
  `stfs`, `xiso`), `bf1943import`, and stockmp's `generate` / `build` / `validate` / `all` (round O5); NavKit's
  `detailed` and `navkit status` answered in Rust. `stockmp build` about 2.5 times faster, `shaderdb needs` about 8
  times.
- **Map editor:** any map exported for any era and multiplayer mode (round E3d). The era (BFBC1, BFBC2, Vietnam,
  WW2) and the mode are project settings (`Op::SetMode`, set in the New project form and the Export window): the
  modes offered are those the era's stock levels have (WW2: Conquest), the donor's mode layer comes into the project
  as editable objects (flags, bases, M-COMs per Rush stage, spawns, combat areas) and the base's own layer goes
  out, one undo step. "Use selection" moves a flag, base or M-COM with what belongs to it.
- **Map export:** the level's closure resolves under the target era's bundles: what another era's commons, another
  level's bundle or a deleted marker would leave missing is copied into the level's bundle and listed with its
  source (`stage.json`'s `closure`, `closure.txt`); a WW2 base's own BF1943 voice banks give way to the target
  era's; the map's terrain bundles stay whatever the era.
- **`mapexport check`:** the closure, the map's height field, and the mode layer's completeness against the donor's
  ("Rush stage 2 has one M-COM (the donor's has 2)").
- **Map editor:** Install and Uninstall in the Export window (Project OPFOR's `mapedit_install.py`: the plan first,
  then the install on confirmation).

### Changed
- **`bfbc2.exe culltree add`** reads the reference levels one at a time: about 80-110 MB instead of 1.65 GB, and
  0.6-0.9 s with a warm cache instead of 7 s. Its row cache is shared with `culltree.py` (either side's warmed cache
  serves both) and is pruned within the cache budget.
- **The last Python format paths run in the Rust core** (round F4; the same results, the Python kept as the
  fallback): `audio.convert` / `decode` whole (the ffmpeg glue included; `convert_many` converts side by side
  without the interpreter's lock), `texqa`'s texture readers (about 9 times faster), NavKit's offline sampler's
  collision assets (`asset_bodies`, its convex hulls; about 6 times faster), `hkpack.Packfile` as a handle (its
  dicts built only when read; `read_object` in Rust), `xenos.disasm`'s header (parse and listing about 8 times
  faster than the Python), `meshview`'s part reader and the cull tree tool's string table.
- **Shader databases are opened in the Rust core end to end** (round SD1; the same results, the Python kept as the
  fallback): `ShaderDb.parse`'s choice of byte order and layout and its messages, `pick_database` and
  `dx_from_name` are one Rust implementation shared by `shaderdb.py` and `bfbc2.exe shaderdb`; `load_levels` reads
  each level bundle's database list once instead of once per Dx.

### Fixed
- **`shaderdb.ShaderDb.parse`** with an unusual kind (a float, a negative or a very large `dx`) raised `TypeError` /
  `OverflowError` from the Rust core instead of reading the file as the Python does (round SD1).

### Removed
- **The Map Inspector** (`bfbc2-map-inspector/inspector.py`, round GR1): its 3D view, layers, search and Selection
  panel were already the editor's; its property tree, "referenced by", link lines and Measure are the editor's All
  fields, Links and Measure (above). The hub's Levels tab opens levels in the map editor; the folder keeps
  `EDITOR_ARCHITECTURE.md` and a README saying where each feature went.

## [0.9.0] - 2026-11-19

### Added
- **Map editor:** the start flow (round G4): Open project, Edit existing level (multiplayer maps of each set:
  BFBC1, BFBC2, Vietnam, WW2) and Create level (from any existing level, or from a height map).
- **Map editor:** context menus per workspace, edit modes, sinking an object into the surface with Alt+wheel, deep
  duplicate, copy and paste, polygon selection, and the asset browser over the whole content set with its Spawners
  and Objectives modes (round E2b).
- **Map editor:** the game-accurate view is interactive and editable: edits show at once and picking works in it,
  with the picture unchanged (round R5c).
- **Map editor:** export profiles for BFBC2, Vietnam, WW2 and BFBC1; an export gets its own name and description in
  every language and its own loading and map-list pictures; the campaign strip checklist for exports from a
  campaign level (round E3b).
- **Map editor:** an export's announcer voice-over follows its profile: WW2 exports use BF1943's voices, Vietnam
  the Vietnam DLC's, BFBC1 and BFBC2 their own; `mapexport check` verifies the voice-over registration (round E3c).
- **The suite's icon** (the author's M67 render) on `bfbc2.exe`, `bfbc2-editor.exe`, `mapexport.exe`, the editor's
  window and the hub's window (round I1).
- **`bfbc2.exe`:** more of the tools' command lines in Rust, with the same arguments, output and files as their
  Python: `aptcheck`, `aptrefs`, `bc2loc`, `exeref`, `crashreader`, `hpdtool`, `hkpack`, `convert`, `aimanim`
  (round O2); `caches`, `stockmp`, `aptgraft`, `audio`, `dbxcatalogue`, `dbxparity`, `portparity`, `meshview`,
  `culltree` (round O3); `navkit`, `prepare_level`, `levelobjects`, `ladders`, `ordercheck`, `navcompare`,
  `graphstats`, `hpdcheck`, `hpdview`, `detailed`, `navparity`, `navregress`, `sample_all`, `harness`, `texqa`
  (round O4).
- **Versions:** the `VERSION` file, this changelog, `ROADMAP.md` and `RELEASING.md`; `bfbc2.exe --version`,
  `bfbc2-editor --version`, `bfbc2core.__version__`, the version in the editor's Help window and window title and in the hub's title
  (round P1).
- **Tests:** `python run_tests.py quick --no-game` runs only the entries that need nothing but the checkout and
  names each one it skips, with the reason; `--ci-steps` leaves out the entries CI runs as steps of their own
  (cargo, ruff, mypy) (round P1).
- **CI:** `.github/workflows/checks.yml` checks every push and pull request on Windows: the module's build, cargo
  fmt, clippy and the workspace's tests, ruff, mypy and the quick tier's game-free entries (round P1).

### Changed
- Cargo builds go outside the checkout (`rust-targets\<checkout>`, or `BFBC2_RUST_TARGETS`), the built binaries to
  `bin\`, and every cache shares one size budget (round D1).
- The prebuilt Rust module's releases: only the newest is kept, with a plain title and notes.
- `harness compare` lists the tests and levels missing from or new in a run sorted by level, scope and test.
- The module's source fingerprint includes `VERSION`, so a new version is a new build of `bfbc2.exe` (round P1).

## [0.8.0] - 2026-11-18

### Added
- **The map editor**, `bfbc2-editor.exe` (rounds G1-G3, E2, E3, R5, R5b): a read-only map view of any level (egui
  on wgpu, the suite's design language in dark and light), team labels by faction, a searchable Help built from the
  action registry; the levels that live only in `game\Package` (Vietnam, the Pacific War's WW2 maps, BFBC1) and a
  "Choose mode" start screen; map projects with an operation log (save, undo / redo, autosave and recovery) and the
  Objects workspace; export of an edited level as a new multiplayer level; a game-accurate view drawing the whole
  level with the game's own Direct3D 11 shaders, pixel for pixel against the C++ render harness.
- **`bfbc2.exe`**, the suite's command lines in one Rust binary: `fbrbtool` and `dbxedit` first (round O1).
- Rust ports of the stock-MP AI layer (E4a), the shader database logic (E4b), aptgraft, aptcheck and aptrefs
  (E4c), dbxport (E4d), NavKit's tools around the build (E4e), the test harness and the crash-reader tools (E4f),
  the file formats that had no Rust (F0: path data, localization, BIGF, STFS / XISO, Wave, mesh headers), the
  console converter's rest, the model tools and Granny writing (F2a), the BF1943 importer's remaining Python, the PE
  reader, xmlinst and navtune (F2b), and the remaining format readers and writers (F3).
- Streamed bundle packing, process-wide BF1943 restart points and the shader coverage index (round S1).

### Removed
- **DbxEdit.exe and FbrbTool.exe** as tools: their commands run in process on the Rust core
  (`bfbc2core.dbxedit`, `bfbc2core.fbrbtool`, `bfbc2.exe dbxedit / fbrbtool`), checked against the exes' recorded
  answers (round R4).
- The ported Python of NavKit (R1), the converters (R2), the shader translator's HLSL emission (R3) and the core
  format readers and writers (F1a): the Rust core is required for them.
- NavKit's research switches (BRIDGE_UNION and the others): the chosen behaviour is the only one.

### Fixed
- dbxport's Rust port read its data tables built in instead of as OPFOR changes them (round E4d's fix).

## [0.7.0] - 2026-11-17

### Added
- **The Rust core** (`core\bfbc2core\bc2_native.pyd`, `rust\`), with a prebuilt module from GitHub Releases
  (`python rust\build_native.py --download`) and a source fingerprint so a stale module is never used silently.
  In Rust: the DBX and FBRB readers (E0); the level loader, terrain, water and collision, the writers (zlib, DBX),
  shader databases and cull trees, texture conversion and NavKit's grid walks, the mesh alias table (E1a-e);
  NavKit's walkability rules and the rest of its generator (E2a, E3c, E3h); the EA-XAS audio encoder (E3a); DbxEdit's
  and FbrbTool's commands in process (E3b, E3k); the bundle cache (E3e); the Xbox 360 shader translator (E3f); the
  BF1943 model, physics and effect ports (E3g); Granny animations (E3i); the stock-MP AI layer's unpacking (E3j);
  the APT decoder (E3n); navdata, kymesh, navpath, havok.verify and texqa (E3o).
- HLSL compiles cached and on all cores; dbxport's template ranking from an index (E3d).
- `run_tests.py` runs tests side by side (`-j N`), each declaring its CPU, memory and shared resources; `--plan`
  prints the schedule.
- Cache housekeeping (`python -m bfbc2core.caches`): one size budget, least recently used first (E3p).
- Logs capped by size and day, archived before they are pruned (E3q).

### Changed
- Large speed-ups across the suite, each byte-identical to the Python it replaced (`rust/BENCHMARKS.md`).
- Whole-file writes wait out a moment's lock on the target (Windows).

### Removed
- The Python of texture conversion and NavKit's grid walks (round E1d's retirement): the Rust core is required.

## [0.6.0] - 2026-11-15

### Added
- Xbox 360 shaders on PC: per-texture sampler routing, terrain strips and terrain patches with the engine's named
  constants.
- Sub-terrain heightfields (the ring around the main block).
- Console texture format 9 (big-endian A8R8G8B8) converted to PC.
- `skinmesh`: a new part in a given vertex layout.

### Changed
- The quick tier builds DbxEdit, FbrbTool and LevelDiff only when a source is newer than the .exe.

### Fixed
- The Xenos translator: MAXA / MAXAS set the address register after the instruction.
- effectport passes its ported partitions' guids (dbxport had nulled valid references); NavKit and stock-MP AI QA
  fixes (2026-11-14).
- The deep scan (2026-11-15): the stock-MP install applies as one transaction, installs and writes are atomic,
  caches are honest, damaged files are reported.

## [0.5.0] - 2026-11-10

### Added
- The BF1943 importer's ground truth and checks: `dbxcatalogue` (BFBC2's own partitions as dbxport's reference),
  the dbxport rewrite with ranked templates, `dbxparity`, `portparity`, `effectport`, `emitteratlas`.
- `mopp.py`: Havok's MOPP byte code decoded.
- `aimanim.py`: generated aim poses read and written.
- `texqa`: offline QA of converted textures; `renderqa`: an offline Direct3D 11 render harness; `xenos_exec` and
  `xdiff`: a Xenos pixel-shader interpreter and the translator's differential test.
- Console texture format 10 (L8), DXT1 with 1-bit alpha in `texwrite`, mesh relayout, mesh rename in meshport,
  physics material slots in havokport, third-person undeploy tracks (`granny.add_tracks`).
- Lint and types: ruff and mypy on every folder in the quick tier (`pyproject.toml`).
- `suite_paths.py`, the one `sys.path` bootstrap, and `bfbc2core.paths`, the one path finder.

### Fixed
- xiso extract stopped looping on a truncated image; stfs follows the live copy of each hash table.
- aptcheck verify takes `--brief`; hpdtool roundtrip counts a truncated file as one failure.
- culltree: a tree reference without a `levels/` segment, and verify / add reading the target as its own reference.
- The test harness logs a file on another drive; the crash reader's code view decodes from the function start.
- NavKit: the detailed-sector cache keyed by sector offsets, apply keeps its report on any failure, the grid
  settings restored after a failed detailed pass.
- The mesh viewer rebuilds a corrupt index cache.

## [0.4.0] - 2026-11-07

### Added
- Console conversion, Xbox 360 to PC: terrain (heightfield, water mesh, material map, visual terrain and water),
  soot meshes, Frostbite Wave sounds (EA-XMA to EA-XAS), textures (DXT5A, RGB565, RGBA16F).
- `shaderdb`: Xbox 360 shader databases read byte for byte.
- Model tools: `texwrite` (PC textures from pictures, DXT1 / DXT5) and `decimate` (quadric edge collapses).
- Menu tools: `aptgraft` frame-label aliases and HUD icons of their own.
- The Xbox 360 shader translator: `xenos.py` (microcode decoder) and `xenos_hlsl.py` (HLSL for Dx10/11), with
  `shaderview` for drawing.
- dbxport: renamed fields, pruning and required instances; havokport's velocity limits from the game's table.

### Fixed
- BF1943 import fixes found by Wake Island's first loads; shape subparts in Havok 5.5.0's placement order.

## [0.3.0] - 2026-10-31

### Added
- The console converter, the shader, cull-tree, mesh viewer, model and animation tools.
- Granny resources read big-endian (Xbox 360) and ported to PC.
- The BF1943 import pipeline: models (`meshport`), rigid mesh sets and vehicles, physics from Havok 4.6.1 to 5.5.0
  (`havokport`), the DBX writer and partition port (`dbxport`).

### Changed
- The project the suite serves is now Project OPFOR BFBC2; the repositories moved to `C:\Programming\Projects`.

## [0.2.0] - 2026-10-26

### Added
- NavKit closer to DICE's path data: area link rule, symmetric portals, the reference extent, headroom band, route
  nodes at AI order positions, detailed zones, ladder links, narrow lips and door sills, exact bridge tracing,
  objective floors, D lines and F cells, vehicle path data, boat meshes, routes across domains, route-island
  pruning, the offline sampler.
- `navparity`, `ordercheck`, `navcompare`, `navdiff` and `sample_all` for measuring builds against DICE's data.
- Navigation Studio overlays: the reference AI layer and anomalies.
- `apt.py`: an APT menu reader for PC and Xbox 360.
- `xmlinst`: level XML instances whatever the attribute order.

## [0.1.0] - 2026-10-16

### Added
- The suite as one repository (BFBC2 Tool Suite, MIT licence): NavKit (path data decoded byte for byte, the
  generator, the driver and its window), DbxEdit and DbxEditGUI, FbrbTool, LevelDiff, the crash reader, the console
  readers (STFS, XISO).
- The shared core: the DBX reader, the level loader, terrain and water, a 3D view with GPU picking.
- The Map Inspector; the Navigation Studio with the path test, walkability edits, links and route edges,
  traversals, the apply report and installing.
- The suite window (hub) with level pickers; error handling and logs for every tool; the test runner's quick and
  game tiers.

## The back-filled versions' last commits

The last commit on `main` of each back-filled version, should they ever be tagged:

| Version | Commit | Date |
|---|---|---|
| 0.8.0 | `c448e14` | 2026-11-18 |
| 0.7.0 | `ee25186` | 2026-11-17 |
| 0.6.0 | `b94ff47` | 2026-11-15 |
| 0.5.0 | `eefeacb` | 2026-11-10 |
| 0.4.0 | `6e01543` | 2026-11-07 |
| 0.3.0 | `5bd9c47` | 2026-10-31 |
| 0.2.0 | `a376e7e` | 2026-10-26 |
| 0.1.0 | `d971a9b` | 2026-10-16 |
