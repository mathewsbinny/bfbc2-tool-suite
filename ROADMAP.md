# Roadmap

The one place for the suite's priorities and its queue of rounds. Keep it current: a round moves to **In progress**
when its branch starts, to **Done** (with a one-line result) when it is merged to `main`, and the priority order
changes only when the user changes it. Each round's evidence stays where it is: its commit messages, its section of
`rust/BENCHMARKS.md`, and `bfbc2-map-inspector/EDITOR_ARCHITECTURE.md` for the editor. Released versions are in
`CHANGELOG.md`; how a round reaches `main` is in `RELEASING.md`.

Last updated: 2026-11-19 (round P1). Version: 0.9.0 (`VERSION`).

## Priority order (the user, 2026-11-19)

1. **The map editor's start and export:** G4 (the start flow), E3b (export profiles, the level's own name and
   picture, the campaign strip checklist) and E3c (the announcer's voice-over follows the export's profile: WW2 uses
   BF1943's voices, Vietnam the Vietnam DLC's, BFBC2 its own) are done.
2. **Rust porting:**
   - **O5**, the last command-line batch: render QA, xdiff, the command lines of shaderdb, the BF1943 importer and
     the console readers, and the Python cores behind O3 / O4's hand-offs.
   - The full test tier, then retiring the Python twins (E4a-f, F0, F2a, F2b, F3, and the O1-O5 command-line
     Python).
   - The shaderdb reader port.
   - **F4**, F3's leftovers: the disassembly's header lines, audio decoding and its ffmpeg glue, hkpack's object
     reading with a handle-based Packfile, navsample_offline's hull geometry, meshview's readers, texqa's readers,
     culltree's string table.
   - **culltree `add`'s memory:** the Rust peaks at 1.65 GB against the Python's 200 MB warm; cut it and add a
     persistent cache.
   - **The GUIs:** per GUI, list what the editor lacks, add it, then retire the GUI.
     - ~~Map Inspector: the link view, the full property tree~~ (GR1: All fields, Links, Measure; retired). The
       other GUIs' features, whether the editor has them and the plan: `EDITOR_ARCHITECTURE.md` section 9.
     - ~~GR2: Navigation Studio into the editor's AI workspace~~ (GR2: both files side by side, the colourings, path
       test, anomaly and compare overlays, edits in the project's log, build / install / restore; retired, its
       `overlay.py` kept).
     - GR3: the mesh viewer into the asset browser's preview (its PNG command line stays).
     - GR4: DbxEditGUI into a generic DBX inspector panel (the last C#).
     - LevelDiff as an editor panel (the visual diff) and a `bfbc2.exe leveldiff` command for scripts (proven on
       real level pairs), then LevelDiff.exe retired (round EX1).
     - The planned AI behaviour simulator, bot AI debugger and tactical map authoring tool become editor workspaces.
     - The hub stays a launcher for now.
   - **OPFOR's pipeline in Rust, in 5 stages:** the WW2 install; the level build; the shader glue; the material grid
     and pictures; Onslaught, menus and testrun. The map export's install side goes with it, and `rebuild_all`
     becomes a Rust step graph.
   - The process items of round P1 not finished there (see **Process** below).
3. **R5d, the editor rendering's visual gaps:** Wake Island's black terrain cells, the sea under the carriers, the
   sky pass, run-varying far-water pixels, game LODs and instancing, the whole terrain with LOD, labels occluded by
   depth, faster Package loads.
4. **The 10 post-Rust improvements** (algorithms, architecture, naming).
5. **Tests ported to Rust.**
6. **OPFOR quality round 2:** the mypy baselines down to 0; the tests baseline (drifted from 193 to 207).
7. **Then:**
   - the editor's **E4** gameplay tools: add and delete flags, spawns, Rush stages with M-COMs, teams / tickets /
     round rules from the game's preset options, the stock formula by default;
   - **E5** terrain sculpt and paint;
   - **E6** vegetation;
   - **E7** Create Level from a height map (with a terrain writer);
   - the rendering that needs a RenderDoc capture: the exact sun, exposure and fog.
8. **Last:** the full suite release (one download: the prebuilt module and the C# tools). The wiki-style docs
   restructure is on hold (the user, 2026-11-20).

## In progress

| Round | Branch | What |
|---|---|---|
| GR1 | `rust-gr1` | the five GUIs inventoried (`EDITOR_ARCHITECTURE.md` section 9); the Map Inspector folded into the editor (All fields, Links, `view-link` lines, Measure) and retired |

## Queued

In priority order (above); the next rounds:

| Round | What |
|---|---|
| VW1 | after SV1, the editor's views (the user, 2026-11-20): toggles that show particle effects, dynamic lights and sound emitters with their reach; toggles for combat areas, capture radii, spawn protection zones and out-of-bounds edges drawn as the game uses them; an optional player's-eye camera (soldier height, the game's field of view); the navmesh and cull tree view modes checked and fixed; collision per destruction state (the Havok per-part bodies split out in the scene loader), selectable with the destruction-state picker, which gains up / down arrows to step through the states (the user, 2026-11-20) |
| XF1 | right after EX1 (the user, 2026-11-20; audit AU1): objects placed from other levels get their shader entries in the export (0x98BD6E) and their own cull shapes (not the template's; unslotted placements checked); the donor level cached and the undo replay checkpointed (undo no longer replays the whole log); deep-duplicate paste (owned sub-objects copied, not shared); typed gameplay operations |
| QW1 | now (approved 2026-11-20; audit AU2's best-value list): the editor / levelwrite / render tests run once per quick tier; dev debug-info lowered and one integration-test binary per crate; the test scheduler's memory declarations from recorded peaks; a stale `bfbc2.exe` warns (strict mode) and each artefact has its own fingerprint; one suite locator in OPFOR with a version check; superseded FBRB cache payloads pruned; one env-variable registry and a pinned Python requirements file; mapexport's staleness check; one GUID helper for OPFOR's uuid5 sites; skipped tests counted as skipped; the matplotlib flake; stale test-time names |
| VW2 | the mode view layer (show / hide by game mode and Rush stage, on the project's mode layer data); time-of-day and weather switches (the weather states each level holds) in both views; Guadalcanal's game view (weather partition not found); Vietnam levels in the game view |
| EX1 | export: a preview mode comparing the export with the original (side by side or toggled), every name of the level editable (file / folder name, in-game level id, on-screen name per language, server browser name), and a visual diff of a project against its base; LevelDiff folded in as an editor panel (a `bfbc2.exe leveldiff` command kept for OPFOR's scripts), LevelDiff.exe retired |
| FM1 | after GR2: the map editor's own folder `bfbc2-map-editor\` (design document, README, run instructions; the code stays in `rust\bc2-editor`), `bfbc2-map-inspector\` deleted and every reference fixed; the retirement policy: `RETIRED.md` (date, replacement, last commit, round) with the Map Inspector, graphstats.py and prepare_level.py; the three README-only planned-tool folders folded into the design document as workspaces and deleted |
| RT3 | RT2's leftovers: hpdtool.parse's and the Wave readers' messages ported, then retired; the library parts split out of the command-line `.py` files that are still imported (aptrefs, xiso with a bytes entry point, culltree, stockmp, crashreader), then their command lines retired; argparse's option abbreviations; the kept references in BENCHMARKS RT2 |
| GUIs | ~~GR2 Navigation Studio~~ (done), GR3 mesh viewer, GR4 DbxEditGUI (LevelDiff in EX1): each folded into the editor, then retired (`EDITOR_ARCHITECTURE.md` section 9) |
| OPFOR in Rust | 5 stages (WW2 install; level build; shader glue; material grid and pictures; Onslaught, menus, testrun); the export's install side; `rebuild_all` as a step graph |
| R5d | the editor rendering's visual gaps (list above) |
| Post-Rust | the 10 improvements |
| Tests in Rust | the Python tests ported, with a synthetic test corpus generated from the specification |
| OPFOR quality 2 | mypy baselines to 0, the tests baseline |
| V1, L1, A1 | the export's robustness: a compatibility / validation layer (plain errors, spec rules); DICE's placement tendencies studied on the stock maps, driving editor warnings ("stage 3 is 40 m from stage 2"); an optional auto shaping / layering of a mode's gameplay; exports regenerating path data with NavKit and the bot module |
| Spec layers 3-4 | the engine subsystems; the suite's transformations (grows with each porting round) |
| E4-E7 | gameplay tools, terrain sculpt and paint, vegetation, Create Level from a height map |
| RenderDoc | exact sun, exposure and fog |
| Release | the full suite release |

### Process (round P1's follow-ups)

- Branch protection on `main`: off (the user's choice). `v0.9.0` is tagged; older versions stay untagged.
- The game tier still runs only on the development PC (it needs the game copy); CI runs the quick tier's
  game-free entries.

## On hold (until the user picks them up)

- The first in-game test of an exported Aurora level.
- Pacific War: the air raid, then repair stations, then Air Superiority / Coral Sea.
- The Dx9 shader port.
- Translating the WW2 weapons and soldiers.
- WW2 navmesh planning.
- A faithful Xbox Onslaught port without Napisal's work.
- The wiki-style docs restructure (topic articles of current truth, dated reports as their sources, one
  reverse-engineering reference).
- In-game tests and playthroughs (paused 2026-11-20; the work continues offline).

## Done

Most recent first; each round's details are in its commit messages and its `rust/BENCHMARKS.md` section.

### Process and specification

| Round | Date | What |
|---|---|---|
| SP1 | 2026-11-19 | the engine specification, layers 1-2 (`spec/`): 439 rules (250 verified, 48 implemented, 134 observed, 7 inferred), `spec/trace.py` checked by the quick tier |
| P1 | 2026-11-19 | `ROADMAP.md`, `CHANGELOG.md`, `VERSION` 0.9.0 (tagged), `RELEASING.md`, CI (`checks.yml`), `run_tests.py quick --no-game` |

### The map editor (`rust/bc2-editor`, `rust/bc2-levelwrite`, `rust/bc2-render`)

| Round | Merged | Result |
|---|---|---|
| E3d | 2026-11-20 | any map exported to any era and any of its multiplayer modes: era and mode as project data (the donor's mode layer as editable objects), cross-set closure copied into the level's bundle, the target era's kits / teams / announcer, Era / Mode pickers, Install / Uninstall in the editor, 15 compatibility rules; six canaries pass check and install / revert on a scratch copy (BFBC2 to WW2 Air Superiority pending that mode). Not yet tested in game |
| E3c | 2026-11-19 | the exported level's announcer voice-over follows its profile (the donor's banks, trigger sets, vo blocks and voice-over bundles); Aurora passes the check on all four profiles |
| I1 | 2026-11-19 | the suite's application icon (the user's M67 render) in `bfbc2.exe`, `bfbc2-editor.exe`, `mapexport.exe`, the editor's window and the hub's |
| E3b | 2026-11-19 | exports for every content profile (BFBC2, Vietnam, WW2, BFBC1) with the level's own name and picture, and the campaign strip checklist |
| G4 | 2026-11-19 | the start flow: Open project, Edit existing level (multiplayer maps), Create level (from a level or a height map) |
| R5c | 2026-11-19 | the game-accurate view interactive and editable, the picture unchanged (live draw list, culling on a grid) |
| E2b | 2026-11-19 | context menus, edit modes, sink by mouse, deep duplicate, the asset browser (content set, Spawners, Objectives) |
| E3 | 2026-11-18 | export of an edited level as a new multiplayer level (levelwrite export, mpconvert, culltree; the Export window) |
| R5b | 2026-11-18 | the game-accurate view draws the whole level, pixel for pixel against the C++ render harness |
| R5 | 2026-11-18 | game-accurate rendering, phase 1: the study and a prototype (`bc2-render`, Direct3D 11) |
| G3 | 2026-11-18 | the levels that live only in `game\Package` (Vietnam, the Pacific War, BFBC1); the "Choose mode" start screen |
| E2 | 2026-11-18 | editing an existing map, first part: the map project and its operation log, levelwrite, the Objects workspace |
| G2 | 2026-11-18 | the user's decisions on G1: faction team labels, 3D-view tokens, Help from the action registry |
| G1 | 2026-11-18 | the editor's shell and read-only map view (egui on wgpu, `bfbc2-editor.exe`) |

### Command lines in Rust (`bfbc2.exe`, `rust/bc2-cli`)

| Round | Merged | Result |
|---|---|---|
| O5 | 2026-11-20 | shaderdb (with merge), renderqa, xdiff, apt, stfs, xiso, bf1943import, stockmp's pipeline, detailed and navkit status in `bfbc2.exe`; 745 recorded lines identical, 194 OPFOR calls identical, stockmp builds byte-identical. Still Python: renderqa / xdiff drawing, bf1943import import / check, navkit build / apply (listed in BENCHMARKS O5) |
| CT1 | 2026-11-19 | `culltree add` streamed: 1.65 GB / 7.3 s down to 80-90 MB / 0.7 s warm; row cache shared with the Python; outputs identical on the 8 Onslaught bundles |
| O4 | 2026-11-19 | NavKit's driver and tools, the editor's prepare, the test harness and texqa in `bfbc2.exe`; 622 recorded lines identical, prepare byte-identical on 56,831 files |
| O3 | 2026-11-19 | caches, stock-MP, aptgraft, audio, BF1943 parity tools, meshview and culltree in `bfbc2.exe`; 1,220 recorded lines identical |
| O2 | 2026-11-19 | the menu, crash-reader and NavKit / converter command lines in `bfbc2.exe`; 966 recorded lines identical |
| O1 | 2026-11-18 | the orchestration inventory; FbrbTool's and DbxEdit's command lines as `bfbc2.exe fbrbtool` / `dbxedit` |

### The Rust core (`bc2_native`) and retiring the Python

| Round | Merged | Result |
|---|---|---|
| D1 | 2026-11-19 | the disk budget: cargo builds outside the checkout, binaries in `bin\`, one budget for every cache |
| S1 | 2026-11-18 | streamed bundle packing, process-wide BF1943 restart points, the shader coverage index |
| RT2 | 2026-11-20 | record then retire (tests/replay.py, `native.Refused`): the terrain, mesh, texture, Havok, havokport, mopp, navtune, xmlinst, menu and PE references, navsample.load v1 / v2, the hpd and audio references after their messages were ported; `--help` and usage at the console's width in `bfbc2.exe` (943 of 943); OPFOR 380 tests, WW2 rebuild 10,852 of 10,852 |
| RT1 | 2026-11-20 | full tier 232 of 232 (5 h, -j 2); retired graphstats.py, the editor's prepare_level.py, aimanim's and navsample_offline.file_bytes's references; `--download` also fetches `bfbc2.exe`; the inventory of every command line and reference with its fate (BENCHMARKS RT1); OPFOR 380 tests, WW2 rebuild 10,852 of 10,852 |
| SD1 | 2026-11-20 | the shader database reader's last Python (open / dispatch, pick_database, dx from name) in one Rust module shared by bc2_native and `bfbc2.exe shaderdb`; 745 databases, 10,068 picks, 89 levels' loads identical; the in-process Merger and path glue stay Python |
| F4 | 2026-11-20 | F3's leftovers in Rust: the disassembler's header, audio decode and ffmpeg glue, hkpack's handle-based Packfile, navsample_offline's hulls (5 tied Railway_Switch hulls still Python), meshview's and texqa's readers, culltree's string table; all identical to the Python; WW2 texture QA 212 s to 134 s |
| F3 | 2026-11-18 | the rest of the Python format readers and writers in Rust (aimanim, hpdtool's mesh layout, audio framing, navsample, hkpack, the Xenos disassembly) |
| F2b | 2026-11-18 | the BF1943 importer's remaining Python (mopp, havokport, dbxcatalogue, emitteratlas, meshport's layout search), the PE reader, xmlinst, navtune |
| F2a | 2026-11-18 | the console converter's rest, the model tools and Granny's writing side in Rust |
| F1a | 2026-11-18 | the core format readers' and writers' Python retired (dbx, fbrb, terrain, water, collision, level) |
| F0 | 2026-11-18 | the file formats with no Rust yet ported (path data, localization, BIGF, STFS / XISO, Wave, mesh headers) |
| R4 | 2026-11-18 | DbxEdit.exe and FbrbTool.exe retired as tools (their commands in process, recorded answers kept) |
| R3 | 2026-11-18 | the shader translator's Python HLSL emission retired |
| R2 | 2026-11-18 | the converters' ported Python retired |
| R1 | 2026-11-18 | the rest of NavKit's ported Python retired |
| E4f | 2026-11-18 | the map and bot test harness and the crash-reader tools in Rust (`bc2-harness`) |
| E4e | 2026-11-18 | NavKit's tools around the build in Rust |
| E4d | 2026-11-18 | dbxport's partition port in Rust (and its parity fix) |
| E4c | 2026-11-18 | aptgraft's movie model, aptcheck and aptrefs in Rust |
| E4b | 2026-11-18 | the shader database logic in Rust (a handle, its queries) |
| E4a | 2026-11-18 | the stock-MP AI layer's logic in Rust |
| E3r | 2026-11-17 | the rest of the BF1943 importer faster by measurement (no Rust needed) |
| E3q | 2026-11-17 | logs capped by size and day, archived before pruning |
| E3p | 2026-11-17 | cache housekeeping: one size budget, least recently used first |
| E3o | 2026-11-17 | navdata, kymesh, navpath, havok.verify and texqa faster, each against its reference |
| E3n | 2026-11-17 | the menu tools' APT decoder in Rust |
| E3m | 2026-11-17 | the sweep of the unprofiled tools, ranked by expected gain |
| E3k | 2026-11-17 | FbrbTool's commands in process (list 4-48x, unpack 2.6-6.3x faster) |
| E3j | 2026-11-17 | the stock-MP AI layer's bundles unpacked in process |
| E3i | 2026-11-17 | Granny skeletons and animations in Rust |
| E3h | 2026-11-17 | the rest of NavKit's generator pieces and parity measures in Rust |
| E3g | 2026-11-17 | the BF1943 model, physics and effect ports faster, byte-identical |
| E3f | 2026-11-17 | the Xbox 360 shader translator in Rust (`bc2-shader`) |
| E3e | 2026-11-17 | the bundle cache: repeated reads no longer decompress again |
| E3d | 2026-11-17 | HLSL compiles cached and on all cores; dbxport's ranking from an index |
| E3c | 2026-11-17 | the rest of NavKit's generator's hot paths in Rust |
| E3b | 2026-11-17 | DbxEdit's commands in process through the Rust core |
| E3a | 2026-11-17 | the EA-XAS audio encoder and decoder in Rust |
| E2a | 2026-11-17 | NavKit's walkability rules, areas and cell picking in Rust |
| E1a-e | 2026-11-16 | the level loader, terrain / water / collision, the writers (zlib, DBX), shader databases and cull trees, textures and NavKit's grid walks, the mesh alias table in Rust |
| E0 | 2026-11-16 | the Cargo workspace, the DBX and FBRB readers in Rust, `bfbc2core.dbx` on Rust with a Python fallback |

(The Rust port rounds E0-E4 and the editor rounds E2, E2b, E3, E3b share a letter; the tables keep them apart.)

### Before the Rust core

| Round | Date | Result |
|---|---|---|
| Deep scan | 2026-11-15 | atomic installs and writes, honest caches, damaged files reported |
| Quality rounds 1-3, M1 | 2026-11-08 - 11-10 | ruff and mypy on every folder, `suite_paths.py`, `bfbc2core.paths` (the one path finder), the architecture audit |
| The suite | 2026-10-16 | the tools merged into one repository; core M1-M4 (DBX reader, level loader, Map Inspector, Navigation Studio and its editing) |
