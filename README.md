# BFBC2 Tool Suite

Tools for building and debugging AI on Battlefield: Bad Company 2 maps, in one repository. The 3D tools
share one core: level loading, a 3D view, picking and the game's data formats.

**Ready to use**

| Folder | Tool |
|---|---|
| `bfbc2-auto-navkit/` | **BC2 NavKit**: generates AI navigation data (`pathdata.hpd`) for levels. Start with `QUICKSTART.md`, or `navkitGUI.pyw`. |
| `bfbc2-dbx-tools/` | **DbxEdit** and **DbxEditGUI**: view, edit, check and repair binary `.dbx` files; XML export and import |
| `bfbc2-fbrb-tool/` | **FbrbTool**: unpack and repack `.fbrb` bundles |
| `bfbc2-console-readers/` | read-only readers for Xbox 360 disc images and content packages |
| `bfbc2-diff-workbench/level-diff/` | **LevelDiff**: what differs between two `.dbx` files, instance by instance |
| `bfbc2-automated-map-and-bot-test-harness/crash-reader/` | **crash reader**: the game's `crashreport.xml` turned into names, code, null pointers and hints |
| `core/` | the shared core: Python readers for `.dbx`, terrain height fields and water; the level loader (entities, teams, collision, areas and routes); the 3D view with picking |
| `bfbc2-map-inspector/` | **Map Inspector**: browse a level in 3D and inspect every object's source, properties and links; measure |
| `bfbc2-navigation-studio/` | **Navigation Studio** (viewing): NavKit's and DICE's navigation data in 3D, by island, sector, pitch or difference; islands and spawns off the main network |

**Planned** (see [DESIGN.md](DESIGN.md))

| Folder | Tool | Milestone |
|---|---|---|
| `bfbc2-navigation-studio/` | editing, the path test and side files for the AI's navigation data | M4 |
| `bfbc2-automated-map-and-bot-test-harness/` | repeatable map and bot tests, results compared between builds | M5, then growing |
| `bfbc2-bot-ai-debugger/` | live view of the AI while the game runs (through Project OPFOR) | M6 |
| `bfbc2-tactical-map-authoring-tool/` | tactical annotations above the navigation data for bots | M7 |
| `bfbc2-diff-workbench/` | ImHex patterns for BC2 formats, format coverage and structure-aware diffs | alongside |
| `bfbc2-ai-behavior-simulator/` | runs OPFOR's bot decision code on simplified maps, without the game | with Goal 3 |

The asset builder & validator, `bfbc2-asset-builder`, is part of the Project OPFOR repository (DESIGN.md
section 4.9).

## The 3D tools and your GPU

The Map Inspector and the Navigation Studio draw with OpenGL 3.3. Each prints the GPU it uses when it starts
(and shows it in its status line). On a laptop with two GPUs, Windows may give Python the slower integrated
one: set `python.exe` to "High performance" in Settings > System > Display > Graphics. On the development
PC that took the Navigation Studio on SP_01 (10.4 million triangles) from the AMD Radeon integrated GPU to
the GeForce RTX 4060, at 144 frames a second.

## Tests

```
python run_tests.py quick      before every commit: no game data, about a minute
python run_tests.py game       before every push: uses Project OPFOR's game copy, about 10 minutes
python run_tests.py full       after changing a format reader: every unpacked file, about 30 minutes
python run_tests.py --list     what each tier runs;   --only <name> runs the tests whose name contains it
```

| Test | Tier | Checks |
|---|---|---|
| `tests/test_repo.py` | quick | no game files or large files in the history, no co-author lines, no build output tracked, READMEs, LICENSE, every Python file compiles |
| DbxEdit unit tests | quick | 93 checks of the format, XML export and import, edits, level check, cull-slot fixer, command line, window |
| `core/tests/test_fbrb.py` | quick, game, full | unpack + pack gives the same bundle (built here; 12 game bundles; all 1,055), `*deleted*` markers handled |
| `bfbc2-diff-workbench/level-diff/tests/test_leveldiff.py` | quick, game | two levels built in the test with known differences (changed field, re-created object, list entries added / removed / reordered, instances added / removed); `--match-objects`, `--ignore`, `--type`, `--summary`, JSON, exit codes; MP_005 against itself and Dist against Package |
| `bfbc2-automated-map-and-bot-test-harness/crash-reader/tests/test_crashreader.py` | quick, game | crash reports built in the game's layout: registers, stack, symbols and hints, null registers, build note, log lines; with the exe: function start, null operand, call before a return address, text on the stack |
| `bfbc2-console-readers/tests/test_console_readers.py` | quick, game | XDVDFS images (two partition offsets, nested and empty folders) and STFS packages (both hash-table layouts, a fragmented file); with your Xbox 360 files: the disc's 693 files, Onslaught path data from Update #2 and #3 identical and identical to Napisal's PC files |
| `core/tests/test_water.py` | game | every level's water read exactly, indices in range, a shader per triangle |
| `core/tests/test_level.py` | game | the loader's counts per category, team, shape, collision, terrain and water against `level_snapshot.json` (`--update` after an intended change) |
| `core/tests/test_terrain.py` | game | heights within 0.1 m of the in-game terrain samples (13 levels) |
| `hpdtool roundtrip / meshtrip` | game | every path-data file and navigation mesh written back byte for byte |
| `core/tests/test_navdata.py` | game | path data as the studio loads it (islands, cache, a file against itself) and the path test (routes, costs from the stored edges, refusals between islands) |
| `core/tests/test_navtune.py` | quick | navigation side files: every operation saves and loads unchanged, undo / redo, the PC's name recorded, invalid edits refused, DICE bases refuse region edits, the base file's hash |
| `bfbc2-navigation-studio/tests/test_editing.py` | game | the studio's edit mode driven as clicks would drive it: every tool, outline and box, undo / redo, save and reload, the click mode, DICE-base limits |
| `tests/test_viewers.py` | game | the Map Inspector and the Navigation Studio open a level, draw it (screenshot not blank) and exit; prints the GPU used |
| `bfbc2-auto-navkit/tools/navregress.py` | game (2 levels), full (13) | NavKit rebuilds levels with the current code; fails if any DICE-parity measure is worse than the reference run `regression` |
| DbxEdit corpus, `core/tests/test_dbx.py` | full | every unpacked `.dbx`: DbxEdit's fingerprints, and the Python reader's XML identical to DbxEdit's |

Each tool's folder has its own README. The tools that existed before the suite were merged in with their
full history (2026-10-16), so `git log -- <folder>/<file>` shows each file's history.

## Licence

MIT (see `LICENSE`) for all code and documentation in this repository. The licence covers only this
repository's own work: no Electronic Arts / DICE game data, code or other content is included, and the
tools need your own copy of the game. Third-party libraries keep their own licences (moderngl, imgui-bundle:
MIT; glfw: zlib; numpy: BSD). Not affiliated with or endorsed by Electronic Arts or DICE; Battlefield and
Battlefield: Bad Company are trademarks of Electronic Arts Inc.

Developed for Project OPFOR. Game data is read from your own installation and never distributed.
Battlefield: Bad Company 2 is © Electronic Arts / DICE.
