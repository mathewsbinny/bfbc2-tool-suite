# Bot AI: decision and control interfaces

What a bot is in BFBC2 and where its decisions and its control of a soldier happen: DICE's AI runtime as the server
runs it, the AI's perception, combat, movement, vehicle and squad layers with the data that tunes them, the game state
a bot would read, and the boundary where Project OPFOR BFBC2's bot module can plug in. The start-up, path data and
level requirements are [ai.md](ai.md) (AI-START-*, AI-KY-*, AI-NEED-*); the Kynapse runtime is
[kynapse_runtime.md](kynapse_runtime.md); the order entities of the level data are
[../model/ai_orders.md](../model/ai_orders.md) (AIO-*); the in-process host is [network.md](network.md) (NET-SES-*).
The section **Implementation interface** at the end lists what OPFOR's bot module must hook, call or replace, and
**Reverse engineering still needed** the open items in priority order.

**Sources.** Round AIS (2026-11-22): static reading of `BFBC2Game.exe` (PC, build 795745, image base 0x400000, no
RTTI) with the analysed Ghidra project of OPFOR's research (a scratch copy, read only); DICE's AI data through the
probe `rust/bc2-formats/examples/ai_probe.rs` and its game-tier twin `rust/bc2-formats/tests/ai_corpus.rs`; OPFOR's
research (`private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md` section 8, `private:opfor-research/research/WW2_LANDING_GEAR_RE2_2026-11-14.md` on the input
path), its DLL probes and the play tests recorded in `bfo:docs/plans/opfor/STOCK_MP_AI_PLAN.md` and `private:opfor-research/DEVELOPMENT_NOTES.md`. Nothing here was
run in game: every exe rule is **observed** (read in the code) or **inferred** (reasoned from it), never verified, until
a play test with OPFOR's probes records it. Data values are DICE's shared AI partitions (`AI/*` and the soldier
templates of `sp_common`), quoted as structure and ranges. Round AIS-rest (2026-11-22) read the rest statically with
the same scratch Ghidra copy and capstone / pefile scans of the executable (six probe runs, no game launch): the AI
body's interfaces and who writes them (BOT-CTL-6 to BOT-CTL-8), bots as scored and named players (BOT-SCO-1 to
BOT-SCO-4), the mode entities' run-time state (BOT-GS-3, BOT-GS-4), the vehicle controls (BOT-VEH-5), the session's
AI difficulty (BOT-DIF-4) and the order runtime (AIO-RT-1). Round SPG2-rest3 (2026-11-22) read the mode entities
the same way (the corrected classes of BOT-GS-4, tickets BOT-GS-6, the death and revive messages BOT-GS-7, Rush lives
and stages BOT-GS-8, the M-COM's interaction BOT-GS-9) with a game-tier probe of DICE's event names against the
executable's hashes (`spec/probes/modes.py`), and found the bot object (BOT-RT-6).

## 1. The runtime

### BOT-RT-1: The AI's singletons

Status: **observed** · Variants: PC

The AI's set-up (0x5B4840, from the AI init chain 0x5B4B20) creates the AI's global objects: the settings holder
`*(0x018903EC)` (0x4A8 bytes, constructor 0x5B24F0; +0x498 settings, +0x4A0 orders, +0x4A4 difficulty, AI-START-2), the
bot list `*(0x018903A8)` (0x9C bytes, 0x5A7D30), the debug draw `*(0x0189427C)` (0x5B0680), the AI manager
`*(0x0189037C)` (0x160 bytes, 0x5AB820; its +0x130 is the physics-query helper of AI-KY-4) and three event listeners
(message channels 0x13, 0x1E and 0x22). It ends by running the per-player hook (BOT-RT-5) with event 6 for every player
already in the game.

[exe: BFBC2Game.exe AI set-up 0x5B4840 (allocations 0x4A8 / 0x9C / 0x24 / 0x160, globals 0x018903EC, 0x018903A8, 0x0189427C, 0x0189037C; listener registration through 0x547BF0 +0x18 vtable +8); per-player loop over 0x532270 (+0x58) calling 0x5AB6C0(6, player)]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:208-221]

### BOT-RT-2: The server's AI frame

Status: **observed** · Variants: PC

Each server frame calls the AI update 0x59E190 (OPFOR probe "AI: update (per frame)"). Unless its re-entry flag is set it
reads `Server.IsAiEnabled` and, when true, runs: 0x599E70 (for every player proxy of the bot list: 0x594E50, 0x58DDE0),
the AI step 0xEBF3B0 (BOT-RT-3) with the frame's time, and 0x59A040 (deletes the proxies queued for deletion). Then it
runs 0x595190 for every entry of the bot list (posting the proxies' pending messages on channel 0x1E, types 0x14 / 0x15)
and 0xEBF460. Everything AI happens on the server's thread, inside this call.

[exe: BFBC2Game.exe AI update 0x59E190 (thiscall(this, dt)): `this+0x18` skip flag, `this+0x19` busy flag, setting Server.IsAiEnabled via 0x4F4C40; callees 0x599E70, 0xEBF3B0, 0x59A040, 0x595190, 0xEBF460]
[doc: opfor:src/dll/probes.cpp:128]

### BOT-RT-3: The AI step's subsystems, in order

Status: **observed** · Variants: PC

The AI step 0xEBF3B0 runs once per new AI time (the clock `*(0x018903C8)` vtable +4 compared with the last) and calls,
in this order: the clock's own update (`*(0x018903B8)` slot 0), 0xEAA1F0, 0xEFD2E0 (a list of time-sliced updates
sharing the frame's budget), 0xE6DCE0 (the bot scheduler, BOT-RT-4), 0xEBC4F0, 0xF2CDA0 (the Kynapse world: the bots'
Kynapse entities, then the world update 0x109D970 with the frame time), 0xEB7270 (expires remembered events older than a
settings time), 0xEBF120 and 0xEBD0A0 (updates the AI's tracked positions of every player's soldier). The roles beyond
the named calls are inferred from what each reads.

[exe: BFBC2Game.exe AI step 0xEBF3B0 (thiscall(double* last, dt)): 0xEAA1F0, 0xEFD2E0, 0xE6DCE0, 0xEBC4F0, 0xF2CDA0(dt), 0xEB7270, 0xEBF120, 0xEBD0A0; Kynapse update 0xF2CDA0 -> 0x109D970(world +0x10, dt)]

### BOT-RT-4: Sensing, decisions and behaviours run time-sliced per bot

Status: **observed** · Variants: PC

The bot scheduler 0xE6DCE0 runs, for the bots due this frame, three passes in order, each a priority-ordered list under
its own time budget: sensing (0xE6D660, list "SensingPriorities"), decisions (0xE6D8B0, "DecisionPriorities") and
behaviours (0xE6DA70, "BehaviourPriorities"); between them 0xE69750, 0xF02070, 0xE52CF0, after them 0xF02080. A bot
skipped this frame keeps its last decision. The budgets come from the AI settings' timing constants (`AI/Settings
TimingConstants`: `UpdateTimePerFrame` 7 and the parts Pathfinding 0.2, Traversals 0.2, AreaDistribution 0.02,
DynamicObjectEnvironment 0.01, Sensing 0.1, Evaluation 0.05, GoalUpdates 0.01, BehaviourSelection 0.1,
BehaviourExecution 0.05, SquadUpdate 0.2); how often a bot is due is set by four `BotPriorityConfigData` bands (angle
offset 15-180, distance 0-125 m, period 1-4 frames, each applying to sensing, decisions and execution): a bot near and
in front of a human is updated every frame, a far one every fourth (inferred from the band values).

[exe: BFBC2Game.exe scheduler 0xE6DCE0; passes 0xE6D660 / 0xE6D8B0 / 0xE6DA70 with their list names (strings 0x014A5278, 0x014A52A0), budgets read from [[0x018903E8]+0x18] +0x40 / +0x28 / +0x30]
[data: AI/Settings TimingConstantData and 4 BotPriorityConfigData (ai_probe)]

### BOT-RT-5: Every player has an AI proxy; a human's is a Kynapse obstacle

Status: **observed** · Variants: PC

The per-player hook 0x5AB6C0(event, player) keeps one AI proxy per server player, in an array by player id
(player +0x154): when the player has a live soldier (vtable +0x30 non-null, 0x57D2F0 false, 0x531850 non-null) it
creates the proxy (0xC0 bytes, vtable 0x01419AA0, constructor 0x5AB5A0, player at +0x70) if missing; when not, it
deletes it. The proxy owns an AI input object (BOT-CTL-2) at +0xC. For a human player (player +0xD84 == 0) it also
registers a "KynapseHuman" entity (0xF21030) in the Kynapse world, so bots path around and perceive humans as Kynapse
entities. Events 3, 4 and 5 never create a proxy (inferred: leaving, dying).

[exe: BFBC2Game.exe player hook 0x5AB6C0 (array at this+4 indexed by player[0x55]); proxy constructor 0x5AB5A0; proxy vtable 0x01419AA0 (slot 1 player vfunc +0x1C, slot 2 player id, slot 3 player +0xD84, slot 14 / 15 transforms); KynapseHuman registration 0xF21030]

### BOT-RT-6: The bot object

Status: **observed** · Variants: PC

A bot's debug print 0xEEF450 shows the bot (brain) object's main fields: +0x14 the bot id, +0x88 the readiness holder
(+0x40 the current readiness), +0x8C a weak pointer to the bot's AI proxy (BOT-RT-5: the pointer's target minus 4 is the
proxy, vtable 0x01419AA0; every behaviour reaches the body this way, BOT-CTL-6), +0x90 the squad member (+0x18 the
squad, whose +0x20 is the squad id; +0x1C set for the squad leader), +0x98 the name. The behaviours also read +0x84 (the
current order and goal state: +0x60 / +0x88 the target, weak pointers) and +0x9C (the behaviour template: +0x4C
`ReloadBelowPercent`, +0x70 a probability BehaviourFire tests against a random number). Its Kynapse side is a bot entity
named `Entity_<soldier>` with a brain, created in 0xF27900 from the Kynapse bot description and kept at +0x1C of the
AI's Kynapse body. 0xEEF450 has no caller, no jump and no pointer to it in the image (a debug function left
unreferenced), and 0xF27900 is reached only through the Kynapse world's table (0x014A8DCC).

The bot itself (found by round SPG2-rest3 from its constructor's stores) is a plain 0xF4-byte structure with no vtable,
constructor 0xE6C0B0(index). The bot manager (0xB24 bytes, global `*(0x01890324)`, made by 0xE6C690 from 0xED7C20,
constructor 0xE6C480) makes all of them up front: one per slot up to the count the AI settings holder's vtable +4
gives (`*(0x018903EC)`, BOT-RT-1), each kept as a 12-byte entry {bot, 0, flags} of a vector; +0x14 is the slot index.
The constructor zeroes the weak pointers and allocates the bot's parts:

| Offset | Size | Constructor | What |
|---|---|---|---|
| +0x00 | 0xC | 0xE4BB70 | |
| +0x6C | 0x1430 | 0xE8E3E0 (+4 = the bot) | the senses: registers the five sensing tasks 0xE8C370, 0xE8CAA0, 0xE8CE80 (`testVisionLines`, BOT-PER-4), 0xE8D130, 0xE8D680, each with weight 1.0, through 0xE87BB0; a 0x1000-byte record block at +0xB0 - +0x10B0 (the target records, inferred) |
| +0x74 / +0x70 | 0x1C | 0xE7F430, vtable 0x014A45D8 | |
| +0x7C / +0x78 | 4 | vtable 0x014A4618 | |
| +0x80 | 0x80 | 0xE8A0F0 | |
| +0x84 | 0x160 | 0xE8DFE0 | the order and goal state (above) |
| +0x88 | 0xB0 | 0xE85650 | the readiness holder (above) |
| +0x94 | 0x40 | 0xE6FD30 | |
| +0xA0 | 0x38 | 0xE82920 | |
| +0xA4 | 0x50 | 0xE855C0 | |
| +0xA8 / +0xAC | 0x48 / 0x58 | 0xE8B060 / 0xE8B260 | |
| +0xB0 | 0x48 | 0xE81800 | |

The proxy (+0x8C), squad member (+0x90), name (+0x98) and template (+0x9C) are set when a bot takes a soldier, not
here. The other parts' roles are not named.

[exe: BFBC2Game.exe bot debug print 0xEEF450 (strings " Bot id: " 0x014A7E64, "Squad: ", " - Squad leader" 0x014A7E50, "Readiness: " 0x014A7E44, "Position: "); Kynapse bot entity 0xF27900 ("Entity_", thunk 0x109EAC0, registration 0x109DFF0); the bot +0x8C reads of BehaviourReload 0xEC6650 (target - 4, then +0xC = the AI input object), BehaviourChangePose 0xEC4EE0 / 0xEC4F60, BehaviourFire 0xEC4FE0 (+0x84 +0x88), BehaviourReload 0xEC64E0 (+0x9C +0x4C); no reference to 0xEEF450 in the image (AIS-rest byte scan for E8 / E9 targets and 32-bit pointers)]
[exe: BFBC2Game.exe bot constructor 0xE6C0B0 (`param_1[5] = index`; allocations 0xB0 / 0x160 / 0x80 / 0x1C / 4 / 0x1430 / 0x38 / 0x50 / 0x48 / 0x48 / 0x58 / 0xC / 0x40 into +0x88 / +0x84 / +0x80 / +0x74 / +0x7C / +0x6C / +0xA0 / +0xA4 / +0xB0 / +0xA8 / +0xAC / +0x00 / +0x94; +0x8C, +0x98, +0x9C zeroed); callers 0xE6C480 (`push 0xF4` per slot, count from `[0x018903EC]` vtable +4) and 0xE6C6C0 (from 0xF10A30); 0xE6C690 (`push 0xB24`, global 0x01890324) from 0xED7C20; senses 0xE8E3E0 (+4 = bot; 0xE87BB0 x 5 with 1.0)]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:202-203]

### BOT-RT-7: Goals, behaviours, plans and squad behaviours

Status: **observed** · Variants: PC

DICE's decision layer is four registries of named creators (each record 0x18 bytes: the name, the creator): 32 goals
(`GoalAttack`, `GoalAltAttack`, `GoalAimAt`, `GoalAirAttack`, `GoalAvoid`, `GoalChange`, `GoalChangePose`, `GoalChase`,
`GoalEvasive`, `GoalFindAttackPosition`, `GoalFireAtArea`, `GoalHide`, `GoalIdle`, `GoalInterlude`,
`GoalInvestigateEnemy` / `Event`, `GoalMoveInCombat`, `GoalMoveInFormation`, `GoalMoveOutOfCover`, `GoalMoveTo`,
`GoalMoveToCoverPosition`, `GoalMoveToRestrictedArea`, `GoalPilot`, `GoalReload`, `GoalSearchAndDestroy`,
`GoalShootAtObject`, `GoalStationaryAttack`, `GoalStunned`, `GoalSuppress`, `GoalTeleport`, `GoalTest`,
`GoalVOAnimation`); 30 behaviours (the primitive actions: `Fire`, `FireAtArea`, `Suppress`, `MoveTo`, `MoveToValid`,
`Chase`, `Melee`, `Reload`, `SwitchWeapons`, `Look`, `LookAround`, `LookAtSpecified`, `ChangePose`, `AimInCover`,
`AlignGunner`, `TurnGunner`, `FlyTo`, `Teleport`, `Stunned`, `StandStill`, `Pause`, `Idle`, `IdleAnimation`,
`Interlude`, `ScriptedAnimation`, `SpecialMovement`, `PreSpecialMovement`, `ShootAtObject`, `Change`, `Test`); plans
(`Fire`, `AttackSuppress`, `Chase`, `IdleGround`, `IdleAir`, `MoveToGround`, `MoveToInfantry`,
`MoveToInfantrySpecial`); and 10 squad behaviours (`Combat`, `AirCombat`, `Defend`, `SearchAndDestroy`,
`MoveInFormation`, `RandomWalk`, `Idle`, `Interlude`, `ShootAtObject`, `Change`). A goal is what a soldier wants
(chosen by the decision pass), a behaviour what it does this frame (the behaviour pass); squad behaviours sit above both
(AIO-RDY-2).

[exe: BFBC2Game.exe creator registries: goals 0x01C08AB4..0x01C08D0C and 0x01C09BAC..0x01C09C3C (IGoal::IGoalCreator), behaviours 0x01C098C4..0x01C09B94 (IBehaviour::IBehaviourCreator), squad behaviours 0x01C0A4A4..0x01C0A4BC and 0x01C0AA0C..0x01C0AACC (ISquadBehaviour::ISquadBehaviourCreator)]
[data: AI/Behaviours AIVehicleBehaviourData Behaviours.Plan values (ai_probe)]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:192-201]

### BOT-RT-9: The registries' creators and the behaviour objects

Status: **observed** · Variants: PC

Each registry entry is registered at start-up by `0x500660(creator object, type record, parent record, create
function)`. The create functions sit in three runs: behaviours 0xED5160-0xED5500 (`BehaviourAimInCover` 0xED5160 to
`BehaviourTurnGunner` 0xED5500, 0x20 apart in name order), goals 0xED5520-0xED55E0 (`GoalAimAt` to `GoalChangePose`)
and 0xE66600-0xE674F0 (`GoalChase` to `GoalVOAnimation`), plans 0xE67530-0xE67760, squad behaviours 0xF281E0-0xF282C0
and 0xEF34F0 / 0xEF3510; the six vehicle controls of BOT-VEH-1 are registered the same way (`SoldierControl` 0xE88E60,
`CarMovementControl` 0xE68060, `TankMovementControl` 0xE88EB0, `TurretControl` 0xE88F00, `HelicopterControl`
0xE88DC0, `NoControl` 0xE88E10). Each creator object's vtable slot 4 allocates the run-time object; for behaviours
that object's vtable holds the per-behaviour entry points (e.g. `ChangePose` instance vtable 0x014A6848: slot 5
0xEC4EE0 can-start, slot 10 0xECFFF0 update, slot 14 0xEC4F60 start; `Reload` 0x014A6B58: slot 5 0xEC64E0, slot 10
0xEC6650; `Fire` 0x014A70A0: slot 5 0xEC4FE0, slot 6 0xECD970 score, slot 13 0xECAA90; `SwitchWeapons` 0x014A6DC8:
slot 5 0xED66E0, slot 10 0xEC75A0). The control objects' vtables: `SoldierControl` 0x014A560C (slot 4 = the update
0xE84CE0), `CarMovementControl` 0x014A48FC, `TankMovementControl` 0x014A5640, `TurretControl` 0x014A567C,
`HelicopterControl` 0x014A5738 (slot 4 each, BOT-VEH-5). A behaviour's +4 is its bot (BOT-RT-6).

[exe: BFBC2Game.exe registration calls `push 0; push create; push parent; push record; mov ecx, creator; call 0x500660` (86 found by a byte scan of the type records whose name holds " :: "), e.g. 0x137CB00 for BehaviourChangePose; creator vtable 0x014A7398 slot 4 0xED27A0 (alloc 0x20, instance vtable 0x014A6848); SoldierControl creator vtable 0x014A590C slot 4 0xE87390 -> constructor 0xE775D0 (instance vtable 0x014A560C, slot 4 0xE84CE0)]

### BOT-RT-8: Urgencies choose the goal

Status: **inferred** · Variants: PC

`AI/Behaviours` holds 44 `BehaviourData` records whose `UrgencyModifiers` (goal name, 0 to 3) weigh the goals: e.g.
`MoveToRestrictedArea` 3, `Suppress` 2.5, `FireAtArea` 2, `MoveOutOfCover` 1.3, `ChangePose` 1.2, `Idle` 0.01. With the
order's and squad's modifiers (AIO-RDY-1, AIO-RDY-2) and the template's `AttackUrgencyModifier` (0-0.1), the decision
pass runs the goal of highest urgency among those the bot's equipment type allows (BOT-VEH-1). The summation and its
tie rules are not traced.

[inferred: the data's shape (urgency weights per goal name, Idle at 0.01 everywhere) and the decision pass of BOT-RT-4; no formula read]
[data: AI/Behaviours: 44 BehaviourData, UrgencyModifiers 180 entries, modifier 0..3 (ai_probe)]

## 2. The control boundary

### BOT-CTL-1: An AI soldier belongs to a server player with the AI flag

Status: **observed** · Variants: PC

A bot is a full server player. An AI spawner's spawn (0x72D160, for spawner data with +0x108 == 1, inferred
`SSAiPlayer`) creates a server player (0x573750), sets its team (0x54F2A0 from the spawner +0xBC), sets the player's AI
flag (0x532190(1): the byte +0xD84; the player constructor 0x572140 clears it) and spawns the soldier (0x70BBC0, with
the spawner's template and kit). The flag is read by the kill handler (0x86DC90, 12 tests: AI-NEED-5), by the per-frame
soldier update that holds the aim-assist gate (0x579DC0), by the player-info serialiser 0x5486D0 and by the seat entry
0x5725E0; more than 60 functions read it in all. What it changes for names and scores is BOT-SCO-1 to BOT-SCO-4.

[exe: BFBC2Game.exe spawner spawn 0x72D160 (player 0x573750, team 0x54F2A0, AI flag 0x532190 -> mov [ecx+0xD84], soldier 0x70BBC0); player constructor 0x572140 (0x5723C4 clears +0xD84); readers 0x86DC90, 0x579DC0, 0x5486D0, 0x5725E0, 0x697CF0; AIS-rest's Ghidra scalar search for 0xD84: 109 byte reads in more than 60 functions]

### BOT-CTL-2: The AI drives its soldier through an input handler, like a remote human

Status: **observed** · Variants: PC

Each AI proxy owns an AI input object (0x1C8 bytes, vtable 0x014198A8, constructor 0x5A7F00(player)). It holds an
`EntryInputState` at +0x28 and timestamps per input at +4. When the player has no input handler (ServerPlayer +0xC38
null, which is the case for an AI player: a human's comes from its client) it creates one (0x623390, 0x1A8 bytes),
installs it (0x531C20) and pushes its state into it (0x6234B0). Each frame the object's slot 7 (0x586F30) copies its
state and pushes it with 0x6234B0. From there the server treats the bot exactly like a remote player: the
controllable-entity update 0x695ED0 reads the seated player's handler and applies it per entry (soldier 0x6B0780,
vehicle 0x697B90). So DICE's AI controls a soldier through the same `EntryInputState` a human's client sends.

[exe: BFBC2Game.exe AI input object 0x5A7F00 (handler 0x623390 / 0x531C20, push 0x6234B0(this+0x28)); vtable 0x014198A8 slot 7 0x586F30 (0x57DB70 copy, 0x612B60 time, 0x6234B0 push)]
[doc: private:opfor-research/research/WW2_LANDING_GEAR_RE2_2026-11-14.md:104-115] [doc: private:opfor-research/research/WW2_LANDING_GEAR_RE2_2026-11-14.md:160-195]

### BOT-CTL-3: The AI input object's actions

Status: **observed** · Variants: PC

The AI writes its input through the object's virtual interface (vtable 0x014198A8):

| Slot | Function | Does |
|---|---|---|
| 1 (+0x04) | 0x586CE0 | clears every input (EIA 0-48) |
| 2 (+0x08) | 0x586EE0 | set a digital action (`0x612A00` on the state), time-stamp it |
| 3 (+0x0C) | 0x586DC0 | set an analog action's level (0x612810 / 0x61C7E0), time-stamp it |
| 4 (+0x10) | 0x586E30 | aim: wraps the yaw, clamps the pitch, and sets both directly on the soldier (player +0x24 vtable +0x30, then 0x6A6460) - not through the input state |
| 5, 6, 10, 11, 12, 13 | 0x57DC40, 0x57DB50, 0x57DCA0, 0x57DCC0, 0x57DC60, 0x57DC80 | read back an action (is down, level) from the state or the handler |
| 7 (+0x1C) | 0x586F30 | push the frame (BOT-CTL-2) |
| 8, 9 | 0x586FA0, 0x586FD0 | was an action set since the last snapshot; take a snapshot |
| 14 (+0x38) | 0x57DCE0 | sprint: `EiaSprint` (31) through 0xAF81F0 |
| 15-19 | 0x58DF80, 0x58E050, 0x58E090, 0x58E100, 0x57DD20 | soldier and weapon calls on the player's soldier (0x58DF80 a move target to the soldier's +0x120 component; 0x58E090 / 0x58E100 the weapon component 0xACC460 on and off; 0x57DD20 a weapon slot 0x612D10) |

The action number the AI uses is not the `EntryInputAction` itself but an index into a 13-entry table at 0x01505158:
0 `EiaThrottle`, 1 `EiaStrafe`, 2 `EiaBrake`, 3 `EiaYaw`, 4 `EiaPitch`, 5 `EiaRoll`, 6 `EiaFire`, 7 `EiaAltFire`,
8 `EiaZoom`, 9 `EiaJump`, 10 `EiaInteract`, 11 `EiaReload`, 12 `EiaUndefined` (the enumeration's values are read from
its field table at 0x01C27C58: Throttle 0 ... Reload 29, Sprint 31, Undefined 49). There is no entry for
`EiaChangePose`, `EiaMeleeAttack`, `EiaThrowGrenade`, the gadget actions (33-36), `EiaChangeVehicle` /
`EiaChangeEntry` / `EiaSelectEntry*` or the weapon-slot selection: DICE's AI does those directly on the soldier (slots
15-19, and other calls not yet traced), not through input.

[exe: BFBC2Game.exe vtable 0x014198A8 slots 0-19 (Ghidra vtable listing); action table 0x01505158 (u32 x 13: 0, 1, 2, 4, 5, 6, 8, 12, 14, 15, 27, 29, 49); EntryInputActionEnum field table 0x01C27C58 (records of 0x18 bytes, name at +0, value at +0x18)]

### BOT-CTL-4: Two ways to plug a bot module in

Status: **inferred** · Variants: PC

A bot module can take control at one of two levels:

1. **Drive DICE's AI**, as the campaign does: author orders, spawners and templates in the level data (AIO-*, the
   suite's stock-MP layer XSMP-AUT-1), issue orders through the level's events, tune through the AI partitions. The AI
   then perceives, decides, paths (Kynapse) and acts itself. This is what Napisal's and OPFOR's Onslaught ports and the
   campaign show working (AI-NEED-2).
2. **Replace the decisions, keep the body**: spawn the bot as DICE does (an AI spawner, so the engine makes the server
   player, the soldier and the AI input object of BOT-CTL-2), then write that input object's `EntryInputState` each
   frame from OPFOR's own logic - movement as `EiaThrottle` / `EiaStrafe` levels, `EiaFire`, `EiaJump`, `EiaReload`,
   `EiaZoom`, `EiaSprint` and also the actions DICE's AI never sends (`EiaChangePose`, `EiaThrowGrenade`, the gadgets,
   `EiaChangeVehicle`), which the server reads as from any remote player - and set the aim with slot 4. DICE's own
   decisions must then be kept off that bot (no order for its spawner; a behaviour that never writes, or the slot-7 push
   replaced), or the two would write the same state.

Path finding stays Kynapse's in both (AI-KY-3, KYRT-*), or OPFOR's own on the same path data.

[inferred: from BOT-CTL-2 (the server applies a handler's state whatever filled it), BOT-CTL-3 (what DICE's AI writes and what it does not) and the input path of the landing-gear study (every digital EIA a client sends reaches the server)]
[doc: private:opfor-research/research/WW2_LANDING_GEAR_RE2_2026-11-14.md:244-247] [doc: bfo:docs/plans/opfor/STOCK_MP_AI_PLAN.md:265-268]

### BOT-CTL-5: Bots are the server's; the in-process client sees replicated soldiers

Status: **inferred** · Variants: PC

The AI starts only on the server (AI-START-1) and runs inside the server's frame (BOT-RT-2). In an offline match
(`hostMultiplayerLocal`, NET-SES-2, NET-SES-3) server and client share the process but not the thread; the bot's
soldier reaches the local client as any remote player's does, through the in-memory transport, and the client needs no
bot code. A bot module therefore runs on the server side: its reads (soldiers, capture points, the physics queries of
AI-KY-4) and writes (the AI input object) belong to the server thread, inside or right after 0x59E190; a client-thread
hook must hand data across (as the landing-gear study proposes for the height cache).

[inferred: from AI-START-1, BOT-RT-2, NET-SES-2..NET-SES-4 and the server-only creation of AI players (BOT-CTL-1); the replication of an AI player's soldier to the local client was seen in every Onslaught play test (bots visible, fighting) but its code path was not read]
[doc: private:opfor-research/research/WW2_LANDING_GEAR_RE2_2026-11-14.md:244-247]

### BOT-CTL-6: The AI body: the proxy and its three interfaces

Status: **observed** · Variants: PC

The AI controls a soldier through its player's proxy (BOT-RT-5, 0xC0 bytes, vtable 0x01419AA0), which the bot reaches
through its +0x8C (BOT-RT-6). The proxy holds:

| Offset | What | Used for |
|---|---|---|
| +0x04 | the body interface (zeroed by the base constructor 0xE7B320): slot 1 the world matrix (translation +0x30), slot 4 (+0x10) the soldier is usable, its +0x14 the current weapon | position, alive and weapon tests in every behaviour |
| +0x08 | the soldier interface (0xC bytes, vtable 0x014197B8, player at +4): direct calls on the player's soldier (type record 0x01BF2390) | stance, aim state, soldier queries (BOT-CTL-8) |
| +0x0C | the AI input object (BOT-CTL-2, vtable 0x014198A8) | input actions, aim, push (BOT-CTL-3, BOT-CTL-7) |
| +0x1C | a pending request posted on channel 0x1E as message 0x14 (set by proxy slot 24, 0x599DB0) | inferred: a kit or spawn request |
| +0x20 | a flag posted as message 0x15 with the player (proxy slot 25, 0x57DAF0) | not traced |
| +0x24 | a pending seat-entry request (weak pointer to an entry descriptor) | vehicle entry (BOT-CTL-8) |
| +0x28 | a pending seat-exit flag (proxy slot 23, 0x57DAD0) | vehicle exit (BOT-CTL-8) |
| +0x30 | the player's controllable's world matrix, refreshed each frame by 0x594E50 | |
| +0x70 | the server player | |

The proxy's own vtable is the AI's view of the player: slot 1 the name (the player's slot 7, 0x531BF0, BOT-SCO-4),
slot 2 the player id (+0x154), slot 3 the AI flag, slot 10 the team (+0xA8), slot 29 the player. The requests at
+0x1C to +0x28 are carried out once a frame by 0x595190 on the server thread (BOT-RT-2).

[exe: BFBC2Game.exe proxy constructor 0x5AB5A0 (+0xC = new 0x5A7F00(player), +8 = 0xC-byte object vtable 0x014197B8 with +4 = player, base 0xE7B320 zeroes +4..+0x18); proxy vtable 0x01419AA0 slots 1-29 (slot 1 0x57D770 -> player vtable +0x1C = 0x531BF0 `lea eax,[ecx+0xC2C]`); request pump 0x595190 (+0x1C -> channel 0x1E message 0x14, +0x20 -> message 0x15, +0x24 -> 0x58DEC0, +0x28 -> 0x56D480(1)); 0x594E50 copies the matrix to +0x30]

### BOT-CTL-7: Who writes which input slot

Status: **observed** · Variants: PC

A capstone scan of the AI's code (0xE40000-0xF30000) for calls through the input object (reached as bot +0x8C, target
minus 4, +0xC; or through a control's local copy of the proxy's three pointers, read at +8 / +0xC) and the decompiled
callers
give this map of writes (action numbers are BOT-CTL-3's table indices):

| Writer | Slot (action) | What |
|---|---|---|
| the AI step's frame hook 0xEBF470 -> 0xE6A8C0, once per AI time | 7 (push, dt), 1 (clear) | every bot's frame is pushed, then cleared for the next |
| `BehaviourReload` update 0xEC6650; 0xE51D30 (from 0xE6B6A0) | 2 (11 `EiaReload`), toggled each frame | press and release, so the weapon sees an edge |
| the named script actions "Fire" 0xEC8380 and "SSFire" 0xEC83C0 (table built in 0xED68D0) | 2 (6 `EiaFire`, on) | fire from scripted animation and interludes |
| `SoldierControl` update 0xE84CE0 -> 0xE80E20 / 0xE81390 / 0xE811C0 | 15 (move target), 14 (sprint), 19 (weapon slot 0), 4 (aim) | infantry movement, sprint, aim |
| 0xE56E30 (from 0xE6B6A0) | 19 (1: the last weapon slot) | switching to the secondary weapon |
| `SoldierControl`'s sub-object +0xB0 slot 2, 0xE80D60 | 16 (a value) | the soldier's own aim code 0x6ABC30 |
| `CarMovementControl` 0xE53130, `TankMovementControl` 0xE78480 | 3 (analog 3 `EiaYaw`, 2 `EiaBrake`, 0 `EiaThrottle`) | driving (BOT-VEH-5) |
| `TurretControl` 0xE78A50 | 3 (analog 5 `EiaRoll`, 4 `EiaPitch`) | turret aim |
| `HelicopterControl` 0xE91040 -> 0xE8EB30, 0xE8ED90, 0xE90830 (0xE8F0B0, 0xE8F250) | 3 (0 throttle, 2 brake, 3 yaw, 4 pitch, 5 roll) | flying |

No site was found that writes actions 1 (`EiaStrafe`), 7 (`EiaAltFire`), 8 (`EiaZoom`), 9 (`EiaJump`) or 10
(`EiaInteract`), and `EiaFire` only from the scripted actions (the combat fire site is RE item 8); infantry does not
walk by `EiaThrottle` / `EiaStrafe` either, but by the move target of slot 15, which hands a position to the soldier's
movement component (+0x120 vtable +0x114). The scan follows only calls whose object is loaded from a +0xC field in the
same few instructions, so a write through a pointer kept elsewhere would be missed: that the AI never sends these five
actions is inferred, and item 1 of the RE list tests it.

[exe: BFBC2Game.exe 0xEBF470 (dt from the AI clock `*(0x018903C8)` vtable +4) -> 0xE6A8C0 (input slot 7 at 0xE6A914 / 0xE6A97F, slot 1 at 0xE6A954); 0xEC6650 `push edx; push 0xB; call [vtable+8]` at 0xEC6691; 0xE51D30 at 0xE51DF5 / 0xE51E0B; 0xEC8380 / 0xEC83C0 `push 1; push 6` (0xEC83A5, 0xEC83F3); the "Fire" / "SSFire" keys at 0xED73B4 / 0xED74EC; 0xE84CE0 (vtable 0x014A560C slot 4) calls 0xE80E20 (0xE84D83), 0xE81390 (0xE84DB8), 0xE811C0 (0xE84E23); slot 15 at 0xE810F4 / 0xE81483, slot 14 at 0xE8145F / 0xE81555, slot 19 at 0xE81561 (push 0) and 0xE56F56 (push 1), slot 4 at 0xE81377; 0xE80D60 (vtable 0x014A55F8) slot 16; analog writes 0xE53440 / 0xE53475 (3), 0xE535A1 (2), 0xE535B4 (0); 0xE787D0 / 0xE78805 (3), 0xE7895F (2), 0xE78972 (0); 0xE78DC5 (5), 0xE78F96 (4); 0xE8ED69 (0), 0xE8ED7C (2), 0xE8F097 (3), 0xE8F234 (4), 0xE8F3A5 (5); slot 15 0x58DF80 -> soldier +0x120 vtable +0x114]
[inferred: the scan's pattern (object from a +0xC field, then its vtable, then the slot) covers the sites listed; actions 1 and 7-10 have no site under it]

### BOT-CTL-8: Stance, weapon slot, sprint, vehicle weapons and seats are set directly

Status: **observed** · Variants: PC

Besides the input state, the AI changes its soldier through direct calls:

- **Stance.** `BehaviourChangePose` starts by calling the soldier interface's slot 20 (0x58DAB0) with the wanted pose:
  1 calls the soldier's 0x69F5B0(1), any other value 0x69F5B0(0); slot 10 (0x5944F0) maps a pose from data (+0x38: 0, 1,
  else 3) onto slot 20. Its update reads the current and the wanted pose back through slots 21 / 22 (the soldier's +0xC0
  component, vtable +0x10 / +0x14) and finishes when they agree. Stance is never `EiaChangePose`.
- **Aim state.** Slots 16 and 17 (0x5998A0, 0x58D920) set bits of a 16-bit state word on the soldier (read 0x68E620,
  written 0x68E5E0) from a mode (1, 2, 3) and a flag; slot 15 (0x58D8C0) one bit, slot 19 (0x58DA40) two more. Which
  bit is aiming, readiness or lowered weapon is inferred from the callers, not named.
- **Sprint.** Input slot 14 (0x57DCE0) calls the soldier's 0xAF81F0 and sets `EiaSprint` (31) on the state.
- **Weapon slot.** Input slot 19 (0x57DD20) selects through 0x612D10 either slot 0 (argument 0) or the soldier's last
  slot (0x69E8D0 minus 1, argument 1); there is no per-slot choice.
- **Vehicle weapons.** Input slots 17 / 18 (0x58E090 / 0x58E100), for a player in a vehicle (type record 0x01BF2400):
  the seat's weapon component gets a target (0xAD1960) and is switched on (0xACC460(1)) or off (0xACC460(0)).
- **Seats.** A pending entry request at proxy +0x24 is carried out by 0x58DEC0: the entry descriptor gives the
  vehicle and seat, the vehicle's vtable +0x104 decides whether the player may take it, then
  `ServerPlayer::enterEntry` 0x5725E0(player, vehicle, seat) seats it. The exit flag at proxy +0x28 calls 0x56D480(1),
  the player's seat exit (message 0x10 with the player's vehicle +0xC6C and seat +0xC68). The seat test 0x697CF0
  lets a human take a seat an AI player holds (and 0x5725E0 then makes the AI leave first, 0x572634), never an AI a
  human's.
- **Grenades and gadgets.** No direct call was found either: no AI code reaches the soldier's grenade or gadget
  functions through these interfaces (BOT-CMB-4 stands).

[exe: BFBC2Game.exe BehaviourChangePose start 0xEC4F60 (`[proxy+8]` vtable +0x50) -> 0x58DAB0 -> 0x69F5B0; update 0xECFFF0 (+0x54 / +0x58); 0x5944F0 -> own +0x50; 0x5998A0 / 0x58D920 / 0x58D8C0 / 0x58DA40 -> 0x68E620 / 0x68E5E0; input slot 14 0x57DCE0 (0xAF81F0, 0x612A00(0x1F)); slot 19 0x57DD20 (0x69E8D0, 0x612D10); slots 17 / 18 0x58E090 / 0x58E100 (type 0x01BF2400, 0x68ED00 +0x84 vtable +0xB4, 0xAD1960, 0xACC460); seat request 0x595190 -> 0x58DEC0 (0xE936D0 descriptor +4 vehicle, +8 seat; vtable +0x104; 0x5725E0 at 0x58DF19); exit 0x56D480 (0x54E8B0 message 0x10, +0xC68 / +0xC6C); 0x697CF0 tests at 0x697D8B / 0x697D94 / 0x697DF5; 0x5725E0 tests at 0x572613 / 0x57261C, exit call 0x572634]

## 3. Perception

### BOT-PER-1: Sensing templates: limits per readiness

Status: **observed** · Variants: PC

An AI template (`SoldierSpawnTemplateData`) names a `SensingTemplateData` and a `BehaviourTemplateData`. The sensing
template gives three sets of limits, used by readiness (`RelaxedLimits` = patrol, `ReadyLimits`, `CombatLimits`), each
with `HearingDistance` (0-100 m), `HearingCutoff` (0-0.85), `ViewAngle` (0-180°), `ViewDistance` (0-200 m),
`PeripheralViewAngle` (0-180°), `PeripheralViewDistance` (0-400 m) and `PeripheralMovementCutoff` (0-0.87); and the
template-wide `ViewAngle` (15-180°), `PeripheralViewAngle` (35-180°), `SeeingDistance` (1-200 m), `HearingDistance`
(0-75 m), `ReadyFOVMultiplier` (0-3.5), `CombatFOVMultiplier` (0-12), `SensingSkill` (0-1), `AwareForgetTime`
(5-60 s), `LostForgetTime` (10-120 s), `TimeUntilUnseenIsLost` (1 s), `MaximumMergeDistance`, `MaximumAlertDistance`
and `MaximumReadinessRaiseDistance` (0-128 m). The grunt template: a 15° central cone to 25 / 40 / 55 m by readiness,
peripheral 45 / 60 / 90° to 55 m, hearing 48 m, alerting others within 48 m. The placeholder `DefaultSoldier` template
(combat field of view 0, no alert range) gives bots that aim and never fire (AI-NEED-3).

[data: 14 SensingTemplateData of sp_common (ai_probe ranges); the grunt template's values]
[doc: bfo:docs/plans/opfor/STOCK_MP_AI_PLAN.md:41-43]

### BOT-PER-2: The sensing constants

Status: **observed** · Variants: PC

`AI/Settings SensingConstants`: `SensingTimeSpan` 1 s, `LineOfSightTestTime` 1 s, `MaxRays` 3, `VisionBoundingBoxSize`
0.65, `VisionHumanBoxOffset` 0.4, `FireKeepTime` 2 s, `AttackerTrackTime` 10 s, `CloseDistance` 2 m, impact and
explosion sensing (`ImpactSenseDistance` 12, `CloseFireImpactDistance` 5, `CloseExplosiveImpactDistance` 8,
`ExplosionInterestRange` 64), motion salience (`MaxSize` 3, `MaxSpeed` 12, size / speed / peripherality / distance
factors), smoke (`SmokeSphereRadius` 10 m for 12 s) and the out-of-sight spawn test (`SpawnOutOfLOS*`). Sound:
`SoundEnvironmentConstants` (`MinimumLoudness` 30, `MemoryTime` 0.5 s, a human soldier's movement volume 45).

[data: AI/Settings SensingConstantData and SoundEnvironmentConstantData (sp_common = mp_common)]

### BOT-PER-3: Line of sight is a ray query of the AI manager

Status: **observed** · Variants: PC

Vision tests go through the AI manager `*(0x0189037C)`: `Senses::isVisible(Pos)` (0xE82D70) and `isVisible(obj)`
(0xE82E30) build a ray set and call the manager's vtable +0x10 with a profiling name; the per-target tests
`Senses::testVisionLines` (0xE8CE80), `testPeripheralVisionLines` (0xE8D680) and `targetLineOfSight1` / `2` (0xE8A420,
0xE8BF10) first compare the manager's vtable +0x28 answer (a region id) for the bot's and the target's positions and
skip the rays when they differ. The manager's physics helper is AI-KY-4's; how the +0x10 query casts its rays (count up
to `MaxRays`, the box of `VisionBoundingBoxSize`) was not traced.

[exe: BFBC2Game.exe Senses::isVisible(Pos) 0xE82D70 (manager vtable +0x10), testVisionLines 0xE8CE80 (manager vtable +0x28 region test, then 0xE82F50); strings 0x014A5820..0x014A59B0]

### BOT-PER-4: Target memory

Status: **inferred** · Variants: PC

A bot keeps a record per known target with a state: in `testVisionLines` (0xE8CE80) a seen target becomes state 2 and
its position and time are stored; a target not seen in state 2 becomes 3 with the time. With the template's
`TimeUntilUnseenIsLost`, `AwareForgetTime` and `LostForgetTime` (BOT-PER-1) the states read as seen, lost, then
forgotten. `TargetEvaluationConstants` re-evaluates targets every `UpdateInterval` 0.5 s and keeps a target at least
`KeepTargetTime` 0.5 s.

[inferred: the state writes 2 / 3 in 0xE8CE80 against the template's forget times; the states' names are not in the executable]
[data: AI/Settings TargetEvaluationConstantData]

### BOT-PER-5: Target selection weights

Status: **observed** · Variants: PC

Choosing among targets uses: the behaviour template's `TargetDistanceFactor` (1-100), `TargetStrengthFactor` (0-50),
`TargetDangerFactor` (0.5-100), `TargetIsAttackingFactor` (0-20) and `TargetIsMarkedFactor` (0-10); the spawner's
`HumanTargetPreference` (-1 to 2) with `DefaultHumanTargetPreference` 1.2; the weapons' `Strengths` against Infantry,
LightArmour, HeavyArmour and Helicopter (0-10, `AI/Weapons`); `MaxBattleStrength` 10; and `TurnAgainstTargetDistance`
(10-64 m). How they combine is not read.

[data: BehaviourTemplateData (14), WeaponData (47), TargetEvaluationConstantData, SoldierSpawnEntityData (ai_probe)]

## 4. Combat

### BOT-CMB-1: The accuracy model's knobs

Status: **observed** · Variants: PC

A bot's shots are made inaccurate on purpose and grow accurate over time: per weapon (`AI/Weapons`, 47 `WeaponData`)
`StartAccuracyPercentage` and `FinalAccuracyPercentage` (0-1), `MinimumAccuracyBeforeFiring` (5-45),
`InnerInaccurateDistance` / `OuterInaccurateDistance` (0-5 / 0-10 m), `InaccuracyFollowsTarget`,
`UseInaccuracyAtCloseRange`, `AdditionalMinimumAccuracyAtMinRange`, a sweep (`SweepType` first time / target lost,
`SweepDistance`, `SweepTime`); per template `AccuracyModifier` (1) and `AccuracyIncreaseTime` (1-8 s); per difficulty
the start and final accuracy modifiers standing and crouching and `AccuracyIncreaseTimeModifier` (BOT-DIF-2);
`CombatConstants.CloseRangeAccuracyDistance` 12 m; `DebugConstants.InaccuracyEnabled` true. Inference: the AI aims at an
offset point that closes from the start to the final accuracy over the increase time.

[data: WeaponData, BehaviourTemplateData, DifficultyData, CombatConstantData, DebugConstantData (ai_probe)]

### BOT-CMB-2: Bursts, reloading, suppression, melee

Status: **observed** · Variants: PC

Bursts: template `MinAttackBursts` / `MaxAttackBursts` (1-99), weapon `BurstLimitNear` / `Far`, `MinBurstCoolDownTime`
/ `Max` (0-4 s), `LostTargetBurstCoolDownModifier`, `AimTransitionFireDelay`, `CombatConstants.PoseTransitionFireDelay`
0.25 s. Reload: template `ReloadBelowPercent` (0-50), weapon `ReloadBehavior` (`RbAnywhere` 32, `RbInCover` 11,
`RbStationary` 4) and extra reload time 0-5 s, `AimWhileReloading`. Suppression: weapon `CanSuppress`, template
`MaxSuppressTime` (6-200 s), `SuppressIndirectProbability`, `SuppressedTimeAdd`, combat constants `MaxSuppressDistance`
200 m and `SuppressMinLOFDistance`. Melee: template `MeleeStartDistance` 2.5 m, `MeleeDelay` 0.35 s, `MeleeDamage`
50-120, `MeleePushForce`, `MeleeAnimationId` 7 - the template, not the knife, sets a bot's melee damage (inferred from
the field). Alternate attacks (grenade, launcher): weapon `CanAltAttack` (6 of 47), template `AltAttackProbability`
(0-1), `RPGSkill` (`AccurateWithTracer`, `AlwaysAccurate`), `ExplosiveObjectChance`. The AI's weapons are clones of the player
weapons with their own data: every AI clone zooms while the button is held (WPN-ZOOM-2), so a bot that zooms holds
`EiaZoom` for as long as it aims through the sights.

[data: BehaviourTemplateData, WeaponData, CombatConstantData (ai_probe)]

### BOT-CMB-3: Firing and aiming reach the soldier two ways

Status: **observed** · Variants: PC

Firing is input: `EiaFire` / `EiaAltFire` / `EiaZoom` / `EiaReload` through the AI input object (BOT-CTL-3; the sites
found are BOT-CTL-7's, and the combat fire site is still open, RE item 8). Aiming is not: the object's slot 4 sets the
soldier's yaw and pitch directly (0x6A6460), turned at the rates of `SoldierControlConstants` (turning 90-150°/s, head
10-60°/s, `PitchSpeed` 60, `TurnAngularAcceleration` 720, `AimMinPrecisionTolerance` 0.1, `AimControlMode`
`AcmDependsOnReadinessAndAimState`). The aim-assist code (the gate at 0x57A3C6 in 0x579DC0, `Game.AutoAimEnabled`) is a
human player's, and no weapon firing, player's or AI clone's, has `UseAutoAiming` (WPN-AIM-1): a bot's accuracy is
BOT-CMB-1's model alone.

[exe: BFBC2Game.exe AI input aim 0x586E30 -> 0x6A6460 (other callers of 0x6A6460: the soldier's own aim code 0x6A6720, 0x6AAF60, 0x6ABC30)]
[data: SoldierControlConstantData (ai_probe)]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:494-500]

### BOT-CMB-4: DICE's AI uses no class gadgets

Status: **inferred** · Variants: PC

The AI input object has no action for `EiaStaticGadget`, `EiaDynamicGadget1` / `2`, `EiaGrenadeLauncher` or
`EiaThrowGrenade` (BOT-CTL-3), no goal or behaviour names a medic, engineer, recon or support action (revive, heal,
repair, resupply, mortar, motion sensor, tracer dart: BOT-RT-7), and the AI's weapon data has 47 weapons with alternate
attacks on 6. DICE's bots use their main weapon, an alternate attack and melee; class play (revive, repair, resupply,
spotting with gadgets, C4 on M-COMs) is OPFOR's to build.

[inferred: from the action table 0x01505158 and the four creator registries (BOT-RT-7) having no such entry]

### BOT-CMB-5: Cover

Status: **observed** · Variants: PC

Cover is searched at run time around the bot (`CoverConstants`: `SearchRadius` 25 m, 32 directions, 4 per time slice,
slots of 0.75 m every 1.5 m with 8 rays each, `BaseHeight` 0.85 and `StandHeight` 1.7 m, up to 500 covers in memory,
`CoverSelectionMethod` `CSMAgainstThreatPosition`, re-evaluated every second, destroyed covers regenerated) and from the
level's `Covers` (LevelData). Templates set `BaseTimeInCover` (1-8 s), `MinEnemyDistanceToUseCover` (8-120 m),
`StandProbability` / `CrouchProbability`, `RollOutOfCoverProbability` and `ExplosionEmergentTakeCoverProbability`. The
goals are `GoalMoveToCoverPosition`, `GoalMoveOutOfCover`, `GoalHide`.

[data: CoverConstantData, BehaviourTemplateData (ai_probe)]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:202-204]

## 5. Movement

### BOT-MOV-1: Kynapse's agents follow the path; DICE's code moves the soldier

Status: **observed** · Variants: PC

The Kynapse world (0xF1B980) is configured with the path-following stack: `Mpd::CPathFinder` (A*, `FollowPath`),
`Mpd::CGotoAgent` and `Mpd::CFollowAgent`, `Mpd::CSteering_SimpleBiped`, `Mpd::CGoto_GapDynamicAvoidance_v2` (avoiding
moving entities: the bots and the KynapseHuman entities of BOT-RT-5), `Mpd::CDetectPathNodeReached_Distance2D5`,
`Mpd::CRefineGoal_NearestInsideAiMesh` (a goal off the mesh moves to the nearest point on it),
`Mpd::CDetectAccident_PredictMove`, `Mpd::CComputeTargetPoint_ShortCut`, `Mpd::CSelectPathNodeCandidate_NextPathNode`
and `Mpd::CLpfManager` (local path finding around dynamic objects). The steering's output reaches the soldier as a
move target (the input object's slot 15, BOT-CTL-7), not as `EiaThrottle` / `EiaStrafe`, and its facing as the aim
(slot 4); `NavigationConstants.IgnoreKynapseSpeed` true means the
soldier's speed is DICE's, not Kynapse's (BOT-MOV-2). The go-to request and its states are AI-KY-3.

[exe: BFBC2Game.exe Kynapse world set-up 0xF1B980 (class-name strings 0x014A9470..0x014A9D4C referenced from it)]
[data: AI/Settings NavigationConstantData]

### BOT-MOV-2: Speeds, acceleration and stance

Status: **observed** · Variants: PC

`SoldierControlConstants`: `SneakingSpeed` 1.5, `RelaxedNormalSpeed` 1.5, `AlertedNormalSpeed` 2.2, `FastSpeed` 4,
`SprintSpeed` 6, `CrouchSpeed` 2.5, `CatchupSpeed` 10 (following), `AimForwardSpeed` 4 and `AimStrafeSpeed` 2.2 (m/s),
`Acceleration` 5, `Deceleration` 3, `MaxForwardDir` 60°, `MaxStrafeDir` 135°. An order's `SpeedState` and `Stance`
(AIO-FLD-1) pick among them. Stance changes are the `ChangePose` behaviour with `CombatConstants.PoseChangeLockoutTime`
2 s and `ChangePoseStopDelay` 0.35 s; stance is not among the AI's input actions (BOT-CTL-3): the behaviour sets it
on the soldier directly (BOT-CTL-8).

[data: SoldierControlConstantData, CombatConstantData (ai_probe)]

### BOT-MOV-3: Stuck bots teleport

Status: **observed** · Variants: PC

`NavigationConstants`: a bot slower than `SpeedThresholdForBeingStuck` 0.15 m/s for `TimeStuckBeforeTeleport` 3 s is
teleported (`GoalTeleport`, `BehaviourTeleport`, not into sector `TeleportForbidSector` 55); a follower more than
`DistanceFromPlayerBeforeTeleport` 40 m behind is teleported to its human. `BotManager::evacuateImmortalsFromArea`
(0xE610B0) moves immortal bots out of an area. Path failures retry after `FailureRetryWaitTime` 1 s; the go-to's own
retry is about 9.6 s (AI-KY-3). Teleporting is a campaign device; in multiplayer it would be visible cheating, so OPFOR's
bots need their own unstuck handling (or the constants raised).

[data: NavigationConstantData, FollowConstantData (ai_probe)]
[exe: BFBC2Game.exe BotManager::evacuateImmortalsFromArea 0xE610B0 (string 0x014A4BB4)]

### BOT-MOV-4: No ladders, no breaches; costs for water and roads

Status: **observed** · Variants: PC

No goal, behaviour or plan climbs a ladder (BOT-RT-7), and DICE's path data has no ladder links: ladder-only floors are
unreachable islands. Scripted moves use `SpecialMovement` / `PreSpecialMovement` with the animations of
`AI/Animations` (`AllowPreSpecialMovementDistance` 3 m). Path costs: `WaterCostModifier` 5, `RoadCostModifier` 0.5;
`FindNearestDistMax` 5 m and `FindNearestSearchRadius` 50 m bound the snap of a goal onto the mesh;
`MaxDynamicObjects` 25 and `LocalPathfindingLoadRadius` 15 m bound local path finding; vehicles are avoided at 10 m.

[data: NavigationConstantData (ai_probe); AIVehicleBehaviourData plans]
[doc: private:opfor-research/DEVELOPMENT_NOTES.md:2084-2088]

### BOT-MOV-5: Formations and following

Status: **observed** · Variants: PC

`AI/Formations` has six formations of slot offsets in metres from the leader (x right, z forward): `Default` (16
slots), `Queue` (5, one metre apart), `Line` (16, two metres apart in two rows), `Fire Team` (4), `CXP` (4) and
`Patrol` (16). Following (`FollowConstants`): within 8 m of a followed human (20 m in combat), 5 m of a followed bot
(100 m in combat), inertia 0.9, mimic the leader's pose within 100 m.

[data: AI/Formations FormationData (6), AI/Settings FollowConstantData (ai_probe)]

## 6. Vehicles

### BOT-VEH-1: Equipment types: what a bot may do in each

Status: **observed** · Variants: PC

`AI/Behaviours` lists 15 `AIVehicleBehaviourData` equipment types; each names its controls, its navigation and the
goals and behaviour-to-plan pairs allowed in it: `Infantry`, `Sniper`, `Support` and `Squad` (control `Soldier`,
`NTGround`, 26 goals), `Car` and `PBL` (`CarMovement`), `Tank` (`Turret` + `TankMovement`), `UnarmedTank`
(`TankMovement`), `Fixed` (`Turret`, `NTNone`, with `StationaryAttack`), `UnarmedFixed`, `Helicopter`,
`UnarmedHelicopter` and `SerdarHind` (`Helicopter`, `NTHelicopter`), `MarkV` (`CarMovement` + `Turret`) and
`OpenVehicle` (`Soldier`, a passenger who fights from the seat). `AI/Settings DefaultVehicleType` is the default.
Each control has its constants: `CarControlConstants` (`YawStrength` 15), `TankControlConstants` (`YawStrength` 50,
turn speed limit 60), `HelicopterControlConstants` (about 50 values: height, tilt, attack runs), `TurretControlConstants`
(look speeds 5-20).

[data: AI/Behaviours 15 AIVehicleBehaviourData (Navigation NTGround 7, NTHelicopter 3, NTNone 5), AI/Settings control constants (ai_probe)]

### BOT-VEH-2: A vehicle's AI description

Status: **observed** · Variants: PC

`AI/Vehicles` gives 41 `ObjectData`: `EquipmentType` (a BOT-VEH-1 type), `Armament` (`ArmamentData`: `ThreatLength`
0-3, `IsAntiAircraft`, `TurretData` with `PitchP` / `YawP` 0.5-10 and fake shooting distances), `Mobility`
(`MobilityData`: `MaxSpeed` 5.5-60, `Use3DWaypoints` on 2), `StrengthType`, `ReuseTime`, `Forbidden`,
`ForbiddenForHuman`, `HasExposedSoldier`, `InterludeOnly` and `InvalidForAI` (the seats themselves are the vehicle's entry components, VEH-SEAT-1); and six `AIAimingConstraintsData` (yaw
±65-180°, pitch -60 to 80°). `KynapseConstants.MaxMobileVehicles` 16 caps the moving vehicles Kynapse tracks. How a
vehicle blueprint names its `ObjectData` is not read.

[data: AI/Vehicles ObjectData 41, ArmamentData 24, MobilityData 10, TurretData 20, AIAimingConstraintsData 6; AI/Settings KynapseConstantData (ai_probe)]

### BOT-VEH-3: DICE seats bots at spawn; it never boards

Status: **observed** · Variants: PC

A campaign bot is put in a vehicle by its spawner (`Vehicle` and `VehiclePos`, seat 0-5: 282 of 1,556 spawners) and
driven by `PatrolOrder` (`VehicleThrottle` 0.2-1, `DefaultSpeed`); `LeaveVehicleOnSearchAndDestroyOrder` (8 of 14
templates) and the `StationaryBailDistance` 55 m make it leave. No goal or behaviour walks to a vehicle and enters it,
and `EiaChangeVehicle` / `EiaChangeEntry` are not AI actions (BOT-CTL-3). The body has a seat-entry request and a
seat-exit flag the server carries out (proxy +0x24 / +0x28, BOT-CTL-8); the exit is set by the proxy's slot 23, the
entry's AI writer was not found. Choosing a vehicle, walking to it, changing seats and choosing a role are OPFOR's to
build, through that request, a direct call of 0x5725E0 on the server thread, or the input actions, which the server
takes from any player. A seat's
index is its `EntryOrderNumber` (VEH-SEAT-1, 0 the driver) and its controls are the input mapping the seat names
(VEH-SEAT-2): a bot module in a seat writes the EIAs that mapping's concepts produce. Napisal's AI
boats and helicopters on Isla Inocentes are seated the same way; the patrol boat's `Navigation` is `NTNone`.

[data: SoldierSpawnEntityData Vehicle / VehiclePos, PatrolOrderEntityData, BehaviourTemplateData (ai_probe)]
[exe: BFBC2Game.exe proxy request pump 0x595190 (+0x24 -> 0x58DEC0 -> 0x5725E0; +0x28 -> 0x56D480(1)); proxy slot 23 0x57DAD0 `mov byte [ecx+0x28], 1`]
[doc: private:opfor-research/DEVELOPMENT_NOTES.md:2164-2170]

### BOT-VEH-4: Helicopters fly without path data

Status: **observed** · Variants: PC

`NTHelicopter` types fly waypoints interpolated every 75 m (`HelicopterWaypointInterpolationDistance`), at least 20 m
up (`MinimumHelicopterHeight`), with a 9 m path radius, a 15 m recalculation radius and 2 search steps; attack runs use
`AttackDistance` 150, `AttackHeight` 55, `AttackHoverTime` 2.5 s (scaled by the difficulty's
`HelicopterHoverTimeMultiplier`). The level's `HasFlyingAI` adds the flying AI's set-up (AI-START-3).

[data: AI/Settings NavigationConstantData, HelicopterControlConstantData, DifficultyData (ai_probe)]

### BOT-VEH-5: The vehicle controls write analog input levels

Status: **observed** · Variants: PC

A seated bot is driven by its equipment type's controls (BOT-VEH-1), each a registered object (BOT-RT-9) whose update
writes analog levels through the input object's slot 3 (BOT-CTL-7): `CarMovementControl` (update 0xE53130) and
`TankMovementControl` (0xE78480) write `EiaYaw` (3, first 0 and then the steering value), `EiaBrake` (2) and
`EiaThrottle` (0) every update, from the angle between the vehicle's forward vector and the path's direction (the sign
of their dot product picks forward or reverse); `TurretControl` (0xE78A50) writes `EiaRoll` (5) and `EiaPitch` (4)
toward its target; `HelicopterControl` (0xE91040) writes throttle and brake (0xE8EB30, from a height target, the
settings' helicopter constants at `[0x018903E8]+0x94`), yaw (0xE8ED90), pitch and roll (0xE90830 -> 0xE8F0B0, 0xE8F250).
The vehicle's weapons are switched by input slots 17 / 18 (BOT-CTL-8), not by `EiaFire`. These are the same
`EntryInputState` levels a human driver's client sends; the server applies them through the seat's own input mapping
(VEH-SEAT-2) in the controllable-entity update (0x695ED0 -> 0x697B90, BOT-CTL-2). That a turret takes its yaw from
`EiaRoll` is the AI's convention read here; whether every turret seat's mapping reads it the same way is inferred.

[exe: BFBC2Game.exe control instance vtables 0x014A48FC (car), 0x014A5640 (tank), 0x014A567C (turret), 0x014A5738 (helicopter), slot 4 each = 0xE53130 / 0xE78480 / 0xE78A50 / 0xE91040; the analog write sites of BOT-CTL-7; 0xE91040 -> 0xE8EB30 (0xE9122F, 0xE9125D), 0xE8ED90 (0xE9128F), 0xE90830 (after 0xE91273) -> 0xE8F0B0 (0xE90AF0), 0xE8F250 (0xE90B00)]
[inferred: the turret's roll-as-yaw against the seats' input mappings, which were not compared]

## 7. Squads and teams

### BOT-SQD-1: Squads in the level data

Status: **observed** · Variants: PC

`SquadEntityData`: `Soldiers[]` (spawners; 0-4 in the campaign, 1-8 on Onslaught), `SquadId` (2-57), `BaseReadiness`,
`Formation`, `SquadStartDispersed` (never set), `Enabled`. The campaign has 301 squads holding 454 spawners; a
spawner's `Squad` field names its squad. A squad's member keeps its squad and a leader flag (BOT-RT-6). These are AI
squads, unrelated to the multiplayer squads of the scoreboard (`SoldierSquadId`, `Squad1` on 13 human spawners).

[data: ai_probe SquadEntityData (301 campaign, 40 Onslaught), SoldierSpawnEntityData.Squad (454)]

### BOT-SQD-2: Squad types and behaviours

Status: **observed** · Variants: PC

`AI/Squads` has three squad types: `Infantry Squad` (`SCInfantry`), `Air` (`SCAir`), and an armoured one
(`SCArmored`), each with the default weight of its squad behaviours and readiness `RSPatrol`; the ten squad behaviour
classes are BOT-RT-7's. The squad level decides together (combat, defend, search and destroy, move in formation); the
soldier level decides alone below it.

[data: AI/Squads SquadTypeData (3) (ai_probe)]
[exe: BFBC2Game.exe squad behaviour creators 0x01C0AA0C..0x01C0AACC]

### BOT-SQD-3: No revive, resupply or objective sharing

Status: **inferred** · Variants: PC

DICE's AI has no team-play layer of multiplayer: nothing revives, heals, resupplies or repairs (BOT-CMB-4), no squad
behaviour spawns on or follows a multiplayer squad, and objectives are not shared out - each order names its soldiers
(AIO-ASN-1). The AreaDistribution subsystem (BOT-RT-4) spreads soldiers inside an order's area (inferred from its name).
Assigning bots to objectives, holding a balance between attack and defence, and squad play are OPFOR's.

[inferred: from the registries (BOT-RT-7), the order data (AIO-ASN-1) and the timing parts' names]

## 8. Game state a bot reads

### BOT-GS-1: DICE's AI reads no mode state

Status: **inferred** · Variants: PC

No AI class reads a capture point, an M-COM, a ticket counter or a combat area (AIO-MODE-1). A bot module that plays the
mode itself reads that state from the mode's entities and events: capture points (`CapturePointEntityData`, GAME-CQ-1
to GAME-CQ-8: `OnlyTakeableByTeam` +0x84, `MinNrToTakeControl` +0x88, times +0x9C / +0xA0; events `OnCaptured`,
`OnLost`), M-COMs and Rush stages (GAME-RUSH-1, GAME-RUSH-9), tickets (GAME-TKT-1 to GAME-TKT-3), combat areas
(GAME-AREA-1) and spawners (GAME-SPAWN-1). The run-time objects behind them are BOT-GS-3 (capture points) and
BOT-GS-4 (counters and objectives); the event graph stays the simplest interface.

[inferred: from AIO-MODE-1 and the gameplay rules; the capture point data offsets are OPFOR's captureprobe reading]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:582-583] [doc: opfor:src/dll/captureprobe.cpp:3-12]

### BOT-GS-2: Positions, players and kills

Status: **observed** · Variants: PC

What a bot module can read today: an entity's world matrix (vtable +4, or 0x610A20; translation at +0x30); a server
player's soldier (vtable +0x30), team (+0xA8 on a player object), id (+0x154), AI flag (+0xD84) and input handler
(+0xC38); the AI's own tracked positions of every player's soldier (0xEBD0A0); kills through the server's game-event
dispatcher (0x870070, type 3, AI-NEED-5); spawner events (AIO-EVT-4).

[exe: BFBC2Game.exe 0xEEF450 / 0xE8CE80 (vtable +4, +0x30 translation), 0x5AB6C0 (+0x154), 0x531C30 (+0xC38), 0x870070]
[doc: private:opfor-research/research/WW2_LANDING_GEAR_RE2_2026-11-14.md:104-115] [doc: private:opfor-research/research/WW2_VO_EVENTS_RE_2026-11-14.md:37-38]

### BOT-GS-3: A capture point's owner and progress

Status: **observed** · Variants: PC

The server capture point (`ServerCapturePointEntity`, constructor 0x7169D0 over the base 0x7159B0, vtable
0x01429470, name "SvCapturePoint" 0x6F6C60) keeps its data at +0xC and its state in floats and ints:

| Offset | What |
|---|---|
| +0x7C | the team the progress belongs to (set to the new team when the point is lost) |
| +0x94 | the owner, copied from +0x7C when the capture completes |
| +0x98 | the last step's rate (1 / capture time, times the step's factor) |
| +0x9C | the progress, 0 to 100 (corrected by round SPG2-rest2: the constant is 100.0, GAME-CQ-10) |
| +0xA0 | reset with the progress when the point is lost |
| +0xAC | the captured flag |

Capturing (0x725630, args: the players in the area, a factor, dt) adds `factor x 100 / data +0x9C (TimeToGetControl)
x dt` to the progress (GAME-CQ-10); at 100 it clears every listed player's +0xCA6, sets the owner, posts message
0x11 / 0x17 on the server's channel, sends the clients a "ROUN"/"CPTA" event and fires `OnCaptured` (hash 0x7D1F6304). Losing (0x711AA0) runs the
same with data +0xA0 (the lose time) downwards; at 0 it posts 0x11 / 0x18, sends "ROUN"/"CPLO" and fires `OnLost`
(0xAA8A2040), then sets +0x7C to its argument. Neither function reads the AI flag: whether bots in the area count is
decided where the area's player list is built: 0x724060, which has no AI test either (BOT-GS-5).

[exe: BFBC2Game.exe 0x7169D0 (base 0x7159B0, vtable 0x01429470); 0x725630 (+0x98 / +0x9C, data +0x9C, `0x68F1E0(+0x7C, 0x7D1F6304)`, message `(0x11, 0x17)`, 0x842F30('ROUN', 'CPTA')); 0x711AA0 (data +0xA0, `0x68F1E0(+0x7C, 0xAA8A2040)`, message `(0x11, 0x18)`, 'CPLO', +0x7C = arg 4); event-name hash at 0x10013E0 (djb2 with xor: h = 5381, h = h * 33 ^ c)]

### BOT-GS-4: Tickets, life counters and mission objectives are server entities with their own classes

Status: **observed** · Variants: PC

The other mode entities have server classes, told apart by the name function in a vtable each constructor writes
(corrected by round SPG2-rest3: round AIS-rest read the name functions of sub-object vtables and put the wrong
constructors under two of the names):

| Class | Constructor | Vtable (sub-objects) | Name | Data |
|---|---|---|---|---|
| `ServerTicketCounterEntity` | 0x66E880 | 0x0141FF18 (+0x2C 0x0141FEF0; listener +0x7C 0x0141FED0) | "SvTicketCounter" | `TicketCounterEntityData` |
| `ServerLifeCounterEntity` | 0x67A4B0 | 0x01420450 (+0x2C 0x01420448, +0x30 0x01420424, +0x74 0x01420410; listener +0x80 0x01420404) | "SvLifeCounter" | `LifeCounterEntityData` |
| `ServerMissionObjectiveEntity` | 0x66E190 | 0x0141FDF0 (+0x2C 0x0141FDE0, +0x38 0x0141FDBC, +0x7C 0x0141FDA8) | "SvMissionObjective" | `MissionObjectiveEntityData` |
| `ServerKillCounterEntity` | 0x6740D0 | 0x0141FCD8 (+0x2C 0x0141FCB0; listener +0x7C 0x0141FC90) | "SvKillCounter" | `KillCounterEntityData` |

The vtable 0x01420320 (constructor 0x67A030) given for the life counter before is a cutscene object's (its +0x58
sub-object is named "SvCutsceneObjectEntityBase"). Each keeps its data at +0xC. The mission objective takes the input
events `ShowObjective` and `Success` and raises `OnShowObjective` / `OnSuccess` (the four hashes its constructor
registers); the kill counter counts its team's kills of the other team (death messages, BOT-GS-7) up to its copy of
`MaxKillCount` (+0x84) and a revive takes one back; every entity's base event handler 0xFFE580 maps `Enable` and
`Disable` to its vtable +0x7C / +0x80. The ticket counter's state: BOT-GS-6; the life counter's (Rush lives and
bases): BOT-GS-8. A bot module can find every one of these objects by its vtable.

[exe: BFBC2Game.exe constructors 0x66E880 (vtables 0x66E8C1 - 0x66E8D4; channels 0x11 / 0x10 subscribed at 0x66E8F1 / 0x66E903), 0x67A4B0 (vtables 0x67A4F9 - 0x67A513; channels 0x16 / 0x10), 0x66E190 (0x68FB90 x 4: 0x87ADD7E8, 0xBB06A967, 0x7F3E7106, 0xD0488C09), 0x6740D0 (+0x84 = data +0x80; listener 0x67D680: death (0x10, 0x11) of the other team by its team -> 0x67D620(1), revive (0x10, 0x21) -> 0x67D620(-1), clamped to 0 .. +0x84); name functions 0x66A250 / 0x6695D0 / 0x669AF0 at 0x0141FEF8 / 0x0142042C / 0x0141FDC4 (slot 2 of 0x0141FEF0, 0x01420424, 0x0141FDBC), "SvKillCounter" at slot 2 of 0x0141FCB0; 0x67A030 writes 0x01420320 and +0x58 0x014202E8; 0xFFE580 (0x8BA423E4 -> +0x7C, 0xF90216B1 -> +0x80); field descriptors KillCounterEntityData `MaxKillCount` +0x80, `TeamId` +0x84, MissionObjectiveEntityData `ObjectiveType` +0xF0]
[data: spec/probes/modes.py, 2026-11-22: the hashes are `OnShowObjective`, `OnSuccess`, `Success`, `ShowObjective`, `Enable`, `Disable` (each found in the function named); DICE's mission objectives' connections: in `ShowObjective` 200, `Success` 97, `Disable` 19, `Enable` 1, out `OnShowObjective` 53, `OnSuccess` 74; kill counters out `OnMaxKillCount` 300]
[test: core/tests/test_spec_probes.py::modes]

### BOT-GS-5: Bots in a capture point's area count like humans

Status: **observed** · Variants: PC

The capture update 0x726B60 (GAME-CQ-10) builds the area's player list in 0x724060: it walks the server's player list
(0x532270) and takes every player whose soldier (vtable +0x30) is alive (health above 0) and whose team (+0xA8) is set,
and who stands in the capture sphere (`CaptureRadius` data +0x11C, `IsCapturedInUpperSphere` data +0x138, through
0x6CA7B0) or in the point's capture volume when it has one (0x6E54C0). It counts the players per team and returns the
count of the progress's team (+0x7C) and the largest other team with its count. Nothing in it reads the AI flag
(+0xD84): a bot standing on a flag counts toward `MinNrToTakeControl` and the capture speed like a human, so DICE's
capture rules need no bot-specific patch. A playtest with bots alone on a flag confirms it.

[exe: BFBC2Game.exe 0x724060 (player list 0x532270; soldier vtable +0x30, its +0x2C component's vtable +8 > 0; team +0xA8; 0x6CA7B0 with data +0x11C / +0x138 or 0x6E54C0; per-team counts through 0x7216E0; outputs the +0x7C team's count, the best other team and its count); no +0xD84 operand in 0x724060 - 0x72425F; caller 0x726B60]

### BOT-GS-6: The ticket counter's state: a death costs one ticket, whoever died

Status: **observed** · Variants: PC

`ServerTicketCounterEntity` (BOT-GS-4) keeps its team's tickets as a float at +0x80. Its fields:

| Offset | What |
|---|---|
| +0x80 | the tickets: `InitialTicketCount` (data +0x94) at the start and at a round reset (0x66EA50) |
| +0x84 | its team's point value: set from the capture points on the first update, then moved by the capture messages |
| +0x8C | the state: 0 counting, 1 reached zero, 2 won |
| +0x94 | the initial tickets (a float copy of data +0x94) |
| +0x98 | a bleed factor, 1.0 from the constructor |

Its listener (+0x7C, handler 0x684050) is subscribed to server channels 0x10 and 0x11, and while the state is 0:

- **a death** (channel 0x10, message 0x11, BOT-GS-7) whose victim's team (+0xA8) is the counter's `TeamId` (data +0xA0)
  takes 1 ticket (0x681AD0(1.0));
- **a revive** (0x10, message 0x21) of a player on its team gives 1 ticket back (+0x80 + 1.0, through 0x681A70);
- **a capture** (0x11, message 0x17, the capture point as sender, BOT-GS-3) of a point whose progress belongs to its
  team adds the point's area value for that team to +0x84 (0x711CD0: `AreaValueTeam1` / `AreaValueTeam2`, data +0x114 /
  +0x118, or the point's per-team table), and **a loss** (0x11, 0x18) of a point it owned (+0x94) subtracts it.

Each frame the update 0x686830 runs the bleed 0x684150(dt) while the state is 0 and tickets are left. It counts the
capture points (0x6751D0: all of them, those fully held by another team - captured flag set, progress 100 - and the
neutral ones):

- every point neutral and no live soldier anywhere (0x675270): `TicketLossWhenLostAll` (data +0x9C) x +0x98 x dt;
- every point held by the other side: `TicketLossWhenLostAllBases` (data +0xA8) x +0x98 x dt;
- otherwise, with the sum of every ticket counter's +0x84 (0x675330): no loss when `HaltTicketLossOnEqualPointCount`
  (data +0xAC) is set and the two teams' values are equal, or when the sum minus `TicketLossStart` (data +0x98) minus its
  own value is under 1; else, with that margin m and the per-minute rate r (`TicketLossPerMin`, data +0xA4, or the
  rate of the `TicketPercentages` entry in force, 0x680620): (m x 0.01 + 1) x r / 60 x +0x98 x dt.

Taking tickets (0x681AD0) does nothing for an amount of 0 or less or at 0 tickets; setting them (0x681A70) clamps at
0 and then (0x6805B0): at exactly 0 the state becomes 1 and `OnReachZero` is raised; when only one ticket counter in
the level still has tickets (0x6752D0), that counter wins (0x67E3F0: state 2, `OnTeamWins`, GAME-TKT-3). The fixed
0.01 is the code's own: `TicketLossPerMinPointCountScale` (data +0x80) is not read here.

Nothing on these paths reads the AI flag (+0xD84): **a bot's death costs its team a ticket and a bot's revive gives
one back**, like a human's; a soldier with no player posts no death message (BOT-GS-7), so a soldier the level spawns
without a player costs nothing.

[exe: BFBC2Game.exe constructor 0x66E880 (+0x98 = 1.0 from 0x14D94F0; channels 0x11 / 0x10); init (data +0x94 -> +0x94 and +0x80 at 0x66E907 - 0x66E920); reset 0x66EA50; handler 0x684050 (message (0x10, 0x11): msg +0x20 +0xA8 against data +0xA0 -> 0x681AD0(1.0) at 0x68413A; (0x10, 0x21) -> 0x681A70(+0x80 + 1.0, 0x14D94D8); (0x11, 0x17) sender +0x7C, (0x11, 0x18) sender +0x94 -> 0x711CD0 into +0x84); update 0x686830 (+0x84 = 0x675170(team) once, then 0x684150, 0x684320); bleed 0x684150 (0x6751D0, 0x675270, 0x675330, 0x6753B0, 0x680620; constants 0.01 at 0x1409E40, 1.0 at 0x14D94D8, 60.0 at 0x1412C48); 0x681AD0, 0x681A70, 0x6805B0 (hash 0xDE2783FB), 0x6752D0, 0x67E3F0 (hash 0x59CE0E1A); field descriptors TicketCounterEntityData 0x1C255D0 - 0x1C256A8]
[data: spec/probes/modes.py, 2026-11-22: DICE's ticket counters' connections: out `OnTeamWins` 125, `OnReachTicketPercentage` 129, no inputs; `OnReachZero` (0xDE2783FB) is no event name of DICE's PC level bundles, named from the game copy's other graphs (GAME-TKT-3)]
[test: core/tests/test_spec_probes.py::modes]

### BOT-GS-7: A soldier's death and a revive are server messages on channel 0x10

Status: **observed** · Variants: PC

When a server soldier dies (0x6C3810, called from 0x6C49E0 and 0x6C53F0) and it has a player (+0x220), it posts on
server channel 0x10 a message of type 0x11 (vtable 0x01423044, slot 1 returns the type): +0x20 the victim's player,
+0x24 the killer's player (0x54A4C0, null for none), +0x28 a value from the soldier's +0x3C component, +0x30 - +0x38 a
position, +0x40 a value of the killer's; it then posts (0x14, 0x15) with the soldier. A revive (0x662F20, after its
team and distance tests) posts type 0x21 (vtable 0x0141CD54) with two players at +0x20 and +0x24. The mode counters
subscribe to channel 0x10 and take these (BOT-GS-4, BOT-GS-6, BOT-GS-8); the kill scoring of BOT-SCO-1 is another
path. Neither poster tests the AI flag. A bot module that wants deaths or revives can subscribe to channel 0x10 (the
server's message object, `*(server +0x18)` vtable +8) and read the same two pointers.

[exe: BFBC2Game.exe 0x6C3810 (skip at `+0x220 == 0`; message on the stack with vtable 0x01423044 at 0x6C3B89, posted through `[server +0x18]` vtable +0x1C with 0x10 at 0x6C3BCA; then vtable +0x18 (0x14, 0x15, soldier)); type functions 0x6959A0 (0x11), 0x639FC0 (0x21); 0x662F20 (vtable 0x0141CD54 at 0x6637B2, channel 0x10)]

### BOT-GS-8: Rush lives and stages: the life counter's state

Status: **observed** · Variants: PC

`ServerLifeCounterEntity` (BOT-GS-4) holds a Rush team's lives and, for the defenders, the stages left:

| Offset | What |
|---|---|
| +0x84 | the lives left: `StartingLifeCount` (data +0xD8) at the start |
| +0x88 | kills toward the next reinforcement |
| +0x8C | lives spent |
| +0x94 | a count from the data's reinforcement table, decremented by one unnamed event (0x27BECF7C) |
| +0x98 | the lives cap: `StartingLifeCount` at the start, then the reinforcement table's step |
| +0x9C | the bases (stages) left: `BaseCount` (data +0x94) at the start |
| +0xA4 | `FallbackTime` (data +0x90) |
| +0xA8 / +0xA9 | lost / won |

Its listener (+0x80, handler 0x689320; channels 0x10 and 0x16) works while neither flag is set:

- **a death** whose victim is on the counter's `TeamId` (data +0xD4) costs a life (0x689200(1)); a kill of another team's
  player by its own team, on a counter that is not the attackers' (`Attacker` data +0xDC false), adds to +0x88
  (0x67D740(1)) and may post a score message to the killer (0x67D890, message 0x47, when 0x6F80A0 accepts the kill for
  one of the level's objective areas: inferred);
- **a revive** of its team's player gives a life back (0x669690(1));
- channel 0x16, message 0x11 adds the message's amount (+0x28) to +0x88 for a player of its team (inferred: a
  reinforcement from another source).

The input events: `DestructBase` takes a base (+0x9C); at 0 on the defenders' counter it posts message 0x46 and raises
`OnTeamLoss` with the lost flag (0x6696B0). `Winner` sets the won flag and posts an end message (0x18) on channel
0x2C. `DecreaseLife`
takes a life; `IncreaseReinforce` adds a kill toward reinforcement; 0xE5A091E9 (name not found) takes the event's
argument in lives. A life lost (0x689200) at 0 sets +0x84 to 0, raises `OnTeamLoss` and the lost flag; crossing
`FightHarderTreshold` (data +0xB8) downwards posts (0x11, 0x20) and raises `OnFightHarder`. Reinforcement (0x67D740):
when +0x88 reaches `ReinforceThreshold` (data +0x98) it is reduced by it, the cap +0x98 moves to the first entry of
the reinforcement table not under the lives, and the lives grow by an amount (0xC52450, 0 when +0x94 is 0) up to the
cap.

So the run-time Rush stage is the defenders' +0x9C (`BaseCount` minus the stages taken: GAME-RUSH-2's stage index is
`BaseCount - +0x9C`), and the attackers' tickets are their +0x84. No AI flag is read: **a bot's death costs its team a
life** like a human's. DICE's data: 30 attacker and 30 defender counters, attackers' lives 20 - 100, defenders'
`BaseCount` 2 - 5.

[exe: BFBC2Game.exe constructor 0x67A4B0 (+0x84 / +0x98 = data +0xD8, +0x9C = data +0x94, +0xA4 = data +0x90; channels 0x16 / 0x10 at 0x67A539 / 0x67A54B); handler 0x689320 (victim msg +0x20, killer +0x24, team data +0xD4, attacker flag data +0xDC; 0x689200, 0x67D740, 0x67D890, 0x669690; channel 0x16: msg +0x28 / +0x30); event slot 6 0x6897D0 (`Winner` 0xBE6EE0AC, 0x27BECF7C, `IncreaseReinforce` 0x2CFC2C6C, `DestructBase` 0x324B3FE6 with message 0x46 and 0x6696B0, `DecreaseLife` 0xC14750A1, 0xE5A091E9); 0x689200 (`OnTeamLoss` 0x59D3259A, `OnFightHarder` 0x2CE6F2B8, message (0x11, 0x20)); 0x67D740 (data +0x98, the table through 0x13AA630 / 0x13AA650); field descriptors LifeCounterEntityData 0x1C25390 - 0x1C25498]
[data: spec/probes/modes.py, 2026-11-22: DICE's life counters' connections: in `DestructBase` 87, `IncreaseReinforce` 78, `Winner` 60, out `OnTeamLoss` 457, `OnFightHarder` 61; 30 counters with `Attacker` true (`StartingLifeCount` 20 - 100) and 30 without (`BaseCount` 2 - 5); `DecreaseLife` is no event name of DICE's PC level bundles (named from the game copy's other graphs)]
[test: core/tests/test_spec_probes.py::modes]

### BOT-GS-9: An M-COM's arming is a game interaction entity

Status: **observed** · Variants: PC

An M-COM's arm and defuse (GAME-RUSH-1) are `GameInteractionEntityData` instances: `InteractionEntityType` (+0xAC,
`IEArmSDK` 2, `IEDefuseSDK` 3 among `IEDefault`, `IEGold`, `IEDisarmTrap`, `IERelease`, `IETriangulate`, `IESyncData`),
`TeamId` (+0xA0), `HoldToInteractTime` (+0xA4), `OnInteractionSendDelay` (+0xB0), `DelayBetweenUses` (+0xB4),
`IsHumanAllowed` (+0xB8) and `SendOnInteractionOnAnimationStart` (+0xB9). The server class is
`ServerInteractionEntity` (vtable 0x01423AA8; "SvInteraction" 0x68FA70 in its +0x2C sub-object's vtable 0x01423A84). Where
the armed state and the timer live was not found: the class's own slots are the base entity's but for slots 2, 3, 33,
34, 37 and 38, and no reader of `IsHumanAllowed` was found. Arming runs through the interaction's events and the
M-COM's event graph (GAME-RUSH-9), which stay a bot module's interface; whether an AI soldier may interact at all is
open (`IsHumanAllowed` suggests a per-entity filter).

[exe: BFBC2Game.exe field descriptors 0x1C25D80 - 0x1C25E10 (GameInteractionEntityData), enumeration InteractionEntityType 0x1C24220 - 0x1C242C8; vtable 0x01423AA8 (slots 2 0x68F8C0, 3 0x6A83E0, 33 0x68F8D0, 34 0x68F940, 37 0x68F9D0, 38 0x68F9E0), sub-object vtables 0x01423A84 / 0x01423A70 / 0x01423A64 written at 0x6A200F and 0x6A838F]

### BOT-SCO-1: The kill handler's AI tests gate stat events, not the kill score

Status: **observed** · Variants: PC

The player-killed handler 0x86DC90 (victim, killer, damage info, ...) returns at once only when the victim is null
(0x86DCAE); a bot's death goes through like a human's. With the server's scoring flag (+0x2A) set it calls the kill
scoring 0x86C070 for every kill (0x86DDEA), whoever died or killed. Its twelve +0xD84 tests guard only: the per-player
stat events 0x8584D0 (event ids 0x51 team kill, 0 / 0xB for the killer, 2 / 0xD for the victim, 1 / 0xC / 3 / 0xE with
the weapon, each only for a human and only with the scoring flag), the skill update 0x851380 (an Elo-style exchange of
two ratings, run only when killer and victim are both human, 0x86E2DA / 0x86E2EA), and a value in the kill message's
loop (0x86E4D9: an AI killer's is 0).

[exe: BFBC2Game.exe 0x86DC90: null victim test 0x86DCA2 / 0x86DCAE; scoring flag `cmp byte [ebp+0x2A], 0` at 0x86DD97; call 0x86C070 at 0x86DDEA; AI tests 0x86DF16 / 0x86DF46 / 0x86DF7D / 0x86DFB4 / 0x86E05B / 0x86E0B4 / 0x86E114 / 0x86E16D / 0x86E29E (each before a 0x8584D0 call), 0x86E2DA / 0x86E2EA (0x851380), 0x86E4D9; 0x8584D0 = map lookup by (player id, event) then the listener's vtable +4; 0x851380 rating doubles at +0x18]

### BOT-SCO-2: Bots never earn score and keep no scoring record

Status: **observed** · Variants: PC

In the kill scoring 0x86C070 the victim's deaths are counted in the victim's scoring record (the scoring object's +8
array by player id), then the function returns if the killer is null or an AI player (0x86C12D): a bot that kills gets
nothing, a human who kills a bot gets `KillScore` (+0x360, through the award 0x86BE20). The scoring records are made
only at join by 0x865D70 -> 0x8634E0, which returns for an AI player (0x865D7F); so a bot has no record and its
deaths are not counted either. Every score award goes through 0x86A7E0, which returns for an AI player (0x86A7EE):
no path gives a bot score (kills, captures, objectives, assists). The per-player scoring step 0x865F60 also skips AI
players.
To give bots scoreboard scores, OPFOR would patch these three tests and create the records (0x8634E0) itself.

[exe: BFBC2Game.exe 0x86C070 (`[param_1+8][victim id]` record, "deaths" through 0x7D27A0; killer tests at 0x86C125 / 0x86C12D; award `0x86BE20(killer, *param_1 + 0x360)`); 0x865D70 (test 0x865D7F; 0x8634E0 allocates the 0x570-byte record into +8[id]); 0x86A7E0 (test 0x86A7EE; adds to record +0x18); 0x865F60 (test 0x865F66)]

### BOT-SCO-3: Scoring offline: what the patch changes

Status: **observed** · Variants: PC

The scoring flag +0x2A is set by the join handler 0x865D70 only when the server has 4 or more clients (0x865F36,
`call 0x53F430; cmp eax, 4`): offline (one client) it stays clear and 0x86DC90 skips the kill scoring for everyone,
humans and bots. That, not the victim test, is why kills of bots scored nothing (AI-NEED-5's reason was wrong: a bot is
a player). With OPFOR's offline-scoring patch (MOD-PATCH-1, the five `cmp eax, 4` sites) the flag is set and a human's
kill of a bot is scored (`KillScore`), as PERS-SCORE-1 reads; the bot's death itself, the bot's own kills and the stat
events stay unscored (BOT-SCO-1, BOT-SCO-2). PERS-OPF-3's mechanism stands; its title holds only without the patch.
Why BFBC2's career save still writes score 0 is a separate question (PERS-SCORE-1).

[exe: BFBC2Game.exe 0x865D70 (flag +0x2A set to 1 unless `+0x2A == 0 && server +0x2C && 0x53F430() < 4`), 0x86DD97 (the handler's flag test); the patch sites 0x865F36, 0x86FD12, 0x842E6A, 0x842E9A, 0x83B591]

### BOT-SCO-4: A bot's name and its scoreboard row

Status: **observed** · Variants: PC

A server player's name is an EASTL string at +0xC2C (begin; +0xC30 end), read by the player's vtable slot 7 (0x531BF0)
and set, with the player's registration and the "player joined" message, by 0x564AE0(server, player, name). The AI
spawn calls it at 0x72D3FB with a string from the spawned soldier (its +0xC0 component's vtable +0xC, then +0xC), so
DICE's bots are named from their soldier's data (the string's owner was not identified). The player-info
serialiser 0x5486D0 sends every player's name, team (+0xA8), +0x158, the 64-bit +0x160, and the AI flag, and only for a
human the stats block (0x847DC0): a bot reaches the clients' player lists with its name and flag but without stats.
That the scoreboard lists a row per player of that list and shows the flag's players is inferred (the client's
scoreboard code was not read).

[exe: BFBC2Game.exe 0x531BF0 `lea eax,[ecx+0xC2C]` (player vtable 0x014188D8 slot 7, at 0x014188F4); 0x564AE0 (`if (name != player+0xC2C) assign`, message 0x54E8B0(0, 0x68, ...), player table +0x70 stride 0x1C); callers 0x575588 (in 0x575570) and 0x72D3FB (AI spawn); 0x5486D0 (0x50E4A0(stream, +0xC2C), +0xA8, +0x158, +0x160 / +0x164, +0xD84, human-only 0x847DC0); reader 0x548640]
[inferred: the client scoreboard's row source]

## 9. Difficulty and tuning

### BOT-DIF-1: One set of AI partitions for campaign and multiplayer

Status: **verified** · Variants: PC

The nine `AI/*` partitions (`Animations`, `Behaviours`, `Difficulty`, `Formations`, `Orders`, `Settings`, `Squads`,
`Vehicles`, `Weapons`) are byte-identical in `sp_common` and `mp_common`, and `mp_common` carries the two default
templates `AI/Settings` names (`DefaultSoldierBehaviour`, `DefaultSoldierSensing`) and no spawn template; the 17
`SpawnTemplate_*` are `sp_common`'s. A multiplayer level therefore has DICE's whole AI tuning but only the default
template, whose bots never fire (AI-NEED-3): a real template must be packed into the level's bundles.

[test: rust/bc2-formats/tests/ai_corpus.rs::ai_partitions_are_shared_by_both_commons]

### BOT-DIF-2: Three difficulty modes

Status: **verified** · Variants: PC

`AI/Difficulty` lists `Casual`, `Normal` and `Hard` in that order (the AI init's argument 0, 1, 2; AI-START-2), each with
16 scalars and two box vectors:

| Knob | Casual | Normal | Hard |
|---|---|---|---|
| HumanHealthModifier | 1.25 | 1 | 1 |
| EnemiesHealthModifier | 0.25 | 0.35 | 0.4 |
| FriendsHealthModifier | 1 | 1 | 1 |
| AccuracyIncreaseTimeModifier | 3 | 2 | 0.5 |
| StartAccuracyModifierStanding / Crouching | 0 | 1 | 50 |
| FinalAccuracyModifierStanding / Crouching | 0.75 | 0.9 | 1 |
| VehicleDamageModifier | 0.75 | 1 | 1 |
| HumanInCriticalHealth | 0.24 | 0.3 | 0.3 |
| HumanInCriticalHealthDamageModifier | 0.5 | 0.75 | 1 |
| CriticalHealthJesusModeTimeModifier | 1.67 | 0.34 | 0.15 |
| ReadinessReactionTimeMultiplier | 1.5 | 1 | 0.5 |
| HelicopterHoverTimeMultiplier | 2 | 1.3 | 0.5 |
| SnapBoxModifier (x, y, z) | 4, 2, 4 | 3, 2, 3 | 2, 2, 2 |
| StickyBoxModifier | 1, 1, 1 | 1, 1, 1 | 1, 1, 1 |
| SuckZoomModifier | 2 | 1.5 | 1 |
| UsePitchZoomSnap | true | true | true |

The health modifiers make the campaign's enemies far weaker than a player (a quarter to 0.4 of the health) and protect
a human in critical health; the snap, sticky and suck values tune the human's aim assist on the console. In multiplayer
these campaign biases would apply to bots and humans alike unless OPFOR chooses the difficulty (or neutral values) for
its matches.

[test: rust/bc2-formats/tests/ai_corpus.rs::difficulty_has_three_modes]
[data: AI/Difficulty DifficultyData (ai_probe), identical on Xbox 360 per OPFOR's console research]
[doc: private:opfor-research/DEVELOPMENT_NOTES.md:1833-1836]

### BOT-DIF-3: The per-template and per-spawner knobs

Status: **observed** · Variants: PC

Below the difficulty, each behaviour template tunes its bots (14 templates, ranges): `AccuracyIncreaseTime` 1-8 s,
`ReadinessReactionTimeMultiplier` 0.25-1, `MinAttackBursts` / `MaxAttackBursts` 1-99, `TargetDistanceFactor` 1-100,
`StunnedWhenHealthPercentageBelow` 0-0.9, `LowHealthThreshold` 0-1, `FrontLineDistance` 0-10, `AllowSleep`,
`AlwaysUpdate` (6 of 14: bots of that template are never skipped by BOT-RT-4's bands); each sensing template its skill
and ranges (BOT-PER-1); each spawner `HumanTargetPreference`, `Immortal`, `FakeImmortal`, `LodDistance`. A template's
`Rank` (0-5) and its empty `AllowedKits` / `AllowedAppearances` are the hooks for varied kits (`bfo:docs/plans/opfor/STOCK_MP_AI_PLAN.md`
section 11).

[data: BehaviourTemplateData (14), SensingTemplateData (14), SoldierSpawnTemplateData (18), SoldierSpawnEntityData (ai_probe)]
[doc: bfo:docs/plans/opfor/STOCK_MP_AI_PLAN.md:220-233]

### BOT-DIF-4: A multiplayer session's AI difficulty is the session's, Normal by default

Status: **observed** · Variants: PC

The session object's constructor (0x561C10) sets its difficulty field +0xC8 to 1 (Normal, 0x561D88). Only the campaign's
start message (type 0x10, constructor 0x918CA0, its +0x34 defaulting to 1 and filled in the menu queue 0x951E00 from a
field of the menu's start request, inferred to be the chosen difficulty) overwrites it, in the session's message handler
0x56A890; the multiplayer start message (type 0x13, 0x918D70, `hostMultiplayerLocal`) carries no difficulty and leaves
+0xC8 as it is. The session's state 5 (0x565450) copies +0xC8 into the server's start parameters; the server-load call
(0x85FA48) passes the start object's +0x90 as the fourth argument of the level load 0x574FB0, which pushes it to the AI
init 0x5B33C0 at 0x57507B. So an offline match runs the AI at Normal unless a campaign level ran earlier in the same
session object, whose difficulty then carries over. OPFOR's live run (difficulty 1) agrees. To choose a difficulty,
OPFOR writes the session's +0xC8 before state 5, or replaces the pushed argument at 0x57507B.

[exe: BFBC2Game.exe session constructor 0x561C10 (`mov dword [esi+0xC8], 1` at 0x561D88); 0x56A890 case 0x10 `+0xC8 = msg +0x34`, case 0x13 no write; 0x918CA0 (`+0x34 = 1`), 0x918D70; 0x951E00 cases 3 / 4 (`msg +0x34 = local difficulty`); 0x565450 (start parameters `+0x20 = session +0xC8`, then the server create through `+0xDC` vtable slot 0); 0x85FA48 (`push [esi+0x90]` as argument 4 of 0x574FB0); 0x57506E / 0x57507B (`mov edx,[esp+0xF8]; push edx; call 0x5B33C0`)]
[inferred: that the start parameters' +0x20 becomes the start object's +0x90 (the server create was not followed)]
[data: OPFOR's live run of Levels/SP_005 through the offline multiplayer path, probe "AI: init (arg1 = difficulty)" = 1]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:225-227]

## Implementation interface

What OPFOR's bot module (Goal 3 and 4 of `bc2-offline-skirmish-project`) must hook, call or replace. Addresses are
`BFBC2Game.exe` build 795745; "hook" means OPFOR's verified-bytes detour (`src/dll/hook.cpp`), as its probes do.
Everything here runs on the server thread (BOT-CTL-5).

**Always needed (either way of BOT-CTL-4):**

| Need | Where | Rule |
|---|---|---|
| Start the AI on a stock level | the HasAI test 0x575046 (`force_ai`) or the level data (HasAI, PathfindingTypes, path data, SectorOffsets) | AI-START-1, AI-START-3, AI-KY-1 |
| A per-frame server hook | 0x59E190 (the AI update; OPFOR probes it already) or right after the AI step 0xEBF3B0; the AI's own input push is 0xEBF470 -> 0xE6A8C0 | BOT-RT-2, BOT-CTL-7 |
| Bots as players | an AI spawner per bot (the engine then makes the player 0x573750, sets +0xD84 with 0x532190, spawns the soldier 0x70BBC0); or a DLL call of the spawner spawn 0x72D160 | BOT-CTL-1 |
| Bot names | hook 0x564AE0(server, player, name) and pass OPFOR's name when the player's +0xD84 is set (the AI spawn sets the flag at 0x72D3CC, then calls 0x564AE0 at 0x72D3FB); or write the EASTL string at player +0xC2C before the player info is serialised (0x5486D0) | BOT-SCO-4 |
| Bots on the scoreboard with scores | patch the three AI tests 0x86C12D (kill score), 0x86A7EE (every award), 0x865D7F (the join step that creates the scoring record 0x8634E0); keep MOD-PATCH-1 for the flag +0x2A | BOT-SCO-1 to BOT-SCO-3 |
| Kills and deaths | the game-event dispatcher 0x870070 (type 3) -> 0x86DC90(victim, killer, ...): every bot death arrives there | BOT-SCO-1 |
| World queries | the physics helper at `[0x0189037C]+0x130`: ray / sphere cast 0xB145C0, capsule 0xB14640, point 0xB145A0 | AI-KY-4 |
| Path requests | Kynapse go-to 0x107FE80 through a bot's pathfinder; or OPFOR's own search on the same `.hpd` | AI-KY-3, KYRT-PATH-1 |
| Capture state | the capture point objects (vtable 0x01429470): owner +0x94, progress team +0x7C, progress +0x9C (0-1), captured flag +0xAC; hook 0x725630 / 0x711AA0 for the moments | BOT-GS-3 |
| Counters and objectives | the ticket counter (0x0141FDF0), life counter (0x01420320) and mission objective (0x0141FCD8) objects by vtable; their fields still unread, so their events (an event hook, names hashed by djb2-xor as 0x10013E0 does) | BOT-GS-4, GAME-RUSH-9 |
| Difficulty | write the session's +0xC8 before state 5, or the argument pushed at 0x57507B (0 Casual, 1 Normal, 2 Hard) | BOT-DIF-4 |

**Way 1, drive DICE's AI:** author orders / spawners / templates per map (the suite's `stockmp`, XSMP-AUT-1); retarget
orders on `CapturePoint.OnCaptured` / `OnLost` and the Rush stage triggers, or fire `IssueOrder` on an order entity from
the DLL (its handler 0x58DD20 takes the event hash 0xD2199312, AIO-RT-1); pick the templates per class; tune through
the shared AI partitions (a Package overlay of `AI/*`). The limits are BOT-CMB-4, BOT-VEH-3, BOT-SQD-3 and BOT-MOV-3
(no gadgets, no boarding goal, no class play, teleporting).

**Way 2, own decisions on DICE's body** (the body is BOT-CTL-6; the writes DICE's own code makes are BOT-CTL-7):

| Need | Where | Rule |
|---|---|---|
| The bot's input state | the AI input object (proxy +0xC, vtable 0x014198A8): its `EntryInputState` at +0x28; write with `0x612A00(state, eia, bool)` (digital) and the analog level setter (slot 3), or the object's own slots 2 / 3 with the action table 0x01505158 | BOT-CTL-2, BOT-CTL-3 |
| Movement | either the move target (input slot 15, 0x58DF80: the soldier's movement component walks there, as DICE's infantry does) or `EiaThrottle` / `EiaStrafe` levels as a human's client sends them | BOT-CTL-7 |
| Aim | slot 4 (0x586E30: yaw, pitch on the soldier via 0x6A6460) | BOT-CTL-3, BOT-CMB-3 |
| Stance | the soldier interface's slot 20 (proxy +8, 0x58DAB0 -> 0x69F5B0) as DICE does, or `EiaChangePose` | BOT-CTL-8 |
| Sprint, weapon slot | input slot 14 (0x57DCE0), slot 19 (0x57DD20: slot 0 or the last); other slots through `EiaSelectWeapon*` input or 0x612D10 | BOT-CTL-8 |
| Vehicle entry and exit | set proxy +0x24 (an entry descriptor) / proxy slot 23 (exit) for the request pump 0x595190, or call 0x5725E0(player, vehicle, seat) / 0x56D480(1) on the server thread | BOT-CTL-8, BOT-VEH-3 |
| Driving and turrets | analog `EiaThrottle` 0, `EiaBrake` 2, `EiaYaw` 3, `EiaPitch` 4, `EiaRoll` 5 as DICE's controls write them; vehicle weapons by input slots 17 / 18 | BOT-VEH-5 |
| Push timing | write before the object's slot 7 (0x586F30) pushes (DICE's push is 0xE6A8C0, called from 0xEBF470 once per AI time), or push yourself with 0x6234B0 on the player's handler (+0xC38) | BOT-CTL-2, BOT-CTL-7 |
| Keep DICE's AI off the bot | no order for its spawner and a template whose behaviours never run, or hook the behaviour pass 0xE6DA70 / the proxy update 0x599E70 per bot | BOT-RT-4, BOT-RT-5 |
| Actions DICE's AI never sends | `EiaChangePose` 26, `EiaGrenadeLauncher` 33, `EiaStaticGadget` 34, `EiaDynamicGadget1` / `2` 35 / 36, `EiaMeleeAttack` 37, `EiaThrowGrenade` 38, `EiaChangeVehicle` 16, `EiaChangeEntry` 17, `EiaSelectEntry0`-`7` 18-25; by BOT-CTL-7's scan also `EiaStrafe`, `EiaAltFire`, `EiaZoom`, `EiaJump`, `EiaInteract` | BOT-CTL-3, BOT-CTL-7, BOT-CMB-4 |
| Perception | OPFOR's own cones and rays (the physics helper) with DICE's sensing values as the reference (BOT-PER-1, BOT-PER-2), or DICE's `Senses` tests 0xE82D70 / 0xE8CE80 (thiscall on a bot's senses object, not yet usable without the bot object) | BOT-PER-3 |

## Reverse engineering still needed

In priority order (what blocks OPFOR's bots first), each with how to settle it. Round AIS-rest settled items 2, 7 and
8 statically and most of 3 and 4; round SPG2-rest3 settled the tickets and Rush counters (item 2's open half, most of
3), found the bot object (item 4) and narrowed the fire path (item 8); what stays open needs the game or a deeper read.

1. **The AI input object in a running game** (needs the game; the plan is below). Proves BOT-CTL-2, BOT-CTL-3 and
   BOT-CTL-7 and decides between the two ways of BOT-CTL-4.
2. **Bots in captures and tickets.** Settled statically: captures (BOT-GS-5, round SPG2-rest2) and tickets and Rush
   lives (BOT-GS-6 to BOT-GS-8, round SPG2-rest3: a bot's death costs a ticket or a life, its revive gives it back).
   The play test (steps 8 and 9 below) confirms both.
3. **The M-COM's armed and destroyed state.** The Rush stage is the defenders' life counter +0x9C (BOT-GS-8); the
   M-COM's arming is a `ServerInteractionEntity` (BOT-GS-9) whose armed state and timer were not found, nor any
   reader of `IsHumanAllowed`. Next: the interaction's input side (the soldier's interaction component,
   "ServerWorldInteractionComponent" 0x1421314) and the M-COM blueprint's event graph from `OnInteraction` to
   `onDestroyed`.
4. **The bot's parts.** The bot is found (BOT-RT-6: 0xF4 bytes, constructor 0xE6C0B0, all made up front by the bot
   manager 0xE6C480) and its senses are +0x6C (0xE8E3E0, five sensing tasks); the roles of +0x80, +0x94, +0xA0 - +0xB0
   are not named, nor the target record layout inside the senses (+0xB0 - +0x10B0, BOT-PER-4). The 0xE82D70 object
   (+4 = the bot) that `BehaviourFire`'s score 0xECD970 calls is one of the unnamed parts.
5. **The order and squad runtime beyond `IssueOrder`.** The issue itself (the order entity's vtable slot 52,
   0x5A5490), how a soldier's goal list takes the order's urgency modifiers, the urgency sum and arg-max (AIO-RDY-2,
   BOT-RT-8) and the squad update: read 0x5A5490 and the decision pass 0xE6D8B0's goal loop.
6. **Who sets the seat-entry request** (proxy +0x24, BOT-CTL-8) in DICE's AI, if anything does: a scan for writes of
   the proxy's +0x24 from the AI's code.
7. **The soldier interface's state bits** (BOT-CTL-8, 0x68E620 / 0x68E5E0) and input slot 16's value (0x58E050 ->
   0x6ABC30): name them from the soldier's readers. The soldier interface's slot 2 (0x594290) sets the soldier's
   +0x298 bit 7 (0x68DE30, a replicated flag) unless the interface's lock byte (+8, slot 3 0x57D4D0) is set; 0xE528B0
   unlocks, writes and relocks it.
8. **The combat fire path.** Not found. Round SPG2-rest3 ruled out: input slots 17 / 18 (0x58E090 / 0x58E100 act only
   for a player in a vehicle); any direct AI call of 0x612A00, 0xACC460 or 0xAD1960 (none in 0xE40000 - 0xF30000); a
   register-indexed slot-2 call through a +0xC object (the three in the AI's code, 0xEE7240, 0xEEA410, 0xEEA460, are
   map lookups); and 0xED2130, which reads `EiaFire` back (input slot 12) for the scripted "SSFire" action.
   `BehaviourFire` (vtable 0x014A70A0) has the default update (slot 10 0xEC0C10) and hands everything to an action
   object at its +0x10 (status +0x1C, update +0x20, valid +0x14, stop +0x30) that its creator (0xED2A70, 0x28
   bytes) leaves null: the next read is whoever stores the behaviours' +0x10 (the behaviour start path) and that
   object's update.
   The play test's return addresses (step 2) name the site.
9. **Award listeners and the career save** (PERS-AWD-4, PERS-SCORE-1): not read this round.

### Play-test plan for item 1 (when in-game tests resume)

One Onslaught round (`ons_mp_002`, Rush, offline, OPFOR's module with `[debug]` probes), about two minutes, then one
seated-driver check. Each step lists what to log and what settles.

1. **Probe set-up.** Hook, log-only, budget 20,000 lines: 0x586EE0 (slot 2: this, action index, value), 0x586DC0
   (slot 3: this, index, level), 0x586E30 (slot 4: yaw, pitch), 0x586F30 (slot 7: dt), 0x57DCE0 (slot 14), 0x58DF80
   (slot 15: the position), 0x57DD20 (slot 19: argument), 0x58E090 / 0x58E100 (slots 17 / 18), 0x58DAB0 (soldier
   interface slot 20: pose). For each line also log the input object's player id (`[this+0x24]+0x154`) and the
   return address. Keep the AI flag (`player +0xD84`) beside every id.
2. **Passive minute.** Stand still near a bot squad for 60 s, then let one bot see you. Expect: slot 7 once per AI
   time per bot; slot 15 while walking and no slot-3 index 0 / 1 for infantry; slot 4 every frame in combat; slot 2
   index 6 (fire) only from 0xEC8380 / 0xEC83C0 or a site not in BOT-CTL-7 (log its return address: a new site means
   BOT-CTL-7's scan missed it); index 11 (reload) toggling; any index 1, 7, 8, 9 or 10 at all (BOT-CTL-7's inferred
   "never").
3. **Stance.** Wait for a bot to crouch in cover: expect 0x58DAB0 with 1 and no `EiaChangePose`.
4. **Write test.** With `[debug] ai_hold_jump=<player id>` set `EiaJump` (9) on that bot's state (+0x28) through
   0x612A00 right before its slot 7 each frame for 5 s: the bot must jump on the local client. Then the same with
   `EiaThrottle` 1.0: it must walk forward against DICE's move target (shows which wins).
5. **Seated driver.** Load SP_004_B through the offline path (`force_ai`), wait for an AI tank: expect slot 3 with
   indices 3, 2, 0 from 0xE78480 every update and 5 / 4 from 0xE78A50 for its gunner; slots 17 / 18 when it fires.
6. **Scores and names** (BOT-SCO-*, needs MOD-PATCH-1): kill two bots, let a bot kill you. The kill probe
   (`[debug] kill_probe=1`) must show 0x86C070 for all three, a `KillScore` award (0x86BE20) for your kills and none
   for the bot's; the scoreboard shows each bot's row with its soldier's name and 0 score.
7. **Record:** the log lines' counts per (site, action) go into BOT-CTL-7's table as `[data: ...]`, and BOT-CTL-2,
   BOT-CTL-3, BOT-CTL-7 and BOT-SCO-2 move to verified when a test reads the log.
8. **Tickets** (BOT-GS-6, BOT-GS-7; a Conquest level, MOD-PATCH-1 not needed): log 0x681A70 (this, new value) and
   0x684050 (channel, type, message +0x20 / +0x24 and their AI flags). Let a bot die with no capture changing: expect
   one (0x10, 0x11) with the bot as +0x20 and its team's +0x80 one lower; a revive of a bot (if any medic AI revives:
   none expected, BOT-CMB-4) one higher. Hold every flag with bots alone for a minute: the losing counter's +0x80 falls
   by (m x 0.01 + 1) x 10 / 60 a second.
9. **Rush lives and stages** (BOT-GS-8, `ons_mp_002`): log 0x689200 (this, amount), 0x6897D0 (event hash) and the
   defenders' +0x9C. A bot attacker's death: the attackers' +0x84 one lower; an M-COM destroyed: `DestructBase` and
   +0x9C one lower; the last: message 0x46 and `OnTeamLoss` (0x59D3259A). Also log 0x68FA70's class's slots 33 / 34
   (BOT-GS-9) while a human arms an M-COM, to find where the armed state is kept.
10. **Fire** (RE item 8): hook `BehaviourFire`'s slots 5, 10 and 13 (0xEC4FE0, 0xEC0C10, 0xECAA90) and log its
   +0x10's vtable; then that vtable's +0x20 with the return address of every input slot-2 call it makes.

## Open questions

The numbered list above; besides:

- Whether DICE's AI ever sends `EiaInteract` (action 10) and what for (M-COM arming is interact for humans); BOT-CTL-7
  found no site.
- The region test of BOT-PER-3 (manager vtable +0x28): a floor id, a sector or a Kynapse area.
- What the four events 3, 4, 5 and 6 of the per-player hook (BOT-RT-5) are.
- What the proxy's message 0x14 (+0x1C) and 0x15 (+0x20) requests do (BOT-CTL-6).
