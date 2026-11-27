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
- **A merge that removes something bumps MINOR at once** (round QW2, audit AU3 AR-9): a command line, a subcommand
  or option, a Python function or module another project imports (Project OPFOR's), a Rust public API, or a file
  format or folder a tool reads or writes. The merge itself writes the new `VERSION` and turns **Unreleased** into
  that version (`RELEASING.md`, "A merge that removes something"), so Project OPFOR's version pin
  (`opforcore.paths.SUITE_REQUIRES`, at least one version and below the next minor) sees every breaking change.
- 1.0 is the full suite release (`ROADMAP.md`, priority 8).

The version lives in one place, the `VERSION` file at the repository's root. `bfbc2core.__version__`,
`bfbc2.exe --version`, `bfbc2-editor --version`, the editor's Help window and the hub's title all read it. How to
release a version: `RELEASING.md`.

Versions 0.1.0 to 0.8.0 were assigned afterwards (round P1, 2026-11-19), from the commit history and
`rust/BENCHMARKS.md`; they were not tagged at the time. Each date is the day the last change of that version
landed on `main`.

## [Unreleased]

## [0.36.0] - 2026-11-27

Round OR10 retires Project OPFOR's Python of the console Onslaught port's stages 5 - 7: OPFOR now needs this
version's `bc2_native.onsport_vehicles` / `onsport_soldiers` / `onsport_shaders`, so its `SUITE_REQUIRES` moves to
0.36.0 - 0.37.0.

### Added

- **The console port's stages 5 - 7 in Rust** (round OR10, `rust/BENCHMARKS.md` "OR10"; spec XCONS-ONS-5 - 7):
  `bc2_install::onslaught::vehicles` (vehicle spawns whose vehicle is console-only dropped, with the vehicle's own
  partitions), `soldiers` (the console's soldier classes and player kits as OPFOR blueprints on the PC soldiers) and
  `shaders` (the level's shader databases merged to cover every mesh it draws), through `bc2_native.onsport_*`;
  Project OPFOR's `console_vehicles.py`, `console_soldiers.py` and `console_shaders.py` keep their command lines, the
  install ledger's parts and the scratch clean-up. On the user's four console levels, stages 5 - 7 chained, every
  work and game file is the retired Python's byte for byte.
- **`bc2_convert::shadercmd`**: `shaderdb.py`'s `needs` and `merge` (with `load`, `level_meshes`, `read_need`,
  `check`, `verify_merge` and the report) as a library, moved from `bfbc2.exe shaderdb` (which keeps its command line
  over it, the same output), so OPFOR's stage 7 merges in process instead of running `shaderdb.py`.

### Fixed

- **OPFOR's console port stage 6 ran again**: `console_soldiers.py` read its class map from `console_onslaught.py`'s
  text, which round OR5 moved into Rust; since then the stage stopped at once (`ValueError: substring not found`). The
  stage now takes `bc2_install::onslaught::SOLDIERS`, stage 1's own map (XCONS-ONS-6).
- **The mesh index follows junctions** as `meshview._bundles` (Python's `os.walk`) does: a game copy whose level
  folders are junctions (a round's checkout, a test's stand-in) had those levels' bundles missing from the index
  `bfbc2.exe meshview`, the map editor and OPFOR's tools share.

### Added

- `spec/engine/eras.md` (round GM1, research): native eras for Project OPFOR. The four values are 0 BFBC2, 1 Vietnam,
  2 BF1943 / Pacific War (tag `PACIFIC`, label `BFBC2143`) and 3 BFBC1 (tag `BC1`, label `BFBC2Beach`). The file covers:
  - a decision (as 0, as 1, or the era's own resource), the reason and round GM2's patch for each of the game mod's
    41 instructions and the values derived from them;
  - the parser and setter changes (era 1's sign-in path unchanged; no gate for eras 2 / 3);
  - a `UISharedConfigAsset` per era, chosen at the unlock and weapon slot reads;
  - DICE's four-era main_nav, every movie's `extern.gameMod` test and the menus' contract for UI0 - UI7;
  - Vietnam's ownership checks kept byte for byte;
  - the risks, and what battlefield-opfor and OPFOR must change.

  21 rules: ERA-NAT-*, ERA-SITE-*, ERA-UI-*, ERA-LIC-2 / 3, ERA-RISK-*.
- `spec/probes/gamemodes.py` now also checks the 45 byte ranges GM2 will patch and the 10 Vietnam check ranges it must
  not touch. For each range it records the address, length and digest, and checks that the range ends on an
  instruction boundary and that no patch range overlaps a kept one. `core/tests/test_spec_probes.py::game_modes`
  checks 24 answers.

### Fixed

- `spec/engine/gamemodes.md` ERA-MOD-1: the unlock-tree reader 0x93D14F is in 0x93D120, which `extern.playerStats`
  reaches through 0x944D50. It is not in 0x93CF40.

## [0.35.0] - 2026-11-27

Round OR12 ("OPFOR in Rust", the Onslaught console port's stages 11 - 13) gives Project OPFOR new `bc2_native`
functions it now requires: this merge releases a minor version, and OPFOR's `SUITE_REQUIRES` moves with it.

### Added

- `bc2_native.onsport_cull` / `onsport_loading` / `onsport_hud` / `onsport_hud_movie` (round OR12):
  `bc2_install::onslaught::{cull, loading, hud}`, the console port's stages 11 - 13 - cull-tree places for the
  console layout's objects (`bfbc2 culltree add`), the console loading video and the description's game mode, the
  Onslaught stopwatch and Onslaught's texts in the PC HUD and loading-screen movies (`bc2_menu::graft` in process).
  Every work and game file is the retired Python's byte for byte on the user's four console levels (spec
  XCONS-ONS-11 - 13). A failing stage prints the lines it printed before the failure, as the Python did.

### Changed

- bc2-install's `onslaught` feature takes bc2-menu (stage 13's movie edits).

## [0.34.0] - 2026-11-27

Round OR11 retires Project OPFOR's Python of the console Onslaught port's stages 8 - 10: OPFOR now needs this version's
`bc2_native.onsport_markv` / `onsport_input` / `onsport_deps` (and the seat rules' `onsport_role_inputs` and
`onsport_seat_edits`), so its `SUITE_REQUIRES` moves to 0.34.0 - 0.35.0. Round GM0, merged before it unreleased, is in this version too.

### Added

- **The console Onslaught port's stages 8 - 10 in Rust** (round OR11, "OPFOR in Rust", `rust/BENCHMARKS.md` "OR11"):
  `bc2_install::onslaught::markv` (Valparaiso's Mark V patrol boat: its console physics, meshes, mesh set and
  textures converted, its nine materials cloned from PC donors into the level's shader databases, its spawn back),
  `onslaught::input` (PC input for the console seats: the PC version's seat, else DICE's role rule; the Mark V entered
  from 5 m) and `onslaught::deps` (every partition the level references but does not load copied in from the stock PC
  level bundles), through `bc2_native.onsport_markv`, `onsport_input` (with `onsport_seat_edits` /
  `onsport_role_inputs` for the shader audit) and `onsport_deps`. OPFOR's `console_markv.py`, `console_input.py` and
  `console_deps.py` keep their command lines and the install ledger; their work files are the retired Python's byte
  for byte on the four console levels (spec XCONS-ONS-8 - 10).
- `bc2_convert::havok::convert` / `classes_of` (a console Havok resource as the PC's, the executable's hkClass rows as
  the converter's classes), `bc2_convert::meshconv::pair_by_kind` public, `bc2_formats::shaderdb::merge::ArenaStore`
  (a merge store that owns its arena).
- `spec/engine/gamemodes.md` (round GM0, research): how game modes and eras are authored - every reader of
  `GameModeData`'s fields and of the game mod in `BFBC2Game.exe` (33 + 8 instructions, the 26 mode-id compares, the
  mode-kind object, the catalogue keyed by ModeID), each era's resources and licence checks, and a design for a new
  mode and a new era with the patch set each needs (24 rules GMD-* / ERA-*).
- `spec/probes/gamemodes.py` (game tier, read only): DICE's game modes, descriptions, matchmaking data and per-era
  shared config, and the executable's game-mod and literal sites; `core/tests/test_spec_probes.py::game_modes` checks
  its 19 answers.

### Fixed

- The console port's stages 8 and 10 no longer take data from the WW2 levels Project OPFOR installed (their folders'
  `opfor-<name>-installed.json`): the Mark V's missing textures and the referenced partitions come from the stock PC
  bundles only, and a redo of either stage builds on the bundle the install ledger hands it, not on the game copy
  holding its own earlier output (spec XCONS-ONS-8, -10).
- Spec GAME-MODE-6: the game mod is parsed from the loaded level's `MatchmakingModID` at the loading screen and the
  HUD's start, not from a stored setting (ERA-MOD-2); GAME-MODE-4 / -5 name the `singleplayer` category and the nine
  `conquest` compares.

## [0.33.0] - 2026-11-27

Round OR14 ("OPFOR in Rust", the picture and menu leftovers) moves Project OPFOR's WW2 and Onslaught menu tools onto
new `bc2_native` APIs (`AptMovie`, `LocFile`, `pacific_main_nav` ...), which OPFOR now requires: this merge releases
a minor version, and OPFOR's `SUITE_REQUIRES` moves to 0.33.0 - 0.34.0.

### Added

- `bc2_native.AptMovie` / `AptError` / `apt_split_image_part` (round OR14): `aptgraft.py`'s movie API over
  `bc2_menu::graft` - loading and writing a movie (`GRAFT_LAYOUT`), its timelines' labels, places and geometry, and
  the edits OPFOR's icon and crosshair tools make (`add_label`, `add_label_alias`, `add_icon_frame`,
  `rename_constants`, `add_crosshair` with image tiles). New in `bc2_menu::graft`: `icon` (`add_icon_frame`, label
  hiding), `tiles` (`split_image_part`), `consts`, `add_crosshair_tiles`.
- `bc2_native.pacific_main_nav` / `pacific_main_root` / `apt_dump_text` (`bc2_menu::pacific`): Project OPFOR's
  Pacific War main-menu edits, the bytes `menu_install.py`'s Python wrote (spec MENU-ED-9).
- `bc2_native.LocFile` / `loc_languages` / `loc_load` (`bc2_menu::loc`, out of `bfbc2 bc2loc`): the localization
  files as `bc2loc.LocFile` read and writes them (spec MENU-ED-10).
- `bc2_native.pictures_side_silhouette` / `pictures_side_ties` / `pictures_polygon` (`bc2_convert::pictures::side`,
  `polygon`): a vehicle mesh's DICE-style side silhouette with Pillow's polygon fill (spec XWW2-PIC-5).
- `bc2_convert::eigh3` (`eigh`, `principal_axis`), `TexWriteJob.axes()` / `encode()`, `texwrite_principal_axes`.
- Parity scripts `rust/parity_silhouette.py`, `parity_texwrite_axes.py`, `parity_menu_movies.py`,
  `parity_pacific_menu.py`.

### Changed

- **texwrite's DXT encoders compute their principal axes in Rust** (`bc2_convert::eigh3`): LAPACK `dsyevd` as NumPy
  2.5 with scipy-openblas computes it on the oracle machine, the same bits on any processor (spec XWW2-PIC-6);
  `texwrite.py` no longer calls `numpy.linalg.eigh` on the native path. Every encoded block is the same as before;
  a texture is encoded about 15 x faster. The editor's award and loading-screen exports
  (`bc2_levelwrite::awards`) use the same axes instead of their own Jacobi solver, so their DXT blocks may differ
  in rare blocks from 0.32.0's.
- **One Pillow resize**: `bc2_menu::pil::Rgba::resize_lanczos` runs `bc2_convert::pictures::resample` (its own copy
  retired); `bc2-menu` depends on `bc2-convert`.
- The console readers' tests (`bfbc2-console-readers/tests`) run on the Rust core against answers recorded from
  `stfs.py` / `xisolib.py` (`console_readers_answers.json`, `record_console_readers_answers.py`); the two twins can be
  deleted in PR1c.
- `aptgraft.py` and `bc2loc.py` are test references only: no Project OPFOR code imports them.
- An APT movie cut short is refused with the reference's words (`... runs past the end of the file`, an `AptError`)
  by the Rust model too.
### Changed

- **The specification's rule index is split, one shard per spec file** (round SPLIT1, `rust/BENCHMARKS.md`
  "SPLIT1"): `spec/rules.json` (977,029 bytes, 23 KB under `tests/test_repo.py`'s 1,000,000-byte limit) becomes
  `spec/rules/<area>/<file>.json` beside each markdown file (`spec/engine/gameplay.md` -> `spec/rules/engine/gameplay.json`;
  90 shards, the largest 40,538 bytes), written with sorted keys and the rules in heading order by
  `python spec/trace.py --emit`. The shards' union is the old file's 1,599 rules, rule for rule. `--check` now also
  fails a hand-edited, stale, missing or leftover shard and any shard over 800,000 bytes; `tests/test_repo.py` warns
  (without failing) of any committed file over 80% of the limit. A branch that still writes `spec/rules.json`:
  delete it after merging and run `--emit`.

## [0.32.0] - 2026-11-27

Round OR9 retires Project OPFOR's Python of the console Onslaught port's stages 2 - 4: OPFOR now needs this version's
`bc2_native.onsport_linux` / `onsport_mode` / `onsport_loadset`, so its `SUITE_REQUIRES` moves to 0.32.0 - 0.33.0. Round SPG2-rest6, merged before it
unreleased, is in this version too.

### Added
- **Specification (round SPG2-rest6), read from the executable and checked on DICE's data** (`rust/BENCHMARKS.md`
  "SPG2-rest6"; 1,587 -> 1,599 rules):
  - the undergrowth's sway: every DICE vertex program with `windParams` moves x and z by
    sin(((x + y + z) x 1000 + w.x) x w.y) x max(y, 0) x w.z (VEG-UG-10, verified by a new check in
    `core/tests/test_spec_probes.py`); `UndergrowthMaxHeight` is how far a cell's bounding box reaches above its
    highest instance, for culling and the fade (VEG-UG-11);
  - the mesh and sample-texture particle evaluators (FX-PROC-6), the quad vertex fill and the colour, transparency,
    alpha, texture-frame, size and clip-scale processors it evaluates (FX-PROC-9), the force vectors (FX-PROC-10),
    the emitters' lifetime step and the mesh particles' spawn (FX-PROC-11); FX-PROC-8's sizes corrected;
  - what each menu FSCommand case does (MENU-RT-8); the award criterion objects' factory and its seven classes
    (PERS-AWD-8, verified), event 0x51 as `StatEvent_Suicide` (PERS-AWD-6);
  - the compiled anim-tree nodes' header, the animation node's and every type's fields (ATREE-NODE-2 - 4, verified),
    block B's cycle distance and angle (ATREE-BLK-5) and block C's five property curves (ATREE-BLK-6); the probes
    `terrain.py`, `effects_chain.py`, `persistence.py` and `resources.py` hold the new counts.
- **The console Onslaught port's stages 2 - 4 in Rust** (round OR9, "OPFOR in Rust"): `bc2_install::onslaught::linux`
  (the console gameplay put on the PC map's Linux server level: the merge, the renamed Linux files, the cull slots as
  the Linux level's), `onslaught::mode` (the Onslaught game mode from PC Conquest, the console spawn manager, every
  flag takeable, the doorway pre-destructions left out) and `onslaught::loadset` (the level's manifest blocks without
  PC mp2_common, the files it takes only from there copied in), through `bc2_native.onsport_linux`, `onsport_mode`
  and `onsport_loadset`. OPFOR's `console_linux_base.py`, `console_mode.py` and `console_loadset.py` keep their
  command lines and the install ledger; their work files are the retired Python's byte for byte on the four console
  levels (spec XCONS-ONS-2 - 4).
- `bc2_pyrt::math`: CPython's `math.dist` (its compensated `vector_norm`), `x ** 2` and `x ** y` bit for bit.

## [0.31.0] - 2026-11-27

Round OR8 retires `effectport.py`'s Python (its emitter index `EmitterIndex`, `templates_reference`, `REFERENCE`,
`_read_many`, `port`'s `shared` / `plan` arguments) and Project OPFOR's sound porter, a Python API Project OPFOR
imports: this merge releases a minor version, and OPFOR's `SUITE_REQUIRES` moves to 0.31.0 - 0.32.0. Round TG4, merged before it unreleased, is in this version too.

### Added

- Round TG4: a Create Level project places its base level's forests, middlegrowth, rocks and natural props again on
  the new terrain (the form's **Vegetation** box, on by default; project format 14's `terrain_import.vegetation`), as
  DICE's multiplayer levels of the biome place them: their density by distance outside the playable area, their
  clustering, slope and height preferences and poses measured on DICE's 19 multiplayer terrains
  (`bc2_terragen::vegetation`, `bc2_levelwrite::vegetation`; spec TGEN-VEG-1 - 4). The export writes them as model
  group member edits, so the groups' leaves, physics and cull trees are made as for any member edit.
- Round TG4: the base's tiled paint layers (paths, dry grass, rubble ...) are laid on a new terrain as the base lays
  them on its own, with their paired physics materials (TGEN-MAT-3), for bases whose layers are one kind each
  (MP_005, MP_006, MP_007).
- Round TG4: `bc2_levelwrite::terrainpaint::layer_weights` (a terrain's tiled layers' weights read back from its patch
  maps and atlas), `newterrain::view` and the editor's `newterrain_view`.

### Changed
- **The WW2 effect porter and sound porter run in Rust** (round OR8, "OPFOR in Rust", the leftovers):
  `bc2_convert::effectport` (the walk, the emitter documents' level copies, the template search over an index of
  BFBC2's emitter documents kept as JSON in the `effectport` cache, the partition ports side by side, the report) and
  `bc2_convert::soundport` (Project OPFOR's `wake_level.port_sounds`: the sound partitions, the donor pool, the fields
  DICE sets, the waves converted side by side, `IsStreaming` false for in-memory audio), through
  `bc2_native.effectport_port` / `effectport_port_many` / `effectport_walk` / `soundport_port`. `effectport.py` keeps
  its API as a wrapper and gains `port_sounds`, `sound_fills`, `not_streaming`; every WW2 effect and sound port gave the
  retired Python's files, report lines and records (spec XWW2-FX-1, XWW2-SND-1 - 2). A wave that does not convert is
  reported with the Rust converter's reason (`wavport::convert_checked`).
- A missing effect template now raises `dbxport.MissingTemplate` naming it (spec XWW2-FX-2), where the Python raised
  the reader's `DeletedEntry`.
- Round TG4: a Create Level project opens on its new terrain (it showed the base's), so the Terrain workspace's
  sculpt and paint work on the new ground and the export lays the strokes on it (TEDIT-NEW-4). Project format 14; a
  format-13 project with terrain strokes keeps them refused (they were made on the base's terrain).

### Removed
- `effectport.EmitterIndex`, `templates_reference`, `REFERENCE`, `EMITTER_BATCH`, `cache_dir`, `_read_many`; the
  variable `BFBC2_EFFECTPORT_REFERENCE`. The index's old pickles (`emitters-*.pkl`) are left to the cache pruning.

## [0.30.0] - 2026-11-27

Round OR7 retires Project OPFOR's Python rebuild runner (`rebuild_all.run_chains` / `run_step`, `stepstamps.Tracker.begin` / `finish` / `why`): OPFOR now needs this version's `bc2_native.rebuild_run`, so its `SUITE_REQUIRES` moves to 0.29.0 - 0.30.0.

### Added
- **Project OPFOR's WW2 rebuild runs as a step graph** (round OR7, "OPFOR in Rust": `rebuild_all` as a step graph):
  `bc2_install::rebuild` (feature `rebuild`) and `bc2_native.rebuild_run` / `rebuild_graph` / `rebuild_derive` run
  OPFOR's 55 rebuild steps, each as soon as the steps it waits for are done, side by side within a memory budget
  (each step's last measured peak, the commit of all its processes through a job object); a step whose code, inputs
  and outputs are as recorded is skipped. Which step waits for which comes from the step records (a file one writes
  and the other reads, edits, writes, or lists the folder of) and OPFOR's declared graph (`rebuild_graph.json`, derived
  from a full rebuild's records, and the folders a step reads whole). The records (`stepstamps.py`'s `finish`, `why`,
  `stale_inputs`, the hash cache) are made and judged in Rust; tracker version 4 holds each step's peak memory and
  time. After a run, two steps the new records order that ran at once fail the run. Spec XDEP-STEP-8 - 10 new,
  XDEP-STEP-4 implemented.

### Fixed
- **The rebuild tracker no longer records 50,000 unrelated files as a step's inputs** (round OR7, spec XDEP-STEP-10):
  the cache budget's prune of `wake_install`'s indexes was handed all of `work\ww2`, and the hook kept the first
  50,000 files of a folder too big to stamp as read. Each level's chain looked to read the other levels' outputs, so
  the chains could not run side by side.

## [0.29.0] - 2026-11-27

Round OR6 moves the map export's install into the suite and changes `bc2_editor::export_ui::install_command` (a Rust
public API), so this merge releases a minor version (`RELEASING.md`, "A merge that removes something"). Project
OPFOR's `SUITE_REQUIRES` moves to 0.29.0 - 0.30.0: its `mapedit_install.py` runs `bfbc2 install mapedit`.

### Fixed
- **The shader translator's forward emitter refuses a bool-constant `cond_exec`** (round XD2), as the nested emitter
  does; it had run the block unconditionally. No program of the corpus has one, so no translation changed.
- **An export install no longer rebuilds the other exports from their live stage folders** (audit AU4-DS-12): each
  install keeps its stage's description, texts and pictures in `Package\levels\<lv>\opfor-mapedit-stage\`, and
  every later install or revert builds that level's share of the shared patches from it (spec XINST-MAP-5); an
  install from before keeps working from its stage folder, with a line saying so.
- **A kill at any write step of an export's install or revert is finished by the same command** (spec XINST-MAP-6):
  the level's record is its folder's first write and a revert's last removal (a kill between the folder and the
  record used to leave a folder the install refused as "not an exported level").
- **DICE's `package.mft` keeps its last blank line under an export install** (spec XINST-MAP-7): only the levels'
  blocks change. The build's scratch (`work\mapedit\<lv>\shared`, `_revert`) is removed when a run goes through
  (XINST-STG-5).

### Changed
- **The map editor installs an export itself** ("OPFOR in Rust", round OR6): Install..., Install now and
  Uninstall... run the map export's install in process (`bc2_install::mapedit`) instead of Project OPFOR's
  `mapedit_install.py` - no Python needed. The dialog shows the `bfbc2 install mapedit` command that does the same;
  `bc2_editor::export_ui::install_command(opfor, step, target)` and `install_step` replace the old
  `install_command(opfor, args)`.
- `xenos_golden_game.json` re-recorded: the 20 Xbox programs the forward emitter now translates (all skinned vertex
  shaders; 4 of BF1943's, 16 only in BFBC2's Xbox levels), each proven exact against the interpreter with both sides
  of its if / else run (round XD2).
- `bfbc2 xdiff --db <name>` with a name other than `wake` no longer stops with `KeyError`: the name is looked up as a
  BF1943 level or resource (or `bfbc2:<level>`).
- **Map project format 13**: a Create Level project records its base level and, for Generate, the generator's
  settings (`terrain_import.generate`); older editors refuse it. A G4 height-map project without a base shows its
  page and asks to be made again.
- **Create Level's map size must be 1024 or 2048 m**, the height map's samples a side (TGEN-REQ-2: 1 m samples).
- **The map editor's navigation edit tools and placing are view tools** (round E4-rest2, PR1b-0's leftovers): the AI
  workspace's nine edit tools are the views' tool while chosen (`Tool::Nav`; choosing Select, Move, Rotate or another
  tool turns them off, and the gizmo is not shown while one is chosen); placing from the content browser, a tile or
  the picker is the views' tool while it lasts (`Tool::Place`), the tool chosen before comes back once the object is
  placed or dropped, and choosing another tool drops it. `NavState`'s drawing fields moved to `Tools::nav`
  (`NavDrawTool`), `BrowserState::placing` to `Tools::place.placing`. A Package level's preparation in the editor runs
  on `Task` (a panic is its error line); only the pipe readers keep bare threads.
- **Taking out a Rush stage** also hands what the next stage's objects did to it (a hold-back timer disabling its
  combat area) on to the stage before's counterparts (round E4-rest2), so a stage inserted and taken out gives the
  level back connection for connection.
- Specification (round SPG2-rest5): MENU-LOAD-1 no longer says the Flash loading roots are named by no data
  (MENU-LOAD-2), and round SPG2-rest4's "slot 12 of vtable 0x142B4C8" is the main menu's third vtable; FX-PROC-4's
  inferred "the input is an offset" is the `EmittableField` enumeration.
- Specification (round SPG4-rest): ENG-TI-1's class member table (0x1C25870) was 16 descriptors in: the table is
  0x1C256F0, an argument of the class's registration (RTTI-REG-1); HUD-KILL-1's "the killed soldier's camera turning
  to the killer, driven by code" is withdrawn (the kill cam is the level's spectate entities, HUD-KILL-2); OCC-TRI-1's
  "more records than triangles" is the padding to an even count (one 192-bit part mask a triangle).
- Specification (round SPG3-rest): round SPG3's guess that the three mouse sliders are the infantry, land and air
  sensitivities is replaced by the code's names (INP-SET-2); SET-CLI-1's finder has 41 callers, not 40; SCR-DEV-1
  points at SCR-DEV-2 for the document server.
- **NavKit: DICE's generator rectangle leaves out the player's alternate spawns** (round NKS-deep D, spec KYG-RECT-1
  corrected): `navregress.spawn_bounds` and `bc2_navbuild::inputs::spawn_bounds` box the soldier and vehicle spawns
  without `AlternateSpawnEntityData`. All 52 campaign sides of DICE's ground now end at the box (45 before: the "7
  sides where the ground ends on its own" were alternate spawns). New maps built without DICE path data get the
  corrected box.
- **navregress: the old exact-match measures no longer fail a level** (round NKS-deep D): `nodes_same`, `edges_same`
  and `costs` are listed when they change (`INFO`); the pairing measures of round NKS-deep E judge the graph. The test
  harness's parity directions (`testharness/runs.py`, `bfbc2 harness`) take the same lists.

- **NavKit: object lips need flat floor on both sides** (round NKS-deep D, the user's ruling 2026-11-27):
  `navwalk.LIP_OBJECT_FLAT_ALL` / `WalkSettings::lip_object_flat_all` **on**: a steep object sample between walkable
  ground is crossed only where the floor on both sides is within 15 degrees of level (door sills), at any steepness.
  On the 15 regression levels `ai_pairwise` 95.57 -> 95.89 %, `nodes_paired` 23.30 -> 23.40 %, `over_connection`
  15.74 -> 15.69 %; the dips (sp_005_b `ai_pairwise` -0.3, sp_006 `outline` -0.11, three node measures -0.05 to
  -0.07, `ground` -0.004) are named exceptions in `spec/transforms/navkit.md`. **Path data NavKit builds changes**;
  navregress's reference `regression` re-recorded (the old one kept as `regression_before_2026-11-27_nksdd.json`).

- **XVAL-EDIT-5** (round E5-rest) no longer refuses every visual paint: the export writes it, and refuses only a paint
  it cannot write (with why); the editor's edit-time finding for paint strokes is gone.

### Added
- **`bfbc2 install mapedit plan | apply | revert | status`** (round OR6, tool group `mods`; `rust/BENCHMARKS.md`
  "OR6"): Project OPFOR's `mapedit_install.py` in Rust (`bc2_install::mapedit`, feature `mapedit`), on the install
  ledger as owner `mapedit` - the same game files, level records, ledger records and printed lines as the Python on
  scratch copies of the real shared files through plan, apply, revert, re-apply, a second export, a level replaced,
  one out of two and all out (spec XINST-MAP-8). OPFOR's script now runs it; its Python twin is retired.
- **`bc2_install::stages`**: `opforcore.stages`' whole-file operations (resume, the old markers' refusal,
  `check_install`, `install`, `check_revert`, `revert`, `source`) for an installer owning shared files - what IN1's
  installer builds on; `bc2_levelwrite::installpatch`: the overlay's texts and the awards bundle's pictures from
  stages (`mapexport overlay | awards` and the install share it), and `LevelWrite`, the install's check and patches
  (`bc2_install::mapedit::Patches`: the install itself does not depend on the export's crates).
- **Map editor: Create Level writes the new level's terrain** (rounds E7 and TG3; spec `transforms/terrain_edit.md`
  TEDIT-NEW-1 - 3, XVAL-TERR-4): "From a height map" (a 16-bit PNG or RAW of 1024 / 2048 samples; the map is as many
  metres across) and **Generate** beside it (the terrain generator's biome, size, seed, border, water share, water
  level, rough relief, objective pads and ring, `bfbc2 terrain generate`'s settings) make a project on a **base
  level** (a multiplayer map: its mode layer, objects, weather and its terrain's look) that opens like any project.
  Its export writes the new terrain under the level's own name (`Terrains/<LV>/<LV>`: height field, ring, material
  map, water, visual terrain, colour / occlusion map, the base's patch shaders cloned into its databases, its terrain
  partitions) in the level's **own terrain bundle**, registered without the base's (TGEN-REQ-5), and its path data is
  sampled on the new ground. `mapexport check` reads it back (XVAL-TERR-4). DICE's 30 main height fields written out
  as height maps import as DICE's samples, bounds and min / max tree. Nothing is loaded in game yet.
- `bc2_levelwrite::newterrain` (the height-map import and the new terrain's entries), `pathdata::bot_measures`
  (connectivity and coverage of path data on a playable area), `bc2_terragen::visual::clone_database`.
- **The map editor inserts a Rush stage between two** (round E4-rest2, spec GAME-RUSH-12): the Gameplay panel's
  **Insert after** on a stage, then a click on the ground, puts a copy of the next stage there, played between the
  two (`bc2_levelwrite::rushstage::insert_stage`; one undo step, the `connect` operation of project format 10). The
  wiring between the two stages is split as DICE's chain is read; every stage plays as before. Checked on every stock
  Rush and Squad Rush level between every two stages, and Port Valdez with a stage inserted exports and passes the
  stage check.
- **Specification (round SPG4-rest): every typeinfo record named with its layout, the run-time behind SPG4's
  data-side files and the resources' opaque insides, read from the executable and checked on DICE's data**
  (`rust/BENCHMARKS.md` "SPG4-rest"; 1,532 -> 1,571 rules; the census's typeinfo records with a rule 816 -> 1,900):
  - `spec/engine/typeinfo/` (four files, 31 rules RTTI-*): how each of the 1,900 records is registered (RTTI-REG-1)
    and its 24-byte member descriptors (RTTI-MEM-1); tables of every record's parent, size, own members with their
    run-time offsets, or an enumeration's numbers, by module (written by the new `spec/probes/typeinfo.py` from the
    new `spec/typeinfo.json`; the quick tier's new `spec-typeinfo` keeps them current);
  - DICE's DBX fields against the run-time members (RTTI-DBX-1, 2): 118 field names are the content pipeline's and
    the game cannot read them - among them the static models' pathfinding switches (`ExcludeFromPathfinding`,
    `ForbidPathfinding`, `AllowPathdataSeeds` ...: they act only through the path data) and the AI's whole sensing
    template (`SeeingDistance`, `HearingDistance`, `ViewAngle` ...);
  - which run-time entity class each data class's factory makes, from the constructors (RTTI-FAC-1, 173 classes);
  - the character controller's nine states, their ids and transitions (SOLD-PHYS-2); spawning on a squad mate: the
    checks and the 13 positions tried (SQD-PLAY-2); the kill cam is the level's spectate entities gated by
    `Server.KillCam` and hardcore (HUD-KILL-2); the campaign save's `header` and `body` streams (CAMP-SAVE-1); the
    sound graph nodes' factories and their enumerations' numbers (SGR-NODE-2);
  - the occluder's triangle block is one 192-bit part mask a triangle (OCC-TRI-1), the soot's flag words pick its
    vertex declaration and its vertices name a part slot and a box (SOOT-FLAG-1, SOOT-SLOT-1), the impulse
    responses' partitions are block-scaled spectra (IR-PART-1);
  - the new probe `spec/probes/runtime.py` and `resources.py`'s new counts (the game tier about 2 minutes longer).
- **Specification (round SPG3-rest): settings, input, scripting and the asset graph, read from the executable and
  checked on DICE's data** (`rust/BENCHMARKS.md` "SPG3-rest"; 1,524 -> 1,532 rules):
  - `GameSettings.bin` is 11 typed lists, one per settings category, of the settings that are not user-exposed (the
    INI holds the rest; SET-BIN-1, checked against a file the game wrote); a number in `GameSettings.ini` that does
    not parse becomes 0, an empty value keeps the default, an unknown `DxVersion` selects no renderer (SET-PARSE-1);
  - no `-<Object>.<Field> <value>` command-line option sets an engine setting: the settings registry's own text path
    is never reached in the retail build (SET-CLI-2);
  - the controls page's sliders: `Sensitivity0` - `2` are the mouse's sensitivity, yaw factor and pitch factor, and
    the invert switches' settings (INP-SET-2);
  - a level's game setting entity inverts every player's vertical look over the network; the console command entity
    does nothing (SCR-CMD-2); of the 107 entity classes DICE's level graphs do not use, 45 make no entity at all and
    62 do, with their input events (SCR-EVT-3); the debug document server (`/settings`, `/stats` ...) is never made
    (SCR-DEV-2);
  - the references held inside resources (mesh sets, shader databases, Flash movies, terrain and water visuals) and
    the voice-over waves the line banks name (XDEP-RES-2); the probes `options.py`, `scripting.py` and
    `dependencies.py` hold the new counts (the game tier about 20 seconds longer).
- **Specification (round SPG2-rest5), read from the executable and checked on DICE's data** (`rust/BENCHMARKS.md`
  "SPG2-rest5"; 1,506 -> 1,518 rules):
  - the render settings that scale the undergrowth (`VisualTerrainSettings`: density, view distance and its fade,
    the cell grid, the instance capacity; VEG-UG-8) and where `WindScale` goes (the undergrowth shader's
    `windParams`, with the clock and `UndergrowthWindSpeed`); the entry's second flag byte has no reader (VEG-UG-9);
  - the particle evaluators' formulas (box, random, random XYZ, rotate vector, sphere, constant, the colour
    interpolation and the polynomial operator, whose operands run from the linear term up: FX-PROC-6), the
    evaluator inputs (only `EfNormTime` is used; DICE's 40 `EfVelocity` processors have no evaluator;
    `EmitterParameter` picks one of the particle's four parameters: FX-PROC-7, verified) and the particle record and
    the spawn and force processors (FX-PROC-8);
  - the loading screen's root is the game mode's `LoaderName` and the HUD's its `HudRootName` (MENU-LOAD-2,
    verified); the screens' FSCommand handlers and the UI manager's shared one, 4 of the movies' 133 commands answered
    by nothing (MENU-RT-7, verified); no code sets a menu event's delay (MENU-EVT-5);
  - when award criteria are reset by `CriteriaType` (PERS-AWD-6) and that every award code's `<Code>_00` / `_01` is a
    `Final` value template in DICE's templates (PERS-AWD-7), with a new game-tier probe `spec/probes/persistence.py`;
  - the 26 compiled anim-tree node types named by the executable (ATREE-NODE-1) and blocks B and C as one entry per
    animation node, B's first float the animation's length (ATREE-BLK-4).
- **NavKit: four walkability and placement switches, measured for the user's ruling** (round NKS-deep D,
  `rust/BENCHMARKS.md` "NKS-deep D"): `navregress.RECT_ON_DICE_LEVELS` (the rectangle in place of DICE's extent on
  campaign levels), `navwalk.TERRAIN_FILL` / `Switches::terrain_fill` (XNAV-BLD-5: the height field's surface in
  narrow sampling holes under thick objects; sp_003_b `ai_pairwise` 74.4 -> 93.4 %), `navwalk.MESH_CORNERS` /
  `Switches::mesh_corners` (KYG-NODE-9: the placement's probes turn cell corners through a side cell) and
  `navwalk.LIP_OBJECT_FLAT_ALL` (on by the ruling, above). The other three stay off: a narrower `terrain_fill`
  (only under the objects the sampler got stuck on) and `mesh_corners` at area boundaries are queued.
- **Spec: the autoseed step and the map builder's derived limits** (round NKS-deep D): KYG-SEED-3 (autoseeds are
  terrain, within the slope limit and open to the sky for 100 m; every sector is probed), KYG-PAR-4 (HoleMax is
  -StepMax; the step and slope limits as ratios).
- **The map editor paints the terrain's other layers** (round E5-rest, spec TEDIT-VT-2, TEDIT-MM-2, TEDIT-RING-1,
  XVAL-TERR-3, TEDIT-SH-1 and TEDIT-OCC-1 implemented): the **Paint layer** brush lays one of the terrain's visual
  layers into its patch cells and mask tiles (a patch shader the terrain lacks is cloned with its textures
  retargeted; 7 of `Dist\win32`'s 23 terrains give each layer one kind and can be painted, the others are refused in
  words), optionally with its **paired material** (measured on the terrain's own cells: MP_005's Asphalt -> Tarmac
  85 %); the **Shade** brush and **Re-shade sculpted area** edit the colour / occlusion map (only the blocks under the
  edit rewritten); **Widen range** raises the height range with the ground kept; **Add material** adds a material of
  the level's grid to the material map; sculpts reach the **sub-terrain ring**, stitched as DICE stitches its rings;
  the material map is drawn around the brush; the game-accurate view follows a stroke while it is made. The export
  writes all of it (`mapexport check`: XVAL-TERR-2 / -3). Project format 12 (`terrain-range`, `material-add`, colour
  and ring strokes, strokes over several rasters).
- **The shader translator's differential test covers vertex shaders and every Xbox database** (round XD2):
  `xdiff.py --vs [N ...]` runs vertex shaders through `xenos_exec.py` (now a vertex-shader interpreter too: vfetch,
  oPos and o0-o15 exports) and draws each output of the translation (oPos and every interpolator) as a pixel of its own;
  `--db` takes any BF1943 level (`iwo_jima_s`), a database's resource name, or `bfbc2:<level>` (a BFBC2 Xbox level's
  database from Project OPFOR's unpacked Xbox copy). Results now list each jump's lanes (taken / not taken) and the CFs
  no lane reached; lanes that read `c[k + a0]` outside the array of `c[k]` are left out of the comparison (counted as
  `out_of_range`). `bfbc2 xdiff` takes the new option.
- Spec XSH-CF-1: the forward emitter for overlapping forward jumps, with its evidence.

## [0.28.0] - 2026-11-27

Round OR4 retires `meshview.page()` and `PAGE` (a Python API Project OPFOR used), so this merge releases a minor
version (`RELEASING.md`, "A merge that removes something"; OR5 merged first with 0.27.0). Project OPFOR's `SUITE_REQUIRES` moves to 0.28.0 - 0.29.0.

### Added
- **Project OPFOR's WW2 material grid and pictures in Rust** ("OPFOR in Rust" stage 4, round OR4), each as the
  retired Python made it:
  - `bc2_install::levelbuild::materialgrid` (`bc2_native.ww2build_grid_roots / _names / _redirect / _merge`): the
    merged grid of a WW2 level - its builds' roots, the names its data reaches, BF1943's redirected references, each
    pair's relation (DICE's, BF1943's, a stand-in), the partition under the clone's name and its read-back check;
  - `bc2_install::levelbuild::textures` (`ww2build_res_types`, `ww2build_stage_textures`): `wake_install`'s
    `res_type` and `stage_textures`;
  - `bc2_convert::physmat` (`bc2_native.physmat_*`): a part's material in the dbx field and the part byte together,
    new materials in the three lists, a model group member's face materials;
  - `bc2_convert::pictures` (`bc2_native.pictures_*`): the loadout pictures' renderer (NumPy's float64 order, its
    `dgemv` written out: XWW2-PIC-1), Pillow's `resize` for 8-bit pictures (XWW2-PIC-2), the loading and map pictures
    and their checks (XWW2-PIC-3), the weapon icons' silhouettes and fit, the minimap plane's paint;
  - `bc2_convert::meshview::page` (`bc2_native.meshview_page`): the preview page, its textures PNG files of the same
    pixels (XWW2-PIC-4).
- Tests: `pictures-native` and `physmat-native` (quick), `bc2-install/tests/materialgrid.rs`.

### Removed
- `bfbc2-mesh-viewer/meshview.py`'s `page()`, `PAGE` and `_png()` (Project OPFOR calls `bc2_native.meshview_page`).

## [0.27.0] - 2026-11-27

Round OR5 ("OPFOR in Rust", stage 5) adds commands and bindings Project OPFOR's menu build, Onslaught import and test
run now need, so OPFOR's `SUITE_REQUIRES` moves to 0.27.0 - 0.28.0.

### Added
- **`bfbc2 aptpatch <in.res> <file.patch> <out.res>`** (round OR5): Project OPFOR's menu patch files applied, the
  port of OPFOR's `AptTool.cs patch` (retired): every expected byte checked, appended strings and `@label` offsets,
  the BIGF archive rebuilt as AptTool rebuilt it. OPFOR's 12 patch files give AptTool's movies byte for byte; a
  malformed line, a label defined twice, a member named twice and a cut archive are refused (spec MENU-ED-7).
- **`bfbc2 menuart onslaught <unpacked mainmenu> [--xbox DIR] [--refresh] [--cache FILE]`** (round OR5): the Offline
  Matches panel's ONSLAUGHT mode picture, the port of OPFOR's `onslaught_art.py` (retired) - from the cache, the
  user's own Xbox 360 Update #3 package or the PC atlas, with Pillow's pixels (`bc2_menu::pil`: Lanczos resizing,
  paste, alpha_composite, DXT5) and numpy's float32 cut-out; OPFOR's menu bundle is the Python build's byte for byte
  from each of the three sources (spec MENU-ED-8). `bc2_native.menuart_atlas_infos` / `menuart_dxt5_decode` /
  `menuart_dxt5_encode` give OPFOR's Pacific War menu step the same pieces.
- **The console Onslaught import in Rust** (round OR5): `bc2_install::onslaught` (feature `onslaught`),
  `bc2_native.onslaught_build` / `onslaught_install_plan` - `console_onslaught.py`'s build (the package read by
  `bc2_formats::stfs`, the world swap, re-pointed and dropped objects, the shared closure, the description) and its
  install's bundles and writes; OPFOR keeps the command line and the ledger commit. On the user's Update #3 package
  the work folder and the writes are the Python's byte for byte (spec XCONS-ONS-1).
- **The test run's logic in Rust** (round OR5): `bc2_harness::testrun`, `bc2_native.testrun_ini_set` / `_get` /
  `_unset` / `_restore` (opfor.ini's edits) and `testrun_watch` (the log's notes while the game runs); OPFOR's
  `testrun.py` keeps starting, watching and closing the game.

## [0.26.0] - 2026-11-23

Round OR2 retires the suite's `terrain._Walk` and `ROAD_VERTEX` (a Python API Project OPFOR used) and the
`install_index.pkl` cache file, so this merge releases a minor version (`RELEASING.md`, "A merge that removes
something"). Project OPFOR's `SUITE_REQUIRES` moves to 0.26.0 - 0.27.0.

### Added
- **Project OPFOR's WW2 level build in Rust** ("OPFOR in Rust" stage 2, round OR2): the new module
  `bc2_install::levelbuild` (feature `levelbuild`) and `bc2_native.ww2build_*` / `Ww2Index` / `Ww2PackJob` -
  `wake_install.build`'s stock index (kept for the process; the `install_index.pkl` pickle retired), our builds'
  files, the closure, the level partition's and our copies' edits (teams, material grid, sky dome, voice-over,
  remaps, weapon unlock entries, kit voices, sky states), the level-root files, the install-time BF1943 ports
  (`meshport::Porter::port_with`: the shader plan and a bare mesh), the terrain files and undergrowth stand-ins, the
  staging folders and their packs, ww2_common, the shared bundles' patches, the description, the Air Superiority
  mode, the menus' level list and the model-group and icon gates. Project OPFOR's Python keeps thin callers; on the
  real game copy a Wake build gives every staged file of the Python build (6,260 files) byte for byte, and the WW2
  oracles stay 7,945 of 7,945 and 21,149 of 21,149 (`rust/BENCHMARKS.md` "OR2").

### Removed
- `terrain._Walk` and `terrain.ROAD_VERTEX` (bfbc2-console-convert, round OR2): only Project OPFOR's
  `wake_install` used them; its VisualTerrain walk is `bc2_install::levelbuild::terrainfiles`.

## [0.25.0] - 2026-11-23

### Added
- **Project OPFOR's BF1943 shader translation driver in the Rust core** (round OR3c, "OPFOR in Rust" stage 3c):
  `bc2_convert::shadertranslate` through `shaderdb.translate_plan` - which of BF1943's programs each mesh-part layout
  and permutation of an entry is made from (by layout, by role or by element order; the soldiers' layouts by
  meaning), its vertex inputs, renderer fragment, key and id, and where the Python driver stopped an entry (its
  `TranslateError` / `XenosError` / `KeyError` / `ValueError`, in place) - and `missing_entries`, `foreign_entries`,
  `discards` (a compiled shader's discard from its shader chunk, without the disassembler: 40,744 of 40,744 shaders as
  the disassembly said), `match_entries`, `entry_perms`, `part_users`, `offer_kinds`, `entry_blocks`, `element_map`,
  `soldier_element_map`, `vertex_elements`, `by_role`, `mesh_of`, `patch_inputs`. `level_scan` gives each entry's
  layout record bodies too (`LevelScan.layout_records`). Project OPFOR's `wake_translate` / `ww2_char_shaders` /
  `wake_install.index_templates` call them: Wake's translation as the retired Python made it, byte for byte, about 3-4
  times as fast (`rust/BENCHMARKS.md` "OR3c"). New quick test `shadertranslate`.
- **NavKit: the graph measured over every DICE node and edge** (round NKS-deep E, spec XNAV-MEAS-1): `navparity`
  and `navregress` pair each DICE route node one to one with its nearest NavKit node of the sector (within half a
  propagation step: 1.675 m coarse, 0.425 m detailed) and report `nodes_paired`, `nodes_paired_candidate` (the
  same pairs as a share of NavKit's nodes), `edges_paired`, `costs_paired` (cost within a tenth) and the
  nearest-node distance histogram (`node_distance`), beside the old 5 cm exact-match `nodes_same` / `edges_same` /
  `costs` (kept for one round). In the Rust core and the Python alike. The reference run was re-recorded with them.
- **NavKit: DICE's seed list and the midline cut's size, decoded** (round NKS-deep E, spec KYG-SEED-2, KYG-AREA-5,
  KYG-AREA-4 corrected): a sector's seeds are its spawns and its perimeter autoseeds, snapped to the grid and never
  locked; the midline cut tests the in-place record (100 + 28 per portal segment + 20 per line + 12 per segment ...)
  after simplification. Both built and measured, both **off** (`navwalk.DICE_SEEDS` / `Switches::dice_seeds`,
  `navwalk.MIDLINE_CUT` / `Switches::midline_cut`): neither leaves every measure as good (`rust/BENCHMARKS.md`
  "NKS-deep E"). DICE's detailed sectors measured again with the offline sampler at 0.2 m (still off). New probes
  `rust/bc2-formats/examples/kynapse_cut.rs` and `kynapse_seeds.rs`. Path data NavKit builds is unchanged.
- **The generated terrain's ring and visual side** (round TG2, spec TGEN-GEN-6, TGEN-RING-1, TGEN-VIS-1 - 4,
  TGEN-WATER-1, TGEN-OCC-1, TERR-HF-12, TERR-VT-8, WATER-VIS-6): `bfbc2 terrain generate` now writes the sub-terrain
  ring (twelve 64-sample blocks stitched to the main block exactly as DICE's 12 single-ring terrains are; `--no-ring`
  for none), gives temperate DICE's rough relief (an escarpment: 10 m / 66 m relief 4.24 / 13.70 m against DICE's
  4.77 / 13.16; `--rough`), and with `--donor <level>` the visual side from that DICE level of the game copy: the
  visual terrain (the donor's patch shaders renamed, each 16 m cell drawn by the rule-only shader its ground asks
  for), the mask atlas, the visual water (one open-water quad), the colour / occlusion map (the donor's values placed
  by the terrain's sky visibility) and the level's Dx9 / 10 / 11 shader databases with the donor terrain's patch,
  undergrowth, cache and water entries cloned under the new name. The water lies at the donor's water level by
  default (`--water-level`): the donor's rule layers shade by height (TGEN-VIS-2, measured in the game-accurate view:
  within 11 - 15 % of MP_005's look band by band at its water level, 1.5 - 3.2 times as bright from 0). Generator
  version 2 (temperate's heights and every terrain's outer rows change).
- **The game-accurate view of a generated terrain** (round TG2): `bc2_render::files::Files::with` (files made in
  memory over a level's), `World::load_generated`, and the editor's `gameview::read_generated` /
  `GameLoad::start_generated` draw a generated terrain in its donor level, nothing written to disk (Create Level's
  "Generate" wires it in round TG3).
- **`bc2_formats` writers** (round TG2), each proved on DICE's data: `terrain_codec::SubBlock` (DICE's 288 sub-terrain
  blocks byte for byte), `visual_terrain` (DICE's 30 visual terrains decoded and written again byte for byte, and the
  terrain renames), `water::VisualWaterFile` (DICE's 30 visual waters byte for byte; `open_water_vertex`, 23,820 of
  23,820 DICE open-water vertices), `itexture` (DXT1 and L8 textures with their mip chain in DICE's level layout;
  DICE's terrain occlusion maps encoded again within a mean of 0.0 - 0.9 levels) and `shaderdb::terrain::clone_terrain`
  (a terrain's shader entries cloned under another name with its textures renamed): shared with the editor's coming
  visual paint and occlusion re-bake (E5-rest).
- **Map editor: terrain sculpt and material paint** (round E5, spec TEDIT-BR-1, TEDIT-NAV-1, XVAL-TERR-2;
  TEDIT-HF-3 and TEDIT-MM-1 implemented): the Terrain workspace's brushes work - raise, lower, smooth, flatten to the
  height where the stroke began, a seeded noise and a ramp between two clicked points, each with a radius, a strength
  and a falloff (smooth, linear, constant), and the Material brush that paints the terrain's physics material map
  (0.5 m or 0.25 m cells) with one of its own materials. A stroke is drawn as it is made and kept as one undoable
  `stroke` operation (its 64 x 64 tiles beside the project); the game-accurate view follows. The export writes the
  sculpted main height field (min / max tree recomputed and lower envelope lowered over the edited region, depth map
  kept) and the painted material map into the staged terrain bundle in place of the base's entries, and
  `mapexport check` reads them back (XVAL-TERR-2); the path-data step samples the stage on the sculpted height field.
  Project format 11 (a stroke on the material map). The visual paint layers, the occlusion map's re-shade and
  widening the height range are E5-rest's. Nothing is proven in game yet (`spec/transforms/terrain_edit.md`
  section 5).
- **`bc2_formats::terrain_codec`** (round E5, shared with TG1's generator): the main height-field block (depth map,
  min / max tree, lower envelope) and the `TerrainMaterialMap` decoded whole and written again - DICE's 30 main blocks
  and 30 material maps byte for byte, the 288 sub-terrain quadtrees equal to the computed ones
  (`rust/bc2-formats/tests/terrain_corpus.rs`).
- **A terrain generator** (round TG1, spec TGEN-GEN-3 - 5): new crate `rust/bc2-terragen` and `bfbc2 terrain generate
  --biome arid|jungle|snow|temperate|vietnam --size 1024|2048 --seed <n> --out <folder>` (new tool group `terrain`):
  a main height field shaped to one of DICE's five biomes (spectral synthesis at the biome's spectrum, hypsometric
  remapping and slope matching to its slope distribution, hydraulic erosion, raised or sunk borders, flat sea floors,
  objective pads with `--pad`, the water level by the biome's share), the material map painted from slope and height,
  a water mesh, and a `terrain.json` with the seed, settings, generator version and the terrain's statistics against
  the biome's DICE medians. The same seed gives the same bytes on any number of threads. Written through round E5's
  terrain codec. On every biome the 99th-percentile slope is now 52 - 76 degrees (DICE's medians 53 - 75; the TG0
  prototype's 20 - 56) and the hypsometric integral within 0.02 of DICE's; temperate's rough small-scale relief is
  left. A 1,024 terrain takes 1 - 2.5 s, a 2,048 one 7 - 10 s. The library is what the editor's Create Level and
  sculpt tools will call.
- **NavKit: DICE's node placement, decoded** (round NKS-deep B, spec KYG-NODE-7, KYG-NODE-8, KYG-EDGE-5,
  KYG-AREA-4): Kynapse's map builder in `BFBC2Game.exe` places route nodes by propagation from the seeds in
  NbSectors directions DistPropagation apart, joins them densely, collapses edges into their middles and then removes
  edges that have a route within 1.2 times their length. NavKit can build its route graphs this way
  (`bc2_navbuild::propagate`, `navwalk.PLACEMENT` / `Switches::placement`, **on** by the user's ruling; the few
  measures that dropped are named exceptions in `spec/transforms/navkit.md`): on the 15 regression levels nodes near DICE's 47.6 -> 56.2 %, AI pairwise routes
  92.7 -> 95.6 %, connectivity 98.8 -> 99.1 %, over-connection 15.3 -> 15.7 %). Path data NavKit builds changes (its route
  nodes and edges); navregress's reference run and the golden builds were re-recorded. New probe `rust/bc2-formats/examples/kynapse_place.rs`; `kynapse_df` also tests StepMax /
  StepLength x pitch as a step.
- **Terrain generation in the specification** (round TG0, documentation and probes only): `spec/transforms/terrain_gen.md`
  (32 rules) says what a valid BFBC2 terrain is end to end (the eight kinds of data, what is generated, derived, reused
  or the engine's, names, registration, collision, water, navigation, lighting), groups DICE's 30 terrains into five
  biomes by their vegetation sets with the materials, atlases, undergrowth, trees, rocks and props each offers (and
  OPFOR's WW2 set apart), measures how DICE's terrains and playable areas are shaped (elevation, slopes, spectra,
  water, combat areas, flags), and designs the generator, its proof and the rounds TG1 - TG5. New probes:
  `spec/probes/terrain_shape.py` and the throw-away prototype `spec/probes/terrain_gen_proto.py` (a seeded height
  field, material map, water mesh and rule-layer visual terrain written and read back; DICE's 30 main blocks written
  again byte for byte), both in `run_tests.py game`'s spec-probes check. No tool changes.
- **Spec: the WW2 rebuild oracle regenerated cleanly** (round WW2-ORC1, with OPFOR; the user's ruling of
  2026-11-23): XDEP-STEP-5 and -6 implemented, XDEP-STEP-7 new (a step that edits another step's output in place
  depends on the version it read; OPFOR's tracker keeps such edits). OPFOR's oracle now holds what its steps
  write: 7,945 files, the same from a seeded-empty `work\ww2` and from its main checkout. The 2,903 relics were
  moved out; the caches are left out; the inputs no step writes are declared and checked. 18 Wake textures were
  reconverted to DICE's L8 convention (TEX-FMT-2, TEX-FLAG-1). The install oracle is 21,149 files.
- **Spec: the WW2 rebuild settles from empty** (round WW2-ORD1, with OPFOR): XDEP-STEP-3 implemented (no step
  reads a later step's output; the proof from a seeded-empty `work\ww2`), XDEP-STEP-5 (the inputs inside
  `work\ww2` no step writes: hand-placed files, the install plan's staging) and XDEP-STEP-6 (what the rebuild
  oracle holds beyond the steps' outputs).
- **NavKit: DICE's terrain rule for D lines** (round NKG3, spec KYG-DF-1): DICE's D sides between two terrain cells
  follow the terrain's slope across the side (above 0.77), not the step between the cell centres; its F cells follow
  the steps. NavKit's build reads the level's height field (`navwalk.build(heightfield=...)`,
  `bfbc2core.level.heightfield_path`) and builds D lines by this rule (`navwalk.DF_SLOPE`, `Switches::df_slope`, on;
  `None` for the step rule). Path data NavKit builds changes (more D lines); navregress's reference run and the golden
  builds were re-recorded. Measured on the 15 regression levels: D precision / recall 50.5 / 18.8 -> 56.7 / 23.9 %,
  every other measure unchanged but sp_02's D recall (21.86 -> 21.53 %, 0.33 points lower: a named exception by the
  user's ruling). The probe `rust/bc2-navbuild/examples/kynapse_df.rs` fits the rule on DICE's files.
- **NavKit's offline sampler in Rust** (round NKS-deep C, spec NAVS-SMP-1 to 4): `bfbc2 navkit sample <level>
  --offline` (and `navkit.py sample <level> --offline`) samples a prepared level from its own Havok collision and
  terrain without starting the game - the 0.8 m pair over the level's box, then the 0.2 m pair over its detailed
  sectors (`--pitch` one of them), into `work\navsample_offline` (`--out`). `--dice` samples DICE's own detailed
  sectors too (all 502 of the 15 regression levels, 40 of them never sampled in game); `--vehicle` adds a
  vehicle-shaped run at 1 m (`<level>_navkit_veh.bin`: KYG-PAR-2's radius 2.5, height 4, step 0.4; not validated, no
  in-game run exists); `--like <in-game files>` samples their grids and compares (`<level>_validate.json`). The
  library (`bc2_navbuild::sampler`) takes the collision and height field in memory; the bindings
  `navkit_offline_sample` / `navkit_offline_sample_level`. The same files as the Python on the 15 regression levels
  (30 of 30 at 0.8 m, 6 of 6 at 0.2 m, 8 of 8 on in-game grids), 3-30 times faster in a tenth of the memory (sp_006:
  0.2 m over 90 sectors and the vehicle run in 29 s, 0.85 GB).
- **The network layer in the specification** (round NET2, documentation only): `spec/engine/network.md` grows from 9 to
  25 rules (the transports and connection layer, network settings and start-up options, versions and disconnect
  reasons, PunkBuster, the server's listen path, the join by address, EA's Plasma back-end and its LAN mode, all read
  statically from `BFBC2Game.exe`), and `spec/engine/network_feasibility.md` studies locally hosted play, Direct IP,
  VLAN and LAN for Project OPFOR (needs, risks, effort, order, and the tests that would prove each). No tool changes.
- **The specification covers options, input, scriptability and dependency graphs** (round SPG3):
  `spec/engine/settings.md`, `spec/engine/input.md`, `spec/engine/scripting.md` and `spec/transforms/dependencies.md`
  (43 rules), with three read-only probes over DICE's data and the executable (`spec/probes/options.py`,
  `scripting.py`, `dependencies.py`) held by the game-tier check `core/tests/test_spec_probes.py`. `dependencies.py
  --steps` prints the WW2 rebuild's step graph from Project OPFOR's step records.
- **The specification's census** (round SPG4): `spec/CENSUS.md` lists everything in DICE's game files - the 16 kinds
  of file outside the bundles, the 37 bundle entry types, the 630 DBX instance classes with their instances and
  members, the executable's 1,900 `typeinfo` records in 22 modules - with the rules that cover each, generated by
  `spec/probes/census.py` from `spec/census.json`. A new quick-tier test, `spec-census` (`census.py --check`), fails
  when a file kind, an entry type, a class with 10 or more instances or a module has no rule and no out-of-scope
  reason; the game tier re-measures the census. 72 rules in 13 new files: the layouts of `OccluderMesh`, `SootMesh`,
  `AnimTreeInfo`, `ImpulseResponse`, `RagdollResource` and the Bink videos (in and outside the bundles), the loose
  files of the game folder (the aerial height map, `database.dbmanifest`, `dfe`, the patcher's record), the soldier,
  squads, the HUD's data, the campaign, text and fonts, sound graphs and voice-over, engine internals and the remaining
  classes by system; a second probe, `spec/probes/resources.py`, holds the formats' counts in the game tier.

### Changed
- `spec/probes/terrain_gen_proto.py` (round TG0's throw-away terrain generator prototype, a dev script) is retired with
  its spec-probes check (round TG2): the generator, its writers and their proofs are `rust/bc2-terragen` and
  `bc2_formats`.
- `navsample_offline.sample_grid` runs in the Rust core (its NumPy casts, capsule test and helpers -
  `raster_hits`, `cast_columns`, `Intervals`, `capsule_blocked`, `_line_tri_interval`, `_edge_dist2`, `terrain_at` -
  are gone; no other project imports them). The export's stage step (`bc2_levelwrite::pathdata::sample_stage`) runs
  `bfbc2.exe navkit sample --offline --stage` instead of `navsample_offline.py stage`.
- **`granny.Granny(b)` is the Rust core's object** (round PR1b-2-rest3, spec GRANNY-EDIT-2): `bc2_native.Granny`
  holds the resource's bytes and tables and answers `root` / `read` / `members` / `size` / `type_name` / `field` /
  `array_at` / `put` / `to_bytes` (the same attributes as before: `where`, `e`, `strings`, `data`, `sections`,
  `section_at`, `reloc`, ...). The Python reader and writer is `granny.GrannyReference`, the twin (pure-Python mode,
  resources the Rust reader refuses). Decided against the Python: `put` takes little-endian formats only (`'<'`
  first) and writes nothing when the place is outside its section (the Python wrote the resource's bytes first); a
  damaged resource whose object tree does not read raises a `GrannyError` saying why. Proven against the twin on the
  built resources (600 writes) and every Granny file the game tiers read (below, rust/BENCHMARKS.md "PR1b-2-rest3").
- Specification (round SPG2-rest4), read from the executable:
  - the undergrowth entry's loader names the 12 numbers by the code that uses them (VEG-UG-5, now verified on DICE's
    3,069 entries), and the generator's formulas are written down (VEG-UG-7);
  - the particle polynomial is a cubic, clamped before the scale (FX-PROC-5);
  - the FSCommand callback's ten commands (MENU-RT-6) and the menu event queue's once-a-frame drain (MENU-EVT-4) are
    described, and the menus probe checks them;
  - a finished award is written as `<Code>_00` / `_01` stats (PERS-AWD-5);
  - the career save drops a round's kills because the scoring record's stats pointer is set only by EA's answer
    (PERS-SCORE-2);
  - the compiled anim tree's three blocks are described from the loader's relocation walkers and checked on all
    1,926 DICE trees (ATREE-BLK-1 to 3).

### Fixed
- Specification (round SPG2-rest4): MENU-EVT-2's "target movie" at UiManager+0x82CF8 is the event drain's frame
  counter; VEG-UG-6 corrected: the type's registration has no factory, and the entry is read by the visual terrain's
  resource factory.
- The offline sampler's 0.2 m runs from `navsample_offline.py all` / `stockmp` and an export's stage sampled a
  detailed sector outside the level's box without its objects (the geometry was cropped to the level's box; sp_009's
  6 flagged sectors east of x 500): the geometry now covers every square a run samples (NAVS-SMP-2).

### Removed
- `bc2_native.granny_header`, `granny_index` and `granny_with_crc` (round PR1b-2-rest3): the Python `Granny`'s
  hand-offs to the Rust core, replaced by `bc2_native.Granny`; nothing else called them. `granny.Granny` is a
  function returning that object (or a `GrannyReference`), no longer a class. A removal: the merge bumps MINOR.

## [0.24.0] - 2026-11-23

Round PR1b-2-rest2 removes public Rust API (culltool's own DBX codec, bc2-menu's `Fail` results), so this merge
releases a minor version (`RELEASING.md`, "A merge that removes something"). Project OPFOR uses none of what was
removed; its `SUITE_REQUIRES` moves to 0.24.0 - 0.25.0.

### Changed
- **Merge step 3: `bfbc2.exe culltree` reads and writes on the suite's one DBX document** (round PR1b-2-rest2):
  `bc2_levelwrite::culltool`'s trees and levels are `EditDbx` documents, read by `bc2_formats::culltree::document`
  (`stream` for the streamed readers of round CT1: one root child at a time) under `culltree.py`'s rules (string
  indexes checked where read, every value's bytes kept, the trailer kept); the element reader, the document and the
  streamed read are one walk (`bc2_formats::culltree::walk`). Every output is the same: old and new `bfbc2.exe
  culltree` on the four Onslaught levels (console_cull's command line) and `add --auto --misplaced 2` on the 89 level
  partitions of OPFOR's `work\levels` (25 written trees and levels) byte for byte, `info` / `verify` on 180 trees
  the same, and every tree and level partition (and damaged copies) written back byte for byte (CT1's test).
- **The menu tools' library returns typed errors** (round PR1b-2-rest2): `bc2_menu::error::MenuError` (`Menu`,
  `Apt`, `NotFound`, `Io`, `Latin1`, `Reference`) and `MenuResult` in place of the Python emulation's `Fail` (about
  220 places in `common` and `graft`); `bfbc2.exe`'s menu and graft groups turn them back into the Python's
  exceptions (`From<MenuError> for Fail`), so their recorded answers are unchanged.
- **`decimate.py` and `shapes.py`'s NumPy work in Rust** (round PR1b-2-rest2): `bc2_convert::decimate` (the edge
  collapses; `bc2_native.model_decimate`) and `bc2_convert::shapes` (`Geo.moved`'s products, `Geo.tangents`, the
  windings of `_facing` / `outward` / `fix_winding`; `shapes_matmul` / `shapes_tangents` / `shapes_turn_over`). NumPy's
  products went through OpenBLAS, whose kernels are chosen by the processor; the port writes out the kernels of the
  machine that recorded Project OPFOR's WW2 weapon LODs (OpenBLAS 0.3.34 SkylakeX: `ddot`'s fused tail, `dgemv` and
  `dgemm`'s orders), so it gives those LODs on any processor (spec XWW2-MOD-1). The NumPy originals stay as
  `decimate_reference`, `Geo.moved_reference`, `Geo.tangents_reference`, `*_reference` for the tests.

### Removed
- `bc2_levelwrite::culltool::Doc` and `El` (culltree.py's codec in Rust): `parse_doc` / `read_doc` / `write_doc`
  and the `El` methods as functions over `EditDbx` (`attr`, `name_of`, `child_index`, `numbers`, `set_numbers`,
  `instances`); `CullTree.doc` / `Level.doc` are `EditDbx`.
- `bc2_menu::common::menu_err` / `bc2_menu::graft::apt_err` return `MenuError`; `bc2_menu::common` no longer
  re-exports `read_text` (`bc2_menu::error::read_text`).

## [0.23.0] - 2026-11-23

### Added
- **`shaderdb.level_scan(levels, dx)`** (round OR3b): each level database's entry signatures, textures, vertex
  layouts, main-pass skinning kinds and mesh parts - what `entry_signatures`, `entry_textures`, `entry_layouts` and
  the parts give on the loaded database - read and answered in the Rust core (`bc2_convert::shaderlevels`), the
  levels in parallel, without making the databases in Python. Project OPFOR's `wake_shaders --clone` and
  `detail_layers` ask it of every stock level.
- **The Kynapse generator's parameters in the specification** (round NKS-deep A, no tool changed):
  `spec/transforms/kynapse_generation.md` section 9 (KYG-PAR-1 to 3, KYG-DOC-1), KYG-NODE-6, `spec/formats/kynapse.md`
  KYN-FILE-2 and `spec/formats/hpd.md` HPD-TAB-4. DICE's generator ran with DistPropagation = GraphAccuracy + 0.35 m
  (3.35 / 0.85 / 15.35 m: the step its node pairs keep), the spanner's 1.2 is DICE's OverConnectionRatio, node ids
  count from 101 sector by sector, and the header words are named from the generator's own HPD writer. Four
  game-tier corpus tests in `rust/bc2-formats/tests/kynapse_corpus.rs`.
- **Whole Rush stages, a new flag's own texts and spawners in the gameplay tools** (round E4-rest,
  `EDITOR_ARCHITECTURE.md` 3.4): the Gameplay tool's **Add stage** adds a Rush or Squad Rush stage after the last - a
  copy of the last stage with its M-COMs, counters, bases, spawns and combat areas, its M-COMs' middle where the click
  is, wired to the last stage as DICE wires its stages (spec GAME-RUSH-11: one base counter per stage, the enable
  chain, the switches of what each stage has in play) and the defenders' `BaseCount` one more; the panel's **Remove
  stage** takes a stage out with what belongs to it, the stage before handing on to the next (taking out the first,
  the round starts as the second did). A flag added takes its own letter's capture messages
  (`HUD_MP_CQ_TOOK_BASE_<letter>`) and spawn-name key, the panel renames the selected flag in DICE's form
  ("E: Harbor"), and the export words what the game has no text for (GAME-CQ-12): the deploy screen's names under the
  level's own keys and the capture messages of letters past F from DICE's F in each language. **Add spawner** adds a
  soldier spawner with its spawn points to the selected (or nearest) flag or base, wired as the flag's own. Each is
  one operation; nothing of it has been tried in game yet (in-game tests are paused).
- levelwrite: `rushstage::{stages, add_stage, remove_stage, counterparts, connections, settled}`,
  `gameplay::{add_spawner, rename_flag, flag_texts, nato}`, `names::{language_texts, flag_message_texts}`,
  `export::flag_loc_keys`, `LevelWriter::connect`, `Ownership::owned_from_except`, `modelayer::clusters`; the operation
  `connect` (event connections added or taken out).

### Changed
- **The level cover index runs in the Rust core** (round OR3b, `bc2_convert::shaderlevels`,
  `bc2_native.shadercover_levels`): `shaderdb.level_cover_prefixes` / `level_coverage` / `donor_levels` give the same
  answers, the levels' databases read in parallel. The cache `shadercover` holds one `<24 hex>.bin` per level bundle
  and Dx, keyed by the bundle's content key (its length, head and gzip trailer's CRC-32 of the whole payload) instead
  of a SHA-256 of every byte of every bundle (`COVER_VERSION` 3).
- **The bundle packer writes DICE's empty gzip member** (round SPV1-fix, FBRB-DEL-2): a zipped bundle whose payload
  is empty (only `*deleted*` markers, or no entries) now ends with the 20-byte gzip member of no data that DICE's 105
  such bundles end with, where FbrbTool (and the suite after it) wrote nothing. `bfbc2core.fbrb.pack` / `pack_many`,
  `bfbc2 fbrbtool pack` and `bc2_formats::fbrb::pack` (new constant `EMPTY_GZIP_MEMBER`) all write it; repacking each
  of DICE's 105 gives DICE's bytes but for the header member's two header bytes (FBRB-HDR-4). FbrbTool's recorded
  pack results for such bundles were re-recorded (each FbrbTool's bundle + those 20 bytes). Every other bundle is
  unchanged.
- **`shaderdb.retexture` runs in the Rust core** (`bc2_formats::shaderdb::merge::retexture`, the merge's own): the
  same blocks, hits and refusals; a refusal names its textures as latin-1, as every other shaderdb message (the Python
  read them as UTF-8, or raised `UnicodeDecodeError` making its message). It needs the Rust core (`bfbc2core.native.require`).

### Removed
- The cover index's Python implementation and its pickles (`shadercover\*.pkl`, a file format a tool wrote:
  nothing reads them; the caches tool still counts and prunes those left on disk) and `retexture`'s Python twin.
- The map project format is version 10 (round E4-rest's `connect`); a version-9 project loads as it is.
- A soldier spawner deleted with its unit takes its spawn points although its flag's cluster lists them (round
  E4-rest); `mapexport overlay --remove <LEVEL>` also takes out the level's flag spawn names (`<LEVEL>_A` to `_Z`).

## [0.22.0] - 2026-11-22

### Changed
- **The DBX catalogue's cache is the Rust core's own file** (round PR1b-1e): `work\tools_out\dbx_catalogue.bin`
  (`bc2_convert::dbxcatalogue::cache`), read and written by `bfbc2 dbxcatalogue`, `bfbc2 dbxparity --predict` and,
  through `bc2_native.catalogue_load`, `dbxcatalogue.load` (Project OPFOR's WW2 tools: the same `Catalogue`, the same
  tables and elements). It is keyed by the content of what the pass reads - each bundle's entry names and content key
  (its length, its header and its gzip trailer's CRC-32 of the whole payload; a bundle stored unzipped hashed whole),
  the executable's SHA-256 - so a moved or copied game copy keeps it and a bundle rewritten at the old time does not.
  `dbxparity --predict` reads it instead of building the catalogue every run (about 15 s -> 5 s; the retired Python took 12 s), and
  `bfbc2 dbxcatalogue` builds it in Rust when the game copy changed (no longer handed to the Python). A bundle the
  pass cannot read is now a problem in the Python's words from the Rust pass too (it made the Python run its
  reference); an incomplete catalogue is still used but not cached.
- `bc2_gamefs::budget::maybe_prune(spec, top, opfor)` runs the overall budget after a folder it pruned (as
  `caches.maybe_prune`) and says whether it pruned.

### Removed
- The catalogue's pickle `work\tools_out\dbx_catalogue.pkl` (a file format a tool wrote): nothing reads or writes it;
  the caches tool still counts and clears one left on disk. `dbxcatalogue.CACHE_VERSION` and `_signature` are gone
  (`dbxcatalogue.CACHE` names the new file); without the Rust core `dbxcatalogue.load` builds the catalogue each time.

### Changed
- **NavKit's route graphs follow DICE's edge rules** (round NKG2, `spec/transforms/kynapse_generation.md`): each
  sector's walkable pairs are pruned to DICE's greedy 1.2-spanner (KYG-EDGE-3) instead of keeping every pair (coarse)
  or the Gabriel graph (detailed), and every edge lives on its source's owner copy (KYG-EDGE-4): an edge out of a
  border copy moves to the sector that owns the node, which gets a copy of the far end. That forms DICE's border band
  (KYG-NODE-2: nodes near each sector line shared by two to four sectors) and makes the graph, as the game walks it,
  strongly connected on every regression level (`strong_share` 99.69 -> 100 %). Path data NavKit builds changes;
  navregress's reference run and the golden builds were re-recorded.
- A NavKit build on a map without DICE path data keeps only the ground inside DICE's generator rectangle, the box of
  the level's soldier and vehicle spawns grown by 24.5 m (KYG-RECT-1; `navregress.spawn_bounds`), where it kept the
  whole sampling box. Levels with DICE path data still use DICE's extent.
### Added
- Specification (round SPG2-rest3, with AIS-rest2's static half): the mode entities read from the executable - the
  ticket counter's state and what moves it (a death costs its team one ticket, a revive gives it back, the bleed's
  formula: BOT-GS-6), the death and revive messages (BOT-GS-7), Rush lives, reinforcement and the stage as the
  defenders' bases left (BOT-GS-8), an M-COM's arming as a game interaction entity (BOT-GS-9); bots cost tickets and
  lives like humans. A game-tier probe (`spec/probes/modes.py`, checked by `core/tests/test_spec_probes.py`) holds
  DICE's event names against the executable's hashes. The bot object found (BOT-RT-6); who drains the bundle mount
  table (STRM-MNT-3); dirt build-up on client vehicles only (MAT-TEX-20); bot_ai.md's play-test plan steps 8 - 10.

### Fixed
- Specification: the server classes of the ticket counter, life counter, mission objective and kill counter
  (BOT-GS-4: two constructors carried the wrong names) and the bundle mount record's fields (STRM-MNT-3: `VfsPath` +0,
  `Bundle` +0xC, `IsResourcePath` +0x18) corrected; the 37-way switch 0x11DD76A named as the C runtime's `toupper`
  (FX-PROC-4).
### Added
- Specification (round SPV1): 127 rules verified against DICE's own files by game-tier checks - the Rust corpus test
  `rust/bc2-formats/tests/spec_corpus.rs` (DICE's 1,021 bundles: containers, resources, DBX, types, instances and
  references, textures, meshes, BIGF) and `core/tests/test_spec_probes.py` (new probes for the level partitions,
  cull trees, vegetation, terrain splines and the stock registries; the weapon, vehicle, material, destruction,
  lighting and weather probes' counts); nine rules corrected where the data contradicted them; `spec/COVERAGE.md`
  section 4, the census of every rule still observed and what would verify it.

## [0.21.0] - 2026-11-22

### Fixed
- **Console conversion of a visual terrain with a set undergrowth string** (round SPG2-rest2): `bc2-convert`'s
  visual-terrain walk (`terrain.visual_terrain` / `visual_terrain_parts` in console-convert) read the undergrowth
  entry's second string as one byte; it reads it as the string it is (spec VEG-UG-2). DICE's PC NAM_MP_003 now walks
  (30 of 30 PC visual terrains); the Xbox and Battlefield 1943 conversions are unchanged (their strings are empty).

### Added
- Specification (round SPG2-rest2): nine rules from the executable - how `OnlyTakeableByTeam` freezes a capture point
  (ONS-CAP-3) and the capture rules frame by frame (GAME-CQ-10), bots counting in a capture area (BOT-GS-5), vehicles'
  dirt build-up (MAT-TEX-20), the bundle mount (STRM-MNT-3), the award tree's loading (PERS-AWD-4), a level's loading
  video (MENU-LOAD-1, with a probe check), the particle processor and evaluator types (FX-PROC-4), the undergrowth
  type's fields (VEG-UG-6); the terrain playtest plan for E5 / E7 (`spec/transforms/terrain_edit.md` section 5).

### Changed
- **`bfbc2 dbxparity` and `bfbc2 portparity` run in Rust** (round PR1b-1d): BF1943's partitions against DICE's own field
  by field (and `--predict`, dbxport's passes scored on fair pools), and the importer's model conversions against
  DICE's PC files, without Python. Their reports, JSON and console lines are the Python tools' byte for byte on
  the user's BF1943 and game copy; `--predict` builds the DBX catalogue in process (it neither reads nor writes
  `work\tools_out\dbx_catalogue.pkl`), and a stale BF1943 source index is read as the Python read it but not
  written (the next Python open writes it). Every argparse form of their command lines is answered in Rust
  (`--`, attached values, positionals around options). A `float2` vertex element is compared as two singles (the
  Python read three; no compared part has one: spec rule XWW2-PAR-3).

### Removed
- `bfbc2-bf1943-import\dbxparity.py` and `portparity.py` (their command lines and `dbxparity.compare` / `predict`):
  `bfbc2 dbxparity|portparity` answer alone (`RETIRED.md`). The library is `bfbc2-bf1943-import\dbxparitylib.py`
  (`instances`, `flatten`, `FAIR_POOLS`, `family`, `fair_pool`, `fair_catalogue`); Project OPFOR's WW2 QA imports it
  from 0.21.0 on.
### Added
- **The map editor's gameplay tools** (round E4, `EDITOR_ARCHITECTURE.md` 3.4): in the Gameplay workspace a
  **Gameplay tool** on the views adds a flag (a copy of the level's smallest flag with its zone, marker, spawns and
  capture announcements, wired as it is, lettered next and owned by the picker's team), a spawn point to the selected
  spawner (or the selected or nearest flag's), or an M-COM to a Rush stage (the stage's counters count it), and
  **carries** a flag, base, spawner or M-COM with what moves with it; **Delete** there takes the selection with its
  unit (a team's last base, the last flag and a stage's last M-COM are refused in words). A **Gameplay panel**
  (Commands: "Show gameplay") lists the capture points with their letters, teams and spawn points, sets the selection's
  team, lists the Rush stages, and edits the **round's rules** - tickets and their loss, the time to take a flag,
  Rush's lives, Squad Deathmatch's kills - with DICE's values as the choices and DICE's rules for the mode and era as
  the default ("DICE's defaults"). Each edit is one operation of the log (one undo step) and the export writes it
  through its existing path. Nothing of it has been tried in game yet (in-game tests are paused).
- levelwrite: `gameplay::{unit, add_flag, add_spawn_point, add_mcom, delete_unit, carry_unit, link_point,
  stage_counters, Rules, stock, choices, rules_of, set_rules}`, `LevelWriter::place_unit`,
  `LevelWriter::ownership_with_events`, `Ownership::owned_from`; the operations `place-unit`, `link-point` and
  `set-rules`.

### Changed
- The map project format is version 9 (round E4's operations); a version-8 project loads as it is. A file whose log
  holds the new operations asks for an editor of version 9 (`min_reader`), also inside a multi-file operation.
- A field set on an instance the scanned partitions do not show (a mode layer grafted from a donor) takes a number's
  type from the instance's own value when the schema has no evidence (the same `looks_whole` rule), where it was
  refused for any value but 0.

## [0.20.0] - 2026-11-22

### Added
- **An XMA2 decoder in Rust** (round PR1b-1c, `rust/bc2-xma`): FFmpeg's `xma2` decoder ported, so the Xbox 360's
  EA-XMA sounds (BFBC2's console files, Battlefield 1943's) decode in process. It gives `ffmpeg -cpuflags 0`'s
  samples to the bit on all 4,657 XMA streams of both sound sets (spec rule XMA-DEC-1). The crate is under FFmpeg's
  licence, LGPL-2.1-or-later (the user's choice: its `LICENSE` and `README.md`, the `license` field of its
  `Cargo.toml`); the rest of the suite stays MIT (`README.md`, "Licence"). `audio.xma_decode` / `bc2_native.audio_xma_decode` expose it.
- Three project caches under the budget (`python -m bfbc2core.caches`, `bfbc2 caches`): `dbxcatalogue`
  (`work\tools_out\dbx_catalogue.pkl`, `BFBC2_DBXCATALOGUE_CACHE_MB`, 256), `ww2qa` (Project OPFOR's
  `work\ww2\_qa\cache`, `BFBC2_WW2QA_CACHE_MB`, 4096) and `ww2install` (its `work\ww2\install_*.pkl`,
  `BFBC2_WW2INSTALL_CACHE_MB`, 256).

### Changed
- **Converting sounds needs no ffmpeg** (`audio.convert`, `audio.decode`, `convert_many`, `bfbc2 audio`, Project
  OPFOR's WW2 sound port). Their samples are ffmpeg's C path's; ffmpeg's default run used CPU-dependent SIMD
  transforms that differ from it in the last bit of about one sample in 6,000, so a converted Wave can differ in a
  few bytes from one made with ffmpeg before. `audio.py` runs ffmpeg (with `-cpuflags 0`) only as the tests'
  reference (`decode_xma` / `decode_reference` / `convert_reference` with `ffmpeg=True`). `AudioError`'s advice no
  longer mentions ffmpeg.
- The cull-tree row cache and the shader-coverage index are keyed by content (round PR1b-1c): a reference level and
  its tree by their size and SHA-256 (`culltree` `CACHE_VERSION` 2), a level bundle by its SHA-256
  (`shaderdb.COVER_VERSION` 2), so a copied, restored or touched file keeps its entries and a file rewritten with the
  same size and time no longer keeps stale ones. Every entry is made again once.
- `mapexport build`: the level's closure and its shader reach walk the level's partitions once (round PR1b-1c,
  ALG-5); the outputs are the same bytes.

### Removed
- `bc2_convert::wavport::Ffmpeg` and the `ffmpeg` argument of `bc2_native.audio_convert` / `audio_decode` (and their
  `'ffmpeg'` answer): the decoder runs in process.

## [0.19.0] - 2026-11-22

### Fixed
- **The same answers on every machine** (round CI1, GitHub's `checks` run): `texture.check`'s correlation (used by
  `convert.py verify-texture` / `bfbc2.exe convert verify-texture`) was NumPy's `corrcoef`, whose last bits follow
  the CPU's OpenBLAS kernel; it is now computed exactly in the Rust core (`bc2_convert::texcheck::correlation`, the same value
  in the module and `bfbc2.exe`). `navpath.route`'s `straight` distance was `np.linalg.norm` (a BLAS dot, the same
  issue) and is now the route estimate's own formula (`navpath.distance`). NavKit's route estimate and hull dots use
  a pure-Rust correctly rounded fma (`libm`) instead of the C runtime's. Spec rule XCONS-PAR-2.
- The test runner's green-crate fingerprints no longer fail when the temporary folder is on another drive than the
  checkout (`tests/test_rust.py`, GitHub's runner: D: and C:).
- Goldens no longer hold the recording PC's temporary folder: stockmp's validate log records compare it as `<temp>`
  in its long or 8.3 short form (`test_cli_o5.py`); `fbrbtool_golden_game.json`'s informational paths likewise.

### Removed
- **The mesh viewer's pickle cache** `work\tools_out\meshview_index.pkl` (round PR1b-2-rest, audit AU5 ARC-4): the
  index of the game copy is `bc2_gamefs::meshindex`'s own file, `meshview_index.bin` (magic, version 4, the bundles'
  signature, the tables, a SHA-1 of it all: a damaged or foreign file is built again, never half read), read and
  written in Rust by `meshview.Index` (through `bc2_native.meshview_open`), `bfbc2.exe meshview` / `dbxcatalogue` and
  the map editor's asset preview. A leftover `.pkl` is counted and cleared by the `meshview` cache's budget.
- **`meshview.py`'s Python index build, part reader and renderer** (`Index._build_reference`, `_add_database`,
  `_read_part` / `_read_part_reference` / `_values`, `_texture`, `use_native`, `CACHE_VERSION`, `COLOUR` /
  `NOT_COLOUR`, `SIZE`, and the module's `meshdata` / `texture` imports): `Index`, `load_mesh`, `mesh_surfaces`,
  `stack_models` and `render` are the Rust port's (`bc2_convert::meshview`); `page()` stays (Project OPFOR's WW2
  preview pages). `bc2_native.meshview_index` and `meshview_part` went (`MeshViewIndex`, `meshview_open`,
  `meshview_stack`, `meshview_render` instead).
- **`bc2-convert`'s `pickle-cache` feature** and its `bc2-pyrt` dependency; `bc2_convert::meshview::cache`'s pickle
  reader and writer (`read`, `cache_bytes`, `same_sig`, `signature`, `mtime_ns`, `Sig`, `CACHE_VERSION`: now
  `bc2_gamefs::meshindex`'s); `bc2_gamefs::meshindex::build` returns the index (what it left out in `skipped`)
  instead of an error.
- **`granny.py`'s Python curve reader** (`_trunc_scale`, `_quat`, `bits_of`): `curve_keys` and `sample` are the Rust
  core's (`bc2_convert::granny_edit::curve_keys` / `sample`, `bc2_native.granny_curve_keys` / `granny_sample`).
- **LevelDiff's command line moved** from `bc2_formats::dbxedit::leveldiff` (`Run`, `parse_args`, `run`, `USAGE`) to
  `bc2_tools::leveldiff`; the comparison stays in `bc2_formats`.
- `tests/test_f4_native.py`'s `meshview` area and `tests/f4_golden_meshview.json` (the part reader they compared is
  the Rust port's whole now).

### Changed
- **`bc2_levelwrite::culltool` has typed errors** (`CullError`: `CullTree`, `Io`, `Mkdir`, `Damaged`, `Sink`;
  `CullResult`) instead of the Python emulation's `Fail`; `bfbc2.exe culltree` turns them into the Python's
  exceptions through `From<CullError> for Fail` (the same texts and exit codes). `RefLevel.rows` holds the tree's
  `CullTreeError` text.
- **One Project OPFOR finder**, `bc2_gamefs::game::find_opfor` / `opfor_candidates` (`Beside::Only` as
  `shaderdb.find_opfor`, `Beside::Above` as `bfbc2core.paths.find_opfor`; `OpforNotFound` names the folders tried),
  for bc2-cli's meshview / bf1943, shader, NavKit and stock-MP groups, bc2-menu and the map editor (six copies).
- The mesh index is built whatever it meets: an unreadable bundle or database is left out, said, and the index not
  cached (the Python reference did this for some failures and raised on others; no DICE file has any).
- `meshview.render` draws in float32 positions (NumPy drew a float64 part in float64; a preview's pixels may differ
  for those). Its shading is NumPy's to the bit: OpenBLAS' `sdot` length and `ddot` facing, found on 19,045 real
  triangles (the port missed a triangle's colour by one level in 9 of 360 parity pictures before).
- The map editor's install run is a `Task` (the last of PR1b-0's background jobs).
- `texwrite.py`'s packing (`resource`, `texture`'s header, `wrapped`, `l8_bytes`) is the Rust core's
  (`bc2_native.texwrite_resource` / `texwrite_wrapped` / `texwrite_l8_bytes`).

### Added
- `bfbc2-anim-tools/tests/test_granny_curves.py`: `curve_keys` / `sample` against the retired Python on built curves
  of every format and, with `--game`, every animation curve of the game copy.

## [0.18.0] - 2026-11-22

### Added
- **`bfbc2 mods`** (round MM1): Project OPFOR's mod manager on the install ledger - `list`, `status`, `enable`,
  `disable`, `install`, `uninstall`, `restore-original`, `verify`, `baseline`, `migrate`, `resume`. The mods: the
  Pacific War (`ww2`), its menus (`pacific-menus`), the Vietnam opt-in (`vietnam`), each Onslaught console port
  (`onslaught:<level>`, or `onslaught` for all), the map editor's exports (`mapedit`), NavKit's path data and stock-MP's
  AI per level (`navkit:<level>`, `stockmp:<level>`), OPFOR's import (`base`). A disabled mod's files are kept aside
  (`<game>\opfor\mods`: hard links, its level folders moved), so `enable` puts them back byte for byte with their
  write times, without a rebuild; the mods built on one (or needing it) are disabled with it after a confirmation
  (`--yes`) and come back with it; `restore-original <file> ... | --all` gives DICE's files back and keeps the installs;
  `verify` checks every file the ledger holds, the level folders and the kept copies; `migrate` is OPFOR's
  `migrate.py` in Rust (the plan first, `--apply` after). Any operation killed part way is finished by the next command.
  Spec `transforms/install.md` section 5 (XINST-MOD-1 to -9); `BC2_MODS_CRASH_AT` for the tests.
- **The hub's Mods tab**: the mods and their state, Enable / Disable / Uninstall / Verify / Restore DICE's files /
  Migrate old markers / Resume, a confirmation for what else changes.
- `bfbc2.exe`'s `mods` tool group (builds alone; in CI's list).

- `bc2_formats::shaderdb::merge`: `Merger::clone_entry` / `clone_entry_from_self`, `add_part` / `add_part_from_self`,
  `add_donor_kept` (a donor read for one call), `finished`, `current`, `retexture`, `name_guid` and the `Store` trait
  (where the records a merge makes live).

### Fixed
- The migration of the markers from before the ledger, killed after the first old backup was archived, left that file
  refused for good on the next run (OPFOR's `migrate.py` too: fixed on its side, branch `mm1-migrate`).

### Removed
- **`shaderdb.Merger`** (`bfbc2-shader-tools/shaderdb.py`'s Python merge class, round OR3a), with `shaderdb.ProgRef`
  and `Merger._shader_key` (now the module's `_shader_key`): the merge is the Rust core's. `shaderdb.merger(base,
  need=(), all_names=False)` makes a `bc2_native.ShaderMerger` (`bc2_formats::shaderdb::merge::Merger`) with the same
  calls (`add_donor`, `clone_entry`, `add_part`, `finish`) and lists (`entry_names`, `part_names`, `added`, `report`,
  `conflicts`, `part_conflicts`, `shared_programs`, `out`); `shaderdb.merge` uses it. The same answers as the Python
  class on its recorded ones (`tests/test_shaderdb_merge.py`: 271 built, damaged and random cases, 1,709 calls; the
  game copy's ONS_MP_008 + mp_008cq, 114 calls). Project OPFOR's 21 merges moved onto it (its `SUITE_REQUIRES` 0.18.0
  to below 0.19.0). It needs the Rust core: there is no Python twin.

### Changed
- The merge no longer keeps its donors: a donor is read from its bytes and only what the merge takes from it is kept,
  so a caller may let a donor go between calls (OPFOR's clone pass reads one donor level at a time). A name that is
  not latin-1 (an entry's or a part's) is refused with a `ShaderDbError`; the Python raised `UnicodeEncodeError` for
  an entry and appended a part it could not write. A donor that only exists in memory (its bytes do not read back:
  a shader naming a table past its end, a short record) is refused whole.
## [0.17.0] - 2026-11-22

### Removed
- **`bfbc2-auto-navkit/tools/navbuild.py`** (round NK5-rest), with its command line (`python navbuild.py <samples.bin>
  <forbid_tris.npy> <XOffset> <ZOffset> <out.hpd> ...`): NavKit's whole build is the Rust core's since round NK5, and
  `navkit.py build` / `apply` / `all` (or `bfbc2.exe navkit build`) build a level from its prepared inputs. Its thin
  calls moved to `navwalk.py` (`navwalk.build`, `water_tops`, `outside_bounds`, `deep_water`); its `BuildSettings`
  and module switches are the Rust defaults (`bc2_navbuild::stage::BuildSettings` / `Switches`), which a caller
  overrides with a dict (`navkit.py --no-detail`: `{'detail_features': False}`; `navvehicle.vehicle_settings()`);
  `bc2_native.navkit_settings()` lists them. Its Python stage, `merge_detailed`, `detail_pass`, `drop_one_way`,
  `restore_one_way` and `half_cells` twins went with it. No other project imported it (Project OPFOR has no caller).
  Every regression level builds the same bytes.
- Four NavKit tests became Rust tests (`rust/bc2-navbuild/src/stage_tests.rs`): `test_column_sectors.py`,
  `test_area_link.py`, `test_portal_symmetry.py` and `test_bridge_union.py` (their runner entries `column-sectors`,
  `area-link`, `portal-symmetry`, `bridge-union` with them); the detail pass's and route lines' checks of
  `test_detail_features.py` and the settings' checks of `test_tool_edges.py` too.

- `bc2_formats::dbxedit::EditFile` and `dbxedit::Element` (the one document is `EditDbx`), `dbxedit::run` /
  `run_many` / `Outcome` / `write_atomic` / `expand_commands` (now `bc2_tools::dbxedit`).
- `bc2_formats::{atomic, gameledger, meshindex, sha256}` and `fbrb::cache` (now `bc2_gamefs::{atomic, ledger,
  meshindex, sha256, bundles}`; `gameledger::Error` is `ledger::LedgerError`, `find_game` is `game::find_game`,
  `pid_alive` / `host_name` are `liveness::`), `fbrb::{read_path, extract_path, read_many, extract_many, pack_path,
  pack_many}` (now `bc2_gamefs::bundles::`; `bc2_formats::fbrb` keeps `read_path_uncached` / `extract_path_uncached`),
  `fbrb::{tool, unpack_path, unpack_bytes, UnpackError, UnpackStats}` (now `bc2_tools::`).
- `bc2_formats::bf1943::Payloads::read`'s folder argument: it takes a `PointsStore` (`bc2_gamefs::points::Folder`).
- `bc2_native.xas_encode` (no caller; `xas_encode_many` does its work).

### Changed
- `bc2_native.navkit_build`, `navkit_stage_grid` and `navkit_write_stage` take the build's settings and switches as
  overrides of the Rust defaults (`None`: the defaults), and refuse an unknown key; new: `navkit_settings`,
  `navkit_stage_answer` (a whole stage's answer, for the tests) and `NavLevel.drop_one_way` / `detail_pass` (the Rust
  stage's own, for the tests). `test_navbuild_native.py` checks the Rust stage's whole stages against answers recorded
  from navbuild.py's stage before it went (the same on all 10), and its E3c tracing through the Rust stage's
  `drop_one_way` and detail pass against the retired Python's recording (456 of 456, unchanged).
- Rounds NK2-NK4's parity scripts (`rust/parity_nk2_file.py`, `parity_nk3_detail.py`, `parity_nk4_walk.py`) hook into
  navbuild.py's stage, so they run against pinned checkouts (at `bda5b8d`, the last with navbuild.py, or earlier) and
  refuse a checkout without it.
- NavKit's path data is closer to DICE's in structure (round NKG1): each sector's table bounds are the box of its
  route nodes (KYN-SEC-2), no edge is written twice (KYN-EDGE-4), and border portals beyond a sector's square link to
  the diagonal neighbour (KYN-LINK-1; 5,922 such link entries on the 15 regression levels, DICE 5,682). Every navparity
  measure is unchanged; built files differ in bytes (`navbuild_golden.json` re-recorded). Edges only on owner border
  copies are written behind `navbuild.OWNER_EDGES`, off: with NavKit's single border row they would cut routes into
  the lower sector (KYRT-PATH-3).

- **One editable DBX document** (round PR1b-2, PR1b-1's decision): DbxEdit's model (`dbxedit::EditFile`) merged into
  `bc2_formats::dbx::edit::EditDbx`, the arena of shared chunks; DbxEdit's commands, the DBX inspector, its panel and
  the Compare panel run on it (`dbxedit::read` / `serialize` keep DbxEdit's reading and failure rules). Every output
  the same: DbxEditGUI's recording 1,232 of 1,232 steps, 31,037 prepared-level files back byte for byte, the export
  canaries. A text value's string index is now kept as the file stored it by the dbxbin reader too.
- **One varint** (`bc2_formats::varint`): the six copies are its callers.
- **The caches' budget, one implementation** (`bc2_gamefs::budget`): `bfbc2.exe caches` and `bfbc2core.caches`
  (its prune, `maybe_prune`, `prune_total` and `clear`, through `bc2_native.caches_prune` / `caches_prune_total` /
  `caches_clear`) prune with the same code; the Python stays for a missing module.
- **The mesh index is built on all cores**, one read of a bundle for all its shader databases, merged in the
  bundles' order (the same index).
- **The map editor's background jobs run on one `Task` type**: the start screen's catalog, a level's load and its
  render meshes, the export's profiles, preview and run, NavKit's comparison read (a worker that dies is an error
  naming the job).
- NavKit's path data is closer to DICE's in structure (round NKG1): each sector's table bounds are the box of its
  route nodes (KYN-SEC-2), no edge is written twice (KYN-EDGE-4), and border portals beyond a sector's square link to
  the diagonal neighbour (KYN-LINK-1; 5,922 such link entries on the 15 regression levels, DICE 5,682). Every navparity
  measure is unchanged; built files differ in bytes (`navbuild_golden.json` re-recorded). Edges only on owner border
  copies are written behind `navbuild.OWNER_EDGES`, off: with NavKit's single border row they would cut routes into
  the lower sector (KYRT-PATH-3).

### Added
- **Two new Rust crates** (round PR1b-2, audit AU5 ARC-2): `bc2-gamefs`, the game layer over the formats - a game
  copy found (`game::Game`, the seam for the later Frostbite games; `find_game`), every file written safely
  (`atomic`), the install ledger (`ledger`), process liveness (`liveness`), the bundle cache with its cached reads
  and the bundle writer (`bundles`), restart points on disk (`points`), the mesh index (`meshindex`), the caches'
  budget (`budget`) and the worker pool (`workers`); and `bc2-tools`, FbrbTool's and DbxEdit's command lines as a
  library (`bfbc2.exe fbrbtool` / `dbxedit` and `bc2_native` run it).
- **One worker knob, `BFBC2_WORKERS`**: sizes the Rust core's one pool in `bc2_native`, `bfbc2.exe`, the map editor
  and `mapexport.exe` (else `RAYON_NUM_THREADS`, else every core); `pcbuild`'s compiler threads take it when
  `BFBC2_HLSL_WORKERS` is not set (`bfbc2core.workers`).
- **The native census is a check** (`python rust\native_census.py --check`, `run_tests.py` quick, `native-census`):
  an export of `bc2_native` that no tool, test or Project OPFOR file calls fails; `--record` keeps OPFOR's callers
  (`rust\native_census.json`) for a checkout without OPFOR beside it.
- **The bot AI specification's static RE, finished as far as it goes without the game** (round AIS-rest;
  specification only, no tool changed): the AI body - the proxy's three interfaces and its seat requests
  (BOT-CTL-6); who writes which slot of the AI input object (BOT-CTL-7: reload and fire as digital actions, the
  infantry's move target, sprint, weapon slot and aim, the vehicle controls' analog throttle, brake, yaw, pitch and
  roll); stance, aim state, weapon slot, vehicle weapons and seats set directly (BOT-CTL-8); the creators of goals,
  behaviours, plans and controls (BOT-RT-9); the vehicle controls (BOT-VEH-5); bots as scored and named players
  (BOT-SCO-1 to BOT-SCO-4: the name at ServerPlayer +0xC2C, the three tests that keep bots from scoring); a capture
  point's owner and progress (BOT-GS-3) and the counters' and objectives' classes (BOT-GS-4); where a session's AI
  difficulty comes from (BOT-DIF-4); one server class for every order and the `IssueOrder` event hash (AIO-RT-1).
  The bot module's implementation interface is updated and the in-game test of the AI input object is written as a
  checklist. 12 new rules (69 in the two files).
- navparity's directed measures, the route graph as the game's runtime walks it (KYRT-PATH-3): `runtime_connectivity`
  and `strong_share` (navregress compares them); `kynapse_probe` counts one-way edges by their ends, C records against
  the square lines and the strongly connected share; two game-tier corpus tests (`one_edge_per_source_and_target`,
  `border_ids_have_edges_in_two_sectors`).

- **The terrain's last undecoded bytes, specified** (round SPG2-rest; specification and probes): a main height-field
  block's tail is a run-length coded depth map (the editor's `DepthMap.png`), a min / max tree of the heights down to
  16-sample cells and a lower envelope of grids 2 to n / 8 + 1 a side (TERR-HF-4, TERR-HF-9 - 11); the physics
  material map is 512 x 256-cell blocks of 4-bit cells, a repeated byte starting a run, over the main block with rows
  along z, its names resolved in the level's material grid by name (TERR-MM-2 - 4). Both re-encode byte for byte on
  DICE's 30 terrains, so the map editor's terrain rounds can write a new main block (E7) and paint footstep and impact
  materials (TEDIT-HF-3, TEDIT-MM-1). The undergrowth entries are the engine's `TerrainUndergrowthType`
  (VEG-UG-3, VEG-UG-5: density, variation, mask range, scales, wind, view distance).
- **Grime, camouflage and the menus' runtime in the specification** (`spec/engine/materials.md` MAT-TEX-5 - 19,
  `spec/engine/menus.md` MENU-AS / RT / EVT / HUD / ED, 41 rules) with two new probes, `spec/probes/grime.py` and
  `spec/probes/menus.py`.
- **Every specification probe is a game-tier check**: `core/tests/test_spec_probes.py` (`run_tests.py game`) now also
  holds SPG's five probes' counts (materials, destruction, lighting, vehicles, weapons), the two new probes' and the
  terrain tails and material maps: 172 checks; 74 rules move to verified.

### Fixed
- **CI's per-group clippy:** `bfbc2.exe`'s navtools and harness groups did not build alone since round NK6 (they
  called the navkit group's error mapping); the mapping now sits in `navshared`, which all three groups build.
- **CI without a game copy** (red since 2026-11-21; the development PC never saw it):
  - the editor's start screen offers the DBX inspector without a game copy too (it showed only "Open project" and the
    inspector, which needs no game, could not be opened);
  - the F4 hkpack test's built packfiles have their own random generator: they followed the damaged copies of the
    game's packfiles, so without a game copy they were other files and never matched their golden (re-recorded);
  - test_cli_o3 always runs from an isolated copy of the checkout: a `rust\target` build folder (CI's) added a line
    to `caches --report` that the goldens do not have.
- **The bot-kill scoring contradiction** (AI-NEED-5 against PERS-SCORE-1): the player-killed handler returns only for
  a null victim, so with OPFOR's offline-scoring patch a human's kill of a bot scores; what blocked it offline was the
  4-client scoring flag. AI-NEED-5, PERS-SCORE-1 and the open-question lists now say so (BOT-SCO-3); a playtest still
  has to confirm it.
- **More of DICE's Kynapse generator decoded** (round NKS-rest2; specification and probes only, no tool changed): the
  generator's rectangle is the box of the level's soldier and vehicle spawns grown by 25 m (KYG-RECT-1, verified on
  the 13 campaign levels by a new game-tier test, `rust/bc2-scene/tests/kynapse_rect.rs`); some route nodes come in
  pairs exactly one step of GraphAccuracy + 0.35 m apart along an axis (KYG-NODE-4, verified by a new test in
  `kynapse_corpus.rs`); every point of DICE's ground sees a node within half of DistEdgeMax (KYG-NODE-5, 96.5 % coarse,
  98.3 % detailed, 95.2 % vehicle). Node placement is not from the Havok collision, detailed sectors follow no
  level-data rule (best 92 %: designer zones), and the midline cut and the spanner's unexplained pairs are narrowed
  further. Two new probes: `rust/bc2-formats/examples/kynapse_rules.rs` (DICE's files) and
  `rust/bc2-scene/examples/kynapse_geom.rs` (DICE's files against the levels' collision, height field and entities).
- **NAM_MP_003's terrain in the game-accurate view**: the visual-terrain reader took an undergrowth entry's second
  string as one byte and refused that terrain, so the view of `nam_mp_003cq` / `r` / `sdm` / `sr` drew no terrain
  patches or road strips. The reader now reads the string and keeps the undergrowth entries
  (`bc2_render::terrain::PatchShader::meshes`); the other 33 visual terrains of the game copy read as before.

## [0.16.0] - 2026-11-22

### Added
- **The Kynapse specification's open rules, fitted** (round NKS-rest; specification only, no tool changed): DICE's
  route edges form a greedy 1.2-spanner (an edge is left out when the graph already has a route at most 1.2 times as
  long; KYG-EDGE-3) and the graph is symmetric by node id, a sector's one-way edges being edges into non-owner
  border copies (KYG-EDGE-4), both checked on DICE's 35 files by two new game-tier tests in `kynapse_corpus.rs`;
  from the executable, the node lookup's box query and its use of the lookup lists in order (KYRT-NODE-2), where the
  owner bit is read (KYRT-PATH-3) and the local-pathfinding layer beside the path data (KYRT-DYN-2). A new probe,
  `rust/bc2-formats/examples/kynapse_fit.rs`, records the per-feature fits.
- **Restart points in a deflate stream** (round PR1b-1-rest, `bc2_formats::restart`): a payload decompressed once
  is read anywhere after by inflating at most a span (1 MiB) from the nearest block boundary (zlib-rs through
  `libz-rs-sys`: `Z_BLOCK` stops, `inflatePrime`, the 32 KiB window as the dictionary, as zlib's `zran.c`). Each span
  keeps its CRC-32 and is checked on every read: bytes that changed are refused, never read as other bytes.
- **One BF1943 bundle reader** (`bc2_formats::bf1943`): `fbrb1943.Bundle`'s check (moved from `bfbc2.exe`'s
  `bf1943src`) and a compressed payload's reads through restart points kept on disk by the payload's content
  (`%LOCALAPPDATA%\bfbc2-tool-suite\cache\bf1943source\*.points`) and in the process (`BF1943_RESTARTS_MB`,
  `BF1943_SOURCE_MEMORY_MB`). `bf1943import.Source` and `RESTARTS` read through it (`bc2_native.bf1943_check` /
  `bf1943_read` / `bf1943_restarts`); the Python reader stays as the reference (`BFBC2_PURE_PYTHON=1`).
- **renderqa's truth index is a cache** (`renderqa`, `BFBC2_RENDERQA_CACHE` / `BFBC2_RENDERQA_CACHE_MB`, default
  64 MB): listed, pruned and cleared by `bfbc2 caches` and counted in the overall budget.
- **The auto layout's vehicles** (round A2, `bc2_levelwrite::vehicles`): a proposal places the mode layer's vehicles
  by team and class from the level's own vehicle spawners (moved, then copies), in DICE's per-mode counts (a Conquest
  base 4, 8 on WW2; a flag 2; Rush's first attackers' and defenders' bases 3 and 2; Squad Deathmatch's one IFV), each
  with its room, a drivable way out, water access for a boat or an open launch area for an aircraft, joined to its
  capture point's `Spawners`; committed with the layout as one operation. On 16 held-out stock levels: 0 errors,
  0 warnings, 446 of 446 statistics inside DICE's ranges (A1: 379 of 382; Squad Deathmatch's vehicle now placed).
- **Three new placement rules** (XVAL-PLACE-6 to 8): a land vehicle shut in (no 20 m way out), a boat without 24 m of
  open water, a helicopter under collision or a plane without a 30 m take-off run; 0 findings on DICE's 707 vehicle
  spawners of 27 stock levels.
- **Several proposals side by side** and **pins**: the Auto layout panel proposes 1 to 4 layouts at once, each in a
  column with its warnings, statistics and vehicles, one drawn in the views; "Pin selection" keeps objects where they
  stand and the rest is laid out around them (`Op::Pin`).
- **The export's path-data step** (opt-in: the Export window's "Stage the level's path data for bots",
  `ExportSpec.path_data`, `bc2_levelwrite::pathdata`): the base level's NavKit build (or DICE's file) staged with the
  level as `ai/infantry/pathdata.hpd` when the project changes nothing a bot walks on, refused in words otherwise;
  `mapexport check` holds it to XVAL-AI-1 (recorded, Kynapse, sorted nodes) and XVAL-PLACE-5; the AI workspace reads a
  stage's path data. Sampling and building an edited level waits for NavKit's port (NK5).
- **NavKit's build runs in Rust end to end** (round NK5): `bc2_navbuild::stage` (sector discovery, the areas, the
  border-node runs, the seam nodes carried between pitches, the one-way portal pieces, the detail pass's glue, the
  meshes' assembly, the route graphs and their ids, `merge_detailed`) and `bc2_navbuild::build` (walkability to the
  file, the build's log lines). `bfbc2.exe navkit build`, `all` and `apply` on a NavKit base no longer hand off to the
  Python (`--no-detail` included; `all` runs only the game's sampling as a `navkit.py sample` child);
  `navbuild.build` and `navvehicle.stage_on_grid` / `write` are thin calls of the module. Every regression level's
  file and log lines are the same bytes as before (15 of 15; the 4 golden builds as recorded); whole builds 234 ->
  151 s over the 15 levels, the largest build's peak memory 11.8 -> 6.9 GB.
- **The mesh viewer's index under the cache budget** (round PR1b-1, audit AU5 ARC-4): `work\tools_out\meshview_index.pkl`
  is the project cache `meshview` (cap `BFBC2_MESHVIEW_CACHE_MB`, default 256): listed, pruned and cleared by
  `bfbc2 caches` / `python -m bfbc2core.caches` and counted in the overall budget.

### Changed
- **The Kynapse specification, corrected by round NKS-rest**: `SectorOverlap` is the path-finding type's
  `DistEdgeMax` (15 m infantry, 50 m vehicle; KYRT-CFG-1); the search's step cost and heuristic are both the f32 3-D
  distance (`Kaim::CConstraintShortestPath`, `CHeuristicEuclidianDistance`) with 100 expansions per call
  (KYRT-PATH-1); the node resolver tests candidates in lookup-list order, not by distance, so the lists' tie order is
  read (KYRT-NODE-1, HPD-NODE-6); DICE's 13 % one-way edges are the border convention, not a directed graph
  (KYN-EDGE-3); the midline cut's "longer side" holds for 67 % of cuts over all files, not 325 of 326 (KYG-AREA-2);
  open questions OQ1, OQ2, OQ6 and OQ8 updated, and NavKit's gap list given NKG2's and NKG3's targets.
- **The bundle cache keeps restart points, not payloads** (`bc2_formats::fbrb::cache`): a large bundle read again
  gets a `.points` file (about 1 % of its payload; the WW2 replay's 8.09 GiB of payload files became 0.1 GiB), keyed
  by the bundle's content, so every copy of the same bundle shares one. A payload past the in-process budget is
  decompressed once per process, its points made in memory (audit AU5 ALG-5: the disk-off replay 150.6 s and
  68.9 GiB inflated before, 46.5 s and 28.0 GiB after); a read's spans are decoded on the worker threads.
- **Less memory** (round PR1b-1): NumPy's OpenBLAS runs one thread unless `OPENBLAS_NUM_THREADS` says otherwise
  (`suite_paths`, `bfbc2core`): each Python process importing NumPy committed about 490 MB for its 16 threads' buffers.
  Test peaks: geometry readers 2.9 -> 1.0 GB, NavKit cells and routes 5.4 -> 1.6 GB, texture helpers 5.2 -> 4.6 GB, model tools 5.5 -> 5.0 GB; the WW2 rebuild's steps each about 480 MB less (level builds 4.35 -> 3.87 GB).
  The batch readers of `bc2_native` (`dbx_parse_many`, `dbx_to_xml_many`, `fbrb_read_many`, `xenos_parse_many`,
  `granny_roots_many`, `granny_to_pc_many`, `texture_convert_many`) decode in a bounded window on all cores while
  the results are built in order, instead of decoding everything first: the Granny trees' peak 1.5 -> 0.9 GB, the
  Xbox shaders' 33 -> 16 s; every result the same.
- **The map editor builds NavKit in process** (round NK6): the AI workspace's **Build with these edits** runs NavKit's
  apply in the editor (`bc2_navbuild::apply` on a worker, its lines shown as they come) with a **Cancel the build**
  button; the files and lines are the `bfbc2.exe navkit apply` child's byte for byte (15 of 15 regression levels, NK1's
  33 apply files, 3 NavKit-base applies with region, seed and graph edits).
- **The export's path data built for the edited level** (round NK6): the path-data step builds the level with NavKit
  in process - the project's spawns and objectives as the build's seeds and objective floors, its navigation edits
  applied (traversal annotations staged beside the file) - and a project that moves static objects or edits static
  group members is sampled offline on its stage first (`navsample_offline.py stage`, into
  `work\navkit_stage\<level>`). On the auto layout's proposals for MP_002 and MP_005 every spawn, flag and M-COM is on
  the staged path data's main route network (XVAL-PLACE-5). Project OPFOR's export install puts the staged path data
  in the game copy through the install ledger.

- **NavKit's level driver is a library** (round NK6): `bc2_navbuild::inputs` (the build's inputs, the detailed
  sectors and their cache, the terrain unpack, DICE's extent, `original_file`) and `bc2_navbuild::apply` serve the
  command line, the editor and the export; `graphedit` no longer depends on `bc2-pyrt` (`bc2_navbuild::pyjson`).
  `bfbc2.exe navkit build` reads the objectives of a level prepared before `area_orders.json` from its XML instead of
  handing the run to the Python. `bc2_levelwrite::pathdata::ready` returns the step's plan (`Plan`: build, sample,
  copy).

### Removed
- **The bundle cache's whole-payload files** (`.payload`, format `BC2FBPL1`): no longer read or written. `bfbc2
  caches --prune` lists them as superseded and deletes them; they count against the cap until then.

## [0.15.0] - 2026-11-22

### Added
- **The Python inventory** (round PYI): `PYTHON_STATUS.md` and `python_status.json`, every `.py` file of the suite
  (358 on the QW3 merge, 104,486 non-blank lines) with its kind, status, Rust counterpart, callers (the suite and
  Project OPFOR BFBC2), retiring round and a note, grouped by folder with totals by status and folder: 22 files live
  (6,105 lines: only Python does the job), 21 partly ported (18,479), 55 twins (19,395), 20 ported thin callers
  (2,503), 3 in retired tools' folders (310), 50 that stay (benchmarks, parity and dev scripts: 7,975), 187 tests and
  helpers (49,719). Generated by `python rust\python_status.py` from its one table (the same bytes on every run); it
  also lists the 23 Python files `bfbc2.exe` embeds with `include_str!`. `tests/test_repo.py` (quick tier) fails when
  a `.py` file has no row or a row names a file that is gone. ROADMAP "Python left" and the README point to it.
- **The rest of the specification's gap pass** (round SPG2): 118 rules in nine new spec files and two extended ones -
  `spec/transforms/terrain_edit.md` (what a terrain sculpt, paint or new terrain must write: heights and their range,
  the patch map cells, the mask-tile atlas and its layouts, the donor rules, what the game computes; for E5 / E7),
  `spec/engine/vegetation.md` (forests, middlegrowth and the terrain's undergrowth; for E6), `spec/transforms/shaders.md`
  (BF1943's Xenos shaders translated for Dx10 / 11, the merge, Dx9's differences), `spec/engine/module.md` (every one
  of the 221 addresses OPFOR's game module patches or calls in `BFBC2Game.exe` build 795745, and how it checks them),
  `spec/engine/animation.md`, `spec/engine/decals.md`, `spec/engine/onslaught.md`, `spec/engine/streaming.md`,
  `spec/formats/bf1943.md`; `spec/engine/persistence.md` (the `Persistence/*.dbx` classes, BFBC2's and BF1943's ranks
  and awards) and `spec/engine/effects.md` (the emitters' processor chain). Two read-only probes
  (`spec/probes/terrain.py`, `spec/probes/effects_chain.py`) and the first game-tier check of probe counts,
  `core/tests/test_spec_probes.py` (`run_tests.py game`, entry `spec-probes:game`), which verifies 14 rules.
  Corrected in place from DICE's 30 terrains: TERR-VT-3, TERR-VT-4, TERR-VT-7, RNDR-TERR-5; LIGHT-PT-3's attribution
  of the game copy's ONS_MP_002 (OPFOR's console port, not Napisal's).
- **The bot AI specification** (round AIS): what a bot AI decision-and-control implementation needs in BFBC2, in
  two new spec files - `spec/engine/bot_ai.md` (41 rules: the server's AI frame and its time-sliced sensing / decision /
  behaviour passes, the per-player AI proxy and the bot object, goals / behaviours / plans / squad behaviours; the
  control boundary - an AI soldier is a server player with the AI flag, driven through an `EntryInputState` its AI
  input object pushes into the player's input handler like a remote human's, with the 13 actions DICE's AI sends and the
  ones it never does; perception, combat, movement, vehicles, squads, the game state a bot reads and the difficulty
  data; an implementation interface for OPFOR's bot module and the reverse engineering still needed in priority order)
  and `spec/model/ai_orders.md` (16 rules: the fifteen order classes, their fields, whom an order commands, issuing and
  chaining, the readiness and urgency data). Measured on DICE's AI partitions and 13 campaign levels by
  `cargo run --example ai_probe`; five rules verified by the game-tier tests `rust/bc2-formats/tests/ai_corpus.rs`.
- **The Kynapse specification** (round NKS): navigation meshing as BFBC2 ships and runs it, in three new spec files -
  `spec/formats/kynapse.md` (what path data means: worlds per pathfinding type, sector kinds and bounds, the resolution
  field as one over the tile size, areas, portals, links, node ids, edges, D / F by type, units and frames),
  `spec/engine/kynapse_runtime.md` (what `BFBC2Game.exe` does with it, from reverse engineering) and
  `spec/transforms/kynapse_generation.md` (DICE's generator as rules measured on its 35 files, with where NavKit
  deviates today and the gaps in priority order). `formats/hpd.md`'s open questions are tracked items, five answered.
  A game-tier corpus test checks the verified rules on DICE's files; `cargo run --example kynapse_probe` measures
  DICE's files or NavKit's builds.
- **The map editor's core before E4** (round PR1b-0, audit AU5 section 6): the views' editing tools are types behind
  one trait (`bc2_editor::tool::ViewTool`: input, click, drawing, keys, Esc, and what each asks for as an outcome),
  so E4's and E5's tools land as small types; background jobs are one type (`bc2_editor::task::Task`: a named
  thread, a guarded body, one answer, a dead worker reported). `cargo run --release -p bc2-editor --example
  editor_core_bench` and `--example preview_bench` measure the undo, memory, member-edit and preview numbers.
- **Auto layout** (round A1): the Gameplay workspace's Auto layout panel proposes where the target mode's flags,
  bases, M-COMs, spawns and combat areas go on the level, in DICE's style - the mode's layer (DICE's own layout,
  from the target's donor) turned, scaled and placed where the level's ground, water, rocks and buildings suit it,
  then held against DICE's statistics from the stock maps. The proposal is drawn over the views with its findings and
  statistics; nothing changes until you accept it, and an undo takes it back. A seed gives other proposals; the same
  seed gives the same one.
- **Placement checks on the ground** (round A1), live in the Problems panel: spawn points over water, inside rocks or
  walls or under the ground (XVAL-PLACE-2), a vehicle spawner without room for its vehicle or out of its domain - a
  tank in the sea, a boat on land (XVAL-PLACE-3), gameplay objects outside every combat area (XVAL-PLACE-4), and,
  once the AI workspace has read the level's path data, spawns, flags and M-COMs the bots cannot reach from the main
  route network (XVAL-PLACE-5). Each bound lies past what DICE's own maps do. `mapexport validate` checks the first
  three on the level's height field and water.
- **The specification's coverage matrix** (`spec/COVERAGE.md`, round SPG): every BFBC2 system against the suite's and
  Project OPFOR BFBC2's tools, the queued rounds and the spec rules that cover it, with what is left in priority order.
- **Eight engine specifications** (`spec/engine/`, 164 rules): `lighting.md` (baked occlusion, irradiance volumes,
  reflection cubes, the outdoor light, point lights and `PointLightEntityData`, the moon and god rays, DICE's night
  recipe), `weather.md` (the weather system's classes, DICE's 355 states and their value ranges, rain and snow
  effects), `effects.md` (effects, emitters and the per-level emitter atlases), `weapons.md`, `vehicles.md`,
  `materials.md` (the material grid, part and face materials, penetration, impact effects, grime and camouflage),
  `destruction.md` and `persistence.md`; the wrapper's material hash is now known (the name's 33-xor hash).
- **Read-only probes** (`spec/probes/`): the scripts that measured DICE's data for these rules (lighting, weapons,
  vehicles, materials, destruction); they print counts and ranges only.

### Changed
- **Undo, redo and History in the map editor are 3-4 times faster, the first ray after them 200-300 times, and 200
  operations' checkpoints hold a quarter of the memory** (round PR1b-0; SP_01: +279 MB -> +73 MB): the terrain, its
  ray grid, the height field and the level's files are shared between the base scene and each undo's, and the
  checkpoints share every part of a file their edits did not touch. A static group member's delete costs about 4 ms
  instead of a sixth of a second on SP_01 (its refusal check is not applied twice and the group is found without
  walking the partition per member type); object rays test only the parts of an object their path crosses; the asset preview's orbit is drawn on all cores (the same pictures). The DBX inspector's undo keeps
  at most 256 MB of steps. Every result is the same as before, bit for bit (`rust/BENCHMARKS.md` "PR1b-0").
- **Rust API** (round PR1b-0): `bc2_scene::scene::Scene::terrain` and `heightfield` are `Option<Arc<..>>`;
  `bc2_levelwrite::files::LevelFiles::partition` and `groups` hold `Arc<LevelWriter>`s; `bc2_scene::raygrid::RayCache`
  is shared by a scene's clones and keys the terrain grid by its mesh's `Arc`; the asset preview's reader answers
  through `Task`.

- **NavKit's walkability runs in Rust, end to end** (round NK4): a build stage hands the Rust core
  (`bc2_navbuild::walk`) the sample files and its inputs once, and the core reads the run, makes it coarse when
  needed, applies the hand edits, the water and reference-extent rules, the slope, clearance and structure rules, the
  seeds, floods, ladders, unseeds and roof islands. The same path data byte for byte on all 15 regression levels of
  the development PC (every stage's run, kept surfaces, gradients and stair flags bit for bit; the 4 golden builds as
  recorded). Walkability is 2.7 times faster over those levels (234 -> 88 s; the largest detailed pass 39.7 -> 8.9 s)
  and its memory peak 2.2-2.5 times lower (largest 12.8 -> 5.6 GB); the largest whole build's peak fell from 15.2 to
  11.9 GB. `navsample.load` reads a version 3 sample file a block at a time instead of whole. Spec XNAV-BLD-4 records
  the precision and order rules the port keeps.

### Fixed
- **A ported shotgun keeps DICE's shotgun body set** (round QW3): the BF1943 import's weapon conventions moved every
  `AnimBaseSet wabsShg` to `wabsRif`; that is right for BF1943's knife, the only BF1943 weapon with it, but DICE uses
  `wabsShg` on 20 of its 42 shotguns (all pump-actions) and on no other class. The rule now leaves it on a weapon
  whose `WeaponClass` is `wcShotgun` (spec WPN-WW2-8). BF1943 has no shotgun, so no WW2 build changes; its snipers
  keep `AlwaysAimHead` false as DICE's 32 do (WPN-WW2-4).
- **hkpack read an array of a type without a size as a list in a list** (round QW3): through the Rust core,
  `Packfile.read_object` / `read_value` returned `[[('?', n)]]` where the Python reads `[('?', n)]` (since round F4);
  `hkpack dump` in Python printed `m_odd: [[(?, 3)]]` in O2's live leg. The core's answer is the Python's again.
- **Tests that shared a fixed file in `%TEMP%`** (round QW3) - the argparse width test's trace
  (`argfmt_trace.txt`: two worktrees' runs at once failed with FileNotFoundError), the APT round-trip's movie
  (`aptgraft_test.res`), O4's prepare stand-in (`o4prep`) and O5's stock-MP stand-in (`o5s`) - now use a folder of
  the run's own; F4's shared mesh-viewer index cache is copied in whole before it is named.

### Removed
- NavKit's Python for walkability (round NK4): `navwalk.walkable`'s NumPy glue, `near_points`, `ladder_ends`,
  `objective_floor`, `coarse`'s loop and `_column_picks`; `navedits.region_mask`'s NumPy and matplotlib body and
  `edits_for`'s mask loop (navedits no longer needs matplotlib); the bodies of `navbuild.water_tops`, `deep_water`,
  `outside_bounds` and `stage`'s walkability block. The functions stay as calls into the Rust core with the same
  arguments and answers; `navedits.regions_of` and `navwalk.walk_inputs` / `walk_answer` / `run_dict` / `samples_of`
  are new. No command line or option went, and nothing outside NavKit used them. A version 1 sample run (no hit
  words), which no build could use before, now builds with every surface taken as terrain.
- **Rust API** (round PR1b-0): `bc2_editor::app::OutlineDraw`, `ViewState::outline` and `Measures::ruler` - the
  polygon's corners and the ruler's points are the tools' own (`bc2_editor::tool::Tools::polygon` / `ruler`).

## [0.14.0] - 2026-11-21

### Added
- **The export's rule registry** (round V1): every way an exported level can fail in the game, or be refused by the
  export, is a named rule with a code (`XVAL-REG-1`), a severity (error, warning or note), a plain-language message
  and the specification rules it rests on (`spec/transforms/validation.md`). `mapexport rules` lists them;
  `mapexport validate <project> --game <game>` checks a project as the editor does (exit code 1 on an error).
- **Live validation in the editor's Problems panel**: after every change the project is checked against the
  export's rules and DICE's placement tendencies, errors first, each row with its rule's code (its message and
  spec citations on hover) and a "Go to" link that selects the object and moves the views to it. Notes can be
  hidden.
- **Placement warnings from DICE's stock maps** (round L1's study): flag spacing, HQ distances, capture zones,
  spawn counts and distances, combat-area margins, Rush stage spacing and spawn distances, Squad Deathmatch spawns
  and vehicles - 49 rules, each finding quoting DICE's range ("Stage 3 is 40 m from stage 2; DICE's Rush stages are
  93-316 m apart"). Warnings and notes only, never errors; no stock level raises a warning.
- New export checks: a deleted marker hiding a sound (0xFF2AC2), the terrain bundle the base's own (black tiled
  cells), Xbox 360 textures or Havok physics in a stage, an edited static group losing a kind or gaining one (0xAB4E58; SG1's layout lines carry the same rule), the
  40 km/h road sign drawn with another level's entries (0x98BD6E), a note that the server and every client need
  the install; the build refuses an object moved or placed beyond +-2048 m in words.
- **Static group members are editable in the map editor** (round SG1). Most of a WW2 map's objects (Wake Island:
  2,999 group members against 1,329 placed objects: its buildings, fences, trees and props) live in model groups; they
  are now selected, moved and turned with the gizmo or the properties' numbers, deleted, duplicated and pasted like
  any object, and the asset browser's new **Place into** list adds an object of a kind a group holds to that group
  (any other kind is placed as a loose object, and the editor says why). Members keep their names across deletes and
  adds (`<group>#<k>` for the base's, `#+<n>` for added ones), every edit is one History step, and the last member of
  a kind in its group is refused in words. The export writes the group files, the group's physics and the client cull
  tree in DICE's layout: a moved member keeps its cull leaf, moved with it; an added one takes a deleted member's
  leaf or a new one. `mapexport check` now holds every model group of a level to the rules all 131 of DICE's groups
  keep.
- **Project format 7** (round SG1): member deletes and adds. Version-6 projects load as they are and are written
  with `min_reader` 6 until they hold one.
- **`bfbc2 install`: the game-copy installer** (round OR1, "OPFOR in Rust" stage 1; new crate `rust/bc2-install`).
  Project OPFOR's WW2 install (`wake_install.py --plan / --apply / --apply-staged / --revert`,
  `ww2_level_install.py`) builds the staging in Python as before, then writes an install plan (its decisions as
  data) and runs `bfbc2 install plan | apply | revert | marks`: the backup guard, each level's bundles and record,
  ww2_common, the DLL's markers, the shared bundles and ini files through the install ledger, the registration last,
  the verification, the opt-out and `--apply-staged`'s record and input fingerprint, with the records, files and
  printed lines of OPFOR's Python installer (proved on scratch copies of the game's affected files). It takes the WW2
  rebuild's lock (`work\ww2\_rebuild\rebuild.lock`, exit code 3 while a rebuild or another install holds it) and
  rolls an interrupted commit back before it reads anything; `BC2_INSTALL_CRASH_AT=<n>` (tests) stops it before its
  n-th write step. The ledger (`bc2_formats::gameledger`) gained OPFOR's stage operations: `install_keep` (an
  install keeping the owner's other files, in OPFOR's record order), `restate`, `file_layers`, `slot_holders`.
- **One whole-file writer for every Rust tool** (round PR1a, audit AU5 ARC-8): `bc2_formats::atomic::write` (a
  temporary file beside the target named per process and per write, flushed to disk, moved over it with retries) is
  what FbrbTool's and DbxEdit's commands, the install ledger, the installer, the caches, the logs' archive, the
  harness, stock-MP's report, texqa and the editor's saves now call; two threads or processes writing one path no
  longer share a temporary file (the ledger's was a fixed `.opfor-ledger-tmp`), and a recovery removes a killed
  writer's temporary files. `bfbc2core.atomicio` names its temporary files the same way.
- **Determinism as a rule** (round PR1a): the Rust workspace lints loops over hash maps (`clippy::iter_over_hash_type`;
  each remaining one says why its order cannot reach an output); the repository rules refuse a new unsorted JSON write
  in the tools' Python (`atomicio.dumps_stable`); `rust\native_census.py` lists who calls each `bc2_native` export.
- **Stable test ids** (round PR1a): every `run_tests.py` entry has an id; its resources, CI skips, durations and the
  round-code allowlists are keyed by it (`--list` shows it, `--only` matches it). A durations file keyed by display
  names is still read.

### Changed
- Every `mapexport check` line names its rule (`ok     XVAL-REG-1  registration: ...`), and every refusal of
  `mapexport build` starts with its rule's code. `CheckReport` has the codes beside its lines (`codes`, `lines()`,
  `findings()`).
- **`bfbc2.exe navkit apply` answers a DICE base in Rust** (round NK1): the side file's links, route edges and
  traversal annotations go into a copy of DICE's path data without Python (`bc2_navbuild::graphedit`), the same
  files, report and output as before (252 of 252 recorded command lines; every DICE file and NavKit regression build
  of the development PC byte-identical), 7-8 times faster and in a tenth of the memory. A NavKit base still rebuilds
  the level through `navkit.py` (its build is Python), whose edit stage now runs in the same Rust core. The map
  editor's AI workspace runs it as before.
- **`bfbc2.exe`'s usage and the crates' descriptions name tools by what they do** (round PR1a, audit AU5 NAM-2), not by
  the round that wrote them; the map editor's export hints and Help text too.
- **The editor's prepared-copy record lists its files in a fixed order** (round PR1a): before, the order of
  `prepared.json`'s files varied from run to run (hash order); `bc2_native`'s scene collision dict is in name order;
  `mapexport validate`'s and the Problems panel's vehicle-spawner findings and the export check XVAL-GRP-1's list of
  problems come out in the same order on every run.
- The Rust crates' Python-runtime emulation (`bc2-pyrt`) is no longer a dependency of the map editor; the mesh index
  cache is `bc2-convert`'s `pickle-cache` feature and NavKit's apply edits `bc2-navbuild`'s `graphedit` feature
  (round PR1a, audit AU5 ARC-1).
- Spec HAVOK-WRAP-1 and the console converter's FORMAT.md call the wrapper's n words part words: part-name hashes in a
  blueprint, the owners' leaves in a model group (round PR1a, audit AU5 NAM-4).
- **NavKit's build writes its path data in Rust** (round NK2): the build's last stage - the shadow-island rule, the
  order nodes' pruning, each sector's node and edge tables, the neighbour links of the runtime meshes and the file -
  runs in the Rust core (`bc2_navbuild::graphfile`). The same files byte for byte (all 15 regression levels of the
  development PC, and the 4 golden builds of `test_navbuild_native.py`), the stage 2.4 times faster over those levels (1.4-3 times per level) with a third less memory. The rest of the
  build (walkability, areas, tracing, D lines and F cells, the stage's glue) is still the Python driving the Rust
  core (rounds NK3-NK5).
- **NavKit's D lines and F cells are made in Rust** (round NK3): `navdetail` (each area's broken sides, D lines
  and F cells) runs in the Rust core (`bc2_navbuild::detail`). On every call of the 15 regression builds it gives
  the old Python's answers, except where the Python left a choice to its sets' iteration order. Those choices are
  now a spec rule (KYM-DF-2), for shapes DICE's own files never contain. An F polygon vertex in an empty 45-degree
  bridge cell, with no cell beside it on its record's side, takes the mean height of the nearest F cells toward that
  side. The Python took whichever nearest cell its set gave first. This moves 13 F vertex heights in 7 regression
  builds by 0.01-0.69 m; nothing else in any file changes. The 4 golden builds are unchanged. The detail step is
  5.9 times faster within a build (17.6 -> 3.0 s over the 15 levels).

### Fixed
- **Test O2 no longer trips Microsoft Defender** (round AV1). Defender's machine-learning scan blocked reads of the
  synthetic build-795745 exe that O2's aptrefs cases write (Trojan:Win32/Bearfoos.A!ml). Round QW2's layout passed
  for a few hours, then was flagged too. The cases now write that stand-in with `SI` in place of its `MZ` and
  `PE\0\0` signatures, so no scanner sees a program. Under `BFBC2_TEST_STAND_IN_EXE=1` (tests only), `bfbc2.exe`'s
  menu tools put the signatures back as they read it, so aptrefs reads the same bytes as before and gives the same
  answers.

### Removed
- `bfbc2-auto-navkit/tools/navlinks.py` and `navtraversal.py` (round NK1): navkit apply's links, route edges and
  traversal annotations are the Rust core's (`navkit.graph_edits` / `write_traversal` call it). No command line or
  option went; nothing outside NavKit imported them.
- Rust public API (round PR1a): `bc2_convert::meshview::cache::{locked, write_atomic}` (now
  `bc2_formats::atomic::{locked, write}`), `bc2_menu::common::STAND_IN_VAR` (the test switch moved to `bfbc2.exe`'s
  menu tools; `PeImage::open` has none, `PeImage::from_bytes` is new), `bc2-cli`'s copies of the shader-database
  helpers; `bc2_convert::meshview::cache` and `bc2_navbuild::graphedit` now need their crate's feature. Test files and
  `bc2-py` modules named by round were renamed by function (`rust/BENCHMARKS.md` "PR1a").
- NavKit's Python for a build's last stage (round NK2): `navislands.components` / `shadowing` (`navislands.prune`
  stays, a call into the Rust core), `navbuild.prune_order_nodes`, `finalize_graph`, `border_cell`,
  `same_grid_resolver` and `cross_grid_resolver`; `navbuild.sector_links` now takes the stage's sector and a
  `link_scope` (the resolvers are the Rust core's), `write_file` the build's anchors. No command line or option
  went; nothing outside NavKit used them.
- NavKit's Python for the detail features (round NK3): `navdetail.broken_sides`, `f_cell_sets`, `d_sides`,
  `_unit`, `corner_cuts`, `_cut`, `d_lines`, `_components`, `_quarters`, `_loops`, `_turn`, `_simplify`,
  `_f_point`, `SIDES` and `F_DIRECTION`. `navdetail.detail_features` and `f_cells` stay as calls into the Rust core. The new
  `navdetail.features` also gives the D sides and the F cells of each axis, which `navbuild.detail_pass` reads. No
  command line or option went, and nothing outside NavKit used them.

## [0.13.0] - 2026-11-21

Released by round GR4 under the rule above: DbxEditGUI, the suite's last C# tool, was removed (the map editor's DBX
inspector replaces it). Project OPFOR needs `SUITE_REQUIRES = ('0.13.0', '0.14.0')`; it calls no DbxEditGUI.

### Added
- **The map editor's DBX inspector** (round GR4: DbxEditGUI folded in): any binary `.dbx` - a file (typed or dropped),
  an entry of a bundle (read in memory), one of the open level's files, or the open project's level partition - as
  DbxEditGUI showed it: the tree, the attributes and value edited and applied (typed like the field, a rounded
  decimal said), Go to reference, Undo, Find / Next, the Instance menu (remove, graft, copy field, add item, copy
  GUID), the Edit menu (replace, replace prefix, remove a field everywhere, set a bool everywhere, the partition's
  GUID), the Level menu (check, against a cull-tree level, fix slots), Save (`.bak` first), Save as, Export / Import
  XML, Save edit script; new: Save into bundle (a copy of the entry's bundle), a field edit of the project's partition
  as an operation of the project (History, Ctrl+Z), a note under a number the XML prints as a whole number that may
  be a tiny decimal (its bits always kept). `bfbc2-editor --dbx <file>`, the start screen's "Inspect a DBX file", the
  command palette's "DBX inspector", the hub's More tools > DBX inspector. Identical to DbxEditGUI's recorded answers
  on 19 real files, 1,232 steps (`rust/BENCHMARKS.md`, "GR4").

### Changed
- The hub's More tools: "DBX inspector" (the map editor on a chosen `.dbx`) replaces "DbxEditGUI".
- `bc2_formats::dbxedit::encode_value_rounded`: `encode_value` and whether a decimal was rounded.

### Removed
- **DbxEditGUI** (`bfbc2-dbx-tools\DbxEditGUI.cs`), the suite's last C# tool, with `DbxEdit.cs` (the C# DBX classes it
  was built on), `build.bat`, `tests\DbxTests.cs`, `tests\run_tests.bat` and `tests\build_if_stale.py`; the test
  runner's "DbxEditGUI build (when stale)", "DbxEditGUI and the DBX classes it is built on" (quick) and "DbxEditGUI's
  DBX classes (DbxEdit.cs) on the corpus" (full) entries. Its window is the map editor's DBX inspector (above); the
  corpus round trip moved to `bc2-editor`'s game tier (`dbxedit_parity.rs`). `RETIRED.md`.

## [0.12.0] - 2026-11-21

Released at round OP1's merge under the rule above: OP1 removed Rust public API of the editor (`set_pending_target`,
the Export window's `project` / `bound` fields, `Install.ok`, `nav::Job.done`; `Then::Create` changed shape). Project
OPFOR needs `SUITE_REQUIRES = ('0.12.0', '0.13.0')`.

### Added
- **Map editor projects can edit any file of a level** (round OP1, audit AU1 R18). An operation names the files it
  changes - the level partition, a static group file, a height field, a paint layer, the navigation side file, or
  the project itself - and the new `files` operation edits several of them as one undo step: the partition and the
  static group members' transforms (Wake keeps 2,999 of its objects in group files). It applies whole or not at all;
  the editor keeps a writer per file in its checkpoints, so undo, redo and History bring every file back together.
  The export writes the edited group files in place of the base's. Editing members in the views is round SG1's.
- **Brush strokes stored as tiles beside the project** (round OP1, audit AU1 R24, for E5's terrain and paint
  brushes). A `stroke` operation names only the 64 x 64 tiles it changed, by sha1; the tiles live once each in
  `<project>.mapedit.json.tiles`. Undo and redo touch only those tiles. On a 2048 x 2048 height field a stroke's
  commit takes about 2 ms and an undo about 3 ms (`rust/BENCHMARKS.md` "OP1"). The export refuses projects with
  strokes until E5 writes terrain.
- **A journalled autosave** (round OP1, audit AU1 R17 / P3). The autosave is now an append-only journal, one
  checksummed line per change instead of the whole project after every edit; a crash (tested by killing the writer
  mid-line) recovers every whole line, and the journal is compacted when it grows. Older whole-project autosaves still
  read.
- **Closing with unsaved edits asks first** (round OP1, audit AU1 R7 / G-N20). Close map and closing the window
  ask "Save your changes?" - Save, Don't save (the edits and their autosave let go), Cancel - after the question about
  running work. A failed save keeps the map open and says why.
- **The game-accurate view loads on a worker** (round OP1, DF1's left item, audit AU4 CQ-6): the editor keeps drawing
  while a large level's shaders, textures and terrain are read; only the Direct3D device is made on the window's
  thread.

### Changed
- **Typed errors in the map editor and the renderer** (round OP1, audit AU1 D13 / AU2 Q3): `bc2_render::RenderError`
  (device, device removed, data, shader, unsupported) and `bc2_editor::error::EditorError` (project, level, render,
  GPU, input, file, tool, worker) replace the 155 `Result<_, String>` of the two crates; the words shown are the same.
  The Problems panel's entries are typed (`Problem`: severity, code, operation, objects): a changed base is a warning,
  an operation that did not apply an error.
- **The editor's side state as explicit states** (round OP1, replacing round DF1's side fields): the close question
  (`Closing`), the Export window's project (`ExportFor`), the preview (`PreviewState`: none, opening, shown), the
  level being prepared with what follows (`Mode::Preparing { job, then }`), the new project's era and mode carried by
  its Create (`Then::Create(name, folder, target)`), the install script's run (`InstallRun`), a navkit job's end, and
  the game-accurate view (`GameSlot`: off, loading, ready).
- **The editor's two god objects split** (round OP1, audit AU1 D9 / AU4 CQ-7): `ViewState` went from 74 fields to 45
  (eight focused parts: `browser`, `menus`, `outline`, `measures`, `inspect`, `props`, `pointer`, `game_view`) and
  `EditorApp` from 37 to 22 (`command`, `new_project`, `start_ui`, `catalog_state`). Behaviour unchanged: the editor's
  tests pass and seven screenshots of the main views (start screen dark, light, WW2; the demo level dark, light, with
  a selection, the navigation view) are byte for byte the same before and after.
- **Project format 6** (round OP1). A project records the sha1 of each level file it edits besides the partition
  (`base.files`) and `min_reader`, the lowest editor version that reads it (the newest operation kind in its log);
  a newer file is read when its `min_reader` allows. Projects of versions 1 - 5 load and are migrated as they load;
  their logs replay to the same partition bytes. A missing or damaged `version` is refused in words instead of being
  read as 0 or cut to 32 bits (audit AU4 BUG-13).

### Fixed
- **Project saves are durable** (round OP1, audit AU4 DS-20 / BUG-12): the temporary file is synced before it is
  moved over the project, the move is retried while another process holds the file for a moment, and the temporary
  file is removed when anything fails.

## [0.11.0] - 2026-11-21

Released by round GR3 under the rule above: the mesh viewer's page for a browser was removed (its command line now
writes the picture). Project OPFOR needs `SUITE_REQUIRES = ('0.11.0', '0.12.0')`; its WW2 tools keep working (they
call `meshview.page()`, which stays, deprecated).

### Added
- **The map editor's Asset preview** (round GR3: the mesh viewer folded in). Click a tile in Place Objects, or a mesh
  in the asset browser's new **Meshes** mode (every mesh of the game copy by name, as `meshview.py find`), and the
  Asset preview panel shows it: drag to turn, right-drag to move, the wheel to zoom (perspective or orthographic);
  **Side, Top and Front** are the mesh viewer's pictures pixel for pixel; every **level of detail**; the **parts**
  (switch each on or off, click one in the picture to select it, drawn in the viewport's selection orange); a
  blueprint's **destruction states**; every **texture** of a part's shader **by channel** (colour, normal, specular,
  mask, occlusion, detail, environment); **wireframe** and edges, **normals**, the **bounds** with their size and
  centre in metres, triangle and vertex counts; Shift+click adds a mesh below the others to **compare**; **Save
  picture** writes `work\tools_out\meshview\<names>_<view>.png` (a preset's is `meshview.py --png`'s). It reads the
  same index and cache as the mesh viewer (`work\tools_out\meshview_index.pkl`, built in Rust when stale).
- **`bc2_convert::meshview`**: the mesh viewer's index cache, `load_mesh`, `stack_models` and `render` in Rust,
  identical to the Python on 18 levels of detail of 8 real meshes (94 parts, 55 pictures; `rust/BENCHMARKS.md` GR3).

### Changed
- `bc2_render` (Rust API, round R5e): `frame::cbuffer`, `live::plan` and `live::plan_with` take the pixel shader's
  block window (`Option<[u8; 2]>`, before `extra`); `material::Draw` has a `ps_window` field; new
  `frame::material_window` and `bc2_formats::shaderdb::query::block_registers`. No caller outside `bc2-render`.
- **`meshview.py <mesh>...` writes the picture** (`<names>_<view>.png`; `--png` is still accepted) instead of the
  page; `--view`, `--size`, `--lod`, `--out` and `find` are unchanged (`bfbc2.exe meshview` too).

### Removed
- **The mesh viewer's page for a browser** (round GR3): `meshview.py` no longer writes `<names>.html` (three.js, the
  models embedded). The editor's Asset preview replaces it (`bfbc2-map-editor/EDITOR_ARCHITECTURE.md` 9.3 lists
  where each feature went). `meshview.page()` stays, deprecated, only because Project OPFOR's WW2 tools
  (`attachments.py`, `scopes.py`) still write their preview pages with it; `RETIRED.md` has the record.

### Fixed
- **The editor tests' hang** (round HG1): two tests making GPU devices at once could deadlock for good on NVIDIA's
  driver (a Vulkan device made while another thread exited through the driver's `DllMain` holding Windows' loader
  lock; 4 hangs in 9 loaded runs). The headless device is made once per process and shared, on Vulkan / Direct3D 12
  only (0 hangs in 30). A test held past `BC2_TEST_HANG_S` seconds now ends the editor test binary with the running
  tests' names and, with `BC2_TEST_HANG_STACKS=1`, each thread's stack (`rust\hang_stacks.py`, from a minidump); the
  quick tier sets both.
- **Exports without the bundle cache** (round HG1): the shader reach and the closure's copies read one partition at a
  time, each a decompression of a bundle payload of hundreds of MB; they now read a step's partitions together. A WW2
  canary export with `BFBC2_FBRB_CACHE=0`: 1,887 -> 96 s, the stage byte for byte the same.
- **The walk camera and surface placement on large levels** (round HG1): a ray tested every one of the terrain's two
  million triangles; the scene now keeps a plan grid of them and each blueprint's radius (about 0.5 ms a ray instead
  of 35 on SP_01, the same hits bit for bit).
- **EX1's preview test** (round HG1) prepares its canary under an id of its own per run, so two runs at once no longer
  share OPFOR's `work\editor\levels` folder.
- **Map editor: the WW2 levels' open sea is drawn** in the game-accurate view (round R5e). Past the shallows, Wake
  Island, Guadalcanal, Iwo Jima and Coral Sea showed a black (or dark grey) plane to the horizon; they now show the
  deep blue sea with the sun's highlight. The view wrote a material's constants only into a shader variable named
  `constants`; OPFOR's translated BF1943 water names its material `waterMaterial`, so its depth factors stayed 0 and
  the water drew nothing at all. A program's material now goes to the registers its parameter block's header names
  (spec RNDR-DRAW-8, RNDR-WATER-3). DICE's own levels draw exactly as before.

## [0.10.0] - 2026-11-21

Released by round QW2 under the rule above, for what rounds RT1 to RT3, GR1, GR2, EX1 and FM1 removed since 0.9.0
(command lines Project OPFOR called among them). Project OPFOR needs `SUITE_REQUIRES = ('0.10.0', '0.11.0')`.

### Added
- **A project lock and a check before every save in the map editor** (round QW2, audit AU1 R4 / G-N17). An open
  project holds `<project>.mapedit.json.lock` (the PC, the process, since when); a second editor refuses it in
  words ("... is already open in another editor (on this PC, process N, since ...). Close it there first ..."),
  a lock whose process is gone is taken over, and the lock goes when the project closes. Each save first checks that
  the file is still the one this editor read or last wrote (its modified time and length); if another editor or
  tool wrote it, nothing is saved, the message says so, and the changes stay in the autosave.
- **`mapexport check` refuses streamed wave data** (`_waveDataSns_` entries) in any staged bundle (round QW2, audit
  AU1 RISK-4; spec SND-STRM-1: the load crashes at 0x49EDF4).
- **Exit codes documented** (round QW2, audit AU2 Q7): README "When something goes wrong".
- **CI builds each of `bfbc2.exe`'s tool groups alone** (round QW2, audit AU2 A18; `checks.yml`); seven groups
  failed to (dead-code warnings in the shared argument formatter), now fixed.
- **Faster test tiers** (round TS1). The quick tier's single cargo entry (fmt, clippy, the workspace's tests: about
  53 minutes alone, every CPU) is four entries - the test build, the quick tests, clippy, fmt - and the tests run
  beside clippy and the Python entries. The whole-level editor, levelwrite and renderer tests (35, marked
  `#[ignore = "game tier: ..."]`) moved to the game tier's new entry; the quick tier keeps one real level per crate.
  Dependencies are optimised in debug and test builds. Crates unchanged since their last green run on this PC are
  left out of clippy and the quick tests, and said (never on CI, in the full tier or with `--no-skip`). Every Rust
  test's own time is kept (`%LOCALAPPDATA%\bfbc2-tool-suite\rust_test_times.json`) and the slowest are printed.
  `tests\test_rust.py list` names every test with its tier.
- **An optional compile cache**: `BFBC2_COMPILE_CACHE=1` with sccache installed runs every cargo of the suite through
  it. Off by default: it serves rebuilds of one build folder but not other worktrees' (cargo's per-folder `OUT_DIR`
  stays in sccache's key; README, "The Rust core").
- **`run_tests.py --timeout S`** and a default time limit per test (4 times its kept duration, at least 30 min /
  3 h / 8 h by tier): a hung test is ended with its processes and fails. The run's results and total go to the day
  log, not only "started".
- **Three test files in a tier at last**: `test_aimanim.py`, `test_column_sectors.py` and `test_trees.py` (quick, and
  game with `--game`); `tests\test_repo.py` fails on a test file no tier runs, and on a new runner entry or
  environment variable named by a round code.
- **Map editor: typed gameplay operations** (round XF1): set a team, link or unlink a soldier spawner and a capture
  point, set a team's tickets, insert or remove an outline point (the `Next` / `Previous` chain and the area's point
  kept). The project format is 5 (VW2 took 4); a version-4 project loads as it is, a newer one is refused in words.
- **`mapexport check`: shader and cull-shape lines** (round XF1): every mesh the level reaches has entries in its
  shader databases; every re-slotted object stands in its own blueprint's shape; no new object at slot 0 that some
  level slots.
- **The game copy's install ledger** (round DF2): every tool that writes the game copy records what it installed in
  one ledger (`<game>\opfor\ledger`, Project OPFOR's `opforcore.ledger` format): what each install replaced is kept
  once by its sha256, a revert gives back exactly what was there before that tool in any order, and an install or
  revert cut short (a locked file, a full disk, a kill) is rolled back by the next command. `bfbc2 gameledger
  status|holders|original|recover <game>` shows it and finishes an interrupted one. The suite's half:
  `bc2_formats::gameledger` and its Python twin `bfbc2core.gameledger`.
  Project OPFOR's half (its own DF2 branch): every Onslaught stage, the Pacific War install and menu mode, the map
  export's install and the import scripts write through `opforcore.stages` on the same ledger, and
  `opforcore\migrate.py` moves a game copy's old `.opfor-*` backup markers into it without losing an original.
- **Map editor: the mode view layer** (round VW2): show every object, the game mode layer only or the level without
  it, and a Rush level one stage at a time as it is played (L1's simulation of the stages' events: MP_005GR,
  MP_005SR, NAM_MP_002R and BC1_OASIS_GR give L1's bases per stage), in both views, the outliner and the overlays.
- **Map editor: weather in the views** (round VW2): every state of the level's weather system in a picker (night,
  dawn, fog, rain and cutscene states marked; the spawn-time switch and ActiveState named); the game-accurate view
  draws a state's light, exposure and sky and opens on the state players see (the spawn switch: MP_005's `day_1`,
  not its dark `default`); the simplified view is tinted by the state's look.
- **Map editor: weather in exports** (rounds VW2a-c; project format 4, `set-weather`): the Export window's Weather
  section makes the level start in one of its own states (ActiveState and the spawn switch set together), in another
  level's state (appended: this level's sky; or that level's whole system: its sky), or at night (DICE's
  `ons_mp_002_night` as the template, the reflection cube and the sky panorama darkened). Every new partition lies
  under `weather/<level>/` in the level's own bundle; `mapexport check` checks the weather. Night and rain are off
  by default and marked "not yet seen in game".
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
- **Code moved between crates** (round QW2, audit AU2 A5): `guid::new_guid` into `bc2-formats` (from `bc2-stockmp`,
  which re-exports it), so `bc2-levelwrite` and with it the editor no longer build `bc2-stockmp`; the 32-bit PE
  reader `pe` into `bc2-formats` (from `bc2-harness`, which re-exports it), so `bc2-menu` no longer builds the
  harness. Same answers: the moved modules' tests pass unchanged.
- **Map editor: the Game-accurate view's sky, levels of detail and labels** (round R5d). The weather's sky dome is
  drawn behind the level (DICE's `EnvironmentMain` with the weather state's sky, sun and halo values: a blue sky over
  MP_005 instead of the flat fog colour; the panoramic clouds are left out until their composite is proven). Far
  terrain patches are drawn coarser (every 2nd to 8th sample past 200 m, their edges kept whole so no cracks open)
  and objects switch to their further levels of detail at the distances their mesh sets' LOD settings give (MP_005's
  wide view: terrain indices 10.7 M to 1.7 M, object indices 6.2 M to 2.9 M). Labels whose point lies behind what the
  game frame drew are hidden (the selection's never). Package levels (Vietnam, the Pacific War) open in 1.4 - 4.6 s
  instead of 14 - 18 s: their bundles are read at once and through the suite's bundle cache.
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
- **The Map Inspector** (`bfbc2-map-inspector/inspector.py`, round GR1): its 3D view, layers, search and Selection
  panel were already the editor's; its property tree, "referenced by", link lines and Measure are the editor's All
  fields, Links and Measure (above). The hub's Levels tab opens levels in the map editor; the folder keeps
  `EDITOR_ARCHITECTURE.md` and a README saying where each feature went.
- **`bfbc2-map-inspector\`** (round FM1): its README's table is `bfbc2-map-editor/EDITOR_ARCHITECTURE.md` 9.1, the
  plan moved to `bfbc2-map-editor\`. **The placeholder folders** `bfbc2-ai-behavior-simulator\`,
  `bfbc2-bot-ai-debugger\` and `bfbc2-tactical-map-authoring-tool\` (a README each, never built): they are planned
  map editor workspaces (`EDITOR_ARCHITECTURE.md` section 10). `RETIRED.md` has the last commits that had them.

### Fixed
- **O4's texqa goldens failed on every run since round DF2** (round QW2): texqa's cache keys hash texqa.py's own
  source, so DF2's edit of `texqa.Dice` re-keyed the cached rows of three cases; the digests were not random (one
  per texqa.py revision). The test now compares the cache with that hash taken out of each key it can rebuild, so
  an edit of texqa.py no longer breaks it and a wrong key still does.
- **O2 on PCs with Microsoft Defender** (round QW2): the stand-in `b.exe` of the aptrefs cases is a loadable image
  (no holes between its sections, an import table), which Defender's machine-learning scan no longer blocks as
  `Trojan:Win32/Bearfoos.A!ml` (Errno 22). No exclusion or security setting is needed.
- **`texqa.Dice`'s reading of the install ledger's originals has a test** (round QW2): a stand-in game copy with a
  ledger.
- **Tests that passed by skipping** (round TS1, audit AU3 TS-7): the simplified-view, view-geometry and view-toggle
  tests turned a level that did not open, a game view that did not start or render meshes not read into a "skipped"
  that passed; with the level in the game copy each is now a failure. The Rust tests' "<level>: skipped, ..." lines
  are counted and named per test.
- **The test runner** (audit AU4 DF4): `--only` matching nothing is exit code 2 (it said "0 of 0 passed", exit 0);
  every test's processes end with the runner however it ends - Ctrl+C, a crash, killed - (its job objects are
  kill-on-close), and a passing test's leftover child process no longer outlives it.
- **`build_native.py`'s install** retries a target held for a moment (a scanner), and when the new file cannot go in
  puts the live one back instead of leaving it moved aside.
- **Map editor: side state no longer outlives its project** (round DF1, audit AU4-SM-1 / -2 / -4): a project
  generation is counted each time a level or project is shown or the map is closed. A Create that fails no longer
  leaves its era / mode target for the next project opened (it was committed and saved into that project). The
  Export window lets go of its project when the map closes (it wrote the old project's names into the next one and
  exported the old one) and never writes into a project it was not opened for. Close map ends a preview first, and a
  preview answered after the project changed is dropped.
- **Map editor: Export, Install and the navigation commands wait for each other** (round DF1, audit AU4-CON-1 / -2):
  Export is disabled while an install or uninstall runs and Install now / Uninstall now while an export runs, each
  with the reason. Install now installs the stage its plan was shown for and refuses one exported again since
  (AU4-SM-3); the stage path is computed in one place from the trimmed level id. Navigation Install, Restore and
  Build wait for a running navkit command, and Close map waits too; install and restore run in the background like
  the build (the window no longer freezes); a navkit command still running when the editor closes is stopped and its
  half-written file removed.
- **Map editor: Export reads the project's own saved state** (round DF1, audit AU4-DS-4): not the autosave file's
  absence (a failed autosave writes none, so an unsaved session could be exported as the file on disk).
- **Map editor: a worker that fails is reported** (round DF1, audit AU4-BUG-4): an export, profile scan, preview or
  install whose worker panicked now ends with an error line (its panic's text) instead of "Exporting..." forever.
- **Map editor: closing the window asks while work runs** (round DF1, audit AU4-DS-13): an install, export or navkit
  command running when the window is closed is named, with Wait or Close anyway; the install's output goes to a log
  file, so a closed editor no longer breaks its pipes mid-install.
- **Map editor: the game-accurate view after a GPU reset** (round DF1, audit AU4-BUG-9 / -15 / -26, CON-11): a
  removed Direct3D device is an error (it was reported as a finished frame), the view is turned off and loads anew
  when turned on again; a failed game-view pick picks nothing and says why (it fell back to the hidden simplified
  view's object); the read-back refuses a non-RGBA8 pass; each shared frame is written only after the editor's
  device finished the frames that sampled it.
- **Map editor: textures freed with their views** (round DF1, audit AU4-CQ-2): closing a map or a preview frees its
  views' and the game view's textures in egui's renderer (they stayed in video memory for the session).
- **Map editor: small fixes** (round DF1): the recent projects compare paths as Windows does (case and slashes,
  AU4-CQ-8); the export settings are written whole and a failed write is said (AU4-BUG-14).
- **Map export: no voice-over without its bundleManifest line** (round DF1, audit AU4-BUG-10, spec XEXP-REG-2): a
  `bundleManifest` that does not read stops the export (it was read as empty: `package.mft` blocks without their
  lines, the checksum crash); a donor with voice-over on a game copy without one is refused. The startup, overlay
  and awards patches remove their scratch unpack on errors too (AU4-CQ-10).
- **Map export: objects from other levels and sets get their shader entries** (round XF1): the export merges into
  the level's Dx9 / 10 / 11 databases the entries of every mesh it reaches that the base lacked (placed objects,
  another mode's layer, another era's kits), from the levels that draw them. Without them the game's renderer reads a
  null program (0x98BD6E).
- **Map export: each object's own cull shape** (round XF1): placed, copied and grafted objects take their own
  blueprint's shape (from the base or another level), never their template's or another level's slot number.
- **Map editor: undo, redo and History no longer replay the whole log** (round XF1): checkpoints and a cached donor
  level; on Aurora as WW2 Conquest with 40 moves an undo went from 9-11 s to under 0.1 s.
- **Map editor: Paste copies what an object owns** (round XF1): a pasted flag has its own capture area and marker,
  so editing the copy leaves the original alone.
- **NavKit and stock-MP could lose DICE's path data** (audit AU4 DS-1): both installed `pathdata.hpd` with their own
  backups and did not see each other; installing NavKit, applying stock-MP, restoring NavKit and reverting stock-MP
  left NavKit's build where DICE's file was, and DICE's original was reported as NavKit's. Both now install through
  the ledger (owners `navkit:<lv>`, `stockmp:<lv>`): a restore under another tool's install takes NavKit's layer out
  from under it, every order of the two ends with DICE's file, and `original_file()` (the NavKit driver, `bfbc2.exe`,
  the map editor) answers the ledger's bottom layer. Their markers of before the ledger are adopted on first use;
  another tool's are refused in words.
- **stock-MP install** (audit AU4 DS-5, DS-6, DS-9, DS-19): `--apply` is one journalled commit (a failure part way,
  the old `Fail::Python` early return included, can no longer leave Dist and Package from different builds); an
  interrupted `--revert` is finished by running it again; the revert reads the ledger and the level's own folders,
  never the whole game folder (junctions and odd names elsewhere no longer refuse it); a stale backup is never
  restored over a newer file; the advice to delete markers is gone.
- NavKit: `tasklist` given 30 s (a hung one counts as the game running, AU4 BUG-20); a level's unpack goes to a
  temporary folder swapped in under a lock, never `rmtree(ignore_errors=True)` under a stamp that says clean (DS-14,
  Python and Rust); the harness, stock-MP report and texqa JSON written through per-process temporary names and the
  retrying replace (CQ-18); one SHA-256 in Rust (`bc2_formats::sha256`, CQ-9).
- **Map editor: the game-accurate view no longer draws the objects a project deleted or hid** (round VW2).
- **Map editor: Wake Island's and Coral Sea's black ground in the Game-accurate view** (round R5d). Three causes in
  the renderer, none in the data: the terrain's height scale was read as the database's placeholder 500 (Wake's
  terrain spans 1,000 m, so BF1943's sea-floor layers drew the beach and shallows as deep water: black); the water did
  not read the terrain's height map; and every terrain cell stretched its whole mask tile, where a tile spans a block
  of 8 x 8 or 4 x 4 cells (each cell repeated the block's mask: the ragged, blocky shorelines and sand of the WW2
  levels, and of DICE's own levels' tiled layers). Wake's lagoon view: 52.8 % of its pixels black before, 0.0 % after;
  Coral Sea's 10.2 % to 0.3 %. The deep sea past the reefs stays dark until the haze over the scene is drawn.
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
