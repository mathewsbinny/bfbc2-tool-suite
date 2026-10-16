# BFBC2 suite

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

**Planned** (see [DESIGN.md](DESIGN.md))

| Folder | Tool | Milestone |
|---|---|---|
| `core/` | the shared core: data layer, scene model, 3D view, camera, picking, panels; the asset pipeline | M1 |
| `bfbc2-map-inspector/` | browse a level in 3D and inspect every object's source and properties | M2 |
| `bfbc2-navigation-studio/` | view, edit and test the AI's navigation data (NavKit / Kynapse) | M3-M4 |
| `bfbc2-automated-map-and-bot-test-harness/` | repeatable map and bot tests, results compared between builds | M5, then growing |
| `bfbc2-bot-ai-debugger/` | live view of the AI while the game runs (through Project OPFOR) | M6 |
| `bfbc2-tactical-map-authoring-tool/` | tactical annotations above the navigation data for bots | M7 |
| `bfbc2-diff-workbench/` | ImHex patterns for BC2 formats, format coverage and structure-aware diffs | alongside |
| `bfbc2-ai-behavior-simulator/` | runs OPFOR's bot decision code on simplified maps, without the game | with Goal 3 |

The asset builder & validator, `bfbc2-asset-builder`, is part of the Project OPFOR repository (DESIGN.md
section 4.9).

Each tool's folder has its own README. The tools that existed before the suite were merged in with their
full history (2026-10-16), so `git log -- <folder>/<file>` shows each file's history.

Developed for Project OPFOR. Game data is read from your own installation and never distributed.
Battlefield: Bad Company 2 is © Electronic Arts / DICE.
