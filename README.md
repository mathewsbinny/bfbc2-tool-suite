# BFBC2 suite

Tools for building and debugging AI on Battlefield: Bad Company 2 maps. They share one core: level
loading, a 3D view, picking and the game's data formats.

| Folder | Tool | Status |
|---|---|---|
| `core/` | the shared core: data layer, scene model, 3D view, camera, picking, panels | planned (M1) |
| `bfbc2-map-inspector/` | browse a level in 3D and inspect every object's source and properties | planned (M2) |
| `bfbc2-navigation-studio/` | view, edit and test the AI's navigation data (NavKit / Kynapse) | planned (M3-M4) |
| `bfbc2-automated-map-and-bot-test-harness/` | repeatable map and bot tests, results compared between builds | planned (M5, then growing) |
| `bfbc2-bot-ai-debugger/` | live view of the AI while the game runs (through Project OPFOR) | planned (M6) |
| `bfbc2-tactical-map-authoring-tool/` | tactical annotations above the navigation data for bots | planned (M7) |
| `core/` (asset pipeline) | batch extraction into common formats, dependency reports | planned (with M1, M2) |
| `bfbc2-diff-workbench/` | ImHex patterns for BC2 formats, format coverage and structure-aware diffs | planned (alongside) |
| `bfbc2-ai-behavior-simulator/` | runs OPFOR's bot decision code on simplified maps, without the game | planned (with Goal 3) |

The asset builder & validator, `bfbc2-asset-builder`, is part of the Project OPFOR repository (DESIGN.md section 4.9).

The design is in [DESIGN.md](DESIGN.md). Developed for Project OPFOR. Game data is read from your own
installation and never distributed. Battlefield: Bad Company 2 is © Electronic Arts / DICE.
