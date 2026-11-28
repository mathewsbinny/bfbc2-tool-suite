# Onslaught: the Xbox 360 co-op mode on the PC

Onslaught is BFBC2's console-only co-operative mode (the Xbox 360 / PS3 DLC of 2010): a squad takes a chain of flags
against waves of bots, against the clock. The PC never had it. Project OPFOR BFBC2 plays it offline through two kinds
of port: **Napisal's** PC ports (a third party's modpack; they run Onslaught as Rush) and **OPFOR's console ports**
(OPFOR's own, from the Xbox data, in 14 stages: XCONS-STG-1). This file is what the mode is in DICE's data, what the
PC engine does with it, and what each port keeps and loses. Written by round SPG2 (2026-11-22) from OPFOR's console
research, the ports' code and a read of the installed levels; the port stages themselves are
[transforms/console.md](../transforms/console.md), the night recipe LIGHT-PT-3 / LIGHT-PT-4 and LIGHT-SKY-1 - 3
([lighting.md](lighting.md)), the ambience areas WTH-FX-2. The user's goal (2026-11-19): a faithful Xbox Onslaught port
that needs none of Napisal's work.

## 1. The mode in DICE's data

### ONS-LVL-1: Four levels on four multiplayer maps

Status: **observed** · Variants: Xbox 360

The Xbox title updates hold four Onslaught levels, each with infantry and vehicle path data: `ons_mp_002` (Valparaiso,
on MP_002's map), `ons_mp_004` (Isla Inocentes' map, MP_004), `ons_mp_005` (Atacama, MP_005), `ons_mp_008` (Nelson
Bay, MP_008). OPFOR's console port of ONS_MP_002 has 5 `CapturePointEntityData`, one `MissionEntityData` and one
`SpawnPointManagerData`, and no M-COM; the research notes give 4 flags each on the other three. Napisal's Atacama is a
fifth PC level, `ons_mp_005gr`, on SP_005's terrain.

[doc: bfo:docs/research/opfor/BFBC2_CONSOLE_RESEARCH.md:144-153]
[code: opfor:src/tools/onslaught/console_common.py:20-23]
[data: round SPG2, 2026-11-22: the game copy's Package ONS_MP_002 level partition: 5 CapturePointEntityData, 1 MissionEntityData, 1 SpawnPointManagerData, 1 reference to gamemodes/onslaught]

### ONS-MODE-1: Onslaught is flags taken in sequence; the mode is data, its name read only by the UI

Status: **observed** · Variants: Xbox 360, PC

Onslaught has no M-COMs: 4 - 5 capture points taken in turn under one mission entity, with a spawn-point manager. A
game mode is data only (`GameModeData`: `ModeID`, HUD layers, the in-game bundle list). The PC executable names
"goldrush" (2 references) and "conquest" (9), never "onslaught", and all 11 are UI code comparing the current mode's
id (the mode owner 0x92AB10, +0x6C) through the string compare 0x4304D0; server gameplay never checks the name.

[exe: 0x4304D0 the string compare; 0x142ABD8 the "conquest" literal; 0x92AB10 the mode owner, its id at +0x6C]
[doc: opfor:DEVELOPMENT_NOTES.md:1279-1285]
[code: opfor:src/tools/onslaught/console_mode.py:9-22]

### ONS-MODE-2: OPFOR's GameModes/Onslaught is PC Conquest re-identified, matchmaking CONQUEST

Status: **implemented** · Variants: PC

OPFOR writes `GameModes/Onslaught.dbx` into the level's Package bundle: the PC `gamemodes/conquest` (mp_common)
under the console mode's GUIDs (so the console level's reference resolves unchanged), `ModeID` `onslaught`,
`MatchmakingQueryID` `CONQUEST`, the description `MP_GAMEMODE_ONSLAUGHT_NAME`. CONQUEST because the session code
(0x5680C0) maps the matchmaking id to a stats category (RUSH / CONQUEST 0x20, SQDM 0x10, SQRUSH 8, else -1): with
ONSLAUGHT no score, kills or round-end stats were kept. (`DEVELOPMENT_NOTES.md` still says ONSLAUGHT.)

[code: rust/bc2-install/src/onslaught/mode.rs:35-43]
[code: rust/bc2-install/src/onslaught/mode.rs:331-353]
[exe: 0x5680C0 the matchmaking id to stats category]

### ONS-MODE-3: The UI reads "onslaught" and "airsuperiority" as "conquest"

Status: **implemented** · Variants: PC

OPFOR's module hooks the string compare 0x4304D0: compared with the "conquest" literal at 0x142ABD8 it also answers
true for "onslaught" and "airsuperiority", which gives Onslaught the flag HUD and Conquest's loading text (MOD-FIX-1).

[code: opfor:src/dll/levelfix.cpp:57-84]
[code: opfor:src/dll/levelfix.cpp:848-853]

## 2. What the PC engine does with it

### ONS-CAP-1: On the PC, Onslaught flags need OnlyTakeableByTeam 0

Status: **observed** · Variants: PC

The console's flags set `OnlyTakeableByTeam` 1 (the defenders cannot retake them); every PC Conquest flag sets 0. In a
playtest (2026-10-29) with 1 the attackers lowered flag A to neutral but could not raise their own; with 2 they could
not take it either; with 0 they could. OPFOR writes 0 on every Onslaught flag, so the console's "defenders cannot
retake" is lost until the module handles it. The field's reader is the capture point's update (round SPG2-rest2:
ONS-CAP-3, GAME-CQ-10), which explains all three results.

[doc: opfor:DEVELOPMENT_NOTES.md:334]
[data: OPFOR playtest 2026-10-29 on a console-ported level: flag A with OnlyTakeableByTeam 1, 2 and 0, as described]
[code: rust/bc2-install/src/onslaught/mode.rs:31-32]

### ONS-CAP-3: OnlyTakeableByTeam T freezes a point whose progress belongs to team T

Status: **observed** · Variants: PC

`OnlyTakeableByTeam` is `CapturePointEntityData` +0x84 (an i32 between `ReturnMultiplier` +0x80 and
`MinNrToTakeControl` +0x88). Its one reader is the server capture point's update 0x726B60: the update runs only while
the point is enabled (+0xAD), `UnableToChangeTeam` (data +0x13A) is clear, and `OnlyTakeableByTeam` is 0 **or differs
from the team the progress belongs to** (+0x7C, BOT-GS-3); otherwise the point is left as it is for that frame. The
progress's team is not the owner: when a point is lowered to neutral, 0x711AA0 sets +0x7C to the team that lowered it,
and that team then raises it (GAME-CQ-10). So with T set:

| The point | What happens |
|---|---|
| held by T (+0x7C = T) | frozen: nobody can lower it (the "defenders cannot retake" reading, from T's side) |
| held by the other team | lowered as usual; at neutral +0x7C becomes the lowering team: if that is T, frozen at neutral |

The 2026-10-29 playtest is this: with 1 the attackers (team 1) lowered flag A (held by team 2) to neutral and could not
raise it (their own progress, +0x7C = 1, froze it); with 2 the flag, held by team 2, could not be touched; with 0 the
test never skips. The console's "defenders cannot retake" (the attackers' flags locked once raised) is not what the
PC's test gives, because the PC freezes T's progress already at neutral. To restore it the module would replace the
test with "skip when +0x7C = T and the point is captured (+0xAC set)"; inferred, untested.

[exe: BFBC2Game.exe 0x726B60 (`data +0x84 == 0 || data +0x84 != this +0x7C`, after the +0xAD and data +0x13A tests; data at this +0xC); 0x711AA0 (+0x7C set to its argument at progress 0); field descriptor 0x1C25870 (OnlyTakeableByTeam, offset 0x84, the i32 type 0x1BF03B4); the data class's getters 0x6F5740 - 0x6F58D0 (0x6F5760 reads +0x84) are called and pointed to nowhere: the reads are inlined]
[data: OPFOR playtest 2026-10-29: flag A with OnlyTakeableByTeam 1, 2 and 0 (ONS-CAP-1)]

### ONS-CAP-2: Door pre-destructions and the console's explosive objects are left out

Status: **implemented** · Variants: PC

Console pre-destruction spheres reaching within their radius + 1.5 m of a placed door are dropped (on the PC they made
walls see-through and doors unusable). The console's explosive objects are not restored: with them Nelson Bay crashed at
load at 0x56C58A, under the AI sphere query (0x577390 -> 0x574780 -> 0x56C4E0), though SP_01 places 20
`ExplosiveObjectData` on the PC without trouble.

[code: rust/bc2-install/src/onslaught/mode.rs:9-15]
[code: rust/bc2-install/src/onslaught/mode.rs:123-170]
[exe: 0x56C58A the fault; 0x577390 the AI sphere query]

### ONS-END-1: A round ends at the last flag; the stopwatch is OPFOR's

Status: **implemented** · Variants: PC

Console Onslaught times each run. Its HUD's timer is fed by two events the PC executable never sends
(`coopRoundTime(fullTime "HH:MM:SS:t", isRunning)`, `coopOldBestRoundTime(best)`); OPFOR's module sends them while
the mode id is "onslaught" (starting 1 s after the cursor is hidden, once a second, stopping when the local player's
stats are committed) and keeps best times in `game\onslaught_times.ini`. On the PC a round ends only when the last flag
is taken: the console's whole-squad-dead loss is absent (`LoseOnWholeTeamDead`, `AutobalanceTeams` are console-only
fields the port strips).

[code: opfor:src/dll/onslaught_hud.cpp:3-15]
[code: opfor:src/dll/onslaught_hud.cpp:106-147]
[code: opfor:src/dll/onslaught_hud_format.h:8-16]
[code: opfor:src/tools/onslaught/console_hud.py:8-25]
[test: rust/bc2-install/src/onslaught/hud.rs::never_patches_twice]
[test: opfor:src/tools/tests/test_onslaught.py::test_hud_passes_each_bundles_source_and_writes_the_records_after_the_install]

### ONS-WAVE-1: Waves are spawners switched on by events

Status: **observed** · Variants: PC

Most bot spawners start disabled and the level's events enable them stage by stage (Napisal's ONS_MP_005GR: 81
Team2 AI spawners, 60 disabled at start, `SpawnDelay` 36 - 45 s, the attackers' `StartingLifeCount` 400); orders are
few (patrols, one search-and-destroy). A template or kit fix must cover every spawner, enabled at start or not. The
levels override `AI/Difficulty.dbx` (profiles `Ons_Casual`, `Ons_Normal`, `Ons_Hard`, `Ons_Hardcore`) and
`AI/Settings.dbx` (multiplayer-scaled: `LoiterActivityDistance` 10 -> 280, `MaxMobileVehicles` 16 -> 32).

[doc: opfor:research/BFBC2_NAPISAL_RESEARCH.md:106-144]
[doc: bfo:docs/procedures/ONSLAUGHT_PROCEDURE.md:96-99]
[data: Napisal's ONS_MP_005GR (OPFOR's research 2026-10-29): the spawner counts listed]

## 3. The ports

### ONS-PORT-1: Napisal's ports run as Rush, without the sector offsets

Status: **observed** · Variants: PC

Napisal's ONS_MP_002, ONS_MP_004 and ONS_MP_005GR use `gamemodes/goldrush` (Rush, attackers Team1) with PC Rush
M-COMs, not the console's flags. They carry DICE's console path data byte for byte but not its
`Precomputed/<Kind>/SectorOffsets` assets, so the runtime uses 0, 0 and route requests from some sectors fail
(ONS_MP_002: sector 0x30a).

[doc: opfor:research/BFBC2_NAPISAL_RESEARCH.md:96-97]
[doc: opfor:research/BFBC2_NAPISAL_RESEARCH.md:196-210]
[data: round SPG2, 2026-11-22: Napisal's backed-up Package ONS_MP_002 level partition: 4,320 instances, 1 reference to gamemodes/goldrush]

### ONS-PORT-2: Sector offsets are the most common per-sector value

Status: **implemented** · Variants: PC

The sector offsets of a path-data kind are the most common value over its sectors of `floor((xmin + xmax) / 200) -
(id >> 8)` (x) and `floor((zmin + zmax) / 200) - (id & 0xFF)` (z); this reproduces all 27 `SectorOffsets` assets DICE
shipped on the PC (the extent rule `floor(min / 100) - 1` fails on 7 of 13 vehicle files). Console values: ons_mp_002
-3, -7; ons_mp_005 -9, -4; ons_mp_008 -3, -5 (vehicle -2, -5). The executable builds the name
`<level>/Precomputed/<Kind>/SectorOffsets` (0x14A93D0); DICE writes it lower case and it still resolves.

[code: opfor:src/tools/onslaught/apply_sector_offsets.py:78-87]
[doc: bfo:docs/procedures/ONSLAUGHT_PROCEDURE.md:148-153]
[exe: 0x14A93D0 the SectorOffsets path format]

### ONS-PORT-3: What the console port drops: the night lamps, the moon, the lighthouse, the ambience

Status: **observed** · Variants: PC

The console port's first stage drops every non-world `ReferenceObjectData`, `GameSoundEntityData` and
`GameEffectEntityData` whose partition the PC lacks and whose console copy has console-only resources (or refers on to
such), then nulls every reference left that resolves neither on the PC nor in the copied console closure. On
ONS_MP_002 that removed DICE's 53 night lamp effects (the DLC re-save of the SP_02 light: LIGHT-PT-3), MP_008's moon
and SP_006's `lighthousenoentry`, and nulled the night ambience of its 10 sound areas; the weather is re-pointed to the
PC map's own. The installed level: 0 lamp effects, 2 `GameEffectEntityData`, no moon, every sound area's `Sound`
null. (WX1 attributed this level to Napisal; it is OPFOR's console port.) A faithful port must convert those
resources instead of dropping them.

[code: rust/bc2-install/src/onslaught.rs:704-736]
[code: rust/bc2-install/src/onslaught.rs:820-899]
[data: round SPG2, 2026-11-22: the game copy's Package ONS_MP_002 level partition (2,861 instances, OPFOR's `.opfor-*-applied` stage markers beside it): the counts listed; Napisal's backed-up ONS_MP_002: 53 lamp-effect references, 143 GameEffectEntityData, 10 sound areas with sounds]

### ONS-PORT-4: The console port rebuilds on the Linux server's level

Status: **implemented** · Variants: PC

The PC client's multiplayer maps lack the static-group physics parts the AI's set-up looks up (0x8F4720); the official
Linux dedicated server's levels have them. The port keeps the Linux level's world (its world types and everything they
reference) and grafts every non-world console instance onto it.

[code: opfor:src/tools/onslaught/console_linux_base.py:9-20]
[code: rust/bc2-install/src/onslaught/linux.rs:27-48]
[doc: opfor:research/BFBC2_NAPISAL_RESEARCH.md:268-271]

### ONS-REG-1: Registration, and what the game copy lists

Status: **observed** · Variants: PC

An Onslaught level needs its `package.mft` blocks and `bundleManifest` lines and its description in Package
`async/startup-00`; without the description the load crashes in state 13 (`_stricmp` in the name table, 0xBD2D30).
The two ports load different commons (Napisal's ONS_MP_002 `nam_common`; the console port package, game, mp_common,
sp_common). OPFOR's `maps.ini` offers Onslaught on MP_002, MP_004, MP_005 and MP_008 (the console ports); Napisal's
ONS_MP_005GR is still installed and registered.

[doc: opfor:research/BFBC2_NAPISAL_RESEARCH.md:173-192]
[data: round SPG2, 2026-11-22: the game copy's Dist package.mft (the ons_mp_002 level block, the ons_mp_005gr blocks), bundleManifest and maps.ini Onslaught lines]
[exe: 0xBD2D30 the level name table compare]

### ONS-NAP-1: What still depends on Napisal

Status: **implemented** · Variants: PC

`import_onslaught.bat` copies Napisal's ons_mp_002, ons_mp_004 and ons_mp_005gr (Dist, Package, path data), merges
their registration, adds their descriptions, fills ONS_MP_004's 10 sp_005 references with a small
`opfor_ons_mp_004` bundle, then applies the sector offsets and the AI template (Napisal's `DefaultSoldier` template is
a placeholder: "FAIL No Template", no field of view, so bots aim and never fire; every spawner gets
`spawntemplate_enemy_grunt`). It refuses a console-ported level unless forced. `console_port.py` refuses to start
without `work\backups\napisal_ons_*`. ONS_MP_005GR exists only as Napisal's.

[code: opfor:import_onslaught.bat:42-103]
[code: opfor:src/tools/onslaught/console_port.py:75-76]
[code: opfor:src/tools/onslaught/apply_ai_template.py:14-16]
[test: opfor:src/tools/tests/test_onslaught_qa.py::test_ai_template_plan_follows_the_bundle]

## Open questions

- Restoring "defenders cannot retake": ONS-CAP-3's patch (test the captured flag too) is inferred; a module patch and
  a playtest.
- The 0x56C58A crash with the console's explosive objects (ONS-CAP-2).
- Whether DICE's night state survives in the console port's weather (it uses MP_002's PC weather system).
- The flag counts of the installed ONS_MP_004 / 005 / 008 ports (an unpack each).
- No console port has been played end to end; no second stage reached with the bot fixes.
- The DLC's full content list against each port (which converted resources each stage drops), for the faithful re-port.
