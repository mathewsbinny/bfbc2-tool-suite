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
[doc: opfor:research/AIR_SUPERIORITY_RE_2026-11-14.md:192-217]

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
SQDM 0x10, SQRUSH 8, anything else -1). A mode with an unknown query id keeps no score, kills or round-end stats, so
a new mode (OPFOR's Onslaught) keeps `CONQUEST` and changes only its `ModeID`.

[exe: BFBC2Game.exe 0x5680C0 maps MatchmakingQueryID to the stats category]
[data: OPFOR's Onslaught with the console's ONSLAUGHT query id kept no score or stats (play test 2026-10-29)]
[doc: opfor:src/tools/onslaught/console_mode.py:57-61] [doc: opfor:research/AIR_SUPERIORITY_RE_2026-11-14.md:242-244]

### GAME-MODE-5: The UI tests ModeID against "conquest"

Status: **observed**

The UI decides the flag HUD and the loading text by comparing the mode's `ModeID` with the literal `conquest`; a mode
with another `ModeID` gets neither unless that comparison is answered for it (OPFOR's DLL answers true for
`onslaught`).

[exe: BFBC2Game.exe literal at 0x0142ABD8, compared at 7 sites through 0x92AB10]
[doc: opfor:research/AIR_SUPERIORITY_RE_2026-11-14.md:245-248]

### GAME-MODE-6: The game mod is BC2 or VIETNAM

Status: **observed**

The client holds a game mod (UiManager +0xC8C8C current, +0xC8C90 requested): 0 BFBC2, 1 Vietnam. At start-up and
at level load it is parsed from the stored `gameMod` setting: `BC2` 0, `VIETNAM` 1, anything else 0, so the profile
can never hold another value. The HUD builds its icon set from it right after: mod 0 asks for BFBC2's icon set,
which a Vietnam-based common bundle lacks (an empty icon table: flag markers drawn at the screen's origin). The
setter re-runs the whole switch even when the mode is unchanged.

[exe: BFBC2Game.exe parser 0x8FF330; setter 0x75FCB0; start-up writer 0x775D0A; HUD icon set 0x775D02 -> 0xF9C9E0; icon lookup 0xF88510]
[data: Wake Island before OPFOR's parser hook: every icon id looked up as 0 and the flag pass drew no diamonds; stock Atacama unchanged]
[doc: opfor:research/WW2_THIRD_MODE.md:15-56] [doc: opfor:research/QA_SWEEP_2026-11-14.md:52]

### GAME-MODE-7: The offline level list is filtered by MatchmakingModID

Status: **observed**

The Offline Matches / Play Now level list keeps only the levels whose description's `MatchmakingModID` equals the
current game mod's tag (`BC2` for mod 0, `VIETNAM` for 1, empty otherwise: an empty list). The server's own game-mod
attribute is the level's mod string, `BC2` when empty.

[exe: BFBC2Game.exe multiplayerMaps builder 0x9421F0 (compare at 0x942247 via 0x508440); server 0x865087 in 0x864E50]
[doc: opfor:research/WW2_THIRD_MODE.md:84-90]

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
carry over as data.

[data: Onslaught play tests 2026-10-29 on PC: a flag with 1 was lowered but not raised by the attackers; with A 0 and B 2, A was captured and B not; every PC Conquest flag 0]
[doc: opfor:src/tools/onslaught/console_mode.py:43-48] [doc: opfor:research/AIR_SUPERIORITY_RE_2026-11-14.md:240-241]

### GAME-CQ-9: Objectives are not on hill tops

Status: **observed**

A DICE flag stands within 7.5 m of the median ground 30-100 m around it (median 0 m; only 4 of 52 are 5 m or more
above), and two flags differ by at most 24 m in height. Rush M-COMs likewise (-8.8 to +9.6 m, median 0). BF1943's
flags favour hill tops (8 of 15 at 5 m or more, median +6.7 m). L1 warning rules: L1-HT-001, L1-HT-002.

[data: L1: 52 DICE flags prominence -6.5 to +7.4 m; 60 flag pairs up to 24 m; 118 M-COMs; 15 WW2 flags]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:79-83]

## 4. Tickets and the end of the round

### GAME-TKT-1: Conquest tickets

Status: **observed**

A Conquest team starts with 200 tickets (24 of 32 teams; 250 on 6, 350 on 2) and `TicketLossPerMin` 10 on every DICE
level. BF1943's ports: 130 and 4 per minute; Coral Sea (Air Superiority) 100 and 4. L1 warning rule: L1-CQ-016.

[data: L1: 32 DICE Conquest ticket counters; WW2 3 levels; Coral Sea's two counters]
[doc: bfbc2-map-editor/placement/L1_placement_report.md:60-61] [doc: opfor:research/AIR_SUPERIORITY_RE_2026-11-14.md:84-97]

### GAME-TKT-2: The ticket counter's bleed fields

Status: **observed**

A `TicketCounterEntityData` per team carries `InitialTicketCount`, `TicketLossPerMin`, `TicketLossWhenLostAllBases`,
`TicketLossWhenLostAll` (BFBC2 only), `TicketLossPerMinPointCountScale` (100), `TicketLossStart`,
`HaltTicketLossOnEqualPointCount` (true on BFBC2's Conquest, false on BF1943's) and `TicketPercentages`: entries of
{`VOEvent`, `TicketLossPerMin`, `MaxPercentOfTicketsLeft`}. A capture point's area value decides which team
out-points the other and bleeds it. A death costs a ticket without any field saying so (engine behaviour).

[data: Atacama Conquest, Wake Island (BF1943) and Coral Sea counters compared field by field]
[exe: BFBC2Game.exe: TicketCountPercentage {VOEvent +0, TicketLossPerMin +4, MaxPercentOfTicketsLeft +8}, TicketCounterEntityData.TicketPercentages at +0x84]
[doc: opfor:research/AIR_SUPERIORITY_RE_2026-11-14.md:84-103] [doc: opfor:research/WW2_VO_EVENTS_RE_2026-11-14.md:24-28]

### GAME-TKT-3: A counter at zero ends the round on the server

Status: **observed**

When a server ticket counter reaches 0 the other counter's team wins: the server raises ETRoundWon (0x40) for the
first player found on the winning team and ETRoundLost (0x41) for the first on the losing team. In the data the
counter's `OnTeamWins` drives the end chain (victory menu, input restriction, end camera, then `Mission.Success`
after a delay) and `OnReachZero` disables player spawning; Conquest has no timer or other end condition.

[exe: BFBC2Game.exe 0x67E3F0 (calls 0x67E53A, 0x67E5AC), reached from 0x6805B0 and 0x548324]
[data: Coral Sea's and BF1943's Conquest event graphs: the same end chain]
[doc: opfor:research/WW2_VO_EVENTS_RE_2026-11-14.md:67-68] [doc: opfor:research/AIR_SUPERIORITY_RE_2026-11-14.md:104-113]

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
[doc: bfbc2-map-editor/placement/L1_placement_report.md:189-194] [doc: opfor:research/AIR_SUPERIORITY_RE_2026-11-14.md:231-234]

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
[doc: rust/BENCHMARKS.md:8167-8186]

### GAME-VEH-1: Vehicle spawners have their vehicle's room and domain

Status: **observed**

DICE's vehicle spawners stand clear of static collision at a vehicle's height (0.5 to 3 m above the spawner) by at
least 2.5 m for tanks, 1 m for cars and boats, 4 m for helicopters and 0.5 m for IFVs and quads; land vehicles stand
on land, boats in 0.4 to 28 m of water (by the shore on the BFBC2 maps, at sea on the WW2 ports; Vietnam's river boats
stand on rivers the level's water mesh does not hold, the mesh far below them), helicopters and the ambient aircraft
may stand outside the terrain; land and sea vehicles stand up to 142 m outside every combat area (an HQ's vehicles
that drive in). The export's XVAL-PLACE-3 and XVAL-PLACE-4 report a spawner past those bounds.

[data: A1: 707 vehicle spawners of the same 27 levels by L1's vehicle class (247 emplacements, 111 ambient, 64 tanks, 54 helicopters, 52 quads, 48 boats, 42 IFVs, 37 cars, 36 UAV stations, 15 planes): the tightest room per class as above]
[doc: rust/BENCHMARKS.md:8167-8186]

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
[doc: bfbc2-map-editor/placement/L1_placement_report.md:143] [doc: opfor:research/AIR_SUPERIORITY_RE_2026-11-14.md:235-239]

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
[doc: opfor:research/AIR_SUPERIORITY_RE_2026-11-14.md:209-230]

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
- **`OnlyTakeableByTeam`**: how PC reads 1 and 2 (GAME-CQ-8); the console's "defenders cannot retake" needs the DLL.
- **Onslaught's stages** could be simulated the same way (flag k's `OnCaptured` as the transition); not measured.
- **Rush vehicles from standalone spawners** started by events, and ambient vehicles told apart.
