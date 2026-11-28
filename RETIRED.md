# Retired tools

The one record of the suite's retired and superseded tools (the user's retirement policy, 2026-11-20). A retired
tool's folder or file is deleted entirely; git history keeps it. "Last commit with it" is the newest commit that still
has it: get it back with `git checkout <sha> -- <path>` (or look with `git show <sha>:<path>`). CHANGELOG.md's
**Removed** says the same once for users, in the version it went.

Not listed here: the Python twins retired inside tools that are still alive (a `*_reference` function, a `use_native`
switch, a reader's Python path). Those are in the benchmark log's inventories ("Retired Python", "Retired C#
tools", RT1 "Part 2: the inventory", RT2: all four now in `rust/BENCHMARKS_ARCHIVE_1.md`; later rounds' in
`rust/BENCHMARKS.md`), in `PYTHON_STATUS.md` and in CHANGELOG.md.

## The rule

1. A retired tool's folder is deleted entirely. A folder survives only if part of it is still a living tool, and then
   its README describes only what is left (as `bfbc2-navigation-studio\` keeps `overlay.py`).
2. One row per retired or superseded tool below: the date, what replaced it and where its features went, the last
   commit that still had it, the round. CHANGELOG.md's **Removed** says it once for users.
3. Planned tools that are not built get no folder (their plans live in the document of the tool they will be part
   of, or in `DESIGN.md`).
4. Python twins retired inside living tools stay in `rust/BENCHMARKS.md`'s inventories and CHANGELOG.md, not here.

### Checklist for a round that retires a tool

1. **Proven equal:** the replacement gives the same answers on recorded data (the tool's own answers recorded before
   it goes, kept as the tests' reference), and the proof is in the round's `rust/BENCHMARKS.md` section.
2. **Nothing points at it:** grep the suite, Project OPFOR BFBC2, the hub, the tests and the docs for its file names,
   folder and command; each mention is repointed, or kept only as dated history (a past CHANGELOG version, a dated
   audit or BENCHMARKS section) with no live link.
3. **The hub:** its launcher entries removed or redirected to the replacement.
4. **OPFOR:** updated (and its tests run) if it called the tool; committed there in the same round.
5. **Delete:** the folder or files (check for reparse points first and remove them with `cmd /c rmdir`), and any
   leftover `__pycache__` in the main checkout.
6. **Record:** a row below (the last commit with it: the parent of the commit that deletes it; check it with
   `git cat-file -e <sha>:<path>`), and a CHANGELOG **Removed** entry.

## Retired

Newest first.

| Tool | Retired | Replaced by, and where its features went | Last commit with it | Round |
|---|---|---|---|---|
| `bfbc2-bf1943-import\dbxparity.py` (command line, `compare`, `predict`) | 2026-11-22 | `bfbc2.exe dbxparity` (`rust\bc2-cli\src\bf1943\dbxparity.rs`: the comparison, `--predict` on the Rust `dbxport` with the catalogue built in process; every report and JSON byte for byte on the user's files, `bfbc2-bf1943-import\tests\test_parity_reports.py`; its 36 recorded O3 lines all answered in Rust). The library is `dbxparitylib.py` (`instances`, the fair pools: Project OPFOR's WW2 QA, `test_dbxport_fair.py`, `rust\bench_bf1943.py`) | `0434c22` | PR1b-1d |
| `bfbc2-bf1943-import\portparity.py` | 2026-11-22 | `bfbc2.exe portparity` (`rust\bc2-cli\src\bf1943\portparity.rs` on `bc2_convert::meshport::Porter`: the 148 shared models, every report line and JSON row byte for byte; the 147 ports identical to `meshport.Porter`'s) | `0434c22` | PR1b-1d |
| **The level cover index's Python and its pickles; `retexture`'s Python twin** (`bfbc2-shader-tools\shaderdb.py`: `level_cover_prefixes`' loading and indexing, `_cover_key` / `_cover_load` / `_cover_save` / `_bundle_digest`, the `shadercover\<24 hex>.pkl` files; `retexture`'s body) | 2026-11-23 | The Rust core: `bc2_convert::shaderlevels` (the index, one `.bin` per level bundle and Dx keyed by the bundle's content key) through `bc2_native.shadercover_levels`; `retexture` on `bc2_formats::shaderdb::merge::retexture` through `bc2_native.shaderdb_retexture`. The same answers as the Python's recorded ones (`bfbc2-shader-tools\tests\test_shaderdb_levels.py`, `levels_golden.json`: 418 retextures, 24 stand-in game copies; `levels_golden_game.json`: every level of the game copy). Kept: `cover_prefixes(db)` and `coverage_from` (a loaded database's prefixes), `donor_levels` / `level_coverage` as thin callers | `4534f26` | OR3b |
| **`shaderdb.Merger`** (`bfbc2-shader-tools\shaderdb.py`'s Python merge class: `add_donor`, `clone_entry`, `add_part`, `finish`, with `ProgRef` and `Merger._shader_key`) | 2026-11-22 | The Rust core's merge: `shaderdb.merger(base)` makes a `bc2_native.ShaderMerger` (`rust\bc2-py\src\shaderdb_merge.rs`) on `bc2_formats::shaderdb::merge::Merger` (`clone_entry`, `add_part`, `add_donor_kept` added); `shaderdb.merge` uses it. The same answers as the class's recorded ones (`bfbc2-shader-tools\tests\test_shaderdb_merge.py`, `merge_golden.json`: 271 built, damaged and random cases; `merge_golden_game.json`: ONS_MP_008 + mp_008cq), the WW2 oracle byte for byte with Project OPFOR's 21 merges moved onto it. Kept: `retexture` (OPFOR's block edits), `verify_merge` / `entry_contents` (the Python command line's check) | `8ce3cd9` | OR3a |
| **DbxEditGUI.exe** (`bfbc2-dbx-tools\DbxEditGUI.cs`, the suite's last C#) with `DbxEdit.cs` (the C# DBX classes it was built on), `build.bat`, `tests\DbxTests.cs`, `tests\run_tests.bat`, `tests\build_if_stale.py` | 2026-11-21 | The map editor's **DBX inspector** (`rust\bc2-editor\src\dbxpanel.rs`, its model `dbxinspect.rs`; `bfbc2-editor --dbx <file>`, the hub's More tools > DBX inspector): every menu, the tree, Apply, Undo, Find, the level checks, XML, the edit script (`EDITOR_ARCHITECTURE.md` 9.4); identical to DbxEditGUI's recorded answers on 19 real files, 1,232 steps (`rust/bc2-editor/tests/dbxedit_parity.rs`, `tests/data/dbxeditgui_answers.json.gz`). Its commands were already `bfbc2.exe dbxedit` (R4); `tests\fingerprints_795745.txt` stays (the Python tests read it) | `851d94a` | GR4 |
| **The mesh viewer's page for a browser** (`bfbc2-mesh-viewer\meshview.py <mesh>` writing `<names>.html`: three.js, orbit, a switch per part, wireframe) | 2026-11-21 | The map editor's Asset preview and the asset browser's Meshes mode (`bfbc2-editor`; `bc2_convert::meshview` reads and draws as `meshview.py` does: 94 parts and 55 pictures of 18 recorded levels of detail identical, `rust\bc2-editor\tests\asset_preview_parity.rs`, `gr3.rs` until round PR1a); every feature's place in `EDITOR_ARCHITECTURE.md` 9.3. The folder stays: `meshview.py`'s picture command line (`<mesh> [--view] [--size]`, now its default output), `find`, and the library other tools import. `meshview.page()` and `PAGE` stayed, deprecated, for Project OPFOR BFBC2's WW2 preview pages (`src\tools\ww2\attachments.py` and `scopes.py` write `work\ww2\preview\ww2_attachments.html` / `ww2_scopes.html`) until round OR4 (2026-11-27) moved the page to the Rust core (`bc2_convert::meshview::page`, `bc2_native.meshview_page`) and retired them | `0badebd` | GR3 |
| `bfbc2-menu-tools\aptrefs.py` (command line) | 2026-11-21 | `bfbc2.exe aptrefs` (82 recorded lines, all answered in Rust); the library is `aptrefslib.py`; the usage `bfbc2.exe` prints is `rust\bc2-cli\src\menu\aptrefs_usage.txt` | `f826078` | RT3 (`7451a85`) |
| `bfbc2-console-readers\xiso\xiso.py` (command line) | 2026-11-21 | `bfbc2.exe xiso` (the hub's Xbox files tab); the library is `xiso\xisolib.py`, which also reads an image held in memory in the Rust core (`find_partition_bytes`, `walk_bytes`) | `f826078` | RT3 (`7451a85`) |
| `bfbc2-culltree\culltree.py` (command line) | 2026-11-21 | `bfbc2.exe culltree` (OPFOR's `console_cull` runs it); the library is `culltreelib.py` (F4's string-table reference, the row cache `bfbc2.exe` shares) | `f826078` | RT3 (`7451a85`) |
| `bfbc2-stockmp-ai\stockmp.py` (command line) | 2026-11-21 | `bfbc2.exe stockmp`; the library is `stockmplib.py` (summary, install, `pathdata_for`) | `f826078` | RT3 (`7451a85`) |
| `bfbc2-automated-map-and-bot-test-harness\crash-reader\crashreader.py` (command line) | 2026-11-21 | `bfbc2.exe crashreader` (the hub's Crash reader tab, OPFOR's testrun); the library is `crashreaderlib.py`, which `exeref.py` imports | `f826078` | RT3 (`7451a85`) |
| `core\bfbc2core\viewer.py`, `core\bfbc2core\pick.py` (the Python 3D view and its level picker) | 2026-11-21 | Nothing: no tool had used them since the Map Inspector (GR1) and the Navigation Studio (GR2) were folded into the map editor; only two tests imported them. `moderngl`, `glfw` and `imgui-bundle` left `requirements.txt` with them | `2bd770a` | RT3 (`1651beb`) |
| **LevelDiff.exe** (`bfbc2-diff-workbench\`: `level-diff\LevelDiff.cs`, `build.bat`, `tests\test_leveldiff.py`, the folder's README of planned tools) | 2026-11-20 | `bfbc2.exe leveldiff` (`bc2_formats::dbxedit::leveldiff`: the same arguments, text, `--json` file and exit codes, identical to LevelDiff.exe's recorded answers on 21 real level-pair cases and 9 test-level cases, `rust/bc2-cli/tests/leveldiff.rs`) and the map editor's Diff panel (`EDITOR_ARCHITECTURE.md` 9.5); the hub's Level diff tab and OPFOR's `opforcore.paths.leveldiff()` run it; the planned ImHex patterns and diff tools stay in `DESIGN.md` 4.7 | `b7deae6` | EX1 (`a9f26b0`) |
| `bfbc2-ai-behavior-simulator\`, `bfbc2-bot-ai-debugger\`, `bfbc2-tactical-map-authoring-tool\` (placeholder folders, a README each: "Planned - see ../DESIGN.md"; never built) | 2026-11-20 | Planned map editor workspaces (the user, 2026-11-19): their plans moved from `DESIGN.md` 4.3, 4.4 and 4.8 to `bfbc2-map-editor/EDITOR_ARCHITECTURE.md` section 10 (10.1 tactical authoring, 10.2 the bot AI debugger, 10.3 the AI behaviour simulator) | `5484ffd` | FM1 |
| `bfbc2-map-inspector\` (the folder: its README of where the Map Inspector's features went, and the editor's plan) | 2026-11-20 | `bfbc2-map-editor\`: `EDITOR_ARCHITECTURE.md` moved there with its history (`git log --follow`), a README of the editor; the feature table is `EDITOR_ARCHITECTURE.md` 9.1 | `5484ffd` (`bfbc2-map-inspector/README.md`, `EDITOR_ARCHITECTURE.md`) | FM1 |
| **Navigation Studio** (`bfbc2-navigation-studio\studio.py`, `editing.py`, their tests, the hub's "Open in Navigation Studio") | 2026-11-20 | The map editor's AI workspace and navmesh view mode (`bfbc2-editor.exe <level> --navmesh`; the hub's Levels > Open navigation): both path-data files, the colourings, path test, anomaly and compare overlays, navigation edits in the project's log, build / install / restore through `bfbc2.exe navkit` (`EDITOR_ARCHITECTURE.md` 9.2). `overlay.py`'s headless exports stay in the folder | `a0a1379` | GR2 (`8303a82`) |
| **Map Inspector** (`bfbc2-map-inspector\inspector.py`, its offline tests and its part of `tests\test_viewers.py`) | 2026-11-20 | The map editor (`rust\bc2-editor`, `bin\bfbc2-editor.exe`): its views were the editor's already; its property tree, "referenced by", link lines and Measure became Properties > All fields, the Links panel, `view-link` lines (K) and Measure (M) (`EDITOR_ARCHITECTURE.md` 9.1) | `e5a20f5` | GR1 (`b18b1ce`) |
| `bfbc2-auto-navkit\tools\graphstats.py` (command line) | 2026-11-20 | `bfbc2.exe graphstats` (same arguments and output; `BFBC2_CLI_PYTHON=1` no longer brings it back) | `6a7dd68` | RT1 (`839f91a`) |
| `rust\bc2-editor\prepare_level.py` (the editor's level preparation) | 2026-11-20 | `bfbc2.exe prepare_level`, which the editor runs | `6a7dd68` | RT1 (`839f91a`) |
| **FbrbTool.exe** (`bfbc2-fbrb-tool\FbrbTool.cs`, `build.bat`) | 2026-11-18 | The Rust core in process: `bfbc2core.fbrbtool`, `bfbc2.exe fbrbtool`, `bfbc2-fbrb-tool\fbrbtool.py` (the same output and exit codes, checked against the exe's recorded answers, `core\tests\fbrbtool_golden*.json`) | `f10522d` | R4 (`1569d9f`) |
| **DbxEdit.exe's command line** (`static class Program` in `bfbc2-dbx-tools\DbxEdit.cs`; `DbxEdit.cs` itself stays as DbxEditGUI's and LevelDiff's library) | 2026-11-18 | `bfbc2core.dbxedit`, `bfbc2.exe dbxedit`, `bfbc2-dbx-tools\dbxedit.py` (checked against the exe's recorded answers, `core\tests\dbxedit_golden*.json`) | `f10522d` | R4 (`1569d9f`) |
| `rust\bench_dbxedit.py`, `rust\bench_fbrbtool.py` (the exes against the in-process tools) | 2026-11-18 | Nothing: the exes they timed were retired (figures kept in `rust/BENCHMARKS_ARCHIVE_1.md`, "Retired C# tools") | `f10522d` | R4 (`1569d9f`) |
| **DbxTool** (`bfbc2-dbx-tools\DbxTool.cs`) | 2026-10-16 | DbxEdit's `xml` command (byte-identical XML on 43,290 files), rewritten from `FORMAT.md` | `f379a5f` | before rounds (`aac43b0`) |
