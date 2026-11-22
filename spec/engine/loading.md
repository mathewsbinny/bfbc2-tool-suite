# Loading: from the startup bundle to a running level

What the game does between start-up and a level in play: the startup bundle it reads first, the level checksum,
how partitions come off the bundle stack and how their references are read, and the server's level load. The data
the loader reads is layer 2 (`model/bundles.md` for the stack and the markers, `model/registration.md` for
`package.mft`, `bundleManifest` and the start-up checksum, `model/level.md` for the level partition); this file is what
the executable does with it, and the crashes it gives when the data is wrong.

**Sources.** Project OPFOR BFBC2's reverse engineering of the PC client (`research/BFBC2_DECOMP_RESEARCH.md`, build
795745), the crash guards and probes of OPFOR's DLL (`src/dll/levelfix.cpp`: each records the function, the fault and
the level that hit it), OPFOR's install tools, and the suite's crash reader (its symbol table
`bfbc2-automated-map-and-bot-test-harness/crash-reader/symbols_795745.txt` holds the known crash sites with their
hints). Every crash address here was hit in a load test or a play test; the data behind it is named where it is
known. Addresses are of the PC retail `BFBC2Game.exe`, build 795745.

## 1. The phases of a load

### LOAD-PH-1: A level loads in session states, and a crash's state places it

Status: **observed** · Variants: PC

A session (campaign or offline multiplayer) runs through the states 5 -> 7 -> 13 -> 4 -> 8 -> 6 -> 9 -> 14 -> 3; state 3
is the level in play (NET-SES-2 for the start of a session). The state a crash happens in narrows where it is: the
level's description is looked up in state 13 (a missing description crashes there, LEVEL-DESC-1), partitions are
loaded in state 4 (the null-partition crash of LOAD-PART-1 was in state 4), and a level whose mode layer is missing
reaches 14 and stays (GAME-LAYER-1).

[exe: BFBC2Game.exe Session::updateState 0x56A6D0, Session::setState 0x557150; state sequences logged at run time, 2026-10-12 to 10-19]
[doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:102-106] [doc: opfor:src/dll/levelfix.cpp:272-275]

### LOAD-PH-2: The server's level load: AI, entities, save game

Status: **observed** · Variants: PC

On the server (the in-process one included) the level load (0x85F8A0) runs 0x574FB0, which logs and runs three steps
in order: "Init AI" (only for a level with `HasAI`, AI-START-1), "Spawn entities", "Load save game". Entity creation
therefore happens after the AI and its path data are set up: a crash in the AI's set-up (AI-CRASH-1, AI-CRASH-2)
comes before any entity of the level exists.

[exe: BFBC2Game.exe server level load 0x85F8A0 -> 0x574FB0 (strings "Init AI", "Spawn entities", "Load save game")]
[doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:99] [doc: bfbc2-automated-map-and-bot-test-harness/crash-reader/symbols_795745.txt:14]

## 2. The startup bundle

### LOAD-START-1: The startup bundle describes every level before any loads

Status: **implemented**

`async/startup-00.fbrb` (the `Package` copy, then the `Dist\win32` one) holds what the game needs before a level is
chosen: each level's description partition `Levels/<LV>_description.dbx` (LEVEL-DESC-1), the game-mode partitions
the descriptions name (`gamemodes/...`), the menus' level list `LevelListQuery.dbx`, `Menu/shared_config.dbx`, and
the engine's own post-process shaders (RNDR-POST-1). The menus build their level catalogue from the descriptions
(MENU-CAT-1); a level reaches a menu only through the level list's two items (`<lv>.dbx`, `<lv>_description.dbx`,
kept sorted as DICE keeps them). A tool that adds a level patches this bundle: the suite adds or takes out a level's
description and its two list items and leaves every other entry as it was.

[code: rust/bc2-levelwrite/src/export.rs:351-358] [code: rust/bc2-levelwrite/src/export.rs:3023-3127]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:848-852] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:655-659]

### LOAD-START-2: A game mode the description names must be in the startup bundle

Status: **observed** · Variants: PC

The game mode a description names is resolved when the level loads; it must be a partition the startup bundles hold.
OPFOR's Air Superiority mode (`GameModes/AirSuperiority`) is not a DICE partition: the first install that placed
Coral Sea without it in the shared startup patch crashed at load at 0x74EAB3 (LEVEL-DESC-2). The fix is data only:
the mode partition goes into the startup patch whenever an installed level runs it.

[exe: BFBC2Game.exe 0x74EAB3, Coral Sea's load, play test 2026-11-14]
[doc: opfor:src/tools/ww2/wake_install.py:3677-3681] [doc: opfor:research/WW2_PORT_PLAN.md:418-420]

## 3. The level checksum

### LOAD-CSUM-1: Md5Buffer: an MD5 pass over the level's Objects

Status: **observed** · Variants: PC

The level checksum is computed by FUN_006EFBC0 (`Md5Buffer` in OPFOR's records): a loop over the level partition's
`LevelData.Objects` that reads each entry and, for a placement, its blueprint. It reads both without a check, so the
two data defects that reach it each have their own fault address: an `Objects` entry that is null or names no
instance faults at 0x6EFF8A (LEVEL-OBJ-2), and a placement whose blueprint was not created (a `ReferencedObject` that
resolves nowhere, or to something that is not an `ObjectBlueprint`) faults at 0x6EFCA4 (CSUM-CHK-3, REF-RES-5). Both
were hit at a level's first load: ONS_MP_008 (a console Objects item naming an envmap the file did not hold) and
Wake Island (BF1943's effects placed as `ReferenceObjectData` naming the effect asset itself).

[exe: BFBC2Game.exe FUN_006EFBC0; faults at 0x6EFF8A (entry) and 0x6EFCA4 (blueprint); the loop's index at [esp+18h] and count at [esp+50h] name the entry]
[doc: opfor:src/dll/levelfix.cpp:513-521] [doc: opfor:research/BFBC2_NAPISAL_RESEARCH.md:275-277]
[doc: opfor:src/tools/ww2/wake_ambience.py:209-212] [doc: bfbc2-automated-map-and-bot-test-harness/crash-reader/symbols_795745.txt:21-22]

### LOAD-CSUM-2: The suite's checks catch both checksum defects before a launch

Status: **verified**

The checksum's two faults are data the suite can see offline: `DbxEdit check` refuses a null or dangling `Objects`
item (LEVEL-OBJ-2), and the map export's check refuses a reference into another partition that names no instance of
a partition the level loads. The crash reader names the 0x6EFF8A site and gives that hint.

[code: rust/bc2-formats/src/dbxedit/check.rs:317-343] [test: core/tests/test_dbxedit.py::cases]
[test: bfbc2-automated-map-and-bot-test-harness/crash-reader/tests/test_crashreader.py::main]
[test: rust/bc2-levelwrite/tests/export_sp01.rs::sp_01_exports_as_a_new_conquest_level]

## 4. Partitions and references at load

### LOAD-PART-1: A partition that does not parse is a null the loader passes on (0x49EDF4)

Status: **observed** · Variants: PC

The partition loader (0x4AAB60, thiscall(loader, desc)) parses a partition from its bundle's stream (0x4A3390) and
hands the result to 0x49EDF0, which reads it without a check: an entry that does not parse as a partition gives a
null there and the load faults at 0x49EDF4. The known cause is streamed wave data placed in a level bundle, which the
loader reads as a partition (SND-STRM-1; Wake Island's first load, session state 4).

[exe: BFBC2Game.exe partition load 0x4AAB60 -> stream parse 0x4A3390 -> 0x49EDF0, fault at 0x49EDF4 (state 4)]
[doc: opfor:src/dll/levelfix.cpp:272-275] [doc: opfor:src/tools/ww2/wake_install.py:181]

### LOAD-PART-2: A reference element must carry its target (0x4EB538)

Status: **observed** · Variants: PC

The data loader's reference reader (0x4EB4B0, thiscall(out, element, flags, ...)) takes an element's `ref` attribute
(or its `hint`) and compares it with `null`. An element flagged as a reference (flag bit 8) that has no `ref` value
dereferences a null string at 0x4EB538. Console input data in a PC level hit it (the Onslaught ports of Valparaiso
and Nelson Bay: the console action maps); OPFOR's port gives such seats PC input instead, and its DLL answers such an
element as a null reference and logs its field.

[exe: BFBC2Game.exe reference reader 0x4EB4B0, fault at 0x4EB538 (crash reports of the Valparaiso / Nelson Bay Onslaught ports, 2026-10-30)]
[doc: opfor:src/dll/levelfix.cpp:184-189] [doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:575-576]
[doc: opfor:DEVELOPMENT_NOTES.md:1270]

### LOAD-PART-3: An object's asset is read without a check (0x5A5A61)

Status: **observed** · Variants: PC

0x5A5A00 (thiscall(object)) reads an asset pointer from an object's data, at +0xD8 or +0x98 by the object's type, and
uses it unchecked: a null one faults at 0x5A5A61. Three data defects reached it, each an object whose asset did not
resolve in the level's bundles: a weapon component whose firing data no loaded bundle held (Napisal's Isla Inocentes
Onslaught: ten entries only `sp_005` has), a console vehicle whose mesh and physics were console-only builds
(Valparaiso's Mark V), and a vehicle part's missing tracer (the Mi-28's). The AI's path-data world set-up was on the
stack in the first case. The cure each time was to make the asset resolve (copy it into a bundle the level loads, or
leave the object out).

[exe: BFBC2Game.exe 0x5A5A00, fault at 0x5A5A61 on a null asset pointer (Valparaiso Onslaught load test, 2026-10-30)]
[doc: opfor:src/dll/levelfix.cpp:249-251] [doc: opfor:research/BFBC2_NAPISAL_RESEARCH.md:186-191]
[doc: opfor:src/tools/onslaught/console_vehicles.py:8] [doc: opfor:src/tools/onslaught/console_loadset.py:155]

### LOAD-PART-4: The voice-over set-up walks each kit's objects (0x6EA3F0)

Status: **observed** · Variants: PC

At load the level's voice-over set-up walks the data reachable from each soldier kit (0x6EA340, cdecl(object, a,
b)). At a `ReferenceObjectData` it follows the referenced `ObjectBlueprint`'s `Object` (+0x78, then +0x50) without a
check; a blueprint whose `Object` is null faults at 0x6EA3F0. OPFOR's soldier pass (console soldier classes as PC
blueprints) hit it in the automated load tests.

[exe: BFBC2Game.exe voice-over walk 0x6EA340 (ReferenceObjectData type 0x1C11E64), fault at 0x6EA3F0]
[doc: opfor:src/dll/levelfix.cpp:378-381] [doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:577-578]
[doc: opfor:DEVELOPMENT_NOTES.md:336-338]

### LOAD-PART-5: A level's sub-files must be its own

Status: **observed** · Variants: PC

The level's material grid and its physics sub-files are initialised against the level (0x6F2310, the material grid's
set-up); sub-files that belong to another level under another name hang the load there instead of crashing. The crash
reader's table records the cure: copy them under the level's name, with new partition GUIDs, as the files the engine
finds by the level's name are copied (PART-NAME-3).

[exe: BFBC2Game.exe 0x6F2310 material grid set-up (a hang, recorded in the crash reader's symbol table)]
[doc: bfbc2-automated-map-and-bot-test-harness/crash-reader/symbols_795745.txt:23]

### LOAD-PART-6: The server needs the level's server-side terrain

Status: **observed** · Variants: PC

The server queries the level's `ServerTerrainEntity` (0x5AC3F0); a level with none (console-only or wrong world data)
faults there. Whatever world a converted level takes, its terrain must exist in the server's form.

[exe: BFBC2Game.exe ServerTerrainEntity query 0x5AC3F0 (recorded in the crash reader's symbol table)]
[doc: bfbc2-automated-map-and-bot-test-harness/crash-reader/symbols_795745.txt:17]

## 5. Model groups at load

The group and member rules are layer 2 (LEVEL-MG-1 to LEVEL-MG-4); their crashes happen while the level's entities
are constructed.

### LOAD-MG-1: Model groups are constructed with their members' physics

Status: **observed** · Variants: PC

The model-group entity constructor (0x8FC260) reads each group's and member's `PhysicsData` and `MeshEntityType`
through 0xAF7140 without a check: an empty one faults at 0xAF7144 (LEVEL-MG-3). Its part set-up then sets a bit per
physics part of each member, by the group's part index: a group whose parts for a member do not match the member's
own physics sets a bit out of range and faults at 0xAB4E58 (LEVEL-MG-4; Wake Island's WallWooden, 6 parts in BFBC2's
copy against BF1943's 2). The static-model-group part set-up (0x8F4720) looks each member's physics part
up by the group's index: client-side group physics that lack a member's bodies fault there on an AI level (AI-CRASH-3).

[exe: BFBC2Game.exe model-group construction 0x8FC260, part data 0xAF7140 / fault 0xAF7144; part bitset fault 0xAB4E58 (0x62E740's loop); static group part set-up 0x8F4720]
[doc: opfor:src/dll/levelfix.cpp:524] [doc: opfor:research/BFBC2_NAPISAL_RESEARCH.md:278-280]
[doc: bfbc2-automated-map-and-bot-test-harness/crash-reader/symbols_795745.txt:30-31]
[doc: opfor:DEVELOPMENT_NOTES.md:1535-1537]

## Open questions

- **Which checksum is the start-up crash.** LOAD-CSUM-1's Md5Buffer runs over a level's Objects when that level
  loads; the start-up crash of a level registered wrongly (CSUM-CHK-1, CSUM-CHK-2) happens before any level loads.
  Whether the start-up computation runs the same function over every registered level, and what it does with
  `package.mft` and `bundleManifest`, is not known.
- **When the `async` bundles are mounted**: the startup bundle is read before any level, `ondemand_*` bundles are
  read by the game itself (SND-OD-1); the order and the precedence between them and a level's stack are not traced.
- **The load hang at 0x6F2310** and the server terrain query at 0x5AC3F0 are known from the crash reader's table only;
  the data that caused them is not recorded.
- **Whether every unchecked read has been found.** Each rule above came from one crash; the loader reads many more
  references without a check (REF-DEL-2). An executable-wide list of the loader's unchecked reads would turn the
  checks into a closed set.
