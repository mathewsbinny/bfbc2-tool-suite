# Crash index: every known fault address and the rule that explains it

An index, not a set of rules: each crash, hang or stall of `BFBC2Game.exe` (PC, build 795745) that Project OPFOR
BFBC2 or the suite has recorded, with the rule that states the data requirement behind it. Ordered by address. A new
crash gets a rule in its subsystem's file first, then a row here. The suite's crash reader
(`bfbc2-automated-map-and-bot-test-harness/crash-reader`) names many of these sites and prints their hints.

| Address | Where | Cause in the data | Rules |
|---|---|---|---|
| 0x00401090 (from 0x01046AB3) | purecall handler, at exit | Kynapse freed after its manager: unsorted path-data nodes | AI-CRASH-4, HPD-NODE-2 |
| 0x0049EDF4 | partition hand-off, state 4 | an entry that does not parse as a partition (streamed waves in a level bundle) | LOAD-PART-1, SND-STRM-1 |
| 0x004EB538 | reference reader 0x4EB4B0 | a reference element without its `ref` value (console input data) | LOAD-PART-2 |
| 0x0056C4E0 | cull-tree walk under the AI's sphere query | a cull tree that does not describe the level's objects | AI-CRASH-1, CULL-TRV-1 |
| 0x005A5A61 | asset user 0x5A5A00 | an object's asset that resolves nowhere (firing data, console-only vehicle, tracer) | LOAD-PART-3, XCONS-STG-2 |
| 0x005AC3F0 | ServerTerrainEntity query | no server-side terrain | LOAD-PART-6 |
| 0x006EA3F0 | voice-over walk 0x6EA340 | a blueprint whose `Object` is null | LOAD-PART-4 |
| 0x006EFCA4 | Md5Buffer 0x6EFBC0 | a placement whose blueprint was not created | LOAD-CSUM-1, CSUM-CHK-3, REF-RES-5 |
| 0x006EFF8A | Md5Buffer 0x6EFBC0 | a null or dangling `LevelData.Objects` item | LOAD-CSUM-1, LEVEL-OBJ-2 |
| 0x006F2310 (hang) | material grid set-up | level sub-files belonging to another level | LOAD-PART-5 |
| 0x0074EAB3 | level load | a description whose `GameMode` does not resolve | LEVEL-DESC-2, LOAD-START-2 |
| 0x008D59FB | client mesh set-up | a mesh set's LOD settings that resolve nowhere | RNDR-DRAW-7 |
| 0x008F4720 | static-model-group part set-up | client group physics on an AI level | AI-CRASH-3 |
| 0x00943B20 | `weaponProficiency$` | the EA stats copy, absent offline | GAME-AWARD-1 |
| 0x0098BD6E | renderer | a mesh part with no program for its vertex layout | RNDR-DRAW-6, XCONS-STG-2 |
| 0x009BC68E | renderer state flush | a released texture view still bound | RNDR-CRASH-2 |
| 0x00AB4E58 | model-group part bitset | a group's parts not matching a member's physics | LEVEL-MG-4, LOAD-MG-1 |
| 0x00AF7144 | model-group construction | an empty `PhysicsData` or `MeshEntityType` | LEVEL-MG-3, LOAD-MG-1 |
| 0x00BD2D30 (`_stricmp`), state 13 | level name table | no description partition for the level | LEVEL-DESC-1 |
| 0x00F30005 | not code, render path | unknown (Coral Sea, once) | RNDR-CRASH-1 |
| 0x00FF2AC2 | client sampler set-up | a null wave (hidden by markers, or an on-demand wave out of its bundle) | SND-NULL-1 |
| 0x010074E0 (hang) | Kynapse wait-ready | a resource the AI start asks for never loads | AI-CRASH-2 |
| 0x013046DA | movie unload 0x1304690 | a sprite with no frames but a non-zero frames pointer | MENU-UNL-1 |
| none: start-up | the server's level checksum | a new level not registered, or registered with absent bundles | CSUM-CHK-1, CSUM-CHK-2 |
| none: session state 14 stall | match start | a level without its mode layer | GAME-LAYER-1, LEVEL-MP-1 |
| none: the menus exit while loading | APT loader | two PushData sharing one list | APT-LOAD-2 |

Defects that do not crash but have their own rules: an object at cull slot 0 draws see-through and cannot be
destroyed (CULL-SLOT-3); a terrain whose mask atlas differs from its donor's draws black cells (TERR-VT-5); a mode
key shared by two modes blanks the server browser's rows (NET-SB-2); a `_NAME` key that names no picture gives a black
loading screen (LVL-DESC-2).

**Not explained yet.** OPFOR's crash logger names a load-test crash at 0x61084D with no recorded cause; it has no rule.
