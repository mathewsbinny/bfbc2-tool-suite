# Code quality audit, 2026-11-08 / 09

Branch `quality/audit-2026-11-08` (worktree `C:\Programming\Projects\bfbc2-tool-suite-quality`), from `main` at 4218381.
Rounds 1-2 were merged into main (198f488); round 3 is on `quality/round3-2026-11-09`. Not pushed. Three rounds:

- **Round 1 (2026-11-08):** audit, configuration, the first eight bugs.
- **Round 2 (2026-11-09):** every remaining finding fixed or decided, the ruff findings cleared, implicit state
  reduced, types added, tests added, rescan and scores.
- **Round 3 (2026-11-09):** the whole suite, including the six folders excluded before. See "Round 3" at the end.

## Scope

In scope: every tool folder except the six being edited by another session (core, bfbc2-bf1943-import,
bfbc2-console-convert, bfbc2-shader-tools, bfbc2-anim-tools, bfbc2-model-tools). Those were only read: to follow
imports, and for the read-only scores at the end.

| Folder | Python | Lines | Entry points | Tests (quick tier / game tier) |
|---|---|---|---|---|
| bfbc2-auto-navkit | 58 files | ~15,300 | navkit.py, navkitGUI.pyw, 30 tools\*.py | 18 quick test scripts (test_tool_edges new), 6 game, plus hpdtool / hpdcheck / navregress runs |
| bfbc2-automated-map-and-bot-test-harness | 13 | ~3,400 | harness.py, crash-reader\crashreader.py, exeref.py | test_harness, test_crashreader, test_exeref (quick + --game) |
| bfbc2-menu-tools | 8 | ~4,500 | aptcheck, aptgraft, aptrefs, bc2loc | test_menu_tools, test_aptgraft (quick), test_bc2loc_write (game) |
| bfbc2-stockmp-ai | 8 | ~2,550 | stockmp.py | test_stockmp (quick + --game) |
| bfbc2-navigation-studio | 5 | ~1,750 | studio.py, overlay.py | test_editing_offline (quick, new), test_editing (game) |
| bfbc2-culltree | 2 | ~1,500 | culltree.py | test_culltree (quick) |
| bfbc2-console-readers | 4 | ~1,050 | apt.py, stfs.py, xiso.py | test_console_readers (quick + --game) |
| bfbc2-mesh-viewer | 3 | ~620 | meshview.py | test_meshview_index (quick, new), test_meshview (game) |
| bfbc2-map-inspector | 1 | 432 | inspector.py | only through tests\test_viewers.py (game, needs a display) |
| bfbc2-diff-workbench | 1 | 171 | (LevelDiff is C#) | test_leveldiff (quick + --game) |
| tests\, run_tests.py, BFBC2 Tool Suite.pyw | 10 | ~1,050 | the hub, the test runner | test_repo, test_lint, test_types (new), test_cli_errors, test_review_fixes{,2} (quick); test_hub, test_viewers (game) |
| bfbc2-dbx-tools, bfbc2-fbrb-tool | C# only | - | - | DbxEdit unit tests (quick) |
| ai-behavior-simulator, bot-ai-debugger, tactical-map-authoring-tool | README only | - | - | - |

## Tools and configuration

- **Versions:** ruff 0.17.3, mypy 2.4.1 (compiled) and coverage.py 7.17.0 were already installed for Python 3.13.15.
  Nothing was installed.
- **`pyproject.toml` (root):**
  - **ruff:** rules F, E, W and B, line-length 120, target Python 3.12. Round 2 raised the target from 3.11:
    bf1943-import already uses 3.12 f-string syntax, and ruff flagged it as invalid under 3.11.
  - **ruff ignores:**
    - E501: about 900 lines run past 120 characters (paths, tables, prose comments).
    - E701 / E702: `if x: continue` is the house style.
    - E731, E741 (`l`), and E402 (imports after `sys.path.insert`).
    - B905.
  - **No reformat.** There was no `ruff format` run; the single-quote setting only applies if someone formats a file.
  - **mypy:** `mypy_path = core`, `ignore_missing_imports`, `explicit_package_bases`, `warn_unused_ignores`,
    `no_implicit_optional`, and `annotation-unchecked` notes off.
  - **Strict modules:** a per-module override checks 14 fully annotated modules strictly (`check_untyped_defs`,
    `disallow_untyped_defs`, `disallow_incomplete_defs`): hpdtool, kymesh, navsample, menucommon, culltree, apt,
    stfs, xiso, dbxbin, pipeline, and testharness.runs / ingame / levelchecks / `__init__`. A new untyped function
    in any of them is an error; this was verified with a probe function.
- **Enforced in the quick tier (new):**
  - `tests\test_lint.py` runs ruff over the covered folders.
  - `tests\test_types.py` runs mypy per covered folder. Findings inside core\ are reported by core's own audit, not
    by this test.
  - Run mypy one folder at a time: several folders have modules with the same name.
- **Coverage:** measured with coverage.py over the whole quick tier, child processes included (`patch = subprocess`).
  The source is the in-scope folders, and test files are left out. The configuration was kept in the scratchpad, not
  in the repository.

## Numbers, before the audit and now

| Measure | Before (4218381) | After round 1 | Now |
|---|---|---|---|
| ruff (F/E/W/B), covered folders | 122 | 101 | **0** (enforced) |
| mypy, default settings, in-folder errors | 27 | 0 | **0** (enforced) |
| mypy `--check-untyped-defs`, in-folder errors | 446 | 430 | **52** |
| Fully annotated functions | 6 of 1,425 (0.4 %) | 22 of 1,425 | **249 of 1,454 (17 %)** |
| Modules checked strictly | 0 | 0 | **14** |
| Quick-tier tests | 43 | 43 | **48** |
| Quick-tier line coverage, in-scope folders | 55.2 % (8,005 of 17,878 lines missed) | - | **57.7 %** (7,696 of 18,181 missed) |
| Files at 0 % quick-tier coverage | 7 | - | 3 (inspector.py, validate.py, studio.py at 10.6 %) |

**The 52 `--check-untyped-defs` errors left:**
- **navkit (41):** mostly tests that monkeypatch module attributes, plus `re.search(...).group()` on a match that
  could be None in navkit.py and navregress.py.
- **culltree tests (4), harness levelinfo (3), aptgraft (2), navigation studio (2):** values the logic guarantees but
  mypy cannot see.

These were left rather than silenced with ignores or casts.

**Coverage gains by file:**

| File | Before | Now |
|---|---|---|
| editing.py | 0 % | 44.8 % |
| meshview.py | 0 % | 24.3 % |
| overlay.py | 0 % | 20.3 % |
| navkitGUI.pyw | 0 % | 18.0 % |
| hpdtool.py | 40.7 % | 48.7 % |
| navcompare.py | 19.0 % | 29.3 % |
| navdiff.py | 78.1 % | 85.8 % |
| navkit.py | 21.6 % | 25.4 % |
| crashreader.py | 68.4 % | 73.2 % |
| pipeline.py | 29.5 % | 33.9 % |

Game-tier code paths, such as level loading and real builds, are not in these numbers.

## Confirmed bugs, all fixed

17 of the 20 have a regression test. 14 of those were run against the old code and fail there. Three fail by
construction (bugs 4, 5 and 16: the old function lacks the helper, or returns the folder, or raises), but were not
run against it. No test for #6 (matplotlib output is checked only for being produced), #8 (a message) or #19 (a
usage line).

| # | Where | What went wrong | Fix (commit) |
|---|---|---|---|
| 1 | console-readers\xiso\xiso.py, extract | Looped forever on a truncated image (`read` returns `b''`) | Error naming the file (79c218d) |
| 2 | menu-tools\aptcheck.py, cmd_verify | `--brief` was never taken out of the arguments; before the movies it was read as a movie name | Taken as an option (2d75957) |
| 3 | navkit tools\hpdtool.py, roundtrip | A truncated file's struct.error ended the whole batch | Counted as one failure (8f982a8) |
| 4 | harness\harness.py, run_level log | `os.path.relpath` across drives raised and lost the whole run | `runs.shown()` (2c2d483) |
| 5 | culltree\culltree.py, tree_path | A reference without `levels/` returned the level's folder, and opening it raised | None (bc57c41) |
| 6 | navkit tools\navdiff.py, pngs | Cells scaled by the area's pitch instead of the 0.8 m comparison grid, so detailed sectors were drawn at a quarter of their size | `GRID` (49aa028); the test checks only that the file is produced |
| 7 | navkit\navkit.py, detailed_sectors | The cache key lacked the sector offsets its ids are relative to | Key includes them (8120ca6); test in 36ea6cf |
| 8 | navkit\navkit.py, sample_run | The message advised `--budget`, which navkit.py does not have | Names `sample_all.py --budget` (8120ca6) |
| 9 | culltree, ref_pairs | A `--ref-dir` holding the target level read it again as a reference; its instances confirmed themselves and hid the unconfirmed-shape warning | Target's level and tree files skipped (7c2ee7e) |
| 10 | console-readers\stfs\stfs.py, _hash_entry | Always read the first copy of a hash table; in writable packages the live copy can be the second, so fragmented files read wrong | Live copy followed (0b8f4cb) |
| 11 | harness testharness\ingame.py, offsets-read | Loads were compared with the Infantry SectorOffsets only; a level whose Vehicle offsets differ failed a correct run | Either asset accepted (50e4d5c) |
| 12 | crash-reader\crashreader.py, code view | Decoding from `eip - 0x60` could start mid-instruction and lose the faulting instruction and its null-pointer explanation | Decodes from the function start (5544ba7) |
| 13 | navkit\navkit.py, apply | A failure without `--out` wrote no report, because the default path was known only inside `_apply` | Report on any failure (fbc7aa1) |
| 14 | navkit ordercheck.py, navcompare.py | Path data with no route nodes raised IndexError / ValueError | No group, 0 points (36ea6cf) |
| 15 | navkit\navkit.py, detailed_sectors | The first call returned lists and later calls returned tuples (from the cache) | Tuples always (36ea6cf) |
| 16 | console-readers\stfs.py, main | `extract` without an output folder raised IndexError | Usage (0b8f4cb) |
| 17 | navigation-studio\overlay.py, main | Neither `--reference` nor `--anomalies` raised KeyError | Usage (ee32bcd) |
| 18 | mesh-viewer\meshview.py, Index | A foreign or corrupt cache pickle raised TypeError instead of being rebuilt | Rebuilt (0009794) |
| 19 | mesh-viewer\meshview.py, usage | `--level` was documented but never read | Dropped from the usage, with a note if it is given (0009794) |
| 20 | stockmp-ai\pipeline.py, sector_offsets | Path data without sectors raised IndexError | BuildError naming the file (d7793e7) |

**Decisions recorded** (the safest compatible behaviour, and the reason):

- **culltree, the target as its own reference.** The target is skipped by path: its level file, and its tree file
  reached from another level. It is not skipped by the existing basename + tree-size key, which dropped a genuine
  reference level of the same name in the test (same name, same tree size). A copy of the target under another path
  still counts. Detecting that would take a content hash, which was left out.
- **stfs, the live hash-table copy.** This follows Velocity's published rule:
  - separation bit 0x02 picks the top table's copy;
  - the status bit 0x40 of an entry in the table above picks a lower table's copy;
  - the allocated block count (u32 BE at 0x395) gives the number of levels.

  Read-only packages are unchanged. Two synthetic packages, one per rule, now read the right chain. The real Update
  #2 / #3 packages read identically (game tier). A real package with the second copy live has not been seen, so the
  level-2 branch is untested.
- **offsets-read.** A load passes when it equals the Infantry or the Vehicle asset's values; the log cannot tell the
  kinds apart. With the Infantry asset unknown, the check is still held. With only Infantry recorded (runs made before
  this change), it behaves as before.
- **crashreader.** It decodes from the function start when that is within 0x2000 bytes. Otherwise it uses the first
  start in `eip - 0x60 .. eip` whose decode lands on eip, which is exeref.window's rule.
- **pipeline.common().** NavKit's shared unpack carries no hash stamp, so freshness is judged by time: it is used
  while its `fbrb.manifest` is at least as new as the game's bundle, and otherwise this tool's own hash-stamped unpack
  is used. The cost of a false "stale" is one extra unpack. Nothing writes into NavKit's folder.
- **Left as they are, on purpose:**
  - **run_tests `--only` with no name.** tests\test_cli_errors.py uses it on purpose to check the unexpected-error
    path. The fix was reverted (c5a03a2).
  - **exeref `parse_int('0100')`.** It refuses with "not an address (hex, e.g. 0x0143375C)". A 4-digit value with a
    leading zero is ambiguous between decimal and hex, and the refusal names the unambiguous form.
- **Asserts for invariants.** Some were added during typing: `children` is not None, `level_entry` found, and
  similar. They turn paths that already crashed with TypeError / AttributeError into AssertionError. No working path
  changed.

## Ruff findings, all cleared (a5266f4)

| Rule | Count | What was done |
|---|---|---|
| B023 (loop variable in a closure) | 45 | Every one was called within its iteration, so none was a live bug. Each closure now binds what it reads as a default argument. validate.py's checks are now (function, arguments) pairs instead of lambdas. |
| B007 (unused loop variable) | 43 | Renamed `_name`, after checking that no later code in the same scope reads it. aptrefs iterates `.values()`. |
| F841 (unused local) | 7 | Dropped; calls with side effects are kept (`g.char(...)` copies a character into the PC movie). |
| B904 (raise in except without `from`) | 6 | `from e`, or `from None` for user-facing messages. |

## Implicit state

| Risk | Before | Now |
|---|---|---|
| navbuild's grid settings (7 module globals rewritten by `set_pitch`) | A failed detailed pass left pitch 0.2 for the next build in the process. navvehicle saved and restored 20 globals by hand. | `navbuild.saved_settings()` restores them on exit, exceptions included. The detailed pass runs inside it, and navvehicle's `vehicle_settings` uses it. Tested (f43db56). |
| Per-build counters (navbuild, navwalk, navislands) | Cleared piecemeal; `REPAIR_STATS` was never cleared or read; `STEEP_STATS` was never cleared | `reset_stats()` clears all of them at the start of a build. `REPAIR_STATS` was removed. |
| `sys.path` growing per call | navkit `_apply` (once per apply), `navregress.level_water` (once per level) and navvehicle (3 sites) inserted core on every call | Inserted once, guarded, at import (eca4983). Module-level inserts in 80-odd entry files remain (one per process). |
| `print` rebound module-wide | navregress and harness | Removed. Their `__main__` sets stdout to line-buffered instead. |
| GUI reader thread | `self.tail` / `self.cancelled` shared with Tk; Cancel killed navkit.py but not its FbrbTool / DbxEdit children | Output and last lines go through the queue only, and `cancelled` lives on the Tk thread. Cancel ends the process tree (`taskkill /T`). Tested (ef5e757). |
| `pipeline.common()` cache | NavKit's unpack used forever | Used only while not older than the game's bundle (8b671f2) |
| meshview cache | A foreign pickle crashed; file handles were left to the garbage collector | Rebuilt; files closed |

**Remaining implicit state:**
- navbuild's settings are still module globals, now with a restore guarantee, not a settings object.
- navkitGUI reads the private `errors._state` (core is out of scope).
- `nav._harness_cells` / `nav._adj` / `Studio._gaps` are caches attached to objects.
- `aptgraft.Grafter.memo` shares PushData list nodes between repeated `code_range` calls of the same range. Safe
  today, and `test_aptgraft` checks `findings()`.
- The harness falls back to the working directory for runs (`os.getcwd()`), and overlay writes a bare `--out` name
  into the working directory.

## Tests run

- **Quick tier:** 43 of 43 before; 48 of 48 now, after every commit.
- **Game tier, after the changes** (on Project OPFOR BFBC2's game copy):
  - console readers on the Xbox files; the crash reader's code view and exeref on BFBC2Game.exe;
  - stock-MP AI, Atacama Conquest built; the test harness on sp_009 and sp_004_b;
  - Navigation Studio edit mode; the menu tools and bc2loc on the game copy; the 3D tools opening and drawing; the
    hub;
  - the selected list below.

  Selected game-tier tests, run on the final code, all 15 passed:
  - NavKit regression on sp_004_b and sp_009: within tolerance of the saved reference, 14 min. This confirms the
    navbuild settings and stats changes and the typing renames changed no build.
  - NavKit vehicle path data on ONS_MP_002; links and route edges; boat mesh and routes on ONS_MP_004.
  - hpdtool round trip: 144 of 144 identical. kymesh round trip: 8,858 of 8,858 identical.
  - hpdcheck on DICE's files; the mesh viewer on the game's models; exeref and the crash reader's code view.
  - The console readers on the Xbox files; stock-MP AI with Atacama Conquest built; the test harness on sp_009 and
    sp_004_b; the menu tools on the game copy; the level loader snapshot.
  - Not run: the tests that install into the game copy (traversals / apply / install) and the full tier.

## Commits (round 2, oldest first)

```
7c2ee7e culltree: never read the target level as its own reference; test
0b8f4cb console-readers: stfs follows the live hash-table copy; extract usage; tests
50e4d5c test harness: offsets-read accepts the Vehicle SectorOffsets too; test
5544ba7 crash reader: the code view decodes from the function start; test
fbc7aa1 navkit: apply keeps its report on any failure, also without --out; test
36ea6cf navkit: ordercheck / navcompare without route nodes; detailed_sectors tuples; tests/test_tool_edges.py
ee32bcd navigation-studio: overlay usage; tests/test_editing_offline.py
0009794 mesh-viewer: corrupt / foreign index cache rebuilt; --level; tests/test_meshview_index.py
d7793e7 stock-MP AI: sector offsets without sectors is a build error; test
a5266f4 ruff clean in the covered folders (B023, B007, F841, B904)
24aa565 tests/test_lint.py in the quick tier; target Python 3.12
f43db56 navkit: navbuild.saved_settings, reset_stats, REPAIR_STATS removed; tests
ef5e757 navkit GUI: reader thread through the queue only; Cancel ends the process tree; test
eca4983 sys.path set once per entry point; print no longer rebound
8b671f2 stock-MP AI: common bundles checked against the game's bundle; test
166ecdb mypy (default) clean again on navkit and the harness
b0a8b64 navkit types: hpdtool, kymesh, navsample fully annotated; containers typed
88d7bfd Types: menucommon, culltree, apt, stfs, xiso fully annotated; other folders' containers typed
b1d87e0 Types: dbxbin, pipeline, harness ingame / levelchecks; tests/test_types.py
6a75bca pyproject.toml: strict per-module mypy overrides; types test in run_tests
(+ this report)
```

Round 1 commits: 79c218d, 2d75957, 8f982a8, 49aa028, 8120ca6, 2c2d483, bc57c41, b1379b9 / c5a03a2 (`--only`, made
and reverted), 92e742b, 8f1efcb, 36a1d45, 401f1b7, 2164faf.

## Scores (0-10)

Each score is for the in-scope code: before the audit (4218381), and now. Each comes with the measured evidence and
the one change that would raise it most.

| Category | Before | Now | Evidence | What would raise it most |
|---|---|---|---|---|
| Code errors / correctness | 6 | 8 | 20 real bugs found in ~31k lines; none corrupted data and all are fixed, 17 with regression tests (14 shown failing on the old code). Wrong-output bugs were among them (stfs stale table, offsets-read false failure, navdiff scale, culltree self-confirmation). 48 of 48 quick tests and the game-tier subset pass. | Coverage on the orchestration code (navkit.py at 25 %, studio.py, inspector.py, validate.py): that is where the remaining unknown errors would live. |
| Implicit state | 4 | 6 | Before: mutable module globals with no restore, write-only counters, per-call `sys.path` growth, `print` rebound, GUI state shared across threads, a cache never checked. Now each has a guard and a test. Still module-level settings globals, and `sys.path` changed at import in 80-odd files. | Replace navbuild's module settings with a `BuildSettings` object passed into `build` / `stage` (navvehicle's override becomes a value, not a patch). |
| Typing | 1 | 5 | 6 → 249 of 1,454 functions fully annotated (17 %); 14 modules strict; `--check-untyped-defs` errors 446 → 52. | Annotate navbuild, navwalk and navkit.py (the largest untyped modules) and switch `check_untyped_defs` on globally (52 errors to clear). |
| Overall code quality | 6 | 7.5 | Excellent docstrings and design notes, a strong test culture, and careful error reporting (errors.guard). Held back by very large modules (aptgraft ~2,000 lines, navbuild ~1,750, navsample_offline ~1,500) and global settings. | Split the largest modules along their sections (aptgraft: movie model / assembler / graft recipes). |
| Readability | 7 | 7 | Clear prose docstrings and comments explaining the why (DICE parity, offsets, formats). About 900 lines over 120 characters (max 204), dense one-line statements in places, and some single-letter names in long functions. Round 2 split some reused names; the layout is unchanged. | Wrap the 900 over-long lines (a formatter run, done per file with the owner's consent; not done here, by instruction). |
| Test coverage | 5 | 6 | Quick-tier line coverage 55.2 % → 57.7 %. Files at 0 %: 7 → 3. A strong game tier exists but is not measured. | Offline tests for navkit.py's orchestration (prepare / build_inputs / install with a stand-in OPFOR folder) and for studio.py's non-GL logic. |
| Conformance to coding standards | 5 | 8 | Before: no lint configuration, 122 F/E/W/B findings. Now: a configuration matching the house style, 0 findings, enforced in the quick tier. E501 and the house's compact statements are deliberately not enforced. | Extend the rule set gradually (I for import order, UP for pyupgrade, SIM) and fix per folder. |
| mypy usage | 0 | 7 | Before: never run, no configuration. Now: a configuration, 0 default errors in 11 folders, 14 strict modules, a quick-tier test, and a probe verifying the strict override bites. | `check_untyped_defs = true` globally once the 52 are cleared. |
| ruff usage | 0 | 8 | Before: not used. Now: configured, clean, run by the quick tier on every `run_tests.py quick`. | Add the next rule families (I, UP) and cover the six excluded folders once their work lands (27 findings waiting). |

### The six excluded folders: provisional, read-only, UNFIXED

These were measured on main's committed 4218381 with the same configuration. Nothing was edited, and no line-by-line
bug review was done, so the correctness score is a guess from the metrics. mypy counts include core's own errors
where a folder imports core.

| Folder | Python files / lines | Tests | ruff | mypy default | `--check-untyped-defs` | Fully annotated |
|---|---|---|---|---|---|---|
| core | 21 / 3,361 | 8 | 5 | 4 | 98 | 0 of 174 |
| bfbc2-bf1943-import | 11 / 2,514 | 4 | 5 | 1 | 27 | 0 of 93 |
| bfbc2-console-convert | 15 / 3,331 | 7 | 7 | 5 | 39 | 0 of 150 |
| bfbc2-shader-tools | 5 / 3,502 | 1 | 8 | 2 | 207 | 0 of 159 |
| bfbc2-anim-tools | 5 / 1,394 | 3 | 1 | 0 | 10 | 0 of 64 |
| bfbc2-model-tools | 7 / 1,119 | 3 | 1 | 2 | 15 | 0 of 55 |

- **ruff, 27 findings in total:** 19 B007, 3 F401, 3 B904, 1 F841 and 1 B028.
- **Implicit-state indicators:** 69 `sys.path` mentions, 13 broad `except Exception` handlers (12 of them in core,
  mostly the error-reporting layer, as designed), and 1 `global` statement (shader-tools).
- **Known core findings** from following imports:
  - `errors.PIP_NAMES` cannot be typed as written.
  - `level.py:217` reuses the exception name `e`.
  - `navdata.py:85` assigns None to an ndarray-typed field.
  - `errors._state` is used as an API by navkitGUI.

| Category | Provisional score (unfixed) |
|---|---|
| Code errors / correctness | 6 (unreviewed; same culture and test density as the reviewed folders) |
| Implicit state | 5 |
| Typing | 1 |
| Overall code quality | 6.5 |
| Readability | 7 |
| Test coverage | 5 (mostly game tier; shader-tools has one test file) |
| Conformance to coding standards | 6 (27 findings) |
| mypy usage | 0 |
| ruff usage | 0 (configured at the root, not yet enforced there) |

## Plan for the rest (as written after round 2; carried out in round 3, below)

1. **navbuild settings object.** Replace `set_pitch` / module globals with a `BuildSettings` value passed through
   `build` / `stage` / `merge_detailed`. navvehicle's override becomes a value, and `saved_settings` can go.
2. **Typing.**
   - Clear the 52 `--check-untyped-defs` errors: mostly tests that monkeypatch, and `re.search(...).group()` guards.
   - Then set `check_untyped_defs = true` globally.
   - Annotate navbuild, navwalk, navkit.py and aptgraft next, and add each to the strict list when clean.
3. **Offline tests for the remaining 0-25 % files:** navkit.py's orchestration with a stand-in OPFOR folder, studio.py
   and inspector.py logic without OpenGL, and validate.py with synthetic builds. Measure game-tier coverage once, so
   the gaps are known.
4. **One `sys.path` bootstrap.** Replace the per-file inserts with a small `suite_paths.py` (or a `.pth` in a dev
   install) once the excluded folders can be edited.
5. **The six excluded folders, after tonight's work lands.** Same pass: ruff (27), mypy (core's 4 first), the bug
   review, and the same tests-first fixes. Extend `test_lint.py` / `test_types.py` to cover them.
6. **Ruff rule families** I, UP and SIM, folder by folder. Long-line wrapping, if wanted, one file at a time and never
   as a mass reformat.

## Round 3 (2026-11-09): the whole suite

Branch `quality/round3-2026-11-09`, from `main` at 198f488 (round 2 merged). Not pushed. All six steps of the plan
were carried out. The six folders excluded before (core, bf1943-import, console-convert, shader-tools, anim-tools,
model-tools) are now in scope.

Two rules applied throughout: nothing was written to or installed into the game copy, and the game was not started
(a playtest ran during part of the round).

### Converter output: identical before and after

Before any edit, the converters were run on a fixed sample from a checkout of 198f488 (`bfbc2-tool-suite-r3base`,
next to the suite so that the BF1943 workspace resolves). The same run was repeated on the final branch. Timings,
temporary paths and dates were normalised.

**What was run:**
- **Core:** FbrbTool on 12 game bundles; the level loader snapshot; water on every level; terrain against the
  in-game samples; navdata; navtune; errors.
- **BF1943 import:** the pipeline's game check; meshport, dbxport and havokport against DICE's own files;
  `portparity.py` (every model both games have); `dbxparity.py` (every partition both games have, 27 min).
- **A fixed import:** `bf1943import.py import garand` into a scratch folder. Its 77 output files were hashed with
  SHA-256.
- **Console converter:** havok, texture, mesh decode, terrain, soot, audio, and tree mesh sets (game tier).
- **Shader databases:** quick tier and Xbox 360.
- **Granny:** read / shift, Xbox to PC, aim animations.
- **Model tools:** models, texwrite, decimate.

**Result:**
- All 30 runs exit 0 before and after.
- The portparity report, its JSON and the dbxparity JSON are byte-identical.
- The 77 imported files have identical hashes.
- Every test's output is identical but one line: the mesh viewer's "indexing the game copy" note, which appears once
  per fresh index cache.

**Typing edits to the converters** were checked a second way as well: the code, with annotations, typing-only imports
and `cast()` stripped, is identical to the original apart from three local renames.

**The fixes below that could touch converter output do not change it on these inputs:**
- havok's arrays of pointers: test_havok's converted files and its report are unchanged.
- dbxport's local references: the read-only scan of the WW2 work folders found no dangling local reference, and the
  dbxport outputs are unchanged.

### NavKit output: one intended difference, proven

The NavKit regression was run on sp_004_b and sp_009 before (198f488) and after, and the `pathdata.hpd` files were
compared with the sectors' date / time stamps set aside.

- **sp_009:** identical.
- **sp_004_b:** the same 40 sectors, 16,131 nodes and 47,466 edges, but 496 bytes more mesh data in 7 sectors. These
  are the normal sectors next to its 27 detailed (pitch 0.2) sectors, and the extra bytes are neighbour links.

**The cause is a bug of the old module-global settings.** After the detailed pass, `merge_detailed` re-links stage a's
normal sectors across the mixed seams, and it did so while the globals were still set for pitch 0.2. So
`Level.connected` judged 0.8 m cells with the 0.2 m pitch and its 0.3 m link step instead of 0.6, and found fewer
neighbour links. With `BuildSettings`, each stage's `Level` carries its own settings, and the re-link uses 0.8 m / 0.6.

**Proof:** a one-line experiment gave stage a's `Level` the detailed settings for the re-link (the old behaviour), and
the build was again identical to 198f488's. That one line explains the whole difference.

**Kept, as a fix:**
- The parity measures are unchanged (ground 98.49, extra 7.23, nodes 0.38, connectivity 100, over-connection 18.21,
  AI 99.4).
- Every other sector is identical.
- To restore the old links, give `a['level']` the detailed settings before the re-link loop in `merge_detailed`.

### Bugs found and fixed in round 3

**Core and the converters** (line-by-line review of all six folders; findings confirmed by reading both caller and
callee):

| # | Where | What went wrong | Test |
|---|---|---|---|
| 21 | core\bfbc2core\terrain.py `find`, water.py `read` | A level name not in lower case found nothing | test_core_edges (fails on 198f488) |
| 22 | core\bfbc2core\errors.py `_window` | A GUI tool's error in a worker thread opened a Tk window from that thread; Tk is not thread-safe and could hang or abort the tool | test_core_edges (fails on 198f488) |
| 23 | core\bfbc2core\navdata.py caches | Two processes writing the same cache entry collided on the fixed `.tmp` name | per-process names; reviewed, no test |
| 24 | core\bfbc2core\dbx.py `_number_text` | A number field of an unusual size printed as one big integer, not as to_xml and DbxEdit show it; no game file has such a size | test_converter_edges (fails on 198f488) |
| 25 | bf1943-import\dbxport.py `_align` | A substituted template value kept the template's own instance references, which dangle in the ported partition. The array and nested paths already removed them; this one did not | test_converter_edges (fails on 198f488) |
| 26 | console-convert\havok.py `_member` | Objects reached only through an array of pointers were never converted (they stayed big-endian); virtual objects were unaffected | test_havok, unchanged output |
| 27 | console-convert\texture.py `check` | Formats without a block decoder (RGBA16F) raised IndexError in verify-texture | test_converter_edges (fails on 198f488) |
| 28 | console-convert\convert.py verify-texture | Exited 0 when a texture failed to convert, against its own usage text | reviewed; needs the game |
| 29 | shader-tools\pcbuild.py | An unused `FORMATS` table with one wrong entry | removed |

**The rest of the suite:**

| # | Where | What went wrong | Test |
|---|---|---|---|
| 30 | navkit\navkit.py `prepare` | With `--box` and `--y` given, it still sized a box from the level's objects, so it failed on a new map with nothing placed | test_navkit_offline (FileNotFoundError on 198f488) |
| 31 | navkit\navkit.py, tools\navregress.py | A SectorOffsets file without its values raised AttributeError; it is now a NavKitError (navregress: RuntimeError) naming the file | typed; message only |
| 32 | stockmp-ai\validate.py `check_assets` | A missing template partition raised TypeError instead of the check's "not packed" | test_validate (TypeError on 198f488) |
| 33 | navkit\tools\navbuild.py `merge_detailed` | The re-link at mixed seams used the detailed settings for normal sectors (see above) | NavKit regression, plus the experiment |

**Decided, not changed:**
- **core\navpath.py.** The A* heuristic can exceed the remaining cost by 1/16 m per edge, because stored edge costs
  are rounded down. A route can therefore be up to a few centimetres above the cheapest. The docstring now says so.
  The algorithm was kept, because the harness's saved route measures depend on it.
- **shader-tools:**
  - `program_block` raises with more than 255 registers where `own_block` clamps. Raising is the safer behaviour.
  - A donor GUID collision in `merge` is refused by `check()` before its report. It is refused either way.
- **console-convert\mesh.py.** `decode` and `convert` would treat a layout holding both 0x0C and 0x0D elements
  differently. No such mesh is known; noted.
- **The read-only review also noted:**
  - havokport's dropped deactivators keep global fixups (4.6.1's deactivator holds no pointers);
  - subparts with near-unit translations are left unswapped;
  - template choice depends on file order;
  - meshport records the first LOD's layout per part name.

  These are latent or order-dependent risks, not bugs on the current inputs.

### Implicit state

| Item | Round 2 | Now |
|---|---|---|
| navbuild settings | Module globals, `set_pitch`, `saved_settings`; navvehicle patched ~20 globals and swapped navsample / navwalk for shims | A frozen `BuildSettings` value passed through `build` / `stage` / `merge_detailed` (`cfg.detailed()` for the 0.2 pass). navvehicle builds a value, and `stage(grid=...)` takes a ready grid, so the shims are gone. `--no-detail` passes a value. Setting an upper-case navbuild global no longer affects a build (nothing in the suite or OPFOR did). |
| sys.path | 236 manipulation lines in 138 files; some grew per call | `suite_paths.py`, one bootstrap: each file locates the root from its own path and calls `suite_paths.use(...)`, and each folder is added once. In a regression run sys.path stays at 11 entries (it was 25, then 33). Every module still imports with only its own folder on sys.path (checked for all 182 files on both trees; no new failure). 45 entry points give the same exit code and first line from their own folder and from the root. Six lines naming sibling projects (bf1943-tool-suite) stay. |
| Tk from worker threads | core errors could open a window from any thread | Main thread only |
| Cache files | navdata's fixed `.tmp` names | Per-process names |
| Name case | terrain / water needed lower case | Any case |

**Left:**
- The per-build counters (`ORDER_IDS`, `ORDER_STATS`, `LADDER_STATS`, `DETAIL_STATS`) are still module-level,
  cleared by `reset_stats()`.
- `errors._state` is module-level by design (one tool per process).
- `level.heightfield_of` reuses an unpacked terrain folder after a game patch.
- `effectport` / `dbxport` template choice depends on file order.

### Typing, mypy, ruff

**mypy:**
- `check_untyped_defs = true` for the whole suite.
- The fully annotated modules are checked strictly (`disallow_untyped_defs`, `disallow_incomplete_defs`):
  - every bfbc2core module;
  - hpdtool, kymesh, navsample, navbuild, navwalk, navkit;
  - menucommon, aptgraft, culltree;
  - apt, stfs, xiso;
  - dbxbin, pipeline, stockmp;
  - shaderdb, xenos, xenos_hlsl, pcbuild;
  - the harness's runs / ingame / levelchecks;
  - 14 test modules.
- mypy reports 0 errors in every folder, default and with check_untyped_defs. Both are enforced by
  `tests\test_types.py` in the quick tier.

**Fully annotated functions:**

| Group | Before round 1 | After round 2 | Now |
|---|---|---|---|
| Six folders | 0 of 695 | 0 of 697 | 316 of 702 (45 %) |
| The rest | 6 of 1,457 | 249 of 1,487 | 573 of 1,586 (36 %) |
| Suite | 6 of 2,152 | 249 of 2,184 (11 %) | 889 of 2,288 (39 %) |

**`--check-untyped-defs` errors:**
- Six folders: 385 at the start of round 3 (core 90, shader-tools 206, console-convert 38, bf1943-import 26,
  anim 10, model 15) → 0.
- The rest: 52 → 0.
- Default mypy, six folders: 4 → 0.

**ruff:**
- Rules F, E, W, B, I, UP and SIM. Left off, with their reasons in pyproject.toml:
  - SIM115: `open(...).read()` is the house idiom.
  - UP031: %-formatting of the Xenos disassembly, which a rewrite could change.
- Findings with the round-3 rules: 231 at 198f488 (93 in the six folders) → 0.
- Enforced over every folder by `tests\test_lint.py`.
- Import sorting surfaced two hidden order dependencies. The menu-tools tests and core's terrain test imported `apt`
  / `navkit` only because an earlier import had put them on sys.path. They now name their folders.

### Tests and coverage

**New quick tests (48 → 54):**
- core edges and converter edges, as above.
- **NavKit's orchestration** on a stand-in OPFOR folder (`test_navkit_offline`): a guard records any write outside the
  temporary folder; there were none.
- **Studio and Map Inspector logic** without a window (`test_studio_offline`, `test_inspector_offline`; panels drawn
  in a windowless ImGui frame).
- **Stock-MP AI validation** on synthetic builds (`test_validate`).

**Quick-tier line coverage:**

| File or group | Round 2 end (198f488) | Now |
|---|---|---|
| navkit.py | 25 % | 98 % |
| studio.py | 11 % | 93 % |
| inspector.py | 0 % | 90 % |
| validate.py | 0 % | 100 % |
| The rest | 57.7 % | 67.0 % |
| Six folders | 21.4 % | 24.4 % |
| Suite | 46.2 % | 53.5 % |

The six folders are mostly tested on game data.

**Game-tier coverage, measured once** (the quick tier plus every game-data test that only reads the game):
71.7 % of the suite's lines (the rest 76.7 %, the six folders 60.9 %). Least covered: the Xenos shader translator and pcbuild (xenos, xenos_hlsl, pcbuild: 0 % - no test exercises them; the WW2 port's scripts use them), portparity (run separately, not under coverage), core's viewer (34 %, needs a window), navkitGUI (19 %), levelobjects, navsample and navregress (35-39 %: their game paths run in NavKit's prepare and sample steps, which start the game). Under coverage the Navigation Studio edit-mode test ran out of its 15-minute wait for the background build; it passes on its own (464 s).

### Tests run at the end

- **Quick tier:** 54 of 54.
- **Converter comparison:** 30 of 30 runs identical, as above.
- **NavKit regression on sp_004_b and sp_009:** passed, within tolerance of the saved reference. Output is identical
  but for the documented re-link.
- **Game-data tests that only read the game:** all passed in the comparison and coverage runs.
  - core: level, water, terrain, navdata, FbrbTool;
  - every converter test;
  - NavKit's vehicle, links and boat tests;
  - the harness on sp_009 and sp_004_b, the crash reader and exeref;
  - the console readers, the menu tools and the mesh viewer;
  - stock-MP AI Atacama;
  - the Navigation Studio edit mode.
- **Not run:**
  - tests that install into the game copy: test_traversal, test_apply, bc2loc set;
  - the full tier (the DbxEdit corpus and all bundles, about 90 minutes plus 5 hours of NavKit regression).

### Commits (round 3, oldest first)

```
2c28336 core: case-free terrain / water names, no Tk from worker threads, per-process cache temp names, dbx number text, navpath docstring; ruff / mypy clean; test_core_edges
2736e59 Converters: dbxport local refs, havok pointer arrays, texture.check, verify-texture exit code, pcbuild FORMATS; ruff / mypy clean
90fab45 tests: lint and types cover every folder; converter and core edge tests in the quick tier
ffc1a7e navkit: BuildSettings value through build / stage / merge_detailed; navvehicle without shims
7877bc7 Types: core fully annotated, the converters and the shader tools typed (output unchanged)
200eb03 navkit: navbuild, navwalk, navkit.py fully annotated; SectorOffsets error; prepare with --box/--y; test_navkit_offline
439846e Types: aptgraft and the rest; validate.py missing template; test_validate
43596d9 navigation-studio, map-inspector: logic split from drawing; offline tests
f5f6963 pyproject.toml: check_untyped_defs everywhere; strict list extended
1df4f64 suite_paths.py: the one sys.path bootstrap
d2d6b3d ruff I: imports sorted; two hidden import-order dependencies fixed
15a5a21 ruff UP and SIM; pyproject.toml selects I, UP, SIM
(+ this report)
```

### Scores, round 3

Each category is scored 0-10 at three points: before round 1, after round 2, and now. For the whole suite, the earlier
scores are weighted by size: about 70 % covered in rounds 1 and 2, 30 % the six folders. The six folders' earlier
scores are the provisional read-only ones.

**Whole suite:**

| Category | Before R1 | After R2 | Now | Evidence now | What would raise it most |
|---|---|---|---|---|---|
| Code errors / correctness | 6 | 7.5 | 8.5 | Every folder reviewed line by line; 33 bugs fixed over three rounds, 24 with regression tests that fail on the old code (two more proven by the NavKit regression and test_havok). The converters' outputs are identical on a 30-run sample (portparity, dbxparity, a 77-file import). Quick, game-data and regression runs pass. | Run the full tier (DbxEdit corpus, all bundles, NavKit on every level) under the new code |
| Implicit state | 4 | 5.5 | 7.5 | BuildSettings value; one sys.path bootstrap; no Tk from threads; caches keyed and named safely. | Move the per-build counters into a result object returned by build |
| Typing | 1 | 3.5 | 8 | 39 % fully annotated (889 of 2,288); 0 mypy errors with check_untyped_defs everywhere; ~45 strict modules. | Annotate the remaining large modules (navsample_offline, navwater_mesh, navdomains, studio, the harness's checks / levelinfo) and grow the strict list |
| Overall code quality | 6 | 7 | 8 | All of the above, plus offline tests for the GUIs' logic. | Split the largest modules (aptgraft ~2,000 lines, navbuild ~1,900) |
| Readability | 7 | 7 | 7.5 | Sorted imports, modern annotations that document the types; long lines unchanged. | Wrap the ~900 over-long lines, file by file with the owner's consent |
| Test coverage | 5 | 5.5 | 7 | Quick tier 46 → 54 %; the rest 67 %; the GUIs' logic 90-98 %; game tier see above. | Quick tests with small synthetic inputs for the converters (they are tested mainly on game data) |
| Conformance to coding standards | 5 | 7 | 8.5 | F/E/W/B/I/UP/SIM clean everywhere, enforced. | Enforce line length per folder once wrapped |
| mypy usage | 0 | 5 | 9 | Every folder, check_untyped_defs, a strict list, quick-tier enforcement. | `disallow_untyped_defs` globally once the remaining 61 % are annotated |
| ruff usage | 0 | 6 | 9 | Seven rule families, every folder, enforced. | Add PL (pylint subset) or RUF and fix folder by folder |

**The six folders:**

| Category | Before (provisional) | After R2 (unchanged) | Now | Evidence now | What would raise it most |
|---|---|---|---|---|---|
| Code errors / correctness | 6 | 6 | 8.5 | Line-by-line review: 9 confirmed bugs fixed, none of them changing output on the sample; output identical on every comparison run. | The full-tier DbxEdit corpus check of the Python DBX reader under the new code |
| Implicit state | 5 | 5 | 7 | No Tk from threads, per-process cache names, case-free lookups, the bootstrap. | Refresh the terrain unpack when the bundle changes; fix template choice to a sorted order |
| Typing | 1 | 1 | 8.5 | 45 % fully annotated; every bfbc2core module and the four shader-tools modules strict; 0 check_untyped_defs errors. | Annotate the converters' remaining functions (dbxport, havokport, mesh, texture, audio) and add them to the strict list |
| Overall code quality | 6.5 | 6.5 | 8 | — | Quick synthetic tests for each converter |
| Readability | 7 | 7 | 7.5 | — | Shorter functions in havokport / audio |
| Test coverage | 5 | 5 | 6 | Quick tier 21 → 24 %; game tier see above. | Quick synthetic round-trip tests (no game data) for mesh, meshset, texture, havok and dbxport |
| Conformance to coding standards | 6 | 6 | 8.5 | ruff clean (93 → 0 findings with the round-3 rules). | As above |
| mypy usage | 0 | 0 | 9 | 0 errors with check_untyped_defs; core and shader tools strict. | Converters on the strict list |
| ruff usage | 0 | 0 | 9 | Enforced in the quick tier. | As above |

## Plan for the rest (after round 3)

1. **Run the full tier** under the new code: the DbxEdit corpus and fingerprints, the Python DBX reader against
   DbxEdit, all 1,055 bundles, and the NavKit regression on every level with DICE path data.
2. **Quick synthetic tests for the converters** (mesh, meshset, texture, havok, dbxport, granny), so they are tested
   without the game.
3. **A build result object.** Return the per-build counters from `navbuild.build` instead of keeping them in module
   globals.
4. **Annotate the converters' remaining functions** and the large NavKit tools; grow the strict list. Then consider
   `disallow_untyped_defs` globally.
5. **Long lines, per file, with the owner's consent.** Then enforce E501 at 120 characters, folder by folder.

## Architecture step M1 (2026-11-09): one path finder - done

Branch `arch/m1-paths-2026-11-09`, from `main` at a482659. This is step M1 of the architecture audit's migration plan
(ARCHITECTURE_AUDIT.md 4.4 and 8.3). Not pushed. Nothing was written to the game copy, and the game was not started.

**What changed:**
- **`core/bfbc2core/paths.py`** is the one implementation of path discovery. It needs only `os` and `sys`, and it
  covers:
  - Project OPFOR BFBC2 (`find_opfor`, `opfor_candidates`, `walk_up`);
  - the game copy (`game_dir`, `game_exe`, `find_game_exe`);
  - the suite (`SUITE`, `suite_root`) and its built tools (`suite_tool`, `find_tool`);
  - the sibling projects (`sibling`, `find_sibling`, `use_folder`);
  - the errors `GameCopyNotFound` and `ToolNotFound`.
- **`errors.explain`** recognises `GameCopyNotFound` by type. The text tests stay, for the tools' own errors.
- **The old locators are wrappers.** Each keeps its signature, return value, exception type and message:
  - `navkit.find_opfor` / `find_tool` / `Project`;
  - `menucommon.find_opfor` / `find_exe`;
  - `shaderdb.find_opfor`;
  - `convert.default_exe`;
  - `crashreader.find_exe`;
  - `bf1943import.default_source` / `fbrb1943`.

  `navparity` / `navvehicle.opfor_root`, `harness.opfor` and `run_tests.opfor` were already wrappers over
  `navkit.find_opfor`, and are unchanged.
- **detailed.py's hard-coded path** is now `detailed.opfor_root()`: `OPFOR_ROOT` as before, else NavKit's search. On
  this machine the search finds the same folder.
- **`pipeline.Paths` and `level.load` / `heightfield_of`** find the folder through `paths`, so they no longer import
  NavKit for it. This partly fixes audit edges 1 and 8.
- **`bf1943import.fbrb1943`** no longer grows `sys.path` on every call.
- **Tests:** the game-data tests' sibling and OPFOR paths, including the last sys.path lines naming
  bf1943-tool-suite, go through `paths`.

**The search orders differ between locators, and each one keeps its own.** They are documented in the module:
- **navkit, menucommon, detailed, pipeline, level:** walk up from the suite. The first candidate is inside the suite.
- **shaderdb and convert:** the folder beside the suite only.
- **crashreader:** walks up from the harness folder, one level lower than the others.
- **convert and crashreader:** take no given folder, and need `game\BFBC2Game.exe`. The others need only `game\`.
- **detailed:** takes `OPFOR_ROOT` unchecked.
- **The BF1943 locators:** `default_source` takes `BF1943_XENON` *before* the folder beside the suite, and does not
  check it. `fbrb1943` takes the folder beside the suite *before* `BF1943_SUITE_FBRB`.
- **The game-data tests:** some read `OPFOR_ROOT` and some do not.
- **Never resolved:** paths are made absolute but not resolved, so a junction is searched beside itself.
- **`OPFOR_TOOLKIT`:** the suite still never reads it.

**Intended differences, all checked by test_locators:**
- `convert.default_exe` now spells the folder beside the suite without `..\..`. It is the same file.
- `pipeline.Paths` and `level.load` raise `GameCopyNotFound` rather than `NavKitError`, with the same message and the
  same `errors.explain` cause. Nothing caught the old type.
- `level.heightfield_of` raises `ToolNotFound` rather than `NavKitError`, with the same message.
- With no OPFOR at all, detailed.py now raises instead of using the fixed path.

**Tests:**
- **Quick tier:** 55 of 55 pass. The new test is `core/tests/test_paths.py`, 42 checks on a stand-in tree:
  - OPFOR beside, inside and above the suite;
  - the given folder before `OPFOR_ROOT` before the search; an empty `OPFOR_ROOT` counts as unset;
  - a missing game copy and missing siblings;
  - relative and junction layouts;
  - importable with only `core` on sys.path.
- **ruff and mypy:** clean. `bfbc2core.paths`, `test_paths` and `test_locators` are on the strict list.
- **`tests/test_locators.py` (game tier, new):** passes. Every locator is asked in a subprocess of this checkout and
  of the main checkout, run with `python -I -B` so nothing is written there. Three runs: plain, with `OPFOR_ROOT` /
  `BF1943_XENON` set, and with OPFOR and the BF1943 suite hidden. Values, exception types, messages and causes are
  identical, apart from the differences above.
- **Importability:** every one of the 187 files imports with only its own folder on sys.path, and with its folder plus
  `core`, as on main. The same 8 fail on both trees: moderngl / imgui_bundle are not visible under `-I`.
- **Game tests that only read the game** all pass:
  - console-convert, 8;
  - bf1943-import and BF1943 related, 11;
  - shader-tools, 2;
  - granny, 3.
- **`bf1943import.py import garand`:** run on main and on the branch, each into a scratch folder. The 78 files (77
  outputs and import.json) are byte-identical, and so is the log.

**Left for later steps:**
- **M4:** `navparity.opfor_root` / `navvehicle.opfor_root` still go through navkit, because they keep NavKitError
  (edges 12-16).
- **M4 / M5:** `pick.py` (edge 3) still uses `navkit.Project`. `level.load` still calls `navkit.Project` /
  `prepare` for an unprepared level (edge 1).
- **Not converted:**
  - the hub's `TOOLS` table (`BFBC2 Tool Suite.pyw:37`, an exe and build.bat pair);
  - the per-module `SUITE = ...` constants;
  - OPFOR's `attachments.py` `DBXEDIT`.
- **Not unified:** the differing search orders, on purpose. Unify them only as a deliberate change. shaderdb
  widening to the walk-up was the audit's suggestion.
- **Not touched:** the implicit `shaderdb` edge in OPFOR's attachments.py (M3).
