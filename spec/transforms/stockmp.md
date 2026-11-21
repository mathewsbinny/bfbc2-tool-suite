# stockmp: an AI layer for a stock multiplayer level

`bfbc2.exe stockmp` (its library `bfbc2-stockmp-ai/stockmplib.py`, its core in `rust/bc2-stockmp`; the Python command line `stockmp.py` was retired in round RT3) gives a stock multiplayer
level, which ships with no AI, the data bots need: `generate` proposes a configuration from the level's own
objectives and spawns (`config.json`, edited by hand); `build` authors the layer into the level partition, packs the
bundles and stages them with the path data under `work\stockmp\<lv>\out\game\` (a mirror of the game folder);
`validate` runs the offline checks; `install --plan | --apply | --revert` copies the stage in and back. Plan:
Project OPFOR's `STOCK_MP_AI_PLAN.md`, option c. What bots need at run time is layer 3's (`engine/`).

## 1. Generate and author

### XSMP-CFG-1: The configuration is checked before a build

Status: **verified**

A configuration is refused, with what to edit, when an order lists soldiers of the other team or a kit is unknown;
the generated one passes (Conquest: one order per team and flag; Rush: one per team and M-COM stage).

[code: bfbc2-stockmp-ai/config.py:83] [code: bfbc2-stockmp-ai/config.py:225] [test: bfbc2-stockmp-ai/tests/test_stockmp.py::quick]

### XSMP-AUT-1: What the layer adds to the level partition

Status: **verified**

The authored partition gains: `HasAI` true and `PathfindingTypes` from `mp_common`'s infantry and vehicle settings;
AI spawners cloned from the level's own spawner (`SSAiPlayer`, the AI template, a faction kit) on the team base's
ground spawn points; an area order per team and objective issued on spawn, on player enter and by an area trigger;
for Rush, each stage's trigger disabling and killing its AI and enabling the next stage's; every new entity in
`Objects` once; the stock `SSAiPlayer` spawners made `SSHumanPlayer` (unless kept); and the
`Precomputed/<Infantry|Vehicle>/SectorOffsets` partitions from the staged path data's own sector ids (HPD-USE-2).
Existing instances change only where intended.

[code: bfbc2-stockmp-ai/author.py:207-260] [test: bfbc2-stockmp-ai/tests/test_stockmp.py::quick] [test: bfbc2-stockmp-ai/tests/test_validate.py::main]

### XSMP-AUT-2: Deterministic guids; a second layer is refused

Status: **verified**

New instances get UUID v5 guids of (`opfor:stockmp`, level, kind, id), so the same configuration gives the same
bytes; two builds give identical bundles and staged path data. Authoring a level that already has a stock-MP AI layer
is refused and changes nothing.

[code: rust/bc2-formats/src/guid.rs:1-31] [test: rust/bc2-formats/src/guid.rs::python_uuid5]
[test: bfbc2-stockmp-ai/tests/test_stockmp.py::quick] [test: bfbc2-stockmp-ai/tests/test_stockmp.py::game]

### XSMP-PAR-1: The Rust core answers as the Python

Status: **verified**

The level model, authoring and validation readings in `bc2-stockmp` give the Python's results on synthetic levels
and on every multiplayer level partition of the game copy, or leave the input to the Python; MP_005, MP_002
and MP_008CQ build byte for byte as the Python builds them.

[test: bfbc2-stockmp-ai/tests/test_stockmp.py::native_parity] [test: bfbc2-stockmp-ai/tests/test_native_levels.py::main]

## 2. Validate

### XSMP-VAL-1: The offline checks

Status: **verified**

A build passes validation when: DbxEdit's `check` finds no new error or warning; `roundtrip` is identical and XML
export / import gives the file back; references, `Objects` and event connections are consistent; only the intended
instances changed; new external references resolve; `SectorOffsets` are integers equal to the staged path data's;
the packed bundles unpack to the built files; every order has a route node near it, every AI spawner stands on the
navigation data and reaches its objectives; Rush stages start disabled with their switch sources present. Each check
fails on a broken synthetic build made to break it.

[code: bfbc2-stockmp-ai/validate.py:1-40] [test: bfbc2-stockmp-ai/tests/test_validate.py::main] [test: bfbc2-stockmp-ai/tests/test_stockmp.py::game]

## 3. Install

### XSMP-INS-1: Install only a complete build, as one journalled commit

Status: **verified**

`install --apply` refuses when nothing is built or the staged folder is not the one build its report lists. The
build's files go into the game copy as one journalled commit of `stockmp:<lv>` in the install ledger (XINST-GL-1):
every check comes first (a file changed outside the ledger since an install refuses the whole commit); what each file
replaced is kept once by its sha256; a failure part way (a locked file, a full disk) or a kill puts every file back,
so Dist, Package and the path data are never from different builds (audit AU4 DS-5). `--plan` changes nothing. stock-MP's
markers from before the ledger, found only in the level's own folders (audit AU4 DS-9), are adopted first; inconsistent
ones (a changed file, an interrupted commit, a backup without its record) are refused in words.

[code: bfbc2-stockmp-ai/stockmplib.py:102-127] [code: rust/bc2-cli/src/stockmp/mod.rs:309-436]
[test: bfbc2-stockmp-ai/tests/test_stockmp.py::quick] [test: tests/test_deep_scan_fixes.py::stockmp_rollback]
[test: core/tests/test_gameledger.py::cross]

### XSMP-INS-2: Revert gives back what was there before stock-MP

Status: **verified**

`install --revert` takes `stockmp:<lv>` out of the ledger from its record (not from the build output): each file's
prior back, or a file the install added removed; under another tool's later independent install stock-MP's layer is
taken out from under it. A file changed outside the ledger is not reverted, and nothing is. An interrupted revert is
rolled back by the next command, which can then run it again (audit AU4 DS-6). A file the install added is never taken for
a stock file afterwards.

[code: bfbc2-stockmp-ai/stockmplib.py:130-195] [code: rust/bc2-cli/src/stockmp/mod.rs:309-436]
[test: bfbc2-stockmp-ai/tests/test_stockmp.py::quick] [test: core/tests/test_gameledger.py::cross]

## Open questions

- In-game proof: the layer has not yet been played (README "Status"); whether bots spawn, path and fight on a stock
  MP level with it is layer 3's question.
- Moving the AI layer into the map editor's AI workspace (EDITOR_ARCHITECTURE 3.4a, E5a).
