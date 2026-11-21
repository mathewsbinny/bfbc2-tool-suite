# DICE's gameplay placement (round L1, 2026-11-20)

How DICE laid out gameplay on the stock multiplayer levels, measured so the map editor can warn about unusual
layouts (round V1) and propose a mode's layout (round A1). Read-only research: nothing here changes the game or a
level.

- `L1_placement_report.md`: the method, the tendencies with their numbers, and the proposed warning rules.
- `l1_distributions.md`: the full tables per mode and era (BFBC2, BFBC1, Vietnam; the WW2 ports separately).
- `l1_rules.json` and `l1_rules_check.md`: the 49 proposed rules (IDs `L1-CQ-*`, `L1-RU-*`, `L1-SD-*`, `L1-AL-*`,
  `L1-HT-*`) with thresholds and messages, each run against the stock levels (no stock level raises a warning).
- `l1_a1_params.json`: a typical layout per mode, the input for the auto-layout round.
- `scripts/`: the measurement and analysis scripts. They read the game copy's bundles in memory through the suite's
  readers (`OPFOR_ROOT` names the folder holding `game\`): run `l1_measure.py`, then `l1_analyze.py`, `l1_rules.py`
  and `l1_a1.py` (about 35 s).

The raw measurements (every level, mode and metric with the source objects' GUIDs and positions) are game-derived
data and are not in this repository; the scripts rebuild them in `l1_measurements.json`.

Measured: 70 levels (Conquest, Rush, Squad Rush and Squad Deathmatch for BFBC2, BFBC1 and Vietnam; the three WW2
Conquest ports; the five Onslaught ports described only). Uncertain readings are marked in the report.
