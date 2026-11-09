# Code quality audit, 2026-11-08

Branch `quality/audit-2026-11-08` (worktree `C:\Programming\Projects\bfbc2-tool-suite-quality`), from `main` at 4218381.
Not pushed.

## Scope

In scope: every tool folder except the six being edited tonight (bfbc2-bf1943-import, bfbc2-console-convert,
bfbc2-shader-tools, bfbc2-anim-tools, bfbc2-model-tools, core). Those were read only to follow imports; core's own
mypy findings are left out of the numbers below.

| Folder | Python | Lines | Entry points | Tests (quick / game tier) |
|---|---|---|---|---|
| bfbc2-auto-navkit | 57 files | 15,209 | navkit.py, navkitGUI.pyw, 30 tools\*.py | 26 test scripts: 17 quick, 6 game, plus hpdtool / hpdcheck / navregress runs |
| bfbc2-automated-map-and-bot-test-harness | 13 | 3,360 | harness.py, crash-reader\crashreader.py, exeref.py | test_harness, test_crashreader, test_exeref (quick + --game) |
| bfbc2-menu-tools | 8 | 4,488 | aptcheck, aptgraft, aptrefs, bc2loc | test_menu_tools, test_aptgraft (quick), test_bc2loc_write (game) |
| bfbc2-stockmp-ai | 8 | 2,514 | stockmp.py | test_stockmp (quick + --game) |
| bfbc2-navigation-studio | 4 | 1,578 | studio.py, overlay.py | test_editing (game only) |
| bfbc2-culltree | 2 | 1,452 | culltree.py | test_culltree (quick) |
| bfbc2-console-readers | 4 | 998 | apt.py, stfs.py, xiso.py | test_console_readers (quick + --game) |
| bfbc2-mesh-viewer | 2 | 519 | meshview.py | test_meshview (game only) |
| bfbc2-map-inspector | 1 | 432 | inspector.py | only through tests\test_viewers.py (game, needs a display) |
| bfbc2-diff-workbench | 1 | 171 | (LevelDiff is C#) | test_leveldiff (quick + --game) |
| tests\, run_tests.py, BFBC2 Tool Suite.pyw | 8 | ~900 | the hub, the test runner | test_repo, test_cli_errors, test_review_fixes{,2} (quick); test_hub, test_viewers (game) |
| bfbc2-dbx-tools, bfbc2-fbrb-tool | C# only | - | - | DbxEdit unit tests (quick) |
| ai-behavior-simulator, bot-ai-debugger, tactical-map-authoring-tool | README only | - | - | - |

Quick tier (`python run_tests.py quick`): 43 of 43 passed before the changes, and 43 of 43 after every commit.

## Tools and configuration

- ruff 0.17.3 and mypy 2.4.1 (compiled) were already installed for Python 3.13.15. Nothing was installed.
- `pyproject.toml` at the root (new):
  - **ruff:** rules F, E, W and B. It ignores E501 (893 lines run past 120 characters), E701/E702 (`if x: continue`
    is the house style), E731, E741 (`l`), E402 (imports after `sys.path.insert`) and B905. `line-length = 120`.
    The quote style is single, but it is only used if someone runs `ruff format` on a file. There was no mass
    reformat.
  - **mypy:** `mypy_path = core`, `ignore_missing_imports`, `explicit_package_bases`, and `check_untyped_defs = false`
    for now, because about 0.5 % of functions are annotated.
- Run mypy one folder at a time, for example `python -m mypy bfbc2-culltree`. Several folders have modules with the
  same name (tests\test_*.py), so a single run over everything trips on duplicates.

## Numbers

| | Before | After |
|---|---|---|
| ruff (F/E/W/B), in-scope folders | 122 | 101 |
| mypy (default config), in-scope findings | 27 | 0 (core's own 4 remain, out of scope) |
| mypy `--check-untyped-defs`, in-scope | 446 | 430 |
| Functions fully annotated (all parameters and return) | 6 of 1,425 | 22 of 1,425 |

The 101 ruff findings left are 45 B023, 43 B007, 7 F841 and 6 B904. All 45 B023 (closure over a loop variable) were
read one by one. Every closure is called inside the same iteration (`timed(lambda: ...)`, `detail_features(...,
eligible, ...)`, the validate checks, aptgraft's `pc`, graphstats' `find`), so none is a bug. The F841s left are
calls kept for their side effect (`g.char(...)` in aptgraft, `harness('run', ...)` in test_harness) or dead locals
(validate.py:258 `info`, author.py:220 `d`, navsample_offline.py:842 `yy`, test_vehicle.py `z`).

Most of the 430 `--check-untyped-defs` findings are inference noise: 184 are "need type annotation" for empty
containers, and many more come from a name reused for different types. Each one sampled (kymesh, hpdcheck,
ordercheck, navparity, culltree `Placed`, aptgraft `add_icon_frame`, apt `sink`, inspector `scene_triangles`) turned
out not to be a bug.

## Confirmed bugs

Each was reproduced or traced by reading both caller and callee. Fixed means a commit on this branch.

| # | Where | What went wrong | Status |
|---|---|---|---|
| 1 | bfbc2-console-readers\xiso\xiso.py:72 | `extract` loops forever on a truncated image or an entry past the end of the file (`f.read` returns `b''`, `left` never falls) | **Fixed**, test (fails on the old code: 60 s timeout) |
| 2 | bfbc2-menu-tools\aptcheck.py:298 | `verify` never takes out `--brief` (documented in the usage). Before the movies it is resolved as a movie name and fails; after them it is ignored | **Fixed**, test (fails on the old code: `no menu movie "--brief"`) |
| 3 | bfbc2-auto-navkit\tools\hpdtool.py:300 | `roundtrip` catches only ValueError. A truncated file raises struct.error and ends the whole batch (`coverage` already caught both) | **Fixed**, test (fails on the old code) |
| 4 | bfbc2-automated-map-and-bot-test-harness\harness.py:234 | `os.path.relpath(path, root)` raises ValueError when `--navkit` points to another drive. It sits outside any try, and a run is saved only at the end, so every level already tested is lost | **Fixed** (`runs.shown`), test |
| 5 | bfbc2-culltree\culltree.py:566 | `Level.tree_path`: a reference without a `levels/` segment returns the level's folder. `CullTree(folder)` then raises PermissionError, which `except CullTreeError` does not catch | **Fixed**, test |
| 6 | bfbc2-auto-navkit\tools\navdiff.py:321 | `pngs` scales cell indices by the area's pitch, but `cells()` indexes the 0.8 m comparison grid (`GRID`). Detailed sectors (pitch 0.2) were drawn at a quarter of their size | **Fixed** (no test: matplotlib output) |
| 7 | bfbc2-auto-navkit\navkit.py:373 | The `detailed.json` cache key leaves out the sector offsets the cached ids are relative to. A new `--box` on a level without path data reused the old ids | **Fixed** (key gains xoff and zoff, so old caches rebuild once) |
| 8 | bfbc2-auto-navkit\navkit.py:518 | The level-ended message says "retry with a larger --budget", but navkit.py has no `--budget`; the option is silently ignored | **Fixed** (message names `tools\sample_all.py --budget`) |

The run_tests `--only` IndexError was reverted, not fixed. `tests\test_cli_errors.py` uses a bare `--only` on
purpose, to exercise the "unexpected error explained in words" path.

### Found, not fixed

These need a decision or data I don't have here:

- **bfbc2-culltree\culltree.py:691-722 (medium).** `--ref-dir` that contains the target level reads the target again
  as a reference. Only reference paths are de-duplicated. The target's instances then confirm themselves, which
  suppresses `add`'s "unconfirmed shape" warning and weakens the leave-one-out checks. The fix is to seed `seen` with
  the target's key, but that changes `verify` / `add` output on existing folders, so it needs the owner's call.
- **bfbc2-console-readers\stfs\stfs.py:51-60 (medium).** `_hash_entry` always uses the first copy of the level-0 hash
  table. In writable (shift 1) packages the live copy can be the second one, so fragmented files read wrongly. The
  docstring states the limit. Fixing it needs a real package with the second copy live to test against.
- **bfbc2-automated-map-and-bot-test-harness\testharness\ingame.py:125 (medium).** `offsets-read` checks every
  SectorOffsets load against the Infantry asset only. A level whose Vehicle offsets differ (stock-MP AI computes
  them separately) would fail a correct game run. **Unverified in game.**
- **crash-reader\crashreader.py:236 (medium).** The code view decodes from `eip - 0x60`, which can land mid-instruction.
  When the decoder does not resynchronise, the null-pointer explanation is skipped. `exeref.window` decodes from the
  function start instead.
- **bfbc2-auto-navkit\navkit.py:699 (medium).** `apply` without `--out`: on failure no `.report.txt` is written,
  because the default output path is computed inside `_apply`.
- **ordercheck.py:145/163, navcompare.main_network (medium).** A path-data file with no route nodes raises (from
  `np.argmin` / `max()` of an empty sequence) instead of reporting 0 %.
- **Minor:**
  - `stfs.py extract` without an output folder raises IndexError.
  - `overlay.py` with neither `--anomalies` nor `--reference` raises KeyError.
  - meshview's `--level` is in the usage text but is never read.
  - meshview's cache `except` does not catch a foreign or corrupt pickle (ValueError / AttributeError).
  - `pipeline.sector_offsets` raises IndexError on a file with no sectors.
  - `exeref.parse_int('0100')` gives "not an address".

## Implicit state

- **navbuild module settings (bfbc2-auto-navkit\tools\navbuild.py).**
  - `set_pitch` rewrites seven module globals (PITCH, SECTOR_SAMPLES, WINDOW, TILE, TILES, RESOLUTION, LINK_STEP).
    `build` resets them at its start, `merge_detailed` switches to 0.2, and `navvehicle.vehicle_settings` saves and
    restores about 20 of them around a vehicle build.
  - `navvehicle.stage_on_grid` also swaps `navbuild.navsample` / `navbuild.navwalk` for shims and restores them in
    `finally`.
  - Correct today, but builds can't run concurrently in one process (the GUI runs navkit as a subprocess, so it is
    unaffected).
  - An exception inside the detailed pass leaves pitch 0.2 set until the next `build`.
- **Report counters as module globals.**
  - navbuild: ORDER_STATS, ORDER_IDS, LADDER_STATS, DETAIL_STATS. navwalk: ROOF_STATS, OBJECTIVE_STATS, STEEP_STATS.
    navislands: STATS, TERRAIN_IDS.
  - `build` clears most of them, and `navbuild.write_file` clears navwalk's. `ORDER_IDS` changes behaviour (order
    nodes pruned by island size), so a caller of `stage` must clear it first, as navvehicle does.
  - `REPAIR_STATS` is never cleared and never read: it is write-only dead state.
- **Process-wide flags.**
  - `--no-detail` sets `navbuild.DETAIL_FEATURES = False` for the process (navkit.py:808, navbuild.py:1716).
  - navregress.py and harness.py rebind `print` module-wide to a flushing partial.
- **runs.py's metric lists depend on what imported.** `PARITY_HIGHER` / `PARITY_LOWER` come from navregress if it
  happens to be importable when the module loads, else from a hard-coded copy that can drift.
- **sys.path.** 84 files insert into `sys.path` (143 calls), some on every call:
  - navkit `_apply`, `navregress.level_water` and `run_tests.opfor()`, so `sys.path` grows during a regression run.
  - meshview puts bfbc2-console-convert first, whose generic names (`mesh`, `texture`) can shadow other modules.
- **Caches.**
  - On disk:
    - navkit `detailed.json` (key fixed, bug 7)
    - `meshview_index.pkl` (size + mtime + CACHE_VERSION; shaderdb changes need a manual bump)
    - levelinfo bundle caches
    - the harness parity cache (sha1 of both files)
    - the navdata pickle cache
    - `pipeline.common()`, used whenever its manifest exists, never checked against the game bundle, so it can go
      stale.
  - On objects: `nav._harness_cells`, `nav._adj`, `Studio._gaps`, and `aptgraft.Grafter.memo`.
  - `Grafter.memo` lives for the whole graft. A source range taken twice shares one memoised PushData list node,
    which is the "shared list" condition aptcheck flags. It is safe today only because those ranges hold no PushData.
- **Working directory.**
  - `harness.runs_folder` falls back to `os.getcwd()`.
  - A relative `candidate` path in a parity row is resolved against the cwd (harness.py:95).
  - `overlay.py --out <bare name>` writes into the cwd.
  - `run_tests` pins `cwd=SUITE` for its children.
- **GUI threads (navkitGUI.pyw).** `self.tail` / `self.cancelled` are shared between the reader thread and Tk without
  a lock. Cancel kills navkit.py but not the FbrbTool / DbxEdit children it started.

## Typing coverage

- **Before:** 6 of 1,425 functions were fully annotated (hpdtool's parse/write, kymesh's parse_mesh/write_mesh and a
  few others). The dataclasses in hpdtool, kymesh, navsample, navdomains and navwater_mesh carry field types.
- **Commit 36a1d45 (after):**
  - `testharness\runs.py` is fully annotated, with `Run` / `Result` / `Log` aliases.
  - The module-level stats dicts and id sets are typed.
  - Optional dataclass fields are now `X | None`.
  - kymesh's `pos` no longer reads `None` for empty lists. The value is only used when the count is non-zero, so
    behaviour is unchanged.
  - mypy at the default settings is clean on every in-scope folder.

## Commits on the branch

```
401f1b7 test harness tests: run names (path_of), metric directions (spec) and status counts (counts_text)
36a1d45 Type annotations: runs.py, navkit stats globals, optional dataclass fields (mypy clean in scope)
8f1efcb Unused imports and placeholder-less f-strings removed (ruff F401, F541, E401)
92e742b pyproject.toml: conservative ruff and mypy settings
c5a03a2 Revert the run_tests --only change (test_cli_errors relies on it)
b1379b9 run_tests: --only without a name is an error message (reverted above)
bc57c41 culltree: tree reference without levels/ finds no tree; test
2c2d483 test harness: a file on another drive is logged by its full path; test
8120ca6 navkit: detailed-sector cache keyed by sector offsets; level-ended message names sample_all.py --budget
49aa028 navkit: navdiff's sector pictures on the comparison grid
8f982a8 navkit: hpdtool roundtrip counts a truncated file as one failure; test
2d75957 menu-tools: aptcheck verify takes --brief; test
79c218d console-readers: xiso extract stops on a truncated image; test
```

## Plan for the rest (in priority order)

1. **Decide and fix the medium findings above:**
   - culltree's `--ref-dir` self-reference
   - `apply`'s missing report on failure
   - the empty-path-data crash in ordercheck / navcompare
   - crashreader's decode start
   - ingame `offsets-read` against both assets (needs a game run)
   - stfs's live hash-table copy (needs a writable package to test)
2. **Contain navbuild's module state.**
   - Make `set_pitch` and the stats dicts one `BuildSettings` / `BuildStats` object that `build` and `stage` take.
   - Or, as a first step, a context manager like `navvehicle.vehicle_settings`, used by `merge_detailed`, so an
     exception cannot leave pitch 0.2 behind.
   - Remove the dead `REPAIR_STATS`.
3. **One `sys.path` setup per entry point.** Move the per-call `sys.path.insert` calls (navkit `_apply`,
   `navregress.level_water`, `run_tests.opfor`) to module level, and stop meshview putting console-convert first
   (after tonight's console-convert work lands).
4. **Annotate the most-imported modules next:** hpdtool's remaining helpers, navsample (Samples), navpath / navdata
   consumers, menucommon, culltree's `Doc` / `CullTree` / `Level`, stockmp's dbxbin and pipeline. Then turn on
   `check_untyped_defs` for those files only, with a per-module override in pyproject.toml.
5. **Ruff next steps:**
   - Rename the 43 unused loop variables to `_name` (B007).
   - Add `from err` / `from None` on the 6 re-raises (B904).
   - Then make `ruff check` a quick-tier test in run_tests.py, so new F/E/W/B findings fail the run.
6. **Tests for the tools with no quick-tier coverage:**
   - map-inspector, navigation-studio (`editing.py` is pure and could get a no-game test), mesh-viewer's index
     logic, and `navdiff.pngs` (a check that the scatter extents equal the sector's 100 m).
   - navkit `detailed_sectors` (cache key) with a fake `Paths`.
7. **After tonight's work lands,** run the same audit on the six excluded folders (core first: its 4 mypy findings,
   including `errors.PIP_NAMES` and `navdata.py:85`).
