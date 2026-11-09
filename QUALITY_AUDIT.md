# Code quality audit, 2026-11-08 / 09

Branch `quality/audit-2026-11-08` (worktree `C:\Programming\Projects\bfbc2-tool-suite-quality`), from `main` at 4218381.
Not pushed. Two rounds:

- **Round 1 (2026-11-08):** audit, configuration, the first eight bugs.
- **Round 2 (2026-11-09):** every remaining finding fixed or decided, the ruff findings cleared, implicit state
  reduced, types added, tests added, rescan and scores.

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

## Plan for the rest

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
