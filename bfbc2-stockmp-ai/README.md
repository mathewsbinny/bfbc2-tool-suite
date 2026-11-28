# bfbc2-stockmp-ai

Gives a stock Bad Company 2 multiplayer level - which ships with no AI layer - the AI spawners, AI orders and event
wiring bots need, as level data the game loads like its own. It is option c of Project OPFOR BFBC2's
battlefield-opfor's `docs/plans/opfor/STOCK_MP_AI_PLAN.md`: the tool proposes the layer from the level's own objectives and spawns, a person tidies the
per-map configuration, and the tool builds and checks it offline.

Nothing EA-owned is in this folder. The tool reads the user's own game copy and writes only under
`<OPFOR>\work\stockmp\<level>\`; an install into the game is a separate, explicit step.

## Use

```
bfbc2 stockmp summary  mp_005                     what the tool reads: mode, teams, flags, Rush stages
bfbc2 stockmp generate mp_005 [--bots 8]          a first configuration: work\stockmp\mp_005\config.json
bfbc2 stockmp build    mp_005                     author, pack, stage:   work\stockmp\mp_005\out\game\...
bfbc2 stockmp validate mp_005                     the offline checks:    work\stockmp\mp_005\validation.json
bfbc2 stockmp all      mp_005                     the three in a row (keeps an existing config.json)
bfbc2 stockmp install  mp_005 [--apply | --revert]         plan (default), copy into the game, or undo
```

Options: `--pathdata <folder>` (a folder under the game's `Output\win32\ai\levels`; default the level's own, else the
DICE folder of the same world, e.g. mp_005 -> ons_mp_005gr), `--minimal` (experiment 0: HasAI, PathfindingTypes,
path data and SectorOffsets only, stock spawners untouched), `--keep-stock-spawners`, `--variant <name>` (a second
build beside the first: `work\stockmp\<level>-<name>`).

Edit `config.json` by hand and run `build` again; the build is deterministic (same configuration, same bytes).

Without Python: `bin\bfbc2.exe stockmp <command> ...` (the suite's `bin\`; rounds O3 / O5) takes the same arguments
and writes the same files, output and exit codes - `generate`, `build`, `validate` and `all` included (round O5:
`build` about 2.5 times faster; mp_005, mp_002 and mp_008cq built byte for byte as the Python builds them). A
hand-written configuration shape the port does not read is handed to the Python (the suite's README, "bfbc2.exe").
Project OPFOR runs it through `bfbc2core.native.cli_command('stockmp')`.

## What a build contains

`out\game\` mirrors the game folder, so an install is a copy with backups:

| file | content |
|---|---|
| `Dist\win32\levels\<lv>\level-00.fbrb`, `Package\levels\<lv>\level-00.fbrb` | the level partition with the AI layer; `Levels/<LV>/Precomputed/Infantry|Vehicle/SectorOffsets` (from the path data's own sector ids); the AI template chain (SpawnTemplate_Enemy_Grunt, its Sense and Behaviour, from the user's sp_common) when the level does not load sp_common |
| `Output\win32\ai\levels\<lv>\infantry|vehicle\pathdata.hpd` | the path data named in the configuration |

The AI layer (author.py, with where each part was taken from):

- **LevelData:** `HasAI` true; `PathfindingTypes` = mp_common's infantry and vehicle PathfindingSettings (every level
  with working AI has them; stock MP levels have the array empty); every new entity in `Objects`; the new
  `EventConnections`.
- **AI spawners:** SoldierSpawnEntityData cloned from the level's own spawner; `PlayerType` SSAiPlayer; the AI template
  (default the campaign grunt - the fix that made Napisal's bots fire); a multiplayer kit of the team's faction, the
  four classes in turn; Napisal's respawning set-up (Amount 0, SpawnSoldierFromBeginning) with DICE's
  SpawnEnemyRadius 20. Placed on the ground spawn points of the team's base, checked against the path data.
- **Orders:** an AreaOrderEntityData per team and objective (AreaMoveTo on Conquest flags; on Rush, AreaHoldGround for
  the defenders and AreaMoveTo for the attackers at each M-COM), with its Soldiers and an AabbGeometry area. Issued by
  each soldier's OnSpawn, the human player's OnPlayerEnter, and an AreaTrigger at the objective (Napisal's pattern).
- **Rush stages:** read from the stock logic (the stage's MultipleTrigger fed by its M-COMs' onDestroyed); its OnTrigger
  disables and kills the stage's AI and enables the next stage's spawners and orders.
- **Stock spawners** marked SSAiPlayer (they are the human spawn points, listed by the capture points) become
  SSHumanPlayer, unless `--keep-stock-spawners`.

## Offline checks (validate.py)

DbxEdit `check` (no new error or warning, references resolved against the built bundle and mp_common); DbxEdit
`roundtrip` and XML export -> import; the suite's second DBX reader: references, Objects, event connections; only the
intended changes to existing instances; new references into other partitions resolve; SectorOffsets stored as
integers and equal to the staged path data's; the packed bundles unpack to the built files; every order has a route
node nearby, every AI spawner stands on the navigation data and is route-connected to its objectives; Rush stages
start disabled and their switch sources exist.

## Files

| file | what |
|---|---|
| `stockmplib.py` | the library of `bfbc2 stockmp` (summary, install; the command line `stockmp.py` was retired in round RT3) |
| `stockcore.py` | the Rust core's switch (round E4a): `native()`; `BFBC2_STOCKMP_PYTHON=1` runs the Python everywhere |
| `dbxbin.py` | binary DBX as an editable typed tree, written back byte for byte (8,216 game files checked); `round_trips` on all cores |
| `levelmodel.py` | the level as the tool sees it: capture points, spawners and ground points, M-COMs, Rush stages |
| `config.py` | the configuration format, defaults and the generator (Conquest, Rush) |
| `author.py` | inserts the layer into the level partition |
| `pipeline.py` | sources, SectorOffsets asset, template packing, bundle packing, staging |
| `validate.py` | the offline checks |
| `tests/test_stockmp.py` | quick: synthetic levels (and the Rust core against the Python on them); `--game`: Atacama Conquest built and validated twice |
| `tests/test_validate.py` | quick: every check on good and broken synthetic builds, each with the Rust core and without |
| `tests/test_native_levels.py` | game: the Rust core against the Python on every multiplayer level partition of the game copy |

Needs Python 3.12+ and `numpy`; the suite's Rust core (`python rust\build_native.py`; FbrbTool.exe and DbxEdit.exe,
the fallback, were retired 2026-11-18); NavKit's tools are imported for the path data (`hpdtool`, `ordercheck`).
No program is started: bundles are unpacked and packed in process (`bfbc2core.fbrb.unpack` / `pack_many`, the
build's variants packed side by side), DbxEdit's checks run in process (`bfbc2core.dbxedit`), and the packed
bundle is checked against the built files in memory (round E3j, `rust\BENCHMARKS.md`). Round E4a moved the tool's own
logic there too (`rust\bc2-stockmp`, on `bc2_formats::dbx::edit`, the port of `dbxbin`): the level model, the
authoring and the heavy readings of validation (structure, changes, references, stages, the partition index, the
packed bundle), with no element tree built in Python; DbxEdit's read-only runs of every variant run at once. Every
piece answers as its Python or leaves the input to it (anything the Python raises or formats itself), so outputs and
messages are the Python's; Atacama's generate + build + validate 26 -> 16 s (`rust\BENCHMARKS.md`, "E4a"). Logs: `%LOCALAPPDATA%\bfbc2-tool-suite\logs\stockmp-*.log`.

## Status

Built and checked offline (2026-10-26): Atacama Conquest (MP_005, 16 bots, 6 orders) and its experiment-0 variant;
Valparaiso Rush (MP_002, 40 bots over 5 stages) as a test of the stage logic. Not yet run in the game: what the
engine does with this layer is open until a play test (battlefield-opfor's `docs/plans/opfor/STOCK_MP_AI_PLAN.md` section 12).
