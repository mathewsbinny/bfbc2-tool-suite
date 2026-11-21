# Roadmap

The one place for the suite's priorities and its queue of rounds. Keep it current: a round moves to **In progress**
when its branch starts, to **Done** (with a one-line result) when it is merged to `main`, and the priority order
changes only when the user changes it. Each round's evidence stays where it is: its commit messages, its section of
`rust/BENCHMARKS.md`, and `bfbc2-map-editor/EDITOR_ARCHITECTURE.md` for the editor. Released versions are in
`CHANGELOG.md`; how a round reaches `main` is in `RELEASING.md`.

Last updated: 2026-11-21 (round QW2). Version: 0.10.0 (`VERSION`; a merge that removes something releases the next
minor: `RELEASING.md`).

## Priority order (the user, 2026-11-19)

1. **The map editor's start and export:** G4 (the start flow), E3b (export profiles, the level's own name and
   picture, the campaign strip checklist) and E3c (the announcer's voice-over follows the export's profile: WW2 uses
   BF1943's voices, Vietnam the Vietnam DLC's, BFBC2 its own) are done.
2. **Rust porting:**
   - ~~O5, the last command-line batch~~ (O5), ~~the full test tier and retiring the Python twins~~ (RT1-RT3), ~~the
     shaderdb reader port~~ (SD1), ~~F4, F3's leftovers~~ (F4), ~~culltree `add`'s memory~~ (CT1): done.
   - **NavKit's build and apply** (NK1, below): the largest Python left.
   - **The GUIs:** per GUI, list what the editor lacks, add it, then retire the GUI.
     - ~~Map Inspector: the link view, the full property tree~~ (GR1: All fields, Links, Measure; retired). The
       other GUIs' features, whether the editor has them and the plan: `EDITOR_ARCHITECTURE.md` section 9.
     - ~~GR2: Navigation Studio into the editor's AI workspace~~ (GR2: both files side by side, the colourings, path
       test, anomaly and compare overlays, edits in the project's log, build / install / restore; retired, its
       `overlay.py` kept).
     - ~~GR3: the mesh viewer into the asset browser's preview~~ (GR3: the Asset preview and the Meshes mode, the
       mesh viewer's reading and drawing ported, its pictures identical; the page retired, the picture command line
       kept; `meshview.page()` stays for OPFOR's WW2 previews until OPFOR in Rust drops it).
     - GR4: DbxEditGUI into a generic DBX inspector panel (the last C#).
     - ~~LevelDiff as an editor panel (the visual diff) and a `bfbc2.exe leveldiff` command for scripts~~ (EX1: the
       Diff panel and the export's preview; `bfbc2.exe leveldiff` identical to LevelDiff.exe on recorded real level
       pairs; LevelDiff.exe retired).
     - The planned AI behaviour simulator, bot AI debugger and tactical map authoring tool become editor workspaces.
     - The hub stays a launcher for now.
   - **OPFOR's pipeline in Rust, in 5 stages:** the WW2 install; the level build; the shader glue; the material grid
     and pictures; Onslaught, menus and testrun. The map export's install side goes with it, and `rebuild_all`
     becomes a Rust step graph.
   - The process items of round P1 not finished there (see **Process** below).
3. ~~**R5d, the editor rendering's visual gaps**~~ (done: Wake Island's black ground and the WW2 shores' ragged
   edges - the terrain's height scale, the water's height map and the mask tiles spanning blocks of cells -, the sky
   dome, mesh and terrain levels of detail, labels hidden behind the game frame, Package levels opened 4-12 x
   faster). Left, with reasons in `rust/BENCHMARKS.md` "R5d": the instanced programs (`StreamInstancingFloat`), the
   panoramic clouds' composite and `MinScreenArea` (capture), the haze over the scene (the fog: RenderDoc, item 7),
   the far-water pixels (not reproduced in 400 runs). ~~The WW2 deep sea drawn black~~ (R5e, done: the water's
   material registers, not the haze).
4. **PR1, the post-Rust improvements** (re-scoped 2026-11-20 by PR0 against S1, QW1 and audit AU2): stable test
   IDs, a determinism rule and two-seed check, one native-backend registry; one cache layer with content keys,
   restart points instead of whole payloads (and one BF1943 bundle reader), CT1-style memory fixes, one cull-tree
   model and one DBX document, tool logic in library crates with typed errors, one worker pool, sounds decoded in
   process; then round codes out of names, an installable package, one naming scheme and OPFOR's `wake_*` as
   `ww2_*`. Already done: stream packing and the per-level shader index (S1); rebuild_all's step graph is OPFOR in
   Rust's.
5. **Tests ported to Rust.**
6. ~~**OPFOR quality round 2:** the mypy baselines down to 0~~ (done in Project OPFOR, 86220cd: mypy 0 in all 8
   folders, the baseline removed).
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
| GR4 | `gr4` | DbxEditGUI (the last C#) folded into a generic DBX inspector panel, then retired |

## Queued

In priority order (above); the next rounds:

| Round | What |
|---|---|
| SG1 | after XF1 (the user, 2026-11-20; audit AU1): editing static group members (their group files), so WW2 maps (Wake: 2,999 members, about 69 % of its objects) are fully editable; OP1 (done) gave it the multi-file operations, so it lands with or after OP1's first part |
| IN1 | before the public release (the user, 2026-11-20; audit AU1 R42-R49): the level installer in the suite (no Python, no OPFOR needed), a game-folder setting, portable and anonymised project sharing (no PC name, no absolute paths), a small "install this project" tool for players, old export stages pruned; the users' diagnostics (the user, 2026-11-21): `bfbc2.exe doctor` (setup, versions, fingerprints, install state, plain fixes), a one-button anonymised bug-report bundle (doctor, crash reading, logs), Help > Diagnostics in the editor |
| VW2-rest | the rest of round VW2 (left 2026-11-21, reasons in `rust/BENCHMARKS.md` "VW2"): **VW2c** the lamp tool (point-light effect emitters with `ActAsPointLight` from SP_01's donors, placed by the user in the Objects workspace: needs placing a blueprint of another level's bundle with its emitters, E2b's "+bundle" and XF1's shader entries); **VW2d** rain and snow beyond the state's values (the grid of camera-proximity area effects over the combat area and the sound areas, from SP_03 rain / MP_007 snow donors: the effects' emitters under the target's emitter namespace and the atlas entries, the sound partitions and their Wave chunks); VW1's leftovers (overlays following project edits, damage and clear-area triggers, wall collision for the walk camera, the FOV and box-evaluator readings proved); all exported night / rain off by default until seen in game |
| VW3 | with VW2-rest, after E5 (the user, 2026-11-21): a weather editor - a state's own values edited, not only picked: the sun (direction, colour, intensity), fog (start / end, colour), ambient and sky tint, exposure and the other fields WX0 inventoried (`weather_inventory.json`), as sliders and colour pickers in a Weather panel; both views follow live (the game-accurate view redraws the edited state, the simplified view re-tinted); the edited state saved in the project (a typed operation, project format bump) and exported as the level's state (a new state appended beside DICE's, never DICE's overwritten); reset to the picked state; values held to the ranges seen across DICE's 355 states with a warning outside them; canaries byte for byte when nothing is edited, off by default until seen in game. No re-bake (WX0: nothing baked depends on the sun) |
| GUIs | ~~GR2 Navigation Studio~~ (done), ~~GR3 mesh viewer~~ (done), GR4 DbxEditGUI (~~LevelDiff~~: EX1, done): each folded into the editor, then retired (`EDITOR_ARCHITECTURE.md` section 9) |
| OPFOR in Rust | 5 stages (WW2 install; level build; shader glue; material grid and pictures; Onslaught, menus, testrun); the export's install side; `rebuild_all` as a step graph. Stage 3 (shader glue) starts with retiring the Python shader `Merger` (audit AU3 AR-5, found too big for QW2): `bc2_formats::shaderdb::merge` has `add_donor` and `finish` but not `clone_entry` / `add_part`, which OPFOR's 21 in-process `Merger` sites use (wake_install 10, ww2_level_build, wake_shaders, wake_translate 2, the weapon ports); port those two, expose the merger through `bc2_native`, move OPFOR's sites onto it with the oracle byte for byte, then delete the Python class (RETIRED.md) |
| PR1 | the post-Rust improvements, re-scoped (PR0, 2026-11-20; audit AU2 A1-A5, A8, A9, P4, P8, S2, S14), after RT3 and OPFOR in Rust, in three parts: **PR1a** stable test IDs (durations, memory and CI keyed by ID), determinism as a rule (two hash seeds checked, Rust hash order linted), one native-backend registry (the 24 `use_native` copies, the per-tool switches); **PR1b** one cache layer (one budget implementation, content keys, the caches outside the budget), restart points instead of the 16 GB of decompressed payloads and one BF1943 bundle reader in `bc2-formats` (the S1 restart points' Rust half), CT1-style memory fixes (the batch readers, NavKit regression's 15 GB, the terrain readers), one cull-tree model, one DBX document, `bc2-cli`'s tool logic in library crates with typed errors and the Python emulation kept to the edge, one worker pool, sounds decoded without an ffmpeg per sound; **PR1c** (before the release) round codes out of names, an installable package, one naming scheme, OPFOR's `wake_*` as `ww2_*`. Done earlier: stream packing and the per-level shader index (S1), the cache budget (E3p, D1), strict mode and superseded payloads (QW1); rebuild_all's step graph is OPFOR in Rust's |
| NK1 | NavKit's build and apply in Rust (audit AU3 AR-4: the largest Python left, 12.5k lines, no round owned it) |
| Tests in Rust | a synthetic test corpus generated from the specification, so CI and other people can run the tests without the game copy, and the Python tests ported (for one language, not for speed: the Python entries run hidden behind the cargo entry) |
| V1, L1, A1 | the export's robustness: a compatibility / validation layer (plain errors, spec rules); DICE's placement tendencies studied on the stock maps, driving editor warnings ("stage 3 is 40 m from stage 2"); an optional auto shaping / layering of a mode's gameplay; exports regenerating path data with NavKit and the bot module |
| Spec layers 3-4 | the engine subsystems; the suite's transformations (grows with each porting round) |
| E4-E7 | gameplay tools, terrain sculpt and paint (with a terrain occlusion / colour paint layer and an optional "re-shade sculpted area": the terrain map is mostly hand-painted and sun-independent, so no separate re-bake round; custom-model import gives new models their own occlusion data), vegetation, Create Level from a height map |
| RenderDoc | exact sun, exposure and fog |
| Release | the full suite release: standalone, WITHOUT the tests (tests, goldens, replays, BENCHMARKS and spec tooling stay in the repo and CI; the user, 2026-11-21); `RELEASING.md` gets an include list and a check that nothing from `tests\` reaches the zip; users get the IN1 diagnostics instead |

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
| QW2 | 2026-11-21 | the audits' leftovers: version 0.10.0 released for RT1-RT3 / GR1 / GR2 / EX1 / FM1's removals and the rule that a merge removing something releases the next minor (`RELEASING.md`; OPFOR's `SUITE_REQUIRES` follows); CHANGELOG's two Removed sections one; a project lock and a modified-time check before every save in the editor (AU1 R4); `mapexport check` refuses streamed wave data (AU1 RISK-4; RISK-7 and RISK-8 were already checked by EX1 and XF1); `guid` and `pe` moved into `bc2-formats` (AU2 A5; culltree's model left in levelwrite for PR1b); each of `bfbc2.exe`'s 17 tool groups builds alone (seven fixed) and CI checks it; exit codes documented; O2's stand-in exe no longer blocked by Defender; O4 texqa's goldens freed from texqa.py's source hash (the red quick-tier entry since DF2); `texqa.Dice`'s ledger originals tested. The Python shader Merger's retirement moved to OPFOR in Rust stage 3 (too big: needs `clone_entry` / `add_part` in Rust and OPFOR's 21 sites moved) |
| OR0 | 2026-11-21 | Project OPFOR's round (merged there as 19dd0da; audit AU3 OP-2, OP-5, TS-9, TS-10, AU4 BUG-1): the WW2 rebuild oracle and an install oracle (21,197 staging digests) committed to OPFOR, relative paths in the emitter-atlas manifests, the step tracker reading only each step's outputs and the Rust core's reads, a rebuild lock, the independent level builds side by side; rebuilds from main and from a worktree 10,852 of 10,852 identical, 15-18 min against ~23 |
| HG1 | 2026-11-21 | the Rust test binaries' hangs and slow tests: the g3 / g4 editor hang was a loader-lock deadlock in NVIDIA's driver (a Vulkan device made while another thread exited through `nvoglv64.dll`'s `DllMain`; dumps read with the new `rust\hang_stacks.py`), fixed by one headless device per process on Vulkan / Direct3D 12 (4 hangs in 9 loaded runs -> 0 in 30), with a hang guard in the editor test binary; the export's shader reach and closure copies read a step at a time (a WW2 canary without the bundle cache 1,887 -> 96 s, the stage identical); the CPU ray on a terrain grid (35 -> 0.5 ms a ray on SP_01, the same hits; the SP_01 walk test 52 -> 10 s); EX1's preview canary under an id of its own |
| TS1 | 2026-11-21 | faster test tiers (audit AU3 TS-1 to TS-8, TS-12 to TS-14; AU4 DF4): quick tier 3,593 -> 854 s on this PC (warm folder; about 740 s cold); per-test Rust times (`rust_test_times.json`, the 30 slowest in BENCHMARKS); 38 whole-level editor / levelwrite / render tests `#[ignore = "game tier: ..."]` in the game tier's new entry; the cargo entry split into build, quick tests, clippy and fmt; green crates skipped (never on CI / full / `--no-skip`); dependencies at opt-level 2 and the workspace at 1 (whole-level tests 1.2-5.8x faster); levels read once per editor test binary; skips that hid failures are failures; three untiered test files tiered; per-test time limits, `--only` nothing exits 2, kill-on-close job objects, build_native's install retried; sccache measured (no cross-worktree hits) and left opt-in. Found: the g3 / g4 editor-test hang and XF1's `export_xf1` slowness (round HG1) |
| DF2 | 2026-11-21 | install and revert safety (audit AU4, 23 findings): one install ledger for every tool that writes the game copy (OPFOR's `opforcore.ledger` format; the suite's whole-file half in `bc2_formats::gameledger` and `bfbc2core.gameledger`); NavKit and stock-MP on it, so AU4-DS-1 (NavKit and stock-MP both backing up `pathdata.hpd`) can no longer lose DICE's original; journalled, fsync'd, retried writes, a killed install rolled back by the next command; `bfbc2 gameledger`; OPFOR's half: every Onslaught stage, the Pacific War install / menu mode, mapedit_install and the import scripts on `opforcore.stages`, `opforcore\migrate.py` for the old markers (this PC: 23 files, all migratable; a scratch copy migrated and reverted byte for byte) |
| QW1 | 2026-11-20 | audit AU2's quick wins: the editor / levelwrite / render tests once per quick tier (quick tier 4,677 -> 2,276 s on this PC); dev builds with line tables and one integration-test binary per crate (debug folder 25 -> 3.4 GB, editor relink 76 -> 18 s); memory declarations from recorded peaks; a fingerprint per artefact, a stale `bfbc2.exe` warned about (`BFBC2_STRICT_NATIVE`), mapexport / editor staleness checks; superseded FBRB payloads pruned (437 MB here); `bfbc2core.envvars` (92 variables) and `requirements.txt`; skipped tests counted as skipped; MPLCONFIGDIR; 60 stale test-time names; OPFOR: one suite locator with a version check, one GUID helper (24 sites, byte-identical) |
| FM1 | 2026-11-20 | the map editor's own folder `bfbc2-map-editor\` (README, `EDITOR_ARCHITECTURE.md` moved with its history, section 10: the three planned AI tools as editor workspaces), `bfbc2-map-inspector\` and the three README-only placeholder folders deleted, every reference repointed (suite and OPFOR); `RETIRED.md` (the retirement policy, its checklist, 10 rows); DOC1's small code / help fixes |
| SP1 | 2026-11-19 | the engine specification, layers 1-2 (`spec/`): 439 rules (250 verified, 48 implemented, 134 observed, 7 inferred), `spec/trace.py` checked by the quick tier |
| P1 | 2026-11-19 | `ROADMAP.md`, `CHANGELOG.md`, `VERSION` 0.9.0 (tagged), `RELEASING.md`, CI (`checks.yml`), `run_tests.py quick --no-game` |

### The map editor (`rust/bc2-editor`, `rust/bc2-levelwrite`, `rust/bc2-render`)

| Round | Merged | Result |
|---|---|---|
| OP1 | 2026-11-21 | the editor's core before E5 (audit AU1 R14 / R17 / R18 / R24, AU4 DS-20 / BUG-12 / BUG-13 / CQ-7, DF1's left items): operations name the files they change and the `files` operation edits several at once, all or nothing, one undo step (project format 6: `base.files`, `min_reader`, versions read whole; old projects replay the same); brush strokes as sha1-named 64 x 64 tiles beside the project (2.6 ms a stroke, 3.4 ms an undo on 10,000 strokes); the autosave a crash-safe, resumable journal; typed errors in the editor and renderer (155 `Result<_, String>` to 0) and typed Problems; DF1's side fields as state enums; the save prompt on close; the game-accurate view read on a worker; `ViewState` 74 -> 45 and `EditorApp` 37 -> 22 fields in focused parts (screenshots byte for byte the same). Left: `rust/BENCHMARKS.md` "OP1" |
| R5e | 2026-11-21 | the WW2 open sea drawn (Wake Island, Guadalcanal, Iwo Jima, Coral Sea): a program's material goes to its block header's registers (`[0x19, 0x18)`), not only to a variable named `constants`; OPFOR's translated BF1943 water (`waterMaterial`) now gets its `g_depthScaleFactors`, so its alpha is no longer 0 everywhere. Wake 600 m out 53.5 % -> 0.0 % near-black; DICE's MP_005 and SP_01 bit for bit as before |
| GR3 | 2026-11-21 | the mesh viewer folded into the asset browser: the Asset preview (a picked blueprint or any mesh by name: orbit, Side / Top / Front as the mesh viewer's pictures, levels of detail, parts on / off and picked, destruction states, textures by channel, wireframe, normals, bounds and measurements, comparisons, Save picture) and the Meshes mode (`find`); `bc2_convert::meshview` ports the index cache, `load_mesh`, `stack_models` and `render` - 18 levels of detail of 8 meshes, 94 parts and 55 pictures identical to the Python's; the page for a browser retired (version 0.11.0), the picture command line kept |
| DF1 | 2026-11-21 | the editor's state and gating (audit AU4, 19 findings, all fixed; three parts left with reasons in `rust/BENCHMARKS.md` "DF1": the game view's load on a worker, a keyed-mutex handover of the shared frames, navkit apply's in-place `--out`): a project generation counted on every show / close, the side state bound to it (a failed Create no longer re-modes the next project; the Export window lets go of a closed project and writes into no other; a preview dropped with its project); Export, Install / Uninstall and the navkit commands wait for each other, in words (Install now takes the planned stage only); dead workers reported; Export reads the project's own saved state; closing the window asks while work runs; device-lost recovery, textures freed with their views; no voice-over block without its `bundleManifest` line. OP1 later turns the side state into state enums |
| R5d | 2026-11-21 | the game-accurate view's visual gaps: Wake Island's and Coral Sea's black ground (the height scale, the water's height map, mask tiles spanning 8 x 8 / 4 x 4 cells: the ragged WW2 shores too), the sky dome (`EnvironmentMain`), mesh LODs from the mesh sets' LOD settings and crack-free terrain LODs, labels hidden behind the game frame, Package levels through the bundle cache (Wake 17.7 s to 4.6 s cold, 1.4 s warm) |
| VW2 | 2026-11-21 | the mode view layer (every object, the mode layer only or the world, one Rush stage as played: L1's simulation, its bases per stage proved on 4 levels) in both views, the outliner and the overlays; the weather picker in both views (the game-accurate view draws a state, opens on the spawn-time switch; the simplified view tinted by the state's look, measured against the frames); exports start in a state (**VW2a**: ActiveState and the spawn switch together), another level's state appended or its whole system swapped (**VW2b**, with the 4-byte WeatherSystem resource), or at night (**VW2c**: `ons_mp_002_night` as the template, cube and panorama darkened; the lamp tool left), rain as a borrowed state only (VW2d's effects and sounds left); six canaries byte for byte against their baselines, install and revert exact; night / rain off by default |
| VW1 | 2026-11-20 | the view toggles: effects (emitter spawn volumes), dynamic lights (point-light emitters) and sound emitters (min distance, falloff, sound areas), combat areas, capture areas and radii, spawn protection radii, all from the level's fields; the player's-eye walk camera (Alt+W); navmesh and cull tree modes checked on three levels (cull legend off the selection orange); collision per destruction state from the Havok per-part header with an up / down state stepper |
| GW1 | 2026-11-20 | the game-accurate view finds a level's weather through its weather entity and the bundle stack (Guadalcanal and Iwo Jima open; all four WW2 levels draw their whole terrain), a default sky when a level has none |
| XF1 | 2026-11-21 | the export registers every mesh the level reaches in its shader databases (merged from the levels that draw it; 0x98BD6E traced to the draw-record builder) and gives every copied, placed or grafted object its own blueprint's cull shape, both with check lines; undo from checkpoints with the donor cached (9-11 s to under 0.1 s on Aurora as WW2 with 40 moves); Paste as a deep duplicate; typed gameplay operations (team, spawner link, tickets, outline point insert / remove; project format 5) |
| EX1 | 2026-11-20 | every name of an export editable and checked together (level id, its folder, the server-browser name, the on-screen name and description per language; the black-screen and checksum rules in `mapexport check`; names in the project's log), Preview export (the stage read only beside the original, the same camera, T switches), the Diff panel (Compare with base / with level, filters, click-to-fly, tints and markers in the 3D view); LevelDiff folded in: `bfbc2.exe leveldiff` identical to LevelDiff.exe on 30 recorded cases, LevelDiff.exe retired |
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
| RT3 | 2026-11-21 | `hpdtool.parse` and the Wave readers retired after their errors were ported (replayed: path data 2,113 game answers, Waves 426,208, 0 wrong); the command lines `aptrefs`, `xiso` (bytes entry point in Rust), `culltree`, `stockmp`, `crashreader` split into `<tool>lib.py` and retired; argparse's option abbreviations in `bfbc2.exe` (193 of 193); one rule when Rust cannot answer (A17); `run_tests.py --help`; the test-file registration guard; viewer.py / pick.py retired; O2's Defender failure fixed; RT2's kept references decided; RT3-ROADMAP |
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
