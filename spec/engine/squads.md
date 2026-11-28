# Teams, squads and players

**Sources.** Round SPG4 (2026-11-23): the census of DICE's partitions (`spec/census.json`, re-measured by the game
tier) and `BFBC2Game.exe`'s names (strings only; the squad code is not traced). The bots' squads are
`engine/bot_ai.md` (BOT-SQD-1 - 3) and their orders `model/ai_orders.md`; spawning is GAME-SPAWN. The tables list the
members DICE's instances carry, less `Name`, `MemoryReportGroup`, `Components`, `Enabled`, `Enumeration` and
`Transform`.

## 1. Teams

### SQD-TEAM-1: A team is a faction with its kits and player caps

Status: **verified** · Variants: PC

`Objects/Teams/` holds the ten `TeamData`s (US, RU, the Vietnam sides, the campaign's and BC1's ...): a faction, the
team's kits, the maximum players and the maximum players per squad (`MaxPlayersPerSquad`; the game's squads are four).
A level filters events by team with `TeamFilterEntityData` (224: `AllowTeam`); `HumanPlayerEntityData` (74, one per
multiplayer level) is the human player's rules: the last-man-standing string, the display time and the delay after
the player is killed; `PreRoundEntityData` (63) holds the round until enough players have joined
(`PlayerCountNeeded`, `NotEnoughPlayersRestartTimer`).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| TeamData (GameSharedResources) | 10 | 10 | `objects/teams` | Faction, Kits, MaxPlayerCount, MaxPlayersPerSquad |
| TeamFilterEntityData (GameSharedResources) | 224 | 21 | `levels/` | AllowTeam |
| HumanPlayerEntityData (GameSharedResources) | 74 | 74 | `levels/` | DisplayTime, LastManStandingSid, PlayerKilledDelay |
| PreRoundEntityData (GameSharedResources) | 63 | 62 | `levels/` | NotEnoughPlayersRestartTimer, PlayerCountNeeded |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json (DICE's partitions, round SPG4, 2026-11-23): the classes above]

## 2. Squads

### SQD-SQ-1: Squad types, formations and the campaign's squad ghosts

Status: **verified** · Variants: PC

`AI/Squads`' one `SquadAsset` lists the squad types (`SquadTypeData`, BOT-SQD-2) and `AI/Formations`' one
`FormationHolder` the formations scripted squads move in (`SquadBehaviourMoveInFormation`). The campaign spawns
squads through `SquadGhostEntityData` (37: a squad) and `SoldierSpawnGhostEntityData` (339: a soldier, an amount,
delays, team, immortality, AIO-ASN-1's spawner-side soldiers).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| SquadAsset (GameSharedResources) | 1 | 1 | `ai/squads.dbx` | SquadTypes |
| FormationHolder (GameSharedResources) | 1 | 1 | `ai/formations.dbx` | Formations |
| SquadGhostEntityData (GameSharedResources) | 37 | 10 | `levels/` | Squad |
| SoldierSpawnGhostEntityData (GameSharedResources) | 339 | 13 | `levels/` | AllowMulipleSpawn, Amount, FakeImmortal, Immortal, InitializedSpawnDelay, Soldier, SpawnDelay, Team |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

### SQD-PLAY-1: The player's squad: join, lead, spawn on, chat, score

Status: **observed** · Variants: PC

Multiplayer squads are code with no data class: the executable's names give the system's parts. Joining: the menus'
externs `joinSquad`, `leaveSquad`, `doJoinSquad`, `autojoinSquad`, `autoLookForSquad`, `lookingForSquad`,
`showJoinSquadQuestion`, `inGameJoinSquadAccept`, `isSquadPrivate`, the game settings' squad options; leading:
`isSquadLeader`, `makeSquadLeader`, `onSquadLeaderPromoted`, the HUD icons `UIHudIcon_SquadLeader` /
`UIHudIcon_SquadMember`; spawning on a squad mate: `selectSquadSpawn`, `SetBestSquadSpawner`, `SquadSpawnRadius`,
`SquadSpawnCamera`, `StatEvent_SquadSpawn`; chat and voice: `TextChatGroup_Squad`, `VoiceChannel_Squad`,
`ConceptSquadChat`; scoring: the squad assist, heal, revive, repair, resupply and order-bonus scores
(`SquadReviveScore`, `SquadAttackOrderScore`, `SquadDefendOrderScore` ...) and their `StatEvent_Squad*` events. Squad
orders a player gives are the spotting / order path of SOLD-SPOT-1 (`CanSpotAndOrder`).

[exe: BFBC2Game.exe strings joinSquad, leaveSquad, doJoinSquad, autojoinSquad, isSquadLeader, makeSquadLeader, selectSquadSpawn, SetBestSquadSpawner, SquadSpawnRadius, SquadSpawnCamera, TextChatGroup_Squad, VoiceChannel_Squad, SquadReviveScore, SquadAttackOrderScore, SquadDefendOrderScore, StatEvent_SquadSpawn, SquadManagerEntityData, SquadEntityData, JoinSquadOrderEntityData]
[data: spec/census.json, 2026-11-23: no DBX class for the players' squads: the six Squad classes with instances (SquadAsset, SquadData, SquadTypeData, SquadEntityData, SquadGhostEntityData, JoinSquadOrderEntityData) are the AI's; SquadManagerEntityData is a typeinfo class with no instance in DICE's partitions]

### SQD-PLAY-2: Spawning on a squad mate: the request's checks and the 13 positions

Status: **verified** · Variants: PC

The client's spawn request (read by 0x83A370: two bools, then two player ids of 0 - 0x7FF) is dispatched by
0x83A3C0: "on a player" to 0x662F20(a, b), the second bool to 0x63AEA0, otherwise a spawn point by index (0x6422C0).
In the normal form a = b, the squad mate's id. 0x662F20 refuses unless:

- the mate is not the requester and has a soldier;
- the two are on the same team (+0xA8) and in the same squad (+0xAC), and the squad is not 24 (0x6C9790: squad id 24
  is `SquadNone`, after `Squad1` - `Squad24` of the `SquadId` enumeration);
- the mate's soldier is not dying, and a player flag (`[+0xBC8] & 4`) is set;
- the anti-spam list (0x156F2D0, tested by 0x673FB0) has no running timer for the requester;
- the mate is inside one of the team's combat areas (0x547760 walks the `ServerCombatAreaTriggerEntity` entities and
  answers "outside" only when none holds the position; a level with no combat area never refuses).

The placement is `ServerPlayer`'s spawn-on-player 0x572CC0 (profiler labels `ServerPlayer/spawnOnPlayer1`, `2`). It
refuses a player already alive or spawning (`[+0xC3C]`) or without a kit (`[+0xD24]`). A mate in a vehicle puts the
requester in the vehicle's first free seat (0x68E7C0), falling back to the on-foot form when the vehicle's health is
0. On foot it tries 13 offsets (a, b) along the mate's two horizontal axes, in order: (0,-2), (0,-1), (2,0), (-2,0),
(2,-1), (-2,-1), (2,-2), (-2,-2), (0,-4), (1,1), (-1,1), (0.5,1), (-0.5,1) metres (built 0x572CFB - 0x572E26; the
loop's bound 13 at 0x5735E9). A candidate is taken when a ray down to -1,000 finds the ground, the line from the
mate's eye height to it is clear, the height difference is at most 10 m (its square against 100.0), the character
fits (0xB095B0) and it is not outside the combat area; when all 13 fail the spawn is refused. So a mate in combat
can be spawned on (no combat test), a mate in a full vehicle only when a free spot on foot is found, and a mate
outside the combat area not at all.

Neither DICE value the data might seem to set is read on this path: `TeamData.MaxPlayersPerSquad` (+0x1C, 4 in all
10 of DICE's instances) has no reader traced, and `CapturePointEntityData.SquadSpawnRadius` (+0xB4: 0 in 249 of 332,
80 - 300 in the rest) is not used by the squad spawn (its reader is not traced). The a != b form (b's vehicle must be
entered, 10 m horizontally, a 2 s timer at `[player+0xD80]`) is likely spawning into a team vehicle; not decoded.

[test: core/tests/test_spec_probes.py::run_time]
[data: spec/probes/runtime.py on DICE's partitions, 2026-11-27: TeamData.MaxPlayersPerSquad 4 in 10 of 10]
[exe: BFBC2Game.exe request reader 0x83A370, handler 0x83A3C0, checks 0x662F20 (squad test 0x6C9790, `cmp eax, 0x18` at 0x6C9796; anti-spam 0x673FB0 over 0x156F2D0; combat areas 0x547760), placement 0x572CC0 (13 candidates, `cmp eax, 0xD` at 0x5735E9; ground ray to -1000.0 at 0x1418B70; height limit 100.0 at 0x14D9638; fit test 0xB095B0; seat 0x68E7C0)]

## Open questions

- The squad manager's code beyond the spawn: squad size (`MaxPlayersPerSquad`'s reader), the a != b spawn form, the
  anti-spam timer's length, the player flag `[+0xBC8] & 4`.
- Playtests (paused): four spawn cases for SQD-PLAY-2, `bfo:docs/tasks/spec-task-notes.md`.
