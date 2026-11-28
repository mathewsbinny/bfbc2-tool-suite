# DICE's gameplay placement (round L1, 2026-11-20)

How DICE laid out gameplay on the stock multiplayer levels, measured so the map editor can warn about unusual
layouts (round V1) and propose a mode's layout (round A1). Read-only research: nothing here changes the game or a
level.

- The report (the method, the tendencies with their numbers, the proposed warning rules, the open questions) is in
  battlefield-opfor's knowledge base since round KB2: `bfo:docs/research/suite/L1_placement_report.md`
  (`L1_placement_report.md` here points to it).
- `l1_distributions.md`: the full tables per mode and era (BFBC2, BFBC1, Vietnam; the WW2 ports separately).
- `l1_rules.json` and `l1_rules_check.md`: the 50 proposed rules (IDs `L1-CQ-*`, `L1-RU-*`, `L1-SD-*`, `L1-AL-*`,
  `L1-HT-*`) with thresholds and messages, each run against the stock levels (no stock level raises a warning).
- `l1_a1_params.json`: a typical layout per mode, the input for the auto-layout round.
- Round V1 checks the rules live in the map editor (`rust/bc2-levelwrite/src/placement.rs`, a Rust port of
  `l1_measure.py`'s measurements for them, equal on every stock level; `l1_rules.json` is built into
  `bc2_levelwrite::rules`, `l1_a1_params.json` into `bc2_levelwrite::autolayout`): warnings and notes in the Problems
  panel, each quoting the stock range (`spec/transforms/validation.md`, section 3).
- `scripts/`: the measurement and analysis scripts. They read the game copy's bundles in memory through the suite's
  readers (`OPFOR_ROOT` names the folder holding `game\`): run `l1_measure.py`, then `l1_analyze.py`, `l1_rules.py`
  and `l1_a1.py` (about 35 s). They write their results beside themselves in `scripts\`; copy them up here to change
  what the editor embeds ("Re-running", below).

The raw measurements (every level, mode and metric with the source objects' GUIDs and positions) are game-derived
data and are not in this repository; the scripts rebuild them in `l1_measurements.json`.

Measured: 70 levels (Conquest, Rush, Squad Rush and Squad Deathmatch for BFBC2, BFBC1 and Vietnam; the three WW2
Conquest ports; the five Onslaught ports described only). Uncertain readings are marked in the report.

## Re-running

Run with the suite's interpreter (the one `bin\python.txt` names) from `scripts\`, in this order. Each step reads
the game copy only. The scripts write their results beside themselves in `scripts\` (`l1_analyze.py` and
`l1_rules.py` take an output path, `l1_a1.py` always writes there): copy the results up into this folder to update
the committed ones, which the Rust embeds (`l1_rules.json` in `bc2_levelwrite::rules`, `l1_a1_params.json` in
`bc2_levelwrite::autolayout`).

1. `l1_measure.py [out.json] [levels...]`: the measurements (about 35 s for all 70 levels; under 1 GB).
2. `l1_analyze.py`: `l1_distributions.md`.
3. `l1_rules.py`: `l1_rules_check.md` and `l1_rules.json`.
4. `l1_a1.py`: `l1_a1_params.json`.

The modules they use:

- `l1_read.py`: bundle and partition reader.
- `l1_level.py`: instances, fields, events, outlines.
- `l1_geom.py`: polygons, raster areas, margins, height field.
- `l1_levels.py`: level list from `maps.ini`.

The checks behind the report's section 3 (its method) are kept as scripts:

| Script | What it checks |
|---|---|
| `l1_scan.py` | event names, vehicle kinds and capture-zone kinds over every level |
| `l1_explore.py` | a Rush level's stage event graph |
| `l1_cps.py` | capture points, spawners and areas |
| `l1_dmg.py` | damage and area triggers |
| `l1_try_ca.py` | combat-area outlines |
| `l1_try_hf.py` | height-field lookup |
| `l1_anchor.py` | spawner versus alternatives |
| `l1_aabb.py` | AABB frames |
| `l1_sdmcheck.py` | Squad Deathmatch spawns outside the area |
| `l1_rushcheck.py` | stages, bases and areas per Rush level |
| `l1_dump.py`, `l1_show.py` | printing helpers |
