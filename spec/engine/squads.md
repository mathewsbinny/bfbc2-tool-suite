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

## Open questions

- The squad manager's code (squad size, the spawn-on-squad rules: whether a squad mate in combat or a vehicle can be
  spawned on) is not traced.
