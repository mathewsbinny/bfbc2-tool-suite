# Gameplay and game modes

What a multiplayer match is made of in BFBC2: a game mode is a small data partition plus a **mode layer** of
entities in the level partition (teams, ticket or life counters, capture points with their spawns, combat areas,
M-COMs and the event glue between them). This file states how the game reads that data (where the executable is
known), how DICE built the layers of the stock levels, and the layout conventions DICE followed, measured on every
stock multiplayer level by the map editor's placement study (round L1).

The layer's types in the level partition (instances, references, event connections) are layer 2
(`model/level.md`: LEVEL-MP-1, LEVEL-ENT-1, LEVEL-ENT-2, LEVEL-EVT-1); the description's `GameMode` and the crash when
it does not resolve are LEVEL-DESC-1 and LEVEL-DESC-2. Which material a hit or explosion damages is HAVOK-MAT-2. The
voice-over system itself (banks, the raise and match functions) is `engine/audio.md`'s; this file names only the
gameplay events that raise voice-over.

**Corpus.** "Stock" is DICE's level bundles in Project OPFOR BFBC2's game copy (their 2010 / 2012 install dates
unchanged). **The L1 study** (`bfbc2-map-editor/placement/`, 2026-11-20) measured 70 levels: the DICE group (BFBC2,
BFBC1 and Vietnam: 16 Conquest, 16 Rush, 14 Squad Rush, 15 Squad Deathmatch levels), OPFOR's three WW2 Conquest
ports (BF1943's design, kept apart) and OPFOR's five Onslaught ports (descriptive only). Distances are horizontal
(XZ) in metres. Counts like "16/16" are levels (or stages, flags) that hold the rule. Each convention rule names the
**L1 warning rules** that check it (`l1_rules.json`, checked against the stock data in `l1_rules_check.md`: none
warns on a stock level); `trace.py --check` refuses an L1 ID that is not in that file.

## 1. Game modes

### GAME-MODE-1: A mode is a GameModeData partition and the level's entities

Status: **observed**

A game mode is a `GameModeData` partition (`gamemodes/<mode>`): `ModeID`, `MatchmakingQueryID`, `Description` (loc
key), `HudRoot` / `HudRootName`, a primary and a secondary HUD logic layer (`hudlayer_mp_flagcapture` for Conquest,
`hudlayer_mp_goldrush` for Rush), `InGameMenu`, `Crosshairs`, `Loader` and the bundle references it adds to the
loader and in-game bundles. A level's description picks one by its `GameMode` reference, with its
`MatchmakingModID`, licences and kit pictures. The rules of play are not in the mode: they are the level's own
entities (GAME-LAYER-2). Vietnam's `GameModes/NamConquest` is `conquest` / `CONQUEST` with Vietnam's HUD root.

[data: stock gamemodes/conquest and gamemodes/namconquest partitions; the WW2 levels' descriptions (GameMode gamemodes/namconquest, MatchmakingModID PACIFIC)]
[doc: bfo:docs/research/opfor/AIR_SUPERIORITY_RE_2026-11-14.md:194-219]

### GAME-MODE-2: Each era's modes

Status: **verified**

The modes a set of levels has are its stock levels' description `GameMode`s: BFBC2 Conquest, Rush (`goldrush`),
Squad Rush, Squad Deathmatch (`teamdeathmatch`) and Onslaught; BFBC1 and Vietnam the first four; WW2 (OPFOR's
BF1943 ports) Conquest and Air Superiority. WW2 has no Rush.

[code: rust/bc2-levelwrite/src/profile.rs:385-397]
[test: rust/bc2-levelwrite/tests/export_e3d.rs::each_eras_modes_and_their_layers]
[data: stock levels per mode: BFBC2 Conquest 9, Rush 10, Squad Rush 7, Squad Deathmatch 8, Onslaught 4; BFBC1 2 each; Vietnam 5 each; WW2 Conquest 3, Air Superiority 1]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1152-1165]

### GAME-MODE-3: Mode keys and names

Status: **verified**

A mode's key in the descriptions and `maps.ini` differs from its name for two modes: Rush is `goldrush`, Squad
Deathmatch `teamdeathmatch`; Conquest, Squad Rush (`squadrush`), Onslaught and Air Superiority (`airsuperiority`)
are their names in lower case without spaces.

[code: rust/bc2-levelwrite/src/modelayer.rs:69-92]
[test: rust/bc2-levelwrite/src/modelayer.rs::mode_names_and_keys]

### GAME-MODE-4: The stats category comes from MatchmakingQueryID

Status: **observed**

The session code maps the mode's `MatchmakingQueryID` to the stats system's mode category (RUSH and CONQUEST 0x20,
SQDM 0x10, SQRUSH 8, else 1 for the ModeID `singleplayer` and -1 for anything else: GMD-SW-3). A mode with an unknown query id keeps no score, kills or round-end stats, so
a new mode (OPFOR's Onslaught) keeps `CONQUEST` and changes only its `ModeID`.

[exe: BFBC2Game.exe 0x5680C0 maps MatchmakingQueryID to the stats category]
[data: OPFOR's Onslaught with the console's ONSLAUGHT query id kept no score or stats (play test 2026-10-29)]
[doc: rust/bc2-install/src/onslaught/mode.rs:35-43] [doc: bfo:docs/research/opfor/AIR_SUPERIORITY_RE_2026-11-14.md:244-246]

### GAME-MODE-5: The UI tests ModeID against "conquest"

Status: **observed**

The UI decides the flag HUD and the loading text by comparing the mode's `ModeID` with the literal `conquest`; a mode
with another `ModeID` gets neither unless that comparison is answered for it (OPFOR's DLL answers true for
`onslaught`).

[exe: BFBC2Game.exe literal at 0x0142ABD8, compared at 9 sites (8 through 0x92AB10, one on the description's mode): GMD-SW-1]
[doc: bfo:docs/research/opfor/AIR_SUPERIORITY_RE_2026-11-14.md:247-250]

### GAME-MODE-6: The game mod is BC2 or VIETNAM

Status: **observed**

The client holds a game mod (UiManager +0xC8C8C current, +0xC8C90 requested): 0 BFBC2, 1 Vietnam. At the loading
screen and at the HUD's start it is parsed from the level description's `MatchmakingModID` (not a stored setting:
ERA-MOD-2): `BC2` 0, `VIETNAM` 1, anything else 0, so a level of another content set plays as BFBC2. The HUD builds
its icon set from it right after: mod 0 asks for BFBC2's icon set, which a Vietnam-based common bundle lacks (an empty
icon table: flag markers drawn at the screen's origin). The setter re-runs the whole switch even when the mode is
unchanged.

[exe: BFBC2Game.exe parser 0x8FF330; setter 0x75FCB0; level-load writers 0x775D0A (HUD start) and 0x77332C (loading screen); HUD icon set 0x775D02 -> 0xF9C9E0; icon lookup 0xF88510]
[data: Wake Island before OPFOR's parser hook: every icon id looked up as 0 and the flag pass drew no diamonds; stock Atacama unchanged]
[doc: bfo:docs/research/opfor/WW2_THIRD_MODE.md:17-58] [doc: bfo:docs/research/opfor/QA_SWEEP_2026-11-14.md:54]

### GAME-MODE-7: The offline level list is filtered by MatchmakingModID

Status: **observed**

The Offline Matches / Play Now level list keeps only the levels whose description's `MatchmakingModID` equals the
current game mod's tag (`BC2` for mod 0, `VIETNAM` for 1, empty otherwise: an empty list). The server's own game-mod
attribute is the level's mod string, `BC2` when empty.

[exe: BFBC2Game.exe multiplayerMaps builder 0x9421F0 (compare at 0x942247 via 0x508440); server 0x865087 in 0x864E50]
[doc: bfo:docs/research/opfor/WW2_THIRD_MODE.md:86-92]

### GAME-MODE-8: Modes the suite does not offer for export

Status: **verified**

The suite offers a mode for an era only when a stock level of that era has it (GAME-MODE-2), and refuses two with a
reason: Onslaught (co-operative play against bots: it needs path data and the AI layer) and Air Superiority (BF1943's
mode; its rules are OPFOR's Pacific War work, still queued).

[code: rust/bc2-levelwrite/src/modelayer.rs:94-103]
[test: rust/bc2-levelwrite/src/modelayer.rs::mode_names_and_keys]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1162-1165]

## 2. The mode layer

### GAME-LAYER-1: Without a mode layer the session waits in state 14

Status: **observed**

A level hosted as a multiplayer match needs its mode's entities (LEVEL-MP-1); a campaign level hosted without them
loads, runs its AI, and the session never leaves state 14: no match starts. The entities DICE added to make campaign
maps playable (Napisal's study of MP_SP_004GR) are the description's `GameMode` and `AttackerTeam`, `AutoTeam`,
three `TeamEntityData`, `HumanPlayer`, two `LifeCounter`s, `MissionObjective`s and human spawns.

[data: OPFOR's offline-MP host of stock SP_005: the whole AI chain ran, the session stalled in state 14 (2026-10-19 on); sp_004_b once reached state 3]
[doc: opfor:research/AUDIT.md:26] [doc: opfor:research/BFBC2_NAPISAL_RESEARCH.md:145-156]
[doc: opfor:DEVELOPMENT_NOTES.md:1677-1680]

### GAME-LAYER-2: What a layer is

Status: **verified**

A level's mode layer is its seed instances (teams, auto-team, player, mission, menus, messages, pre-round,
spectate, squads, team filters, ticket / life / kill counters, voice-overs, every capture point and combat area,
arm / disarm awards, mission objectives, WW2's end-of-round spawns) and its **objectives** (the placed objects whose
events reach an arm / defuse `GameInteractionEntityData`, with the objective props their events reach), closed over
what they reference and what their events reach among the layer's types (spawns, alternative spawns, map markers,
area geometry, the event glue, interactions, area triggers, sounds, sequences). Measured layer sizes: `mp_009cq` 238
instances, `mp_005gr` 821, `mp_005sr` 279, `mp_009sdm` 172, `nam_mp_002r` 507, `bc1_oasis_gr` 1,082, WW2
`guadal_canal` 277. Vehicle spawns, world objects, effects and cutscenes are not layer.

[code: rust/bc2-levelwrite/src/modelayer.rs:46-67] [code: rust/bc2-levelwrite/src/mpconvert.rs:333-362]
[code: rust/bc2-levelwrite/src/modelayer.rs:253-288]
[test: rust/bc2-levelwrite/tests/export_e3d.rs::each_eras_modes_and_their_layers]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1182-1190]

### GAME-LAYER-3: Every stock Conquest layer has the same types

Status: **observed**

A stock Conquest layer is the same set of types on every level: `AutoTeam` and three `TeamEntityData` (neutral and
two teams), 12 `TeamFilter`s, two `TicketCounter`s, `PreRound`, `Spectate`, `SquadData`s, 16 `Message`s, the
announcer's `VoiceOver`s and lines, an MP `HumanPlayer`, `Mission`, `Menu`s, `InputRestriction`, the event glue
(`EventSplitter`, `DelayTrigger`), two team bases with HQ spawns, the flags (each with a `MapMarker`) and a
`CombatAreaTrigger` with its outline.

[data: the Conquest layer compared on every stock Conquest level and DICE's MP_SP_005CQ conversion (round E3)]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:818-829]

### GAME-LAYER-4: A layer complete for its mode

Status: **verified**

The suite counts a layer (player, mission and auto-team; bases per team; flags; combat areas; ticket, life and kill
counters; teams; soldier spawns; M-COMs per Rush stage) and holds it against the donor's: Conquest keeps a base per
team and at least one flag and its ticket counters; Rush and Squad Rush every stage with the donor's M-COM count, its
combat areas and the attackers' life counters; Squad Deathmatch its squads (teams) and kill counters; every mode a
soldier spawn and a combat area.

[code: rust/bc2-levelwrite/src/modelayer.rs:545-576] [code: rust/bc2-levelwrite/src/modelayer.rs:587-680]
[test: rust/bc2-levelwrite/src/modelayer.rs::mode_names_and_keys]
[test: rust/bc2-levelwrite/tests/export_e3d.rs::rush_to_conquest_as_a_project_operation]

### GAME-LAYER-5: A new mode replaces the old layer

Status: **verified**

Giving a level another mode takes out its whole layer when it has one (it has an `AutoTeamEntityData`), and in any
case the instances of the types the layer replaces (player, mission, menus, spawns, combat areas, markers,
announcer); vehicle spawners stay for the campaign checklist. Atacama's Rush made Conquest keeps no M-COM stage and
has a complete Conquest layer.

[code: rust/bc2-levelwrite/src/modelayer.rs:289-306]
[test: rust/bc2-levelwrite/tests/export_e3d.rs::rush_to_conquest_as_a_project_operation]

## 3. Conquest

### GAME-CQ-1: Bases and flags are capture points

Status: **implemented**

Both a team's base (HQ) and a flag are a `CapturePointEntityData`: a base has `UnableToChangeTeam` true and
`InitialOwnerTeam` Team1 or Team2; a flag has it false and stands with a flag-pole blueprint and a `MapMarker`. Its
zone is its `Geometry` (an outline, a sphere or an axis-aligned box) or, without one, `CaptureRadius`.

[code: rust/bc2-levelwrite/src/mpconvert.rs:541-560]
[data: L1: every Conquest level's capture points; flag counts match the maps (Panama Canal 4: A-D)]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:132-144]

### GAME-CQ-2: Three or four flags (WW2 five)

Status: **observed**

DICE's Conquest levels have 3 or 4 flags (12 levels 3, 4 levels 4). BF1943's ports have 5. L1 warning rules:
L1-CQ-001, L1-CQ-001w.

[data: L1: 16 DICE Conquest levels 3-4 flags; 3 WW2 levels 5 each]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:46-48]

### GAME-CQ-3: Flags and HQs keep their distance

Status: **observed**

No two flags are closer than 58 m (nearest neighbour 58-450 m, median 121 m); an HQ is 85-384 m from its nearest flag
(median 151 m); the HQs are 285-1,358 m apart (median 424 m); the two HQs' distances to their nearest flag differ by
at most 54 m (median 14 m). WW2: flags 103-205 m apart, HQs 202-633 m from the nearest flag and 794-812 m apart,
the asymmetry up to 94 m. L1 warning rules: L1-CQ-002, L1-CQ-002w, L1-CQ-003, L1-CQ-003w, L1-CQ-004, L1-CQ-005.

[data: L1: 52 DICE flags, 32 HQs on 16 levels; min nearest-flag distance 58 m mp_006cq, max HQ-HQ 1,358 m mp_sp_005cq; WW2 15 flags, 6 HQs]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:211-234] [doc: bfbc2-map-editor/placement/L1_placement_report.md:331-341]

### GAME-CQ-4: Every flag lies between the HQs

Status: **observed**

Along the HQ-to-HQ axis every flag sits at 0.1-0.9 of the way from Team 1's HQ (median 0.5), within 0.4 HQ-distances
of the line (WW2 up to 1.1). The flag letters run in order along the axis (|rho| >= 0.8 on 10 of 14 levels, either
direction); the flags' `Index` order does not. L1 warning rules: L1-CQ-006, L1-CQ-007.

[data: L1: 67 of 67 flags (DICE and WW2) at 0.1-0.9; 52 of 52 DICE flags within 0.4 sideways]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:52-53] [doc: bfbc2-map-editor/placement/L1_placement_report.md:266-270]

### GAME-CQ-5: Capture zones of 7.5-37 m

Status: **observed**

DICE's capture zones (outlines, spheres and `CaptureRadius`-only flags) are 7.5-37 m in equivalent radius (median
19 m): 30 outlines (7.5-33 m; BFBC1 and Vietnam use outlines throughout), 2 spheres (26-37 m), 4 radius-only flags
(12-20 m) and 16 boxes, whose `Extents` reading (half or full size) is not settled. WW2: 12-20 m. L1 warning rule:
L1-CQ-008 (boxes left out).

[data: L1: 52 DICE flags by zone kind 30 / 2 / 4 / 16; 39 zones measured without the boxes]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:236-245] [doc: bfbc2-map-editor/placement/L1_placement_report.md:177-185]

### GAME-CQ-6: Capture takes 20 seconds (WW2 ten)

Status: **observed**

A flag's `TimeToGetControl` is 20 s on DICE's levels (51 of 52; one 25 s) and 10 s on BF1943's (15 of 15). L1
warning rule: L1-CQ-009.

[data: L1: TimeToGetControl read as an integer from its raw bits: DICE 51 x 20, 1 x 25; WW2 15 x 10]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:55-56]

### GAME-CQ-7: Flag and HQ spawns

Status: **observed**

A flag has one spawner per team (36 of 52) or one neutral spawner (16 of 52), with 2-21 spawn points (median 12) lying
8-87 m from the flag, all within 137 m. An HQ has 4-13 points (median 9), typically within 25 m, all within 95 m.
WW2: one neutral spawner per flag with 4-5 points 10-53 m out. L1 warning rules: L1-CQ-010, L1-CQ-011, L1-CQ-012.

[data: L1: 52 DICE flags, 67 flags with WW2, 38 HQs]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:57-59] [doc: bfbc2-map-editor/placement/L1_placement_report.md:226-229]

### GAME-CQ-8: Any team takes a PC flag only with OnlyTakeableByTeam 0

Status: **observed**

On PC a flag with `OnlyTakeableByTeam` 1 or 2 cannot be taken by the other team (it can be lowered to neutral, not
raised); every PC Conquest flag has 0. The console's Onslaught rule "the defenders cannot retake" (value 1) does not
carry over as data. Why: the update skips a point whose progress belongs to the named team (ONS-CAP-3).

[data: Onslaught play tests 2026-10-29 on PC: a flag with 1 was lowered but not raised by the attackers; with A 0 and B 2, A was captured and B not; every PC Conquest flag 0]
[doc: rust/bc2-install/src/onslaught/mode.rs:9-11] [doc: bfo:docs/research/opfor/AIR_SUPERIORITY_RE_2026-11-14.md:242-243]

### GAME-CQ-9: Objectives are not on hill tops

Status: **observed**

A DICE flag stands within 7.5 m of the median ground 30-100 m around it (median 0 m; only 4 of 52 are 5 m or more
above), and two flags differ by at most 24 m in height. Rush M-COMs likewise (-8.8 to +9.6 m, median 0). BF1943's
flags favour hill tops (8 of 15 at 5 m or more, median +6.7 m). L1 warning rules: L1-HT-001, L1-HT-002.

[data: L1: 52 DICE flags prominence -6.5 to +7.4 m; 60 flag pairs up to 24 m; 118 M-COMs; 15 WW2 flags]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:79-83]

### GAME-CQ-10: How a capture point moves, frame by frame

Status: **observed** · Variants: PC

The server capture point's update (0x726B60) runs while the point is enabled, `UnableToChangeTeam` is clear and
`OnlyTakeableByTeam` allows it (ONS-CAP-3). It counts the living players in the area per team (BOT-GS-5: bots too):
`own`, those of the team the progress belongs to, and `enemy`, the largest other team. The step's factor (0x7066B0):

- nobody in the area: the progress drifts back at `ReturnMultiplier` (0 stops it);
- `SeesawCapturing` clear: only enemies (at least `MinNrToTakeControl`): n = -enemy; only the own team (at least
  `MinNrToTakeControl`): n = own; both present: 0, the point stands still;
- `SeesawCapturing` set: n = own - enemy when the larger side has at least `MinNrToTakeControl`;
- n is clamped to +-`MaxCaptureMultiplier` and the factor is +-(1 + `AddedMultiplierPerPlayer` x (|n| - 1)).

A positive factor raises the progress (0 - 100) by factor x 100 / `TimeToGetControl` per second (0x725630), a negative
one lowers it by factor x 100 / `TimeToLoseControl` (0x711AA0). At 100 the point is captured (+0xAC set, the owner
copied from +0x7C, `OnCaptured`); at 0 it is lost (`OnLost`) and +0x7C becomes the lowering team, which raises it from
there. `StartAtBottom` starts the progress at 0 instead of 100 (0x7118E0); `DisableIfEnemyInside` and the radius /
upper-sphere fields are read in the same update. DICE's PC values: GAME-CQ-5 (radius), GAME-CQ-6 (times).

[exe: BFBC2Game.exe 0x726B60 (update: +0xAD, data +0x13A, data +0x84, 0x724060, 0x725D40, data +0x136); 0x725D40 (factor > 0 -> 0x725630, < 0 -> 0x711AA0, 0 -> +0x98 = 0); 0x7066B0 (data +0x80 ReturnMultiplier, +0x88 MinNrToTakeControl, +0x134 SeesawCapturing, +0xB8 MaxCaptureMultiplier, +0xBC AddedMultiplierPerPlayer; constants 1.0 at 0x14D94F0, -1.0 at 0x14D9420); 0x725630 (100.0 at 0x14D9638 over data +0x9C); 0x711AA0 (data +0xA0); 0x7118E0 (+0x9C = 0 when data +0x132 StartAtBottom, else 100); field descriptors 0x1C25750 - 0x1C25A68]
### GAME-CQ-11: A flag's unit is its own

Status: **verified**

Every DICE flag has a unit no other capture point shares: what moves with it (its zone's outline or box, its map
marker, its spawners and their spawn points) and what only its own events reach (`OnCaptured` / `OnLost` to two team
filters per team, each filter starting the flag's own capture and loss messages and voice-over lines, and on some
flags its vehicle spawners). The capture messages name the flag (`HUD_MP_CQ_TOOK_BASE_B`), the marker's `Sid` is its
letter (the minimap's and the HUD's). So a new flag is a copy of a flag's unit wired as the template is, and a flag
is deleted with its unit (the editor's gameplay tools, XLOG-GAME-1).

[data: E4: 19 Conquest levels of every set, 67 flags: no unit shares an instance with another capture point's; 246 team filters in the units; Valparaiso's flag C (MP_01_Factory): 31 instances - its box, marker, spawner, 10 spawn points, 4 team filters, 4 messages, 4 voice-over entities and their lines, one vehicle spawner]
[test: rust/bc2-levelwrite/tests/gameplay_tools.rs::every_conquest_flag_has_a_unit_of_its_own]
[test: rust/bc2-levelwrite/tests/gameplay_tools.rs::a_flag_added_wired_and_deleted_on_valparaiso]
[code: rust/bc2-levelwrite/src/gameplay.rs:435-450]

### GAME-CQ-12: A flag's texts are keyed by its letter

Status: **verified**

A flag's texts name it by its letter (its marker's `Sid`, GAME-CQ-11): its capture messages' `MessageSid` is
`HUD_MP_CQ_TOOK_BASE_<letter>` / `HUD_MP_CQ_LOST_BASE_<letter>` ("WE HAVE TAKEN POSITION ALPHA"), keys DICE's
localisation has for A to F in all eight languages (the NATO word, FOXTROT, kept in six of them), and its soldier
spawners' `LocationNameSid` (the deploy screen's name) is a key of the level ending in the letter (`MP_001_A` = "A:
COMM CENTER"). So a new flag's messages take its own letter's keys and its spawners the level's key for it; where the
game has no text, the export adds one: a spawn name `<LEVEL>_<letter>` worded as DICE's ("E: HARBOR", the flag's name
in capitals) and the messages of a letter past F worded as DICE's F with the letter's NATO word in each language that
says FOXTROT (German "WIR HABEN POSITION GOLF ÜBERNOMMEN!"), English where it does not (Russian, Japanese).

[data: E4-rest, 2026-11-22: every stock Conquest level of each set, 67 lettered flags: 208 of 208 capture messages name their flag's letter, 103 of 103 soldier spawners' LocationNameSid end in it, all 103 with a text; HUD_MP_CQ_TOOK_BASE_A - F and LOST_BASE_A - F in all 8 languages, G on none]
[test: rust/bc2-levelwrite/tests/flag_texts.rs::every_conquest_flags_texts_name_its_letter]
[test: rust/bc2-levelwrite/tests/flag_texts.rs::a_new_flags_texts_on_valparaiso]
[test: rust/bc2-levelwrite/tests/flag_texts.rs::a_new_flags_texts_on_a_synthetic_level]
[code: rust/bc2-levelwrite/src/gameplay.rs:1115-1283] [code: rust/bc2-levelwrite/src/names.rs:227-322]

## 4. Tickets and the end of the round

### GAME-TKT-1: Conquest tickets

Status: **observed**

A Conquest team starts with 200 tickets (24 of 32 teams; 250 on 6, 350 on 2) and `TicketLossPerMin` 10 on every DICE
level. BF1943's ports: 130 and 4 per minute; Coral Sea (Air Superiority) 100 and 4. L1 warning rule: L1-CQ-016.

[data: L1: 32 DICE Conquest ticket counters; WW2 3 levels; Coral Sea's two counters]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:60-61] [doc: bfo:docs/research/opfor/AIR_SUPERIORITY_RE_2026-11-14.md:86-99]

### GAME-TKT-2: The ticket counter's bleed fields

Status: **observed**

A `TicketCounterEntityData` per team carries `InitialTicketCount`, `TicketLossPerMin`, `TicketLossWhenLostAllBases`,
`TicketLossWhenLostAll` (BFBC2 only), `TicketLossPerMinPointCountScale` (100), `TicketLossStart`,
`HaltTicketLossOnEqualPointCount` (true on BFBC2's Conquest, false on BF1943's) and `TicketPercentages`: entries of
{`VOEvent`, `TicketLossPerMin`, `MaxPercentOfTicketsLeft`}. A capture point's area value decides which team
out-points the other and bleeds it. A death costs a ticket without any field saying so (engine behaviour: the
server counter's code, BOT-GS-6, which also gives the bleed's formula).

[data: Atacama Conquest, Wake Island (BF1943) and Coral Sea counters compared field by field]
[exe: BFBC2Game.exe: TicketCountPercentage {VOEvent +0, TicketLossPerMin +4, MaxPercentOfTicketsLeft +8}, TicketCounterEntityData.TicketPercentages at +0x84]
[doc: bfo:docs/research/opfor/AIR_SUPERIORITY_RE_2026-11-14.md:86-105] [doc: opfor:research/WW2_VO_EVENTS_RE_2026-11-14.md:24-28]

### GAME-TKT-3: A counter at zero ends the round on the server

Status: **observed**

When a server ticket counter reaches 0 the other counter's team wins: the server raises ETRoundWon (0x40) for the
first player found on the winning team and ETRoundLost (0x41) for the first on the losing team. In the data the
counter's `OnTeamWins` drives the end chain (victory menu, input restriction, end camera, then `Mission.Success`
after a delay) and `OnReachZero` disables player spawning; Conquest has no timer or other end condition.

[exe: BFBC2Game.exe 0x67E3F0 (calls 0x67E53A, 0x67E5AC), reached from 0x6805B0 and 0x548324]
[data: Coral Sea's and BF1943's Conquest event graphs: the same end chain]
[doc: opfor:research/WW2_VO_EVENTS_RE_2026-11-14.md:67-68] [doc: bfo:docs/research/opfor/AIR_SUPERIORITY_RE_2026-11-14.md:106-115]

### GAME-TKT-4: Ticket percentages raise their own voice-over

Status: **observed**

The client watches the replicated `TicketPercentages` index; when it changes it reads that entry's `VOEvent`: ETNone
raises nothing, a local player on the counter's team hears the entry's event (ETTicketsLow 0x42,
ETReachTicketPercentage 0x43, whatever the data names), the other team ETTicketsMirror (0x44). These events are
raised only when the data holds such entries.

[exe: BFBC2Game.exe 0x8E9AF0 (calls 0x8E9B7C, 0x8E9B9C), index change 0x8F0610]
[data: Wake (BF1943) puts ETTicketsLow at 20 % and ETReachTicketPercentage at 80 %; Atacama's entry is 15 % with no event]
[doc: opfor:research/WW2_VO_EVENTS_RE_2026-11-14.md:69-71]

### GAME-TKT-5: Capture voice-over is the client's

Status: **observed**

When a capture point's replicated state changes to an owner team, the client raises ETWonCapturePoint (0x3E) when
the local player's team owns it, otherwise ETLostCapturePoint (0x3F), also for a flag the player's team never held.

[exe: BFBC2Game.exe 0x8C7190 (calls 0x8C71DD, 0x8C71FF), from 0x8ECD94]
[doc: opfor:research/WW2_VO_EVENTS_RE_2026-11-14.md:65-66]

## 5. Rush and Squad Rush

### GAME-RUSH-1: M-COMs and stages from the event graph

Status: **verified**

An M-COM is a placed `ReferenceObjectData` whose events reach an arm / disarm `GameInteractionEntityData`
(`objects/objects/mpobjective/mpobjective` on BFBC2 and BFBC1, `mp_objective_vietnam_01` on Vietnam). A Rush stage is
the M-COMs whose `onDestroyed` triggers the same set of stage counters (`MultipleTriggerEntityData`): `mp_005gr` 4
stages of 2, `nam_mp_002r` 3 of 2, `mp_005sr` 2 of 1.

[code: rust/bc2-levelwrite/src/modelayer.rs:219-232] [code: rust/bc2-levelwrite/src/modelayer.rs:499-520]
[test: rust/bc2-levelwrite/tests/export_e3d.rs::each_eras_modes_and_their_layers]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1192-1194]

### GAME-RUSH-2: Stage order follows the counters' enable chain

Status: **observed**

Starting from each instance's `Enabled` field and following the M-COMs' `onDestroyed` events through the glue
(`MultipleTrigger`, `DelayTrigger`, `EventSplitter`, `TeamFilter`, `EventGate`) with its `Enable` / `Disable`, the next
stage is the one whose counters are now enabled; bases are enabled and disabled per stage the same way. The stage
count equals the defenders' `LifeCounterEntityData.BaseCount` on every level, and the attackers are the team of the
`LifeCounterEntityData` with `Attacker` true (Team1 on every stock Rush level).

[data: L1 stage simulation: 30 of 30 Rush and Squad Rush levels resolved without ambiguity, stage count = BaseCount on 30 of 30, base names in the game's order]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:146-159]

### GAME-RUSH-3: Three to five stages of two M-COMs

Status: **observed**

Rush has 3-5 stages (BFBC2 3-5, BFBC1 4-5, Vietnam always 3), each of exactly 2 M-COMs; Squad Rush 2 stages of 1.
L1 warning rules: L1-RU-001, L1-RU-002.

[data: L1: 16 Rush levels, 59 of 59 stages with 2 M-COMs; 14 Squad Rush levels, 28 of 28 stages with 1]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:22-29]

### GAME-RUSH-4: A stage's M-COMs and the step between stages

Status: **observed**

A stage's two M-COMs are 12-91 m apart (median 40 m) and within 8.5 m of each other in height; consecutive stages
are 93-316 m apart (median 174 m, centroid to centroid), Squad Rush stages 45-169 m (median 119 m). L1 warning rules:
L1-RU-003, L1-RU-004, L1-RU-004s, L1-RU-014.

[data: L1: 59 M-COM pairs (min 11.7 m mp_007gr s1, max 91 m nam_mp_006r s1); 43 Rush steps (93 m mp_sp_002gr s3 to 316 m bc1_oasis_gr s2); 14 Squad Rush steps]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:272-293]

### GAME-RUSH-5: Defenders spawn close behind, attackers far in front

Status: **observed**

The defenders' nearest spawn is 6-61 m from an M-COM (median 34 m) and their spawn centroid behind the objectives as
the attackers see them (85 of 87 stages); the attackers' nearest spawn is 72-459 m away (median 160 m) and 64-490 m
in front on all 87 Rush and Squad Rush stages, 1.8-21 times as far as the defenders' (median 5). L1 warning rules:
L1-RU-005, L1-RU-005s, L1-RU-006, L1-RU-006s, L1-RU-007, L1-RU-007s, L1-RU-008, L1-RU-009.

[data: L1: 59 Rush and 28 Squad Rush stages]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:30-37]

### GAME-RUSH-6: The attackers' first base is outside the defenders' area

Status: **observed**

At stage 1 the attackers' base lies outside the defenders' combat area (2-141 m, median 91 m): the only spawn
protection Rush has. From stage 2 the attackers' forward base usually lies inside it, 27-321 m (median 75 m) from the
previous stage's defender base. L1 warning rule: L1-RU-010.

[data: L1: 30 of 30 Rush and Squad Rush levels]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:38-41]

### GAME-RUSH-7: Combat areas grow stage by stage

Status: **observed**

Each stage's combat area keeps 99-100 % of the previous one (median 99.9 %; exceptions Cold War `mp_sp_002gr` stage 3,
38 %, and `nam_mp_006r` stage 3, 78 % for the defenders). Stage 1's areas are the smallest, the defenders' smaller
than the attackers'; from stage 2 both sides usually share one outline. Every M-COM is inside both sides' areas, at
least 5.7 m from the edge. L1 warning rules: L1-RU-011, L1-RU-012, L1-RU-013.

[data: L1: 41 of 43 stage steps keep 99 % or more; 146 M-COM margins]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:42-44] [doc: bfbc2-map-editor/placement/L1_placement_report.md:295-298]

### GAME-RUSH-8: Attacker lives

Status: **observed**

The attackers' life counter starts at 75-100 on Rush (BFBC2 100, BFBC1 and Vietnam 75) and 20 on Squad Rush; the
defenders' at 1,500 (Squad Rush 1,000) with `BaseCount` the number of stages. L1 warning rule: L1-RU-015.

[data: L1: 16 of 16 Rush and 14 of 14 Squad Rush levels]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:292] [doc: bfbc2-map-editor/placement/L1_placement_report.md:470]

### GAME-RUSH-9: A stage as it is played: the counters' events from the start state

Status: **verified**

What is in play during a Rush stage follows from the data alone: every instance starts from its `Enabled`; a stage
is played while all of its counters (`MultipleTriggerEntityData`) are enabled; its M-COMs' `onDestroyed` events,
followed through the event glue (multiple and delay triggers, event splitters, team filters, gates), `Enable` and
`Disable` the next stage's bases, combat areas, spawns and counters, never through another stage's counters. The
editor's mode view layer shows one stage so: the other stages' M-COMs with what belongs to them, and the bases,
combat areas and spawns disabled during it, are hidden. On MP_005GR, MP_005SR, NAM_MP_002R and BC1_OASIS_GR the
bases and the two combat areas in play per stage are L1's, in L1's play order, with no ambiguity.

[code: rust/bc2-levelwrite/src/modelayer.rs:594-725] [code: rust/bc2-levelwrite/src/modelayer.rs:729-780]
[test: rust/bc2-levelwrite/tests/vw2.rs::rush_stages_as_played_match_l1]
[test: rust/bc2-editor/tests/vw2.rs::mode_view_layer_hides_by_mode_and_rush_stage]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:146-163]

### GAME-RUSH-10: A stage's counters count its M-COMs

Status: **verified**

Each `MultipleTriggerEntityData` a Rush or Squad Rush M-COM's `onDestroyed` reaches directly has `TriggerAmount` equal
to the number of the stage's M-COMs that reach it (2 on Rush, 1 on Squad Rush): a stage ends when all of them are
destroyed. An M-COM added to a stage makes every counter of the stage count one more, one taken out one fewer.

[data: E4: 31 Rush and Squad Rush levels of every set, 221 stage counters, all equal]
[test: rust/bc2-levelwrite/tests/gameplay_tools.rs::every_rush_stage_counter_counts_its_mcoms]
[test: rust/bc2-levelwrite/tests/gameplay_tools.rs::an_mcom_added_to_a_rush_stage_and_taken_out]
[code: rust/bc2-levelwrite/src/gameplay.rs:479-500]

### GAME-RUSH-11: A stage is wired to the next by its base counter

Status: **verified**

Every stage of a DICE Rush or Squad Rush level has one **base counter**: the counter (GAME-RUSH-10) its M-COMs reach
whose `OnTrigger` takes a base from the defenders (`DestructBase` on their `LifeCounterEntityData`, BOT-GS-8), as
many as the defenders' `BaseCount`. They form the **enable chain**: stage 1's starts enabled, every later one
disabled; stage k's `OnTrigger` enables stage k+1's and nothing else enables it; the last stage's enables no counter.
A stage's other counters (the team triggers starting the announcer) are enabled from the start. What is in play
during one stage only - its defenders' base with its spawns, the attackers' forward base, the two combat areas, the
markers and interactions of its M-COMs - is switched on as the stage before ends: the forward base and the defenders'
area by the stage before's base counter (`Enable`), the defenders' base by the stage's own base counter's
`OnEnabled`, the attackers' area by its hold-back timer; the stage's base counter switches its own off (`Disable`,
and `DisableSpawnPointFunctionality` on its vehicle spawners) as the stage ends, and a combat area's `OnEnabled`
disables the one it replaces. DICE's last stages leave the forward base and the combat areas on (21 of 30 still give
the attackers their reinforcement, `IncreaseReinforce`); MP_006's last counter switches on a sixth forward base for a
stage the level does not have. The play's model follows a stage's end in no time, so a timer of the M-COMs that
enables a spawn (DICE's "Enable spawns in B") can come after the base counter's disable of it: the base counter has
the last word. A stage added after the last is a copy of the last stage wired so (XLOG-GAME-3).

[data: E4-rest: rust/bc2-levelwrite/examples/rush_facts.rs on 31 Rush and Squad Rush levels, 2026-11-22: 30 levels with one base counter per stage and the exact chain (87 stages), BaseCount equal on 30 of 30; base counters' `OnTrigger` CapturePoint `Enable` of the next stage's 54 of 57 middle stages, CombatArea `Enable` 43, `Disable` of its own 36 (0 on the last); 78 later stages' other counters enabled from the start, 3 not; `ons_mp_005gr` (the Onslaught install's) has no defenders' life counter]
[data: E4-rest sweep, 2026-11-22: on all 30 levels a stage added after the last plays last with its copies as the last stage's objects (72 - 323 copies, 7 - 62 connections added) and, taken out again, gives back every instance and stage state; each of the 87 stages taken out leaves the others in their order and states but one hold-back combat area on nam_mp_002r]
[test: rust/bc2-levelwrite/tests/rush_stages.rs::every_rush_level_is_wired_stage_by_stage]
[test: rust/bc2-levelwrite/tests/rush_stages.rs::stages_added_and_taken_out_on_a_synthetic_level]
[code: rust/bc2-levelwrite/src/rushstage.rs:1-702]
[code: rust/bc2-levelwrite/src/rushstage.rs:1054-1173]

### GAME-RUSH-12: A stage inserted between two splits the wiring between them

Status: **verified**

A Rush stage put between stage k and stage k+1 (round E4-rest2) is wired by GAME-RUSH-11's chain read twice. It is
a copy of stage k+1's set (a later stage: its counters, bases and combat areas start disabled as a middle stage's
do). Every connection between the two stages' sets is split: one from stage k to stage k+1 (stage k's base counter
enabling the next base counter, the forward base, the defenders' area; its timers) goes from stage k to the copy,
and from the copy to stage k+1 through the object of the copy that plays the source's part (its counterpart: the base
counter for the base counter, else the own object that comes into play the same way); one from stage k+1 to stage k
(a combat area's `OnEnabled` disabling the one it replaces, the attackers' hold-back timer disabling stage k's area)
goes from the copy to stage k, and from stage k+1 to the copy's counterpart of its target. Stage k+1's links to the
level's shared logic are copied; its links to the stages after it are not (they stay stage k+1's). The defenders'
`BaseCount` counts one more. The stages then play `..., k, new, k+1, ...`, each old stage with the states it had
(the objects whose state DICE's play leaves uncertain aside, GAME-RUSH-11's last word), the new stage with the copies
in play as stage k+1's objects were and stage k+1's own objects waiting as during stage k. Taking the new stage out
again (GAME-RUSH-11's removal, which also hands what the next stage's objects did to the removed stage's on to the
stage before's counterparts) gives back every instance, stage state and connection of the level. A connection whose
end has no counterpart is left as it was (counted). What the next stage's counter switches on for a stage after it
that no stage plays (MP_006's sixth forward base) is not copied: it stays the stages after's. Unproven until a
playtest: the game playing the longer chain.

[data: E4-rest2 sweep, 2026-11-27: on all 30 Rush and Squad Rush levels a stage inserted between every two stages (57 insertions, 72 - 900 copies and 15 - 238 connections a level): each plays between the two with every old stage as before and, taken out again, gives back every instance, stage state and connection; 42 connections had no counterpart to split (0 - 5 a level); MP_006's dormant sixth forward base is not copied with its last stage; Port Valdez (mp_012gr) with a stage inserted after the first built in 28 - 37 s and held by `export::check`]
[test: rust/bc2-levelwrite/tests/rush_stages.rs::every_rush_level_is_wired_stage_by_stage]
[test: rust/bc2-levelwrite/tests/rush_stages.rs::stages_added_and_taken_out_on_a_synthetic_level]
[test: rust/bc2-levelwrite/tests/rush_stages.rs::port_valdez_with_a_stage_inserted_exports]
[code: rust/bc2-levelwrite/src/rushstage.rs:703-1053]

### GAME-RULES-1: The round's rules and DICE's values for them

Status: **verified**

The rules of a round are the mode layer's own values: Conquest each ticket counter's `InitialTicketCount` and
`TicketLossPerMin` and each flag's `TimeToGetControl` / `TimeToLoseControl`; Rush and Squad Rush the life counters'
`StartingLifeCount` (the attackers' `Attacker` true); Squad Deathmatch the kill counters' `MaxKillCount`. No server
option of the game changes them (the server's presets are hardcore, friendly fire, kill cam, minimap, spotting,
crosshair and third-person vehicle cameras: `Server.*` settings, not tickets). DICE's values per mode and era are the
editor's defaults (GAME-TKT-1, GAME-CQ-6, GAME-RUSH-8, GAME-SDM-1): Conquest 200 tickets losing 10 a minute, a flag
taken in 20 s (WW2 130, 4, 10 s); Rush attackers 100 lives (BFBC1 and Vietnam 75), defenders 1,500; Squad Rush 20
against 1,000; Squad Deathmatch 50 kills. The values DICE used are the choices (Conquest tickets 200, 250, 350; a flag
20 or 25 s); another value is the user's own, which the export's L1 rules warn about.

[exe: BFBC2Game.exe strings: Server.IsHardcore, Server.FriendlyFireEnabled, Server.KillCam, Server.Minimap, Server.MinimapSpotting, Server.ThreeDSpotting, Server.Crosshair, Server.ThirdPersonVehicleCameras; no ticket or round-length setting]
[test: rust/bc2-levelwrite/tests/gameplay_tools.rs::round_rules_stock_set_and_refused]
[code: rust/bc2-levelwrite/src/gameplay.rs:656-760]

## 6. Squad Deathmatch

### GAME-SDM-1: Four squads, one pool of spawns, one vehicle

Status: **verified**

Squad Deathmatch has four squads: four capture points Team1-Team4 with one spawner each, all spawners sharing the same
alternative spawn points (14-34 distinct points, 13-38 m apart, spread over 159-289 m); `TeamEntityData` five (the
neutral team and the squads) and four `KillCounterEntityData`, the target 50 kills; one vehicle (an IFV or tank
shared by all). L1 warning rules: L1-SD-001, L1-SD-002, L1-SD-003, L1-SD-006.

[test: rust/bc2-levelwrite/tests/export_e3d.rs::each_eras_modes_and_their_layers]
[data: L1: 15 of 15 DICE levels 4 squads, one vehicle, MaxKillCount 50; on 13 of 15 the squads' capture points stand at one spot]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:70-74] [doc: bfbc2-map-editor/placement/L1_placement_report.md:303-316]

### GAME-SDM-2: A small area around the spawns

Status: **observed**

The Squad Deathmatch combat area is 15,000-84,000 m2 (median 40,000) and every spawn point is at least 2 m inside it
(the spawners themselves may stand outside: `nam_mp_007sdm` 14-20 m). L1 warning rules: L1-SD-004, L1-SD-005.

[data: L1: 15 levels]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:74] [doc: bfbc2-map-editor/placement/L1_placement_report.md:189-194]

## 7. Spawning and combat areas

### GAME-SPAWN-1: Spawners and their alternative points

Status: **observed**

A capture point lists its `Spawners` (`SoldierSpawnEntityData`, with `Team` and `PlayerType`); a spawner's
`AlternativeSpawnPoints` (`AlternateSpawnEntityData`) are the places a soldier appears. With alternatives present the
suite treats the spawner's own transform as an anchor, not a spawn place (it counts only when it has none: 1 of 441).
No stock BFBC2 level fills a spawner's `Vehicle` array (spawning into a vehicle).

[data: L1: 441 spawners; Squad Deathmatch spawners stacked 2.5 m apart while their alternatives spread over the map]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:189-194] [doc: bfo:docs/research/opfor/AIR_SUPERIORITY_RE_2026-11-14.md:233-236]

### GAME-SPAWN-2: Spawners check for enemies within 20 m

Status: **observed**

Every soldier spawner's `SpawnEnemyRadius` is 20 m (409 of 410; one 10 m on `mp_005gr`). With per-team combat areas
it is the only spawn protection; no dedicated spawn-protection entity exists. L1 warning rule: L1-AL-001.

[data: L1: 410 spawners on the stock levels]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:78] [doc: bfbc2-map-editor/placement/L1_placement_report.md:202-205]

### GAME-SPAWN-3: DICE's spawn points stand on open ground

Status: **observed**

On the stock levels DICE's soldier spawn points stand on the ground or on an object's walkable top: none over water
deeper than a soldier wades (1.2 m from the surface to the ground or pier under it), none outside the terrain, none
more than 4.6 m under the height field (one, under a cliff overhang on `mp_008`). Measured as a vertical line's
crossings of each static object's collision (a closed solid is crossed an odd number of times above and below a
point inside it; a point standing on a surface of the object up to 0.6 m below it is in an interior - a hangar, a
container - not in a solid), 13 of 1,475 stand a little inside an object's collision, up to 4.2 m below its surface
(boulders, a Vietnam rock: the soldier is put on top); vegetation left out. The export's XVAL-PLACE-2 reports a spawn
point past those bounds.

[data: A1: 1,475 spawn points of 27 levels (mp_001 to mp_012sr of game Dist; mp_005gr, nam_mp_002cq, nam_mp_005r, and OPFOR's WW2 ports wake_island_s, guadal_canal, iwo_jima_s) against their height field, water mesh and static collision: 0 over water deeper than 1.2 m, 0 outside the terrain, deepest under the ground 4.6 m, 13 buried 0.01 to 4.24 m]
[doc: rust/BENCHMARKS_ARCHIVE_1.md:8167-8186]

### GAME-VEH-1: Vehicle spawners have their vehicle's room and domain

Status: **observed**

DICE's vehicle spawners stand clear of static collision at a vehicle's height (0.5 to 3 m above the spawner) by at
least 2.5 m for tanks, 1 m for cars and boats, 4 m for helicopters and 0.5 m for IFVs and quads; land vehicles stand
on land, boats in 0.4 to 28 m of water (by the shore on the BFBC2 maps, at sea on the WW2 ports; Vietnam's river boats
stand on rivers the level's water mesh does not hold, the mesh far below them), helicopters and the ambient aircraft
may stand outside the terrain; land and sea vehicles stand up to 142 m outside every combat area (an HQ's vehicles
that drive in). The export's XVAL-PLACE-3 and XVAL-PLACE-4 report a spawner past those bounds.

[data: A1: 707 vehicle spawners of the same 27 levels by L1's vehicle class (247 emplacements, 111 ambient, 64 tanks, 54 helicopters, 52 quads, 48 boats, 42 IFVs, 37 cars, 36 UAV stations, 15 planes): the tightest room per class as above]
[doc: rust/BENCHMARKS_ARCHIVE_1.md:8167-8186]

### GAME-VEH-2: DICE's vehicles can leave their spawners, in counts per mode

Status: **observed**

Every land vehicle DICE placed (tanks, IFVs, cars, quad bikes: 184 on the 27 levels measured) can drive 40 m or more
from its spawner on ground no steeper than a 1.2 m step per 2 m and clear of static collision 0.6 to 2.5 m up; every
boat whose water the level's mesh holds reaches 30.5 m or more of open water at least 0.5 m deep (`mp_009gr`'s the
tightest); every helicopter has no static collision less than 60 m over it within 3.5 m (one Vietnam helipad,
`nam_mp_005r`, has some 3.1 m up 7 m out); every WW2 plane has a straight take-off run of 34 m or more on some bearing.
The spawners a capture point lists spawn by themselves (`AutoSpawn` true on all 172); 64 unlisted ones wait for events.
DICE's counts per mode (L1's medians): a Conquest base's pool 4 (8 on the WW2 ports), a flag's 2 (one per team, or a
neutral spawner); Rush's attackers 3 and defenders 2 a stage; Squad Rush none; Squad Deathmatch one IFV, owned by the
layer's neutral capture point on every stock level. The export's XVAL-PLACE-6 to 8 report a spawner that cannot leave;
the auto layout places vehicles to these counts (XLAY-AUTO-2).

[data: A2: rust/bc2-editor/examples/vehicle_sites.rs on 707 vehicle spawners of 27 levels (21 Dist, 6 Package), 2026-11-22: way out per land class 40 m (the limit) on all; boats 30.5 m least; helicopters 60 m (the limit) within 3.5 m; planes 34 m least; owners and AutoSpawn as above]
[data: bfbc2-map-editor/placement/l1_a1_params.json: hq_vehicle_pool_150m median 4 (CQ DICE) and 8 (CQ WW2), flag_vehicle_pool_150m 2, attacker_vehicles_per_stage 3, defender_vehicles_per_stage 2, SR 0, SDM vehicles 1]

### GAME-AREA-1: A combat area is a union of outlines, per team or shared

Status: **observed**

A `CombatAreaTriggerEntityData`'s `Geometry` is one or more outlines (LEVEL-ENT-2) whose union is the playable area;
`Team` with `UseTeamRestriction` says the team it applies to. It also has `SurroundingGeometry` (a larger outline on
many levels; its role is not known), `AllowedVehicles` (DICE uses it for the UAV station), `SpawnHeight`,
`InitalSpeed` (DICE's spelling) and the deserter fields.

[data: L1: every combat area of the 70 levels read and rasterised; SurroundingGeometry on mp_005 410,000 m2 against 356,000]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:143] [doc: bfo:docs/research/opfor/AIR_SUPERIORITY_RE_2026-11-14.md:237-241]

### GAME-AREA-2: Conquest areas and spawn protection

Status: **observed**

A Conquest combat area is 44,000-479,000 m2 (median 99,000); flags are at least 11 m inside it (median 67 m), every
HQ inside its own team's area and every spawn point at least 1.3 m inside its team's. Where each team has its own area
(Vietnam Conquest, WW2) each HQ lies outside the enemy's area (Vietnam 29-88 m, WW2 3-35 m); BFBC2 and BFBC1 Conquest
share one area and have no such protection. L1 warning rules: L1-CQ-013, L1-CQ-014, L1-CQ-015, L1-CQ-017, L1-AL-002.

[data: L1: 16 DICE Conquest levels, 67 flags, 38 HQs, 1,146 spawn points inside their team's area, 16 of 16 per-team HQs outside the enemy's area]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:62-66]

## 8. Damage areas

### GAME-DMG-1: Damage area triggers are collapse kill zones

Status: **observed**

DICE's `DamageAreaTriggerEntityData` instances on multiplayer levels are building-collapse kill zones: triggered by
an object's `onDestroyed`, 10,000 damage per second. Which part and material a hit damages is HAVOK-MAT-2.

[data: L1: every DamageAreaTrigger of the stock levels (l1_dmg.py)]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:202-205]

## 9. Awards and stats offline

### GAME-AWARD-1: weaponProficiency$ crashes without the EA stats copy

Status: **observed**

The menus' per-player answers (`playerStats`, `vehicleGrid`, `unlockGrid`, `weaponProficiency$<id>`) read the
client's copy of the EA stats (UiManager +0xC38D4), created only by the online stats download, which also loads the
award list (+0xC3D58). Offline the copy is null: the lists are undefined or empty, and `weaponProficiency$` at
0x943B20 dereferences the missing copy and crashes. Any answer to the extern (OPFOR's: undefined award fields, the
game's own answer for an item without an award) keeps 0x943B20 from running.

[exe: BFBC2Game.exe 0x943B20 weaponProficiency$; stats download handler 0x9644A0; extern getter 0x76EC30]
[doc: opfor:DEVELOPMENT_NOTES.md:1099-1107] [doc: opfor:DEVELOPMENT_NOTES.md:1128-1129]

## 10. Air Superiority (BF1943)

### GAME-AS-1: BF1943 mode classes BFBC2 lacks

Status: **observed** · Variants: BF1943

BF1943's Air Superiority uses classes BFBC2's executable does not have (`DmzEntityData`, `RepairStationEntityData`,
`HotZoneEntityData`) and the field `MessageEntityData.SecondMessageSid`; BFBC2's `CapturePointEntityData` has every
field of Coral Sea's point (plus `SquadSpawnRadius`) and no field restricting capture to a vehicle type. Its one
capture point, tickets (GAME-TKT-1) and combat area port as data; whether the server counts a player in an aircraft
inside the zone is not settled.

[exe: BFBC2Game.exe CapturePointEntityData field table 0x01C25870 (40 fields); ServerCapturePointEntity type info 0x1BF2E88]
[data: Coral Sea's partition against BFBC2's class catalogue]
[doc: bfo:docs/research/opfor/AIR_SUPERIORITY_RE_2026-11-14.md:211-232]

## Open questions

- **AABB capture zones.** Whether a box zone's `Extents` are half or full sizes (GAME-CQ-5): the engine's containment
  test would settle it.
- **`SurroundingGeometry`**: a minimap frame or a deserter zone (GAME-AREA-1).
- **A spawner's own transform** when it has alternatives: a spawn place or only an anchor (GAME-SPAWN-1); a play test
  of a Squad Deathmatch level would settle it.
- **Ticket bleed with equal points** on BF1943's data (`HaltTicketLossOnEqualPointCount` false), and what a
  helicopter spawner's 0 s respawn delay means (103 of 141).
- **Capture by aircraft**: whether `ServerCapturePointEntity` counts players in vehicles and high in a sphere zone
  (GAME-AS-1).
- **Server voice-over on the in-process host**: whether ETRoundWon / ETRoundLost raised on the server system are heard
  on hostMultiplayerLocal.
- **`OnlyTakeableByTeam`**: read by the capture update (ONS-CAP-3, round SPG2-rest2); the console's "defenders cannot
  retake" needs a module patch of that test.
- **Onslaught's stages** could be simulated the same way (flag k's `OnCaptured` as the transition); not measured.
- **Rush vehicles from standalone spawners** started by events, and ambient vehicles told apart.
