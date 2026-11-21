# Retired tools

The one record of the suite's retired and superseded tools (the user's retirement policy, 2026-11-20). A retired
tool's folder or file is deleted entirely; git history keeps it. "Last commit with it" is the newest commit that still
has it: get it back with `git checkout <sha> -- <path>` (or look with `git show <sha>:<path>`). CHANGELOG.md's
**Removed** says the same once for users, in the version it went.

Not listed here: the Python twins retired inside tools that are still alive (a `*_reference` function, a `use_native`
switch, a reader's Python path). Those are in `rust/BENCHMARKS.md`'s inventories ("Retired Python", "Retired C#
tools", RT1 "Part 2: the inventory", RT2) and in CHANGELOG.md.

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
| **LevelDiff.exe** (`bfbc2-diff-workbench\`: `level-diff\LevelDiff.cs`, `build.bat`, `tests\test_leveldiff.py`, the folder's README of planned tools) | 2026-11-20 | `bfbc2.exe leveldiff` (`bc2_formats::dbxedit::leveldiff`: the same arguments, text, `--json` file and exit codes, identical to LevelDiff.exe's recorded answers on 21 real level-pair cases and 9 test-level cases, `rust/bc2-cli/tests/leveldiff.rs`) and the map editor's Diff panel (`EDITOR_ARCHITECTURE.md` 9.5); the hub's Level diff tab and OPFOR's `opforcore.paths.leveldiff()` run it; the planned ImHex patterns and diff tools stay in `DESIGN.md` 4.7 | `b7deae6` | EX1 (`a9f26b0`) |
| `bfbc2-ai-behavior-simulator\`, `bfbc2-bot-ai-debugger\`, `bfbc2-tactical-map-authoring-tool\` (placeholder folders, a README each: "Planned - see ../DESIGN.md"; never built) | 2026-11-20 | Planned map editor workspaces (the user, 2026-11-19): their plans moved from `DESIGN.md` 4.3, 4.4 and 4.8 to `bfbc2-map-editor/EDITOR_ARCHITECTURE.md` section 10 (10.1 tactical authoring, 10.2 the bot AI debugger, 10.3 the AI behaviour simulator) | `5484ffd` | FM1 |
| `bfbc2-map-inspector\` (the folder: its README of where the Map Inspector's features went, and the editor's plan) | 2026-11-20 | `bfbc2-map-editor\`: `EDITOR_ARCHITECTURE.md` moved there with its history (`git log --follow`), a README of the editor; the feature table is `EDITOR_ARCHITECTURE.md` 9.1 | `5484ffd` (`bfbc2-map-inspector/README.md`, `EDITOR_ARCHITECTURE.md`) | FM1 |
| **Navigation Studio** (`bfbc2-navigation-studio\studio.py`, `editing.py`, their tests, the hub's "Open in Navigation Studio") | 2026-11-20 | The map editor's AI workspace and navmesh view mode (`bfbc2-editor.exe <level> --navmesh`; the hub's Levels > Open navigation): both path-data files, the colourings, path test, anomaly and compare overlays, navigation edits in the project's log, build / install / restore through `bfbc2.exe navkit` (`EDITOR_ARCHITECTURE.md` 9.2). `overlay.py`'s headless exports stay in the folder | `a0a1379` | GR2 (`8303a82`) |
| **Map Inspector** (`bfbc2-map-inspector\inspector.py`, its offline tests and its part of `tests\test_viewers.py`) | 2026-11-20 | The map editor (`rust\bc2-editor`, `bin\bfbc2-editor.exe`): its views were the editor's already; its property tree, "referenced by", link lines and Measure became Properties > All fields, the Links panel, `view-link` lines (K) and Measure (M) (`EDITOR_ARCHITECTURE.md` 9.1) | `e5a20f5` | GR1 (`b18b1ce`) |
| `bfbc2-auto-navkit\tools\graphstats.py` (command line) | 2026-11-20 | `bfbc2.exe graphstats` (same arguments and output; `BFBC2_CLI_PYTHON=1` no longer brings it back) | `6a7dd68` | RT1 (`839f91a`) |
| `rust\bc2-editor\prepare_level.py` (the editor's level preparation) | 2026-11-20 | `bfbc2.exe prepare_level`, which the editor runs | `6a7dd68` | RT1 (`839f91a`) |
| **FbrbTool.exe** (`bfbc2-fbrb-tool\FbrbTool.cs`, `build.bat`) | 2026-11-18 | The Rust core in process: `bfbc2core.fbrbtool`, `bfbc2.exe fbrbtool`, `bfbc2-fbrb-tool\fbrbtool.py` (the same output and exit codes, checked against the exe's recorded answers, `core\tests\fbrbtool_golden*.json`) | `f10522d` | R4 (`1569d9f`) |
| **DbxEdit.exe's command line** (`static class Program` in `bfbc2-dbx-tools\DbxEdit.cs`; `DbxEdit.cs` itself stays as DbxEditGUI's and LevelDiff's library) | 2026-11-18 | `bfbc2core.dbxedit`, `bfbc2.exe dbxedit`, `bfbc2-dbx-tools\dbxedit.py` (checked against the exe's recorded answers, `core\tests\dbxedit_golden*.json`) | `f10522d` | R4 (`1569d9f`) |
| `rust\bench_dbxedit.py`, `rust\bench_fbrbtool.py` (the exes against the in-process tools) | 2026-11-18 | Nothing: the exes they timed were retired (figures kept in `rust/BENCHMARKS.md`, "Retired C# tools") | `f10522d` | R4 (`1569d9f`) |
| **DbxTool** (`bfbc2-dbx-tools\DbxTool.cs`) | 2026-10-16 | DbxEdit's `xml` command (byte-identical XML on 43,290 files), rewritten from `FORMAT.md` | `f379a5f` | before rounds (`aac43b0`) |
