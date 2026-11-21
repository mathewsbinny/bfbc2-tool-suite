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
- **`xisolib` reads disc images held in memory** in the Rust core (round RT3: `find_partition_bytes`, `walk_bytes`;
  an `io.BytesIO` goes through them), so the Xbox disc reader has no Python-only path left.
- **`run_tests.py --help`** (also `-h`, `/?`) prints the runner's text and runs nothing (round RT3); an argument the
  runner does not know is a usage error (exit code 2) instead of being ignored.
- **A repository rule** (round RT3, `tests\test_repo.py`): every `tests\*.rs` of a Rust crate with `autotests = false`
  must be a `mod` of its test binary, so a test file can no longer silently never run.
- **Map editor: every name of an exported level** (round EX1). The Export window's **Names**: the level id (what the
  game loads, `Levels/<ID>`, and where its `<ID>_NAME` / `<ID>_DESC` texts come from), its folder under
  `Package\levels\` (the id in lower case, as on every level of the game: renaming the folder renames the id), the
  **server-browser name** (`maps.ini`: the launcher's row and the browser's `OPFOR - <name> (<mode>)`), and the
  **on-screen name and description per language** (the menus, the loading screen, the browser's map column), with
  each language's own letters (French, Russian, Japanese ... stored through that language's text table). The names
  are checked together and refused in words: a stock level's id or another project's installed export, an id
  holding `_NAME` (the loading screen would look for another picture: a black screen), a text or loading picture the
  game already has (`SP_05_A` would rename Heavy Metal), a folder that is not the id, a browser name another map has
  or too long for the row, a letter a language cannot show. They are kept in the project's log (`Op::SetNames`,
  "Keep in project", and on Export). `mapexport names <export.json>` checks them; `mapexport build` refuses names
  that fail; `mapexport check` repeats the rules on a stage and proves every place names the same level (the
  description, the menus' list items, every `package.mft` block and `bundleManifest` line, the name key's picture).
- **Map editor: Preview export** (round EX1). After an export, the stage opens read only beside the original base
  level, with the same camera: T switches between them, Close preview gives the project back. The Diff panel lists
  what the export changed (the mode layer in and out, the campaign content stripped, objects from other sets).
- **Map editor: the Diff panel** (round EX1, LevelDiff folded in). **Compare with base** (a project against its base
  level, again by itself after each edit) and **Compare with level** (the open level against any other): every
  object added, removed, moved or changed, with filters (kind, group - mode layer, from other sets, campaign
  content, world -, text, placed objects only); a click selects it and flies there; its fields as LevelDiff prints
  them (Inspect opens All fields). In the 3D view the differing objects are tinted and a box marks where an object
  of the other level stands, with a line from a moved object's old place: 4 new colour tokens (`view-diff-added`,
  `-removed`, `-moved`, `-changed`; never orange, the selection's).
- **`bfbc2.exe leveldiff`** (round EX1): LevelDiff's command line in Rust - the same arguments, text, `--json` file
  and exit codes, proven byte for byte on LevelDiff.exe's recorded answers on real level pairs - and 4 to 40 times
  faster. The hub's Level diff tab and Project OPFOR's `opforcore.paths.leveldiff()` run it.
- **Map editor: the AI workspace's navigation** (round GR2, the Navigation Studio folded in). The navmesh view mode
  and the AI workspace draw a level's path data: NavKit's newest build and DICE's file side by side (read on a worker
  thread), the walkable ground coloured by island, sector, pitch, difference (NavKit's extra ground, DICE's ground
  NavKit lacks, ladder ground) or anomaly, and each file's outlines, portals, border portals, route graph, ladder
  ground and ladder links as layers. Panels: **Navigation** (the files and their figures, the colouring, the layers,
  the selected area or spawn, the path test, the anomaly overlay, Compare with a reference AI layer - `level:`,
  `xbox:` or a partition - with filters, the nearest NavKit node of a marker and Export JSON), **Islands and
  spawns** (largest first, the spawns off the main network, each to fly to) and **Navigation edits**. The **path
  test** (Shift+P, or "Path test from here" on the ground): each file's cheapest route between two clicks, or why
  there is none. **Navigation edits** in a project: walkable and unwalkable regions, seeds, unseeds, linked areas,
  route edges added or removed, traversals and notes, drawn with tools (and the AI ground menu), each an operation
  of the project's log (undo, redo, saved with the project); the level's side file (`navtune.json`) saved and
  reloaded; **Build with these edits** (`bfbc2.exe navkit apply`), Open the result, **Install into the game** and
  **Restore DICE's original**, each showing what it writes in the game copy's AI path data before it runs. The
  hover shows the area under the pointer. 25 new 3D colour tokens (`view-nav-*`, `view-ref-*`).
  `bfbc2-editor <level> --navmesh` opens a level's navigation; the hub's Levels tab has **Open navigation**.
- **Map editor: the Map Inspector's views** (round GR1). Properties' **All fields**: every field of the selected
  object, nested, with its type (`int32`, `float32 x16`, `bool`, `text`, `reference`, `array [n]`, `struct`) and its
  value as DbxEdit's XML writes it; references lead to their object. A **Links** panel: what the object references,
  what references it, and the event connections it sends and receives (from the level's `EventConnections`); a row
  goes to that object, Back returns. The links are drawn in the views in the new `view-link` colour (references
  solid, event connections dashed, arrows by direction; K shows or hides them). **Measure** (M): clicks add points;
  the last segment's length, horizontal distance, height and slope, the path's total, the height above the terrain
  and below the water. A project's links and fields are read from its edited partition. The plan for the other
  GUIs is `bfbc2-map-editor/EDITOR_ARCHITECTURE.md` section 9.
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
- **Map editor: view toggles** (round VW1). The View dropdown's **Overlays** draw what the level's data holds, each
  with labels: **Effects** (each placed effect's emitters and their spawn volumes), **Lights** (emitters that act as
  point lights: their radius and colour; BC2 has no spot lights), **Sounds** (emitters with their minimum distance -
  BC2's sounds have no maximum, the label gives the falloff slope - and sound areas with their fade edge), **Combat
  areas** (the out-of-bounds edge as a wall, the outer edge, the team they bound), **Capture areas** (the flags'
  polygon and height or capture radius, HQs) and **Spawn protection** (each spawn point's enemy-check radius). Three
  new view tokens: `view-fx-effect`, `view-fx-light`, `view-fx-sound`.
- **Map editor: player's-eye camera** (View dropdown, Alt+W): the 3D view at a soldier's eye height (1.6 m standing,
  1.0 m crouching, from the game's soldier physics) with the level's field of view (55 degrees, x 0.82 on the
  campaign), walking on the ground with W A S D (Shift sprints, C crouches); Esc returns to the camera before.
- **Map editor: collision per destruction state.** The scene reads each blueprint's Havok parts
  (`bc2_formats::havok::parts`, `Collision::parts`); the Display section's State picker also chooses the collision
  drawn (Collision, Visual + collision), gains the states between intact and destroyed, up / down arrows and Page Up
  / Page Down, and says when a blueprint's collision is one piece in its files.
- **`requirements.txt`** (round QW1): the Python packages pinned to the versions CI uses; CI installs it, and so
  does README's first step (`python -m pip install -r requirements.txt`).
- **One registry of environment variables** (round QW1): `bfbc2core.envvars` lists every `BFBC2_*`, `BC2_*`,
  `BF1943_*` and `OPFOR_*` variable the suite reads, with what it does (`python -m bfbc2core.envvars [--set]`); a
  quick-tier test fails on a variable read but not registered.
- **`BFBC2_STRICT_NATIVE=1`** (round QW1): a stale or missing `bfbc2.exe` is an error for the ported command lines
  instead of a fall-back to their Python. `build_native.py --fingerprint [module | bfbc2 | bfbc2-editor |
  mapexport]`; `bfbc2-editor --source-hash` and `mapexport --source-hash`.

### Changed
- **`bfbc2.exe` answers argparse's option abbreviations** as Python 3.13 does (`--fo` for `--format`, `--fo=v`, `--he`
  for `--help`) and its `ambiguous option: ... could match ...` error, in caches, convert, audio, shaderdb, renderqa,
  xdiff, dbxparity, portparity and texqa, instead of handing those command lines to the Python (round RT3). A
  `nargs='*'` option takes `--x=v`; texqa answers its help when it is the first argument. Single-dash abbreviations,
  `--` and texqa's argparse errors still go to the Python.
- **One rule when the Rust port cannot answer** (round RT3, audit AU2 A17; README "bfbc2.exe"): while a tool's Python
  is alive it answers (the command line handed to it, or a binding's Python reference); once it is retired, a plain
  error, never a silent fallback. The retirement error of `bfbc2.exe` names the round that retired the tool.
- **`hpdtool.parse` and `audio.read_wave` / `read_sps` / `read_snr` run in the Rust core only** (round RT3): a damaged
  file raises the error and message the Python raised (`ValueError` or `struct.error` for path data, `AudioError` or
  `struct.error` for sounds); a NaN in a file is read as Python's `struct` reads it instead of going to the Python.
- **`navkit.py`'s usage** says a level's navigation side file is made in the map editor's AI workspace (the
  Navigation Studio was retired in GR2).
- **Map editor: the simplified view draws the objects' render meshes** (round SV1). Trees, bushes and palms now look
  like trees, bushes and palms instead of "spike balls": the view draws each object's render mesh (the one the
  Game-accurate view draws) with its leaves cut out by their texture and a muted tint from its texture (vegetation
  green, rocks grey), instead of its collision. Invisible walls, occluders and see-through-only meshes (light shafts,
  vista smoke) are hidden at the start. New in the View dropdown: **Show objects as** Visual, Collision, Visual +
  collision, Wireframe or Bounds (`C` cycles them) and **Invisible objects** Hidden / Outline / Solid; the object
  menu's **Show as** and **Show every one like it as** (Visual, Collision, Both, Bounds, Hidden) for the selection or
  its blueprint; Properties' **Display** section: Show as, the mesh's **level of detail** (Auto / LOD 0 - n, where its
  mesh set has more) and a destructible object's **State** (Intact / Destroyed / All parts, from its blueprint's
  health states). These are view settings, kept per level with the editor's settings and never in the project's
  log; **Reset view overrides** clears them. A destructible building shows its intact parts (its mesh also holds its
  destroyed pieces). Clicking picks what is drawn; snapping and placing still use collision.
- **The map editor's own folder** (round FM1): `bfbc2-map-editor\` holds the editor's README (what it does, how to
  build and run it, its command-line switches, `mapexport.exe`, its tests) and its design document,
  `EDITOR_ARCHITECTURE.md` (moved from `bfbc2-map-inspector\` with its history; new section 10: the planned AI
  workspaces). The root README keeps a summary. `RETIRED.md` is the one record of retired tools, with the
  retirement checklist (`RELEASING.md` points at it). The editor's Help guide has an Export and install step; the
  hub's Xbox files tab runs `bfbc2.exe stfs` / `xiso` (the Python without the binary); `bfbc2.exe caches` and the
  Python no longer class OPFOR's deleted `work\import_backup` and `work\ledgertest`.
- **Map projects are format version 2** (round GR2: the navigation edits are new operation kinds). Version-1
  projects load unchanged (`bc2_levelwrite::project::migrate`, one step at a time; 1 to 2 changes nothing); a
  project from a newer editor is refused in words ("made by a newer editor version (project format N); this editor
  reads up to 2"), and an operation of a kind the editor does not know is named, not a bare JSON error. An editor
  before GR2 refuses version-2 projects the same way.
- **`bfbc2.exe culltree add`** reads the reference levels one at a time: about 80-110 MB instead of 1.65 GB, and
  0.6-0.9 s with a warm cache instead of 7 s. Its row cache is shared with `culltree.py` (either side's warmed cache
  serves both) and is pruned within the cache budget.
- **The last Python format paths run in the Rust core** (round F4; the same results, the Python kept as the
  fallback): `audio.convert` / `decode` whole (the ffmpeg glue included; `convert_many` converts side by side
  without the interpreter's lock), `texqa`'s texture readers (about 9 times faster), NavKit's offline sampler's
  collision assets (`asset_bodies`, its convex hulls; about 6 times faster), `hkpack.Packfile` as a handle (its
  dicts built only when read; `read_object` in Rust), `xenos.disasm`'s header (parse and listing about 8 times
  faster than the Python), `meshview`'s part reader and the cull tree tool's string table.
- **`python rust\build_native.py --download`** also downloads `bfbc2.exe` into `bin\` (checked by its SHA-256 and
  its source fingerprint), so a PC without Rust gets the command lines whose Python was retired (round RT1). A
  release made before RT1 has no `bfbc2.exe`: the module is installed alone and the download says so.
- **The map editor** prepares a level with `bin\bfbc2.exe prepare_level` only; without the binary the start screen
  says how to build or download it.
- **Retired command lines** (`graphstats`, `prepare_level`): a damaged or unusual input their Rust port does not
  read now ends with an error naming the retirement (exit code 1) instead of being handed to the Python.
- **Aim animations and navsample files** (`aimanim`, `navsample_offline.file_bytes`): read and written by the Rust
  core only. Inputs it refuses now raise (`AimError`, `ValueError`) where the Python twin used to answer some of
  them: an aim animation holding a NaN, samples of another dtype.
- **Shader databases are opened in the Rust core end to end** (round SD1; the same results, the Python kept as the
  fallback): `ShaderDb.parse`'s choice of byte order and layout and its messages, `pick_database` and
  `dx_from_name` are one Rust implementation shared by `shaderdb.py` and `bfbc2.exe shaderdb`; `load_levels` reads
  each level bundle's database list once instead of once per Dx.
- **`bfbc2.exe` answers `--help` and usage errors in a console and with `COLUMNS` set** (round RT2): `caches`,
  `convert`, `audio`, `shaderdb` (and its subcommands), `renderqa`, `xdiff`, `dbxparity`, `portparity` lay their
  help out at the console's width as Python's argparse does, instead of handing the command line to the Python;
  `texqa --help` too.
- **More of the suite reads and converts in the Rust core only** (round RT2: their Python twins retired against
  their recorded answers): the console converter's terrain, soot, mesh header / console layouts / decode / relayout,
  mesh set reader, Havok wrapper and class scan, texture levels / decode / check; the menu tools' BIGF reader and
  executable reader (`menucommon.parse_bigf`, `PEImage`); `navsample.load` (every version); the path-data mesh
  walk (`hpdtool.mesh_layout` / `mesh_coverage`); the EA-XMA parts, XMA2 RIFF and EA-XAS blocks of `audio.py`; the
  BF1943 importer's Havok port and MOPP code (`havokport`, `mopp`); `navtune`, `xmlinst`, NavKit's vehicle obstacle
  distance and offline sampler's terrain blocks. They now need the Rust core (`BFBC2_PURE_PYTHON` no longer switches
  them to Python). An input the core does not read (damaged data, a value of an unusual type) raises
  `bfbc2core.native.Refused` (a `ValueError`, and the module's own error class where it has one) with a message in
  the module's words; real files read as before.
- **Error messages kept:** the path-data mesh walk raises the Python's own `ValueError` / `struct.error` messages
  ("mesh trailer not found", "tile table at ...", reads past the end) from the Rust core, so `hpdtool coverage`
  prints them as before (now without the Python); the EA framing raises `AudioError`'s messages.
- **Each built artefact has its own source fingerprint** (round QW1): the module, `bfbc2.exe`, `bfbc2-editor.exe`
  and `mapexport.exe` are stale only when a crate they link (or a file they include) changes, so an edit to the
  editor or the renderer no longer makes the module and `bfbc2.exe` stale. `build_native.py --if-stale` rebuilds
  only `bfbc2.exe` when only its crates changed. The release tag stays the module's fingerprint.
- **A stale `bfbc2.exe` is said** (round QW1): the tools still fall back to their Python command lines, now with one
  warning per process naming the fix (`python rust\build_native.py`). `BFBC2_CLI_PYTHON=1` asks for the Python
  with no warning.
- **`mapexport.exe` and the map editor are found with a staleness check** (round QW1): Project OPFOR's
  `mapedit_install.py` refuses a stale `mapexport.exe` with the fix; the hub opens a stale editor with a note.
- **`caches --prune` deletes the bundle cache's superseded payloads** (round QW1), and the Rust core deletes a
  bundle's old payload when it writes the new one (a bundle rewritten at the same path left its old payload behind
  until the size cap reached it).
- **Tests** (round QW1): the map editor's, levelwrite's and the renderer's tests run once per quick tier (in the
  workspace's cargo test; `tests\test_editor.py` stays for a run by hand); each of those crates' integration tests
  is one test binary (`cargo test -p bc2-editor --test editor g2::` runs a file's tests); debug and test builds keep
  line tables only; the test runner counts a test that ran nothing as skipped, not passed (and names the tests that
  skipped parts), schedules each test with at least its last recorded memory peak, keeps matplotlib's font cache in
  `%LOCALAPPDATA%\bfbc2-tool-suite\matplotlib`, and forgets durations of tests that no longer exist.

### Removed
- **The Python command lines `aptrefs.py`, `xiso.py`, `culltree.py`, `stockmp.py` and `crashreader.py`** (round
  RT3). `bfbc2.exe` runs them alone (`python rust\build_native.py --download` fetches it; the hub and Project OPFOR's
  testrun use it); their libraries are `aptrefslib.py`, `xisolib.py`, `culltreelib.py`, `stockmplib.py` and
  `crashreaderlib.py` (`exeref.py` imports the last). A damaged or unusual input that used to go to the Python now
  ends with an error naming the retirement (exit code 1). `RETIRED.md` has the last commit with each.
- **The Python references `hpdtool.parse_reference`, `audio.read_wave_reference`, `read_sps_reference` and
  `read_snr_reference`** (round RT3), after their errors were ported; their recorded answers are the tests' reference.
- **`core\bfbc2core\viewer.py` and `pick.py`** (round RT3), the Python 3D view and its level picker that no tool has
  used since GR1 / GR2; with them `moderngl`, `glfw` and `imgui-bundle` leave `requirements.txt`.
- **LevelDiff.exe** (round EX1): `bfbc2-diff-workbench\` (`level-diff\LevelDiff.cs`, `build.bat`, its tests, and the
  folder's README of planned tools, which `DESIGN.md` 4.7 keeps). Run `bfbc2.exe leveldiff` (above) with the same
  arguments; its answers on recorded real level pairs are LevelDiff.exe's, byte for byte. `RETIRED.md` has the last
  commit with it.
- **Navigation Studio** (round GR2): `bfbc2-navigation-studio\studio.py` and `editing.py`, their tests, the hub's
  "Open in Navigation Studio" and its NavKit-file field, `tests\test_viewers.py` and `bench_sweep.py`'s
  `studio_load`. Everything it did is in the map editor's AI workspace (above; `EDITOR_ARCHITECTURE.md` 9.2), proven
  on its recorded answers. `overlay.py`'s headless exports (`--reference`, `--anomalies`) stay.
- **Python twins retired (round RT1):** the command lines `bfbc2-auto-navkit\tools\graphstats.py` and
  `rust\bc2-editor\prepare_level.py` (run `bfbc2.exe graphstats` / `prepare_level`; `BFBC2_CLI_PYTHON=1` no longer
  brings them back), and the Python references `aimanim.parse_reference` / `write_reference` / `to_pc_reference`
  (with aimanim's `use_native` switch) and `navsample_offline.file_bytes_reference`. The answers they gave stay
  recorded as the tests' reference data.
- **Python twins retired (round RT2):** the `*_reference` functions of `terrain`, `soot`, `mesh` (with
  `mesh.use_native`), `meshset.parse`, `havok` (`classes_reference`, `convert_wrapper_reference`,
  `use_native_wrapper`), `texture` (`levels`, `decode`, `check`), `menucommon` (`parse_bigf_reference`, `PEImage`'s),
  `navsample` (`load_reference`, `use_native`), `hpdtool` (`mesh_layout_reference`, `mesh_coverage_reference`),
  `audio` (`xma_parts_reference`, `xma2_riff_reference`, `_xas_blocks_reference`), `havokport` (14), `mopp` (5),
  `navtune` (3), `xmlinst` (2), `navvehicle.obstacle_distance_reference` and
  `navsample_offline.terrain_blocks_reference`. Their answers are recorded (`*_replay*.json` beside the tests,
  `tests\f3_golden_*.json`) and the tests check the Rust core against them (`tests\replay.py`).

### Fixed
- **O2's command-line test on a PC running Microsoft Defender** (round RT3): Defender's machine-learning scan took
  the menu group's stand-in executable for a trojan and blocked every read of it (`OSError 22`); the stand-in now
  carries a text mark no tool reads, and a blocked file is reported as an antivirus block.
- **`run_tests.py --help`** ran the quick tier (round RT3).
- **Map editor: the cull tree view mode** called its no-slot colour orange (the selection's); it is yellow. Objects
  with a slot of their own are now green, and the slots and spheres use the 3D view's status tokens (round VW1).
- **`shaderdb.ShaderDb.parse`** with an unusual kind (a float, a negative or a very large `dx`) raised `TypeError` /
  `OverflowError` from the Rust core instead of reading the file as the Python does (round SD1).
- **The map editor's game-accurate view opens OPFOR's WW2 levels** (round GW1): Guadalcanal and Iwo Jima failed with
  "no WeatherSettings / WeatherSystem partition under weather/wake_island/". The view now finds a level's weather
  the way the game does: the placed weather entity (`WeatherSystemEntityData`) names its `WeatherSystem` and
  `Settings` partitions, read wherever the level's bundles hold them, instead of looking for partitions named
  `weathersettings*` in a guessed folder (the WW2 levels name their settings after the level). A level with no
  weather data opens under a default sky and the status line says so, instead of failing.

### Removed
- **The Map Inspector** (`bfbc2-map-inspector/inspector.py`, round GR1): its 3D view, layers, search and Selection
  panel were already the editor's; its property tree, "referenced by", link lines and Measure are the editor's All
  fields, Links and Measure (above). The hub's Levels tab opens levels in the map editor; the folder keeps
  `EDITOR_ARCHITECTURE.md` and a README saying where each feature went.
- **`bfbc2-map-inspector\`** (round FM1): its README's table is `bfbc2-map-editor/EDITOR_ARCHITECTURE.md` 9.1, the
  plan moved to `bfbc2-map-editor\`. **The placeholder folders** `bfbc2-ai-behavior-simulator\`,
  `bfbc2-bot-ai-debugger\` and `bfbc2-tactical-map-authoring-tool\` (a README each, never built): they are planned
  map editor workspaces (`EDITOR_ARCHITECTURE.md` section 10). `RETIRED.md` has the last commits that had them.

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
