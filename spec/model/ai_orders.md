# AI orders: the level data that tells DICE's bots what to do

How a BFBC2 level directs its AI soldiers: the order entities, their fields, which soldiers an order names, how an
order is issued, completes and chains, and how an order's type weighs the squad behaviours. The runtime that carries
the orders out is [../engine/bot_ai.md](../engine/bot_ai.md); the spawners, path data and start-up a level needs are
[../engine/ai.md](../engine/ai.md); the suite's authoring of an order layer on stock multiplayer maps is
[../transforms/stockmp.md](../transforms/stockmp.md) (XSMP-AUT-1).

**Sources.** Round AIS's probe `rust/bc2-formats/examples/ai_probe.rs` over the game copy (2026-11-22): the shared AI
partitions of `sp_common` / `mp_common` and the level partitions of DICE's 13 campaign levels (`sp_*`, the Package copy,
which the game loads first) and of the five Onslaught levels OPFOR installs (`ons_*`, from Napisal's and the console's
data: a second, not-DICE-PC corpus); its game-tier twin `rust/bc2-formats/tests/ai_corpus.rs`; the executable's type
names (`BFBC2Game.exe`, build 795745, read with the analysed Ghidra project of OPFOR's research). Counts are of the 13
campaign levels unless a rule says otherwise. No names, positions or other content of the levels are given.

## 1. The order classes

### AIO-CLS-1: Fifteen order classes

Status: **observed** · Variants: PC

The executable's type registry has fifteen `*OrderEntityData` classes: `AreaOrderEntityData`, `PatrolOrderEntityData`,
`SearchAndDestroyOrderEntityData`, `ShootAtObjectOrderEntityData`, `FireAtAreaOrderEntityData`,
`InterludeOrderEntityData`, `CancelOrderEntityData`, `ChangeOrderEntityData`, `ChangeReadinessOrderEntityData`,
`ChangeFormationOrderEntityData`, `FollowAISoldierOrderEntityData`, `FollowClosestHumanOrderEntityData`,
`JoinSquadOrderEntityData`, `AimAtOrderEntityData` and `RandomWalkOrderEntityData`, all derived from `OrderEntityData`.
No class is a game-mode objective (no capture, M-COM, ticket or flag order): see AIO-MODE-1.

[exe: BFBC2Game.exe type-name strings 0x0146ADC0..0x0146B070, type-info records 0x01C02434..0x01C025BC (stride 0x18), base name ">@OrderEntityData" at 0x01419C56]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:236-241]

### AIO-CLS-2: What DICE's campaign uses

Status: **observed** · Variants: PC

Over the 13 campaign levels: Interlude 1,229 (13 levels), Patrol 1,141 (13), Cancel 997 (13), Area 904 (13),
ShootAtObject 180 (11), SearchAndDestroy 74 (10), FollowClosestHuman 50 (10), Change 37 (8), FireAtArea 28 (9), AimAt
14 (5), JoinSquad 2 (1), RandomWalk 1 (1); ChangeReadiness, ChangeFormation and FollowAISoldier are never placed. The
Onslaught levels in the game copy use Area 117, Patrol 31, SearchAndDestroy 28, Change 2 and FollowClosestHuman 1: a
combat level needs only Area, Patrol and SearchAndDestroy; Interlude and Cancel serve the campaign's scripting.

[data: ai_probe over 13 campaign levels (Package copies) and 5 Onslaught levels of the game copy, 2026-11-22]

## 2. The fields

### AIO-FLD-1: The fields every order has

Status: **observed** · Variants: PC

Every order carries: `Enabled`; `Squads[]` and `Soldiers[]` (whom it commands, AIO-ASN-1); `IssueOrderDelay` (s; 0-22
in the campaign); `OnCompleteTimeOut` / `OnFailedTimeOut` (s, 0-45); `IsUrgent` (AIO-RDY-3); `ReissueIfSoldierIsKilled`;
`EventsOnStart` / `EventsOnComplete` / `EventsOnFail`, each `{Sensing, Target, Immortal, VoiceOvers}` of `ESONoChange`
/ `ESOEnable` / `ESODisable` (switching the soldier's senses, whether it may be targeted, immortality and voice-over on
those moments); `ReadinessOnStart` / `ReadinessOnComplete` / `ReadinessOnFail` (`RLNone`, `RLPatrol`, `RLReady`,
`RLCombat`); `AllowStunAnimations` (`SASNoChange` everywhere); `UnspawnOnComplete` / `UnspawnOnFail`; `Stance`
(`SONoChange`, `SOStanding`, `SOCrouching`); `SpeedState` (`SSONoChange`, `SSOWalking`, `SSORunning`, `SSOSprinting`,
`SSOSneaking`); `AimState` (`AsoNoChange`, `AsoAiming`, `AsoNotaiming`). An order's stance, speed and aim are what the
soldier keeps while it carries the order out: of the campaign's 1,141 patrols 630 run and 171 sprint.

[data: ai_probe field tallies over 4,657 campaign orders; the enumerators listed are every value seen]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:236-241]

### AIO-FLD-2: The fields of each order class

Status: **verified** · Variants: PC

| Class | Own fields (campaign values) |
|---|---|
| Area | `Area[]` (always exactly one geometry entity), `AreaType` (`AreaHoldGround` 877, `AreaMoveTo` 27), `AnimationIndex` (-1 to 20), `IdleAnimation` |
| Patrol | `RouteWaypoints[]` (always exactly one waypoint-route entity), `TypeOfRoute` (`RouteStop` 1,121, `RouteCircular` 11, `RoutePingPong` 9), `UsePathFinding` (913 true, 228 false), `DefaultSpeed`, `MinDistance` (0.1-20 m), `VehicleThrottle` (0.2-1) |
| SearchAndDestroy | `Area[]` (0 or 1), `Aggressive`, `TargetAreaFollowsTarget`, `TargetPosKnowledge` (`TargetPosKnown` 67, `TargetPosUnknown` 6, `TargetPosFind` 1), `FindTargetTime` (0-20 s), `TargetBotSoldier` (never set) |
| ShootAtObject | `IgnoreSensing` |
| FireAtArea | `Area[]` (one), `StopIfPlayerIsSensed` |
| Interlude | `Crouch`, `IsCutscene` |
| Change | `Instant`, `DisallowedIndices[]` (always empty) |
| RandomWalk | `Geometry[]` |
| Cancel, FollowClosestHuman, JoinSquad, AimAt | the common fields only (AIO-FLD-1) |

The test checks the two "exactly one" shapes on all 904 area and 1,141 patrol orders.

[test: rust/bc2-formats/tests/ai_corpus.rs::campaign_order_shapes_and_events]
[data: ai_probe field and array tallies over the 13 campaign levels]

### AIO-FLD-3: An order's area is a geometry entity

Status: **observed** · Variants: PC

An Area, FireAtArea or SearchAndDestroy order names its area as a geometry entity of the same partition:
`AabbGeometryEntityData` 775, `SphereGeometryEntityData` 112, `AreaGeometryEntityData` 82 (and one in another
partition) over the campaign; 110 / 29 / 6 on the Onslaught levels. The order's own `Transform` also carries a position.
Napisal's PC-proven orders use boxes of about 3 m half-extent on the objective.

[data: ai_probe "order Area targets by type", campaign and Onslaught]
[doc: bfo:docs/plans/opfor/STOCK_MP_AI_PLAN.md:309-311]

## 3. Whom an order commands

### AIO-ASN-1: Orders name spawners and squads

Status: **observed** · Variants: PC

An order's `Soldiers[]` names `SoldierSpawnEntityData` instances (every soldier that spawner spawns), its `Squads[]`
names `SquadEntityData` instances, whose `Soldiers[]` names spawners. In the campaign an order names 0-7 spawners and 0-4
squads; 1,340 of the 1,497 enabled `SSAiPlayer` spawners (89.5 %) are named by some order, directly or through a squad
(Onslaught 246 of 275). The rest receive orders through scripting (`ChangeOrder`, cut-scenes) or stand idle.

[data: ai_probe: enabled SSAiPlayer spawners 1,497, named 1,340 (campaign); 275 / 246 (Onslaught); Soldiers 0..7, Squads 0..4 per Area order]
[doc: bfo:docs/plans/opfor/STOCK_MP_AI_PLAN.md:281-293]

### AIO-ASN-2: The spawner fields an AI soldier takes

Status: **observed** · Variants: PC

A campaign AI spawner sets: `PlayerType` (`SSAiPlayer` 1,543, `SSHumanPlayer` 13), `Team` (`Team1` 123, `Team2` 1,359,
`TeamNeutral` 74), `Template` (the AI template, AI-NEED-3; the grunt template on 706 of 1,556), `SoldierKit` (61
distinct kits), `Squad` (454 spawners in a squad), `Vehicle` and `VehiclePos` (282 spawners seated in a vehicle,
seat 0-5), `Amount` (0-5), `SpawnDelay` / `InitializedSpawnDelay` (0-5 s / 0-3 s), `SpawnSoldierFromBeginning` (48
true), `Immortal` (174), `FakeImmortal` (33), `HumanTargetPreference` (-1 to 2), `OnSpottedType` (`OSPlayer` 1,555),
`TryToSpawnAwayFromSight` (67), `SpawnEnemyRadius` (20 everywhere), `LodDistance` (0-500).

[data: ai_probe SoldierSpawnEntityData tallies over 1,556 campaign spawners]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:229-235]

## 4. Issuing, completing, chaining

### AIO-EVT-1: An order runs when its IssueOrder event fires

Status: **verified** · Variants: PC

An order does nothing until its `IssueOrder` input event fires; `Enable` / `Disable` only gate it. Every event
connection into an order in the campaign is one of the three: `IssueOrder` 2,943, `Disable` 71, `Enable` 1. Issuing
gives the order to the named soldiers (after `IssueOrderDelay`), replacing what they were doing.

[test: rust/bc2-formats/tests/ai_corpus.rs::campaign_order_shapes_and_events]
[data: ai_probe "events into AI entities": IssueOrder per class Patrol 1,180, Area 982, Interlude 275, ShootAtObject 235, SearchAndDestroy 89, Cancel 54, FollowClosestHuman 50, FireAtArea 32, Change 28, AimAt 15, JoinSquad 3]

### AIO-EVT-2: What issues an order

Status: **observed** · Variants: PC

The campaign's `IssueOrder` sources: `EventSplitter.OnImpulse` 1,351 (fan-out), `PatrolOrder.OnComplete` 590 (route
chains), `AreaTrigger.OnEnter` 272 (the player arriving), `ShootAtObjectOrder.OnComplete` 94, cut-scene ends 143,
`DelayTrigger.OnTrigger` 83, `MultipleTrigger.OnTrigger` 69, `SoldierSpawn.OnSpawn` 33, `AreaOrder.OnComplete` 24,
`SoldierSpawn.OnSpotted` 19, and a tail of other triggers. On the Onslaught levels: `AreaTrigger.OnEnter` 81,
`EventSplitter` 36, `MultipleTrigger` 32, `PatrolOrder.OnComplete` 20, `CapturePoint.OnCaptured` 3,
`VehicleSpawn.OnDestroy` 5. Orders are therefore scripted state: the level's event graph decides when each order
starts, not the AI.

[data: ai_probe "IssueOrder sources", campaign and Onslaught]
[doc: bfo:docs/plans/opfor/STOCK_MP_AI_PLAN.md:305-308]

### AIO-EVT-3: Orders report issue, completion and failure

Status: **observed** · Variants: PC

Orders raise `OnIssue`, `OnComplete` and `OnFailed`. In the campaign `PatrolOrder.OnComplete` is wired 769 times (a
patrol completes at its route's end), `ShootAtObject.OnComplete` 129, `InterludeOrder.OnComplete` 46,
`AreaOrder.OnComplete` 38, `ChangeOrder.OnComplete` 29; failures are rare (`PatrolOrder.OnFailed` 30,
`ShootAtObject.OnFailed` 13). `AreaHoldGround` is the campaign's default (877 of 904) and is wired to complete only 38
times: a hold order is a standing state, not a task (inferred from the wiring). `OnCompleteTimeOut` /
`OnFailedTimeOut` delay the events.

[data: ai_probe "events out of AI entities", campaign]

### AIO-EVT-4: Spawner events that script soldiers

Status: **observed** · Variants: PC

A spawner takes `Spawn` 170, `UnSpawnSoldier` 316, `KillSoldier` 194, `Enable` / `Disable` 68, `EnableSenses` /
`DisableSenses` 174, `EnableAsTarget` / `DisableAsTarget` 86, `EnableImmortal` / `DisableImmortal` /
`EnableFakeImmortal` 72 and `EnableVoiceOvers` / `DisableVoiceOvers` 128, and raises `OnKill` 985 (its soldier died),
`OnSpawn` 93, `OnSpotted` 73, `OnFirstDamage` 59, `OnFirstFire` 32, `OnHeardEnemy` 30, `OnBulletHit` 20, `OnDamage` 9
and `OnAllKilled` 4. Squads take `KillAll` (and on Onslaught `Enable`, `Disable`, `KillAndDisableAll`, raising
`OnDisabled`).

[data: ai_probe "events into / out of AI entities", campaign and Onslaught]

### AIO-RT-1: One server class runs every order; IssueOrder is an event hash

Status: **observed** · Variants: PC

All fifteen order data classes run as one server entity class (`ServerOrderEntity`, vtable 0x014199C8, which 0x594740
and 0x599970 write). Its event handler (slot 6, 0x58DD20) first passes the event to the base handler 0xFFE580; then,
when the order is enabled (+0x26 bit 3) and the event's hash (+0xC) is 0xD2199312 - "IssueOrder" under the engine's
event-name hash, djb2 with xor (h = 5381, then h = h * 33 ^ c, as 0x10013E0 computes it) - it either marks the order
pending (+0x78 = 1) and asks for updates (0x627640(5, 1)) when the data's `IssueOrderDelay` (+0xBC) is above 0, or
issues at once through its slot 52 (0x5A5490). Completion (0x599BC0) and failure (0x599C30) fire `OnComplete`
(0x25B1196D) and `OnFailed` (0x5EEF6327) through 0x01002950. A bot module can therefore issue any order of the level
from the DLL by calling that handler with an event of hash 0xD2199312, or 0x5A5490 directly, on the server thread;
`Enable` (0x8BA423E4) and `Disable` (0xF90216B1) are hashed the same way. How 0x5A5490 hands the order to its soldiers
and squads was not read (RE item 5 of `bot_ai.md`).

[exe: BFBC2Game.exe vtable 0x014199C8 (slot 6 0x58DD20 `cmp dword [edi+0xC], 0xD2199312` at 0x58DD38; slot 52 0x5A5490); data +0xBC tested at 0x58DD44; constructors referencing the vtable at 0x594749 / 0x5999A6; 0x599BC0 `0x1002950(0x25B1196D, 0, 1)`; the OnFailed hash at 0x599C5D; hash loop 0x10013F8..0x1001416 (`mov eax, 0x1505; shl 5; add; xor`)]

## 5. Priority

### AIO-RDY-1: One OrderReadiness per order type

Status: **verified** · Variants: PC

`AI/Orders` (`OrderSettings`, the same bytes in `sp_common` and `mp_common`) holds eleven `OrderReadiness` records, one
per order type: `Change`, `FollowPlayer`, `HoldGround`, `Interlude`, `MoveTo`, `Patrol`, `RandomWalk`, `FireAtArea`,
`SearchAndDestroy`, `ShootAtObject`, `AimAt`. Each gives an `Initial` and equal `Minimum` readiness (`RSCombat` for
FireAtArea and ShootAtObject, `RSPatrol` for the rest) and `UrgencyModifiers`, a list of (squad behaviour, modifier):

| Order type | Urgency modifiers | Not-urgent set |
|---|---|---|
| MoveTo | Defend 1, Idle 0.01 | Combat 1.5 |
| HoldGround | Defend 1, Idle 0.01 | none |
| Patrol | Move 0.5, Idle 0.01 | Combat 1 |
| RandomWalk | RandomWalk 0.5, Idle 0.01 | Combat 1 |
| SearchAndDestroy | SearchAndDestroy 2, Defend 1.5, Combat 1, Idle 0.01 | none |
| FollowPlayer | Move 0.5, Defend 1 | - |
| FireAtArea | Combat 1 | none |
| ShootAtObject | ShootAtObject 1 | - |
| Interlude, AimAt | Interlude 1 | none |
| Change | Change 1 | none |

The two `AreaType`s map to MoveTo and HoldGround (inferred from the names).

[test: rust/bc2-formats/tests/ai_corpus.rs::order_readiness_per_order_type]
[data: ai_probe OrderReadiness: 11 instances, OrderType 11 distinct, Initial = Minimum]

### AIO-RDY-2: A squad picks its behaviour by weight

Status: **inferred** · Variants: PC

A squad type (`AI/Squads`: `SCInfantry`, `SCAir`, `SCArmored`) gives each squad behaviour a `DefaultModifier`
(Combat 0.5 and Idle 0.1 for infantry and armour, AirCombat 0.5 for air, every other behaviour 0); the active order's
type adds its urgency modifiers (AIO-RDY-1). The squad runs the behaviour of highest weight, so an order outweighs
idling (Defend 1 against Idle 0.1) and a SearchAndDestroy order (2) outweighs combat (0.5 + 1). The ten squad behaviour
classes are BOT-SQD-2.

[inferred: from the data's shape (per-behaviour default weights, per-order additive modifiers named after the squad behaviours, the 0.01 Idle entries) and the time-sliced decision pass of BOT-RT-4; the summation and the arg-max were not traced in the executable]

### AIO-RDY-3: IsUrgent drops the order's combat allowance

Status: **inferred** · Variants: PC

Each `OrderReadiness` names a `ModifierSets` list of `UrgencySetData` named "Not Urgent"; the sets of MoveTo, Patrol and
RandomWalk add Combat 1 or 1.5. Inference: a non-urgent order adds that set, so its squad turns to fight what it meets
on the way, while an urgent one (`IsUrgent` true) does not and the soldiers carry the order through. DICE marks 998 of
1,141 patrols urgent but only 22 of 904 area orders; Napisal's PC-proven attack orders are urgent `AreaMoveTo`.

[inferred: the set's name and contents against IsUrgent's use; not traced in the executable]
[data: ai_probe IsUrgent tallies per class]
[doc: bfo:docs/plans/opfor/STOCK_MP_AI_PLAN.md:309-311]

### AIO-RDY-4: Readiness levels and their reaction times

Status: **observed** · Variants: PC

`AI/Settings.ReadinessLevels` gives each readiness a reaction time drawn between `MinReactionTime` and
`MaxReactionTime`, falling over `ReductionTime`: Patrol 0-3 s (no reduction), Ready 0.1-0.2 s (30 s), Combat 0.6-0.8 s
(30 s). An order's `Readiness*` fields and a squad's `BaseReadiness` (`RLNone` 201, `RLCombat` 59, `RLReady` 36,
`RLPatrol` 5 of 301 campaign squads) set it; the sensing template's limits per readiness are BOT-PER-1.

[data: AI/Settings AISettingsData.ReadinessLevels (sp_common = mp_common); ai_probe SquadEntityData.BaseReadiness]

## 6. Game modes

### AIO-MODE-1: Orders know no game mode

Status: **observed** · Variants: PC

No order, goal, behaviour or squad behaviour class concerns a capture point, an M-COM, tickets or a combat area: the
four creator registries (BOT-RT-7) and the fifteen order classes have no such name. A mode's state reaches DICE's AI only
through the level's event graph: on the Onslaught levels `CapturePoint.OnCaptured` issues 3 orders and
`MultipleTrigger.OnTrigger` (the Rush stage triggers, GAME-RUSH-1) 32; the campaign has one
`MissionObjective.OnSuccess`. Conquest and Rush play for bots is therefore an order layer wired to the mode's events
(XSMP-AUT-1), or a bot module of OPFOR's own that reads the mode state itself (BOT-GS-1).

[exe: BFBC2Game.exe creator registries 0x01C08AB4..0x01C08D0C (goals), 0x01C098C4..0x01C09C3C (behaviours, goals), 0x01C0A4A4..0x01C0AACC (squad behaviours); order type names 0x0146ADC0..0x0146B070]
[data: ai_probe IssueOrder sources: CapturePointEntityData.OnCaptured 3 (Onslaught), MultipleTriggerEntityData.OnTrigger 32 (Onslaught) / 69 (campaign), MissionObjectiveEntityData.OnSuccess 1 (campaign)]

## Open questions

- How an order's `Area` and `AreaType` turn into a destination and a stopping rule in the runtime (the runtime class
  and its `IssueOrder` handler are AIO-RT-1; the issue itself, 0x5A5490, was not traced).
- Whether AIO-RDY-2's weighting is a sum and an arg-max, and when a squad re-decides (the decision pass's period,
  BOT-RT-4).
- What `Enumeration`, `AnimationIndex` and `IdleAnimation` select (animations of `AI/Animations`).
- How `ReissueIfSoldierIsKilled` finds the respawned soldier (by spawner, inferred).
