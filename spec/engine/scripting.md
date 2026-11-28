# Scriptability: every way behaviour can be authored or changed

A survey of the ways BFBC2's behaviour is authored by DICE or can be changed by a modder: the level's data-driven
entity event graph, the menus' action script, the engine's settings and command line, data entities that reach the
engine, OPFOR-style DLL hooks, and what is left of developer consoles and server administration in the PC client.
For each: what it can reach and what it cannot, and at the end what a modder-facing scripting layer would need.

**Sources.** Round SPG3 (2026-11-23): DICE's 74 level partitions through `spec/probes/scripting.py` (its `checks()`
hold the counts below; the event connections' form is LEVEL-EVT-1 / LEVEL-EVT-2), `BFBC2Game.exe` (PC build 795745)
read statically, and the rules of the other engine files cited. **The dedicated server build is not on this PC**: what
is said about server administration is only what the client executable shows. Round SPG3-rest (2026-11-27) traced the
entity factories, the setting and console entities' handlers and the document server statically (capstone over a
scratch copy of the executable; the probe's checks are byte scans), with event names found by their hashes
(`h = h * 33 ^ c` from 5381, BOT-GS-3) against the executable's strings and DICE's 186 event names.

## 1. The layers

### SCR-LAY-1: Six layers author behaviour, each with its own reach

Status: **inferred** · Variants: PC

| Layer | Authored as | Reaches | Cannot reach | Spec |
|---|---|---|---|---|
| Level and asset **data** | DBX partitions: entity data, their fields, blueprints, weapons, vehicles, AI orders | every tuning value the engine reads from data; which entities exist and where | new behaviour: an entity does only what its class's code does | model/level.md, engine/weapons.md, model/ai_orders.md |
| The level's **event graph** | `LevelData.EventConnections`: "when Source fires SourceEvent, send TargetEvent to Target" | sequencing and conditions between existing entities (triggers, counters, gates, delays, splitters, spawners, orders, objectives, sound, effects) | arithmetic beyond the logic entities' own, loops with state outside them, anything no entity class implements (SCR-EVT-1) | SCR-EVT-1, SCR-EVT-2, LEVEL-EVT-1 |
| The **menus' action script** | APT movies (ActionScript 1 / 2 byte code) | the screens; the engine only through the name table's getters, commands and events | game state the name table does not expose; any write outside its commands | menus.md (MENU-RT, MENU-EVT, MENU-ED), SET-UI-1, INP-UI-1 |
| **Settings** and the **command line** | `settings.ini`, `GameSettings.ini`, the input files, `-<name> <value>` | the values those stores hold (quality, sensitivities, bindings, a few engine switches) | behaviour | settings.md, input.md |
| **Data entities that reach the engine** | `GameSettingEntityData`, `ConsoleCommandEntityData`, the debug-menu entities, placed in the event graph | one game setting, the vertical look inversion, from an event (SCR-CMD-2) | any engine command: the console command entity does nothing in the retail build (SCR-CMD-2) | SCR-CMD-1, SCR-CMD-2 |
| **DLL hooks** (OPFOR's module) | C++ code patched into `BFBC2Game.exe` at fixed addresses | anything the process does | another build of the exe; a hook is only as safe as its address check (MOD-EXE-1, MOD-HOOK-3) | module.md |

DICE authored the campaign and the modes in the first two layers; OPFOR's work uses the data layers (ports, stock-map
AI, exports), the menus (patched movies) and the DLL for what data cannot do (the offline host, the controller, the
award engine, the landing gear).

[inferred: from the rules cited per row and SCR-EVT-1 - SCR-CMD-1]

## 2. The event graph

### SCR-EVT-1: DICE's event graphs use 77 classes and 186 event names

Status: **verified** · Variants: PC

Across DICE's 74 level partitions the 57,667 event connections (LEVEL-EVT-1) start at instances of 56 classes and
end at 72, 77 classes in all; 1,739 ends are another partition's instance (an external reference, LEVEL-EVT-2). They
use 82 distinct source event names and 104 target event names, 186 names in all - a level script is a graph over a
small vocabulary. By connection ends the most used classes are `ReferenceObjectData` (a placed blueprint: its events
are the blueprint's interface), `EventSplitterEntityData`, `DelayTriggerEntityData`, `GameEffectEntityData`,
`VoiceOverEntityData`, `GameInteractionEntityData`, `AreaTriggerEntityData` and `MultipleTriggerEntityData`; the
modes' entities (`CapturePointEntityData`, `TicketCounterEntityData`, `LifeCounterEntityData`,
`MissionObjectiveEntityData`) and the AI's orders (AIO-EVT-1) are among the rest. An event is matched by its name's
hash (BOT-GS-3), so a new event name means something only to a class whose code compares that hash.

[test: core/tests/test_spec_probes.py::event_graph]
[data: spec/probes/scripting.py, 2026-11-23: 74 levels, 57,667 connections, 1,739 external ends, 56 source and 72 target classes (77), 82 + 104 event names (186), the eight classes above by connection ends]

### SCR-EVT-2: 180 entity data classes exist; DICE's graphs use 73 of them

Status: **verified** · Variants: PC

The executable names 180 classes ending in `EntityData` (its type names), among them the generic logic DICE's
graphs hardly use (`LogicEntityData`, `CounterEntityData`, `EventGateEntityData`, `EventMemoryEntityData`,
`EventIfSwitchEntityData`, `EventSequenceTriggerEntityData`, `SpeedEventGateEntityData`, `DelayEntityData`). 73 of
the 77 classes in DICE's graphs are among them; the other four are not entity data (`ReferenceObjectData`,
`SequenceData`, `CSITextData`, `ForbiddenAIAreaData`). A modder can therefore wire about a hundred more classes than
DICE's levels show, but each one's events and fields must be read from the executable (its event-hash compares,
BOT-GS-3) before it is used: what an unused class does is unproven.

[test: core/tests/test_spec_probes.py::event_graph]
[data: spec/probes/scripting.py, 2026-11-23: 180 `...EntityData` type names in the exe; 73 of the graphs' 77 classes]

### SCR-EVT-3: The 107 classes DICE's graphs do not use: which make an entity, and the events they take

Status: **verified** · Variants: PC

A placed entity data object makes an entity only through a factory registered for its exact class: the server's
registry (0x71C7F0, 148 registrations) and the client's (0x8B6AC0, 100) map a data class to a factory, and the lookup
(0x716BF0) takes the object's own class, with no fall-back to a parent class. Factories exist for 123 of the 180
`...EntityData` classes. Of the 107 classes DICE's level graphs do not use (SCR-EVT-2):

- **45 have no factory on either side**, so an instance in a level makes no entity and its events go nowhere:
  `AabbGeometry`, `AlternateSpawn`, `AreaGeometry`, `ChangeFormationOrder`, `ChangeReadinessOrder`, `Controllable`,
  `Counter`, `Cover`, `DebugMenuSubMenu`, `DebugTest`, `Delay`, `DynamicPhysics`, `ExplosionImpulseTest`,
  `FollowAISoldierOrder`, `Game`, `GamePhysics`, `Garrison`, `Geometry`, `GraphGeometry`, `Group`, `Interaction`,
  `KillAll`, `Light`, `Logic`, `LogicUtil`, `MeshProjectile`, `MiddlegrowthModel`, `Physics`, `Projectile`, `Pulse`,
  `RandomPulse`, `RandomWalkOrder`, `SoldierSpawnBase`, `SoldierSpawnGhost`, `Spawn`, `SphereGeometry`, `SquadBase`,
  `SquadDebugMenu`, `SquadGhost`, `SquadManager`, `Test`, `TreeMesh`, `Trigger`, `TriggerEvent`, `VisualTerrain`
  (each `...EntityData`). Most are base classes; `CounterEntityData`, `DelayEntityData`, `PulseEntityData`,
  `RandomPulseEntityData` and the four order classes look usable in data but make nothing (DICE's graphs use
  `DelayTriggerEntityData` and the order classes that have a factory). Other objects may make some of them (a
  spawn, a squad's members) outside the factories: not traced.
- **62 have a factory.** Their input events, read from the hashes their event handler (vtable slot 6) compares:

| Classes (`...EntityData`) | Input events | Raised |
|---|---|---|
| `StaticModel`, `FenceModel`, `Ladder`, `InteractableStaticModel` (server) | `Destroy`, `Heal`, `Hide`, `Show`, `PlayAnimation`, `StopAnimation`, `Disable` (the interactable one adds `Start`, `Stop`) | |
| the same (client) | `PlayAnimation`, `StopAnimation` | |
| `AmmoBox`, `MedicBag` | `Detonate` | |
| `Pickup`, `KitPickup`, `DynamicWeaponPickup` | `Pickup` | `OnPickup` |
| `DisableTeam` | `Start`, `Stop` | |
| `EventMemory` | `LoadMemory`, `ClearMemory`, `FireMemory` | |
| `EventSequenceTrigger` | `Trigger` | `OnTrigger` |
| `SpeedEventGate` | `Impulse` | `OnImpulse` |
| `Continue` | `Continue`, `FakeSave` | |
| `TacticalObjective` | `BriefPlayer` and two hashes not named (0xA6AF1FAA, 0xA6C00F29) | |
| `ProjectileImpulseTest` | `StartImpulseTest` | |
| `GameTest` | three hashes not named (0x36D2F147, 0x9E758F6A, 0xA22F7884) | |
| `GameSetting` | `ChangeSetting` (SCR-CMD-2) | |
| `ConsoleCommand` | none (SCR-CMD-2) | |
| `DebugText` | `Enable` shows the text (server 0x698790) | |
| `Bullet`, `DeathAreaTrigger`, `DynamicModel`, `EndOfRoundSoldierSpawn`, `Grenade`, `Hud`, `Missile`, `PreRound`, `RigidModel`, `RigidModelGroup`, `ServerConnectTrigger`, `Soldier`, `StaticPosition`, `Target`, `Team`, `TracerDart`, `Vehicle`, `VehicleProjectile`, `Forest`, `MiddlegrowthModelGroup`, `ForceArea` | the base handler's `Enable` / `Disable` only | |

Every entity's base handler 0xFFE580 maps `Enable` and `Disable` (BOT-GS-3). The remaining factories (the lights,
the emitter, the effect, sound and weather entities, the mesh and banger classes, `CombatActionTrigger`,
`StaticEnvmap`, `WeaponCallback`) were not followed to a handler that names its events. A class missing from DICE's
graphs is not unused: the static models, pickups and vehicles are placed and driven by other data.

[test: core/tests/test_spec_probes.py::event_graph]
[exe: BFBC2Game.exe server registry 0x0157041C filled in 0x71C7F0 (`mov ecx, 0x157041C; call 0x71C790` x 148), client registry 0x01572768 in 0x8B6AC0 (`call 0x8B6A60` x 100); factory slot 1 `mov eax, <data class info>; ret`; data class infos from the static initialisers `push <type record>; mov ecx, <info>; call 0x500880`; lookup 0x71DD00 -> 0x716BF0 (exact class: lower bound and equality, 0 when absent); handlers: static models 0x714C90 (client 0x8DD7F0), ammo box / medic bag 0x6FB890, pickups 0x6C73B0, disable team 0x673870, event memory 0x66D9B0, event sequence trigger 0x724FC0, speed event gate 0x674EB0, continue 0x6C3F10, tactical objective 0x674F90, projectile impulse test 0x6987E0, game test 0x681290, game setting 0x67A400, console command 0x68EFF0, debug text 0x698790, base 0xFFE580]
[data: spec/probes/scripting.py, 2026-11-27: 148 and 100 registrations, 123 data classes with a factory, of the 107 unused 62 with and 45 without; each named event's hash in its handler's first 0x300 bytes]

### SCR-CMD-1: Entities that set a setting or run a command; the debug menu in DICE's data

Status: **verified** · Variants: PC

The executable defines `GameSettingEntityData` (with `ChangeGameSettingType`), `ConsoleCommandEntityData` and a
server class `ServerConsoleCommandEntity`: data entities that, from an event, change a game setting or run an engine
command. DICE's levels place none of them. They do place the debug menu's entities: `DebugMenuItemEntityData` (104
instances, in 8 multiplayer and 13 campaign levels), `DebugMenuSubMenuEntityData` (5) and `DebugTextEntityData` (9),
wired into the graphs (450 connection ends on the menu items) - development leftovers, whose menu the retail menus do
not show (`enableDevOpts`, SCR-DEV-1). `DebugTestEntityData` is never placed.

[test: core/tests/test_spec_probes.py::event_graph]
[exe: BFBC2Game.exe type records: ConsoleCommandEntityData 0x01C05DC0, DebugMenuSubMenuEntityData 0x01C05DD8, GameSettingEntityData 0x01BFDF0C, ChangeGameSettingType 0x01BFDEF0, ServerConsoleCommandEntity 0x01BF1DD8]
[data: spec/probes/scripting.py, 2026-11-23: instance counts of the six classes in DICE's 74 level partitions; DebugMenuItemEntityData in mp_002, mp_004, mp_005gr, mp_006, mp_007gr, mp_008, mp_009gr, mp_012gr and 13 campaign levels]

### SCR-CMD-2: The game setting entity inverts the look axis; the console command entity does nothing

Status: **verified** · Variants: PC

`GameSettingEntityData` makes a server entity (`ServerGameSettingEntity`, no client side). On `ChangeSetting`, while
the entity is enabled (bit 3 of +0x26), it sends a `ChangeGameSetting` network message (id 0x13, the data's
`ChangeSetting` +0x80 in 2 bits) to every player whose connection is up. The client's message reads the value and,
for `CGSInvertVerticalLookAxis` (1), flips `Scheme1FlipY`, `Scheme2FlipY` and `Scheme4FlipY` (each 0 <-> 1) in the
game settings; it does not save them, so the change lasts until the next settings save writes it or a load replaces
it. `CGSNone` (0) does nothing. That is the whole of `ChangeGameSettingType` (`CGSCount` 2): a level can invert
every player's vertical look, nothing else.

`ConsoleCommandEntityData` makes a server entity (`ServerConsoleCommandEntity`, 0x2C bytes: the base entity, no state
of its own, no client side) whose event handler is a bare `ret 4`: it ignores every event, even `Enable` and
`Disable`, and its `Commands` (+0x80) are never read. In the retail build it does nothing.

[test: core/tests/test_spec_probes.py::event_graph]
[exe: BFBC2Game.exe factories 0x6FCC80 (GameSettingEntityData, class info 0x1BFECB0 -> constructor 0x6690A0, vtable 0x141EA98) and 0x6FDE10 (ConsoleCommandEntityData, 0x1C05EFC -> 0x698680, vtable 0x14230A0); ServerGameSettingEntity handler 0x67A400 (`cmp [edi+0xC], 0x5A257055` "ChangeSetting" at 0x67A418) -> 0x673CD0: for each player of the game's list (+0x208) 0x64E8C0 -> message 0x839D20 (vtable 0x1432EB4: id 0x13 at 0x839D40, 2-bit write 0x839D60), sent at 0x64E90C when the connection's state is 4; client message vtable 0x141CEA0 (read 2 bits 0x63C2E0 -> +4, run 0x642F90: `cmp [ecx+4], 1`, then 0x80D2E0 / 0x82FAB0 on "Scheme1FlipY", "Scheme2FlipY", "Scheme4FlipY"); ServerConsoleCommandEntity handler 0x68EFF0 (`C2 04 00`); enumeration CGSNone 0, CGSInvertVerticalLookAxis 1, CGSCount 2 (0x01C251C8 - 0x01C251F8)]
[data: spec/probes/scripting.py, 2026-11-27: "ChangeSetting" hashed into 0x67A400's bytes; 0x68EFF0 is `ret 4`]

## 3. Consoles, developer options, server administration

### SCR-DEV-1: No developer console in the PC client; developer options and debug pages are switched off

Status: **observed** · Variants: PC

No interactive console was found in the retail PC client: a search of its strings found no console command
registry, no command or variable listing and no "unknown command" text of its own (the one such string is Havok's
MOPP error); a hidden one is not ruled out. What remains:

- the menu name table's `enableDevOpts` (extern 0xF3), backed by the `Client` object's `IsMenuDevOptsEnabled`
  (+0xF4), and `debugdata` (0xD2), `consoleTrace` (0x8C, the movies' trace to the engine's log): the menus' developer
  options, off unless the engine setting is set;
- HTML page generators of a debug document server - `/settings` and `/settings/*` (the settings objects and their
  values, SET-ENG-1), `/stats`, `/types` - registered with a server object passed in (0x501BF0); the retail build
  never makes that server (SCR-DEV-2);
- the engine settings by name (SET-ENG-1) and the command line (SET-CLI-1), which OPFOR's DLL uses as its own
  switches (`opfor.ini`, MOD-DEV-1).

[exe: BFBC2Game.exe name table entries enableDevOpts 0xF3, debugdata 0xD2, consoleTrace 0x8C; Client field IsMenuDevOptsEnabled +0xF4 (field table 0x01C14338); "/settings" 0x14123A0 and "/settings/*" 0x1412394 registered in 0x501BF0 (handlers 0x4FFEC0, 0x4FFC60, "Settings - %s"); "/stats" 0x1411DA4, "/types" 0x140FDF8; "Unknown command - This mopp data has been corrupted" 0x148EF90 (Havok)]

### SCR-DEV-2: The debug document server is never made

Status: **verified** · Variants: PC

The engine's debug pages are document providers: the settings manager (`/settings`, `/settings/*`: the settings
objects and their values), the type registry (`/types`, `/types/classes`), the core database (`/db`,
`/db/domains/*`), the statistics (`/stats`, `/stats/*`) and two more register through 0x4D06B0 (six call sites),
which adds the provider to a set and then hands it every document server already in a second set, so that the server
can serve its pages (the provider's one method, for example 0x501BF0, registers its paths with the server). The
server set is touched only by its getter 0x4D0640 (and its exit-time destructor), and the only function that adds a
server, 0x4D07D0, is named by no call, jump or stored pointer. So no document server exists in the retail build: the
pages are registered with nothing, no page is served and no socket is opened for them. The HTML writer (0x4BADD0,
`<html xmlns=...>`, a 5-second refresh) stays unused with them.

[test: core/tests/test_spec_probes.py::options]
[exe: BFBC2Game.exe provider registration 0x4D06B0 (callers 0x43BFEA, 0x43C022, 0x4A9885, 0x4D09CA, 0x4F7076, 0x4FEDB9 in the settings manager's constructor 0x4FED50, vtable 0x1411CBC = {0x501BF0}); provider set getter 0x4D05D0, server set getter 0x4D0640 (globals 0x155CF3C - 0x155CF5C, referenced only there and in the destructor 0x1381130); add server 0x4D07D0 (no E8 / E9 site, no stored pointer); "/types" 0x140FDF8 (0x4AF5D0), "/db" 0x140FE10, "/stats" 0x1411DA4 (0x5002F0), "/settings" 0x14123A0 (0x501BF0); HTML head writer 0x4BADD0]
[data: spec/probes/options.py, 2026-11-27: 6 provider registrations, 0 references to the add]

### SCR-SRV-1: Server administration: traces only; the server build is not on this PC

Status: **observed** · Variants: PC

The PC client and the dedicated server share code: the entry tests `-dedicated` and `-server` (SET-CLI-1) and names a
second mutex `BFBC2Server-124DD424-A478-4d95-A026-DA20E544D4E2`; `ServerSettings` and the `Server.*` names are
there, and so are `ServerConsoleCommandEntity`, the PunkBuster commands (`set_cl_punkbuster`, `set_sv_punkbuster`,
`punkbuster kick`) and the menus' `kickPlayer` (0xB4), `voteKickPlayer` (0x39) and `playerKickedReason` (0x13F). No
remote administration protocol (the retail server's RCON) is in the client: none of its command words or its login
was found. The dedicated server executable is not on this PC, so its admin commands are out of reach of this
specification; offline, OPFOR's in-process host (NET-SES-2) has no admin interface and needs none.

[exe: BFBC2Game.exe "-dedicated" 0x140982C (0x401F29, 0x401F65), "-server" 0x1409824 (0x401F48); "BFBC2Server-124DD424-..." pushed at 0x401FA5; "set_cl_punkbuster" 0x141AE3C, "set_sv_punkbuster" 0x141AF4C, "punkbuster kick" 0x1417978; name table kickPlayer 0xB4, voteKickPlayer 0x39, playerKickedReason 0x13F]

## 4. Action script and hooks

### SCR-AS-1: The menus' script reaches the game only through the name table

Status: **inferred** · Variants: PC

A movie's code runs in the APT interpreter (MENU-RT-1 - MENU-RT-5) and sees the engine only as the externs it may
read, the commands (FSCommands) it may send and the events it receives, all named in the executable's table of 574
names (APT-LOAD-3, MENU-EVT-1 - MENU-EVT-3). So menu script can change what a screen shows and send any command the
table has - start a match, set a setting, change a binding (SET-UI-1, INP-UI-1) - but cannot read a value no getter
answers or act inside a match beyond the HUD's names. OPFOR's DLL widens it by answering new extern names in the
getters (MENU-EVT-3: `opforPacific`, `playerStats` ...), the one way to give menu script new reach without a new
interpreter.

[inferred: from MENU-RT-1 - MENU-RT-5, MENU-EVT-1 - MENU-EVT-3, APT-LOAD-3 and MENU-ED-3]

### SCR-DLL-1: Hooks reach everything, for one build

Status: **inferred** · Variants: PC

A DLL loaded into the process (OPFOR's through its `dinput8.dll` proxy) can replace any function, read any object
and add behaviour no data can express: OPFOR's 221 patch points (MOD-CNT-1) include the offline host, the pad read,
the award engine, repair stations and the landing gear. The price: every address is build 795745's (MOD-EXE-1), the
objects' layouts are reverse engineered (BOT-RT-6, BOT-CTL-6), and the code runs on the engine's threads with their
rules (BOT-CTL-5: bot logic on the server thread).

[inferred: from MOD-EXE-1, MOD-CNT-1, MOD-HOOK-1 - MOD-HOOK-3, BOT-CTL-5]

## 5. A modder-facing scripting layer

### SCR-NEED-1: What a scripting layer for modders would need

Status: **inferred** · Variants: PC

No layer of the shipped game lets a modder write new game logic in a script language. A modder-facing layer built
on the findings above would need:

1. **A host in the DLL** (SCR-DLL-1): an embedded interpreter (Lua or similar) called from the hooks OPFOR already
   has - the server's AI frame (BOT-RT-2) for game logic, the UI frame for menu logic - on the thread each belongs
   to.
2. **Bindings by name, not address**: entity lookup by the level's names and GUIDs, an event send that hashes a name
   as the engine does (BOT-GS-3) and calls an entity's event handler, and reads of the engine settings by name
   (SET-ENG-1) - so scripts survive a change of hook addresses.
3. **An event bridge both ways**: a script-backed entity class (or an `InterfaceEntityData` the hook watches) so the
   level's event graph (SCR-EVT-1) can call scripts and scripts can fire graph events - the graph stays the authoring
   surface, the script adds logic the classes lack.
4. **The bot boundary**: the input object (BOT-CTL-3, BOT-CTL-4) and the game-state reads of bot_ai.md section 8,
   for script-driven bots.
5. **Menu reach**: new extern names answered by the getter hook (SCR-AS-1, MENU-EVT-3) and new commands in the
   dispatcher, so menu script can call scripts.
6. **Safety**: scripts must not hold engine pointers across frames (objects die with their level), must stay off the
   loading thread, and must be refused, not crash the game, on a missing name - the editor's validation registry
   (transforms/validation.md) is the model.

[inferred: from SCR-LAY-1 - SCR-DLL-1 and the bot module plan (BOT-CTL-4)]

## 6. Open questions

- Answered by round SPG3-rest: the setting and console command entities (SCR-CMD-2), the document server (SCR-DEV-2:
  never made), the unused classes' factories and input events (SCR-EVT-3). Left: the events of the factories
  SCR-EVT-3 did not follow (lights, emitters, meshes, bangers, `WeaponCallback`), five unnamed hashes, the events each
  class raises beyond the few named there, and a playtest of `ChangeSetting` (`bfo:docs/tasks/spec-task-notes.md`).
- What the debug-menu entities DICE left in the levels do when `IsMenuDevOptsEnabled` is set (a test start: `bfo:docs/tasks/spec-task-notes.md`).
- The fields of the 62 unused classes that have a factory: what each handler does with its data (their names are in
  the census's field tables; their meaning per class is open).
