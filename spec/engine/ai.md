# AI and navigation: what the bots run on

What BFBC2's server does to run bots on a level: when the AI starts, what it loads, the Kynapse world that reads the
path data, how a route request is answered, what a level must carry for bots to fight on it, and the crashes the AI's
set-up and shutdown are known to give. The path data's bytes are layer 1 (`formats/hpd.md`, with its runtime notes
HPD-USE-1, HPD-USE-2, HPD-NODE-2, HPD-RT-1 and HPD-RT-2); the suite's samples of the engine's physics are
`formats/navsample.md`; building, applying and installing path data is layer 4 (`transforms/navkit.md`), the stock
multiplayer AI layer `transforms/stockmp.md`.

**Sources.** Project OPFOR BFBC2's reverse engineering and live probes of the PC client
(`private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md` sections 8, 9 and 12, build 795745), its stock-map bot plan with the data
survey of 54 level files (`bfo:docs/plans/opfor/STOCK_MP_AI_PLAN.md` section 12), the Onslaught imports (`private:opfor-research/research/BFBC2_NAPISAL_RESEARCH.md`),
OPFOR's DLL probes, and the suite's crash reader table. "Working AI level" means a level on which bots were seen to
move and fight in a play test: DICE's campaign levels, Napisal's PC Onslaught ports, OPFOR's console Onslaught ports.

## 1. Starting the AI

### AI-START-1: The AI starts only for a level with HasAI, on the server

Status: **observed** · Variants: PC

The server's level load (LOAD-PH-2) tests the level data's `HasAI` byte (+0x3AE) at 0x575046 and, only when it is
set and the engine setting `Server.IsAiEnabled` is true, runs the AI's init (0x5B33C0), its set-active (0x5958F0,
which stores `HasAI` and `Server.IsAIEnabled` as the AI's active flag) and the path-data world (0x5AF5F0). A level
with `HasAI` false skips all three: every stock multiplayer level (AI-NEED-1). Setting the flag later (at the
set-active) does nothing; OPFOR's `force_ai` research option must act at the 0x575046 test.

[exe: BFBC2Game.exe server level load 0x574FB0: cmp byte [level+0x3AE], 0 at 0x575046; AI init 0x5B33C0, set-active 0x5958F0, path-data world 0x5AF5F0; settings Server.IsAiEnabled / Server.IsAIEnabled]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:208-221] [doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:472-475]
[doc: private:opfor-research/DEVELOPMENT_NOTES.md:344-347]

### AI-START-2: The AI's settings come from seven shared partitions

Status: **observed** · Variants: PC

The AI's init (0x5B33C0, with the difficulty: 0 casual, 1 normal, 2 hard) loads `AI/Behaviours`, `AI/Settings`,
`AI/Orders`, `AI/Difficulty`, `AI/Formations`, `AI/Squads` and `AI/Animations`; `AI/Settings` holds the sensing,
cover and navigation constants and `AI/Difficulty` the per-difficulty health, accuracy, reaction and aim values. A
level's bundles must reach these partitions (Napisal's Onslaught levels carry their own `Ons_*` difficulty profiles).

[exe: BFBC2Game.exe AI init 0x5B4B20 -> 0x5B33C0(difficulty); singletons at *(0x018903EC) +0x498 settings, +0x4A0 orders, +0x4A4 difficulty]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:211-214] [doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:242-245]
[doc: private:opfor-research/research/BFBC2_NAPISAL_RESEARCH.md:125]

### AI-START-3: One Kynapse world per pathfinding type the level lists

Status: **observed** · Variants: PC

The path-data world set-up (0x5AF5F0) hands the level data to 0x5A89F0, which walks `PathfindingTypes` (the array at
LevelData +0x36C, `PathfindingSettings` references) and collects each entry's `OutputType` (+0x3C, the
`PathfindingType`: `InfantryData`, `VehicleData`) into a list. 0xF2EFF0 then checks the runtime licence string and,
when the list is empty, returns at once: no Kynapse world, no path data, no error. Otherwise the world set-up
(0xF1B980) configures one path-data world per listed type from that type's `OutputName` (the folder, HPD-USE-1),
`SubgraphBoxSize` and `DistEdgeMax` (KYRT-CFG-1); a type whose `OutputName` is `Infantry` is recorded as the
infantry world. So what decides the worlds is each setting's `OutputType`, not the setting's own name, and a level
with `HasAI` and an empty `PathfindingTypes` starts the AI with no path data at all. After the worlds, 0x5AF5F0 runs
the flying AI's set-up (0x59A140, its `AerialHeightmap`, GFILE-AHM-1) only when `HasFlyingAI` (+0x3AD) is set, hands
`DestructibleLocalPathfindingParts` (+0x388) to the local-pathfinding layer (KYRT-DYN-3) and sets up each of
`ExplosiveObjects` (+0x398, 0x5A6A40).

[exe: BFBC2Game.exe 0x5AF5F0 -> 0x5A89F0 (LevelData +0x36C through 0x13A5660; each entry's +0x3C read at 0x5A8A34-0x5A8A41) -> 0xF2EFF0 (licence check 0xF2F0BF with the string at 0x014A9EA4; empty list `cmp [ebp], [ebp+4]` at 0xF2F0D2 -> return 0 at 0xF2F4F9; world set-up called at 0xF2F0F3; `Infantry` compared against the type's +0x30 name, the world's +0x3EC written); `HasFlyingAI` call 0x5AF6D3; +0x388 through 0x13A5640 to 0xF27E10 at 0x5AF6B6 / 0x5AF6BE; +0x398 through 0x13A5680, each entry to 0x5A6A40]
[data: PathfindingTypes infantry + vehicles on Napisal's ONS_MP_005GR, ONS_MP_002, ONS_MP_004, DICE's Xbox ONS_MP_002 and SP_004_B; empty on every stock MP level read]
[doc: bfo:docs/plans/opfor/STOCK_MP_AI_PLAN.md:294-298] [doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:345-347]

### AI-START-4: Demo playback is the only other branch of the level load

Status: **observed** · Variants: PC

Before the `HasAI` test (AI-START-1) the server's level load reads a flag of the start-up demo settings: the
68-byte object the game's main object builds at start-up (+0xAC), handed to the server when it is created and kept at
the game context's +0x20. Its +0x1C is 1 only when `Demo.PlaybackDemoFileName` is set (a demo being played back) and
cleared when `Demo.RecordDemoFileName` is set. With the flag set the level load skips the AI's start, the normal
entity spawn (0x571CF0) and the save-game load, and creates only one kind of entity through 0x571F10. In every
ordinary game, offline matches included, the flag is 0: the gate is no obstacle to bots, and a host must simply not
set `Demo.PlaybackDemoFileName`.

[exe: BFBC2Game.exe level load 0x574FB0: the context (0x547BF0, the object at 0x01566B18) +0x20 -> +0x1C tested at 0x57501B-0x575031 (else 0x571F10), again before the save-game load at 0x5751B0-0x5751B6; the settings object made by 0x596AF0 at the main object's +0xAC in 0x56EDE0, +0x1C = 1 at 0x56F26D (playback), = 0 at 0x56F1A9 (recording); passed by the server create 0x54B0B0 (its +0x1C as argument 5) to 0x756D40 (stored at +0x20, 0x756D6B) and by 0x853B00 to the context's +0x20 (0x853C7E)]

## 2. The Kynapse runtime

### AI-KY-1: The path-data world is configured from the level's own assets at load

Status: **observed** · Variants: PC

The path-data set-up (0x00F209F1-0x00F20C8E) builds the path of `.../AI/.../PathData.hpd` and of
`<level>/Precomputed/<kind>/SectorOffsets`, loads the offsets asset as a typed resource and passes its `XOffset` and
`ZOffset` to Kynapse as `SectorOffsetX` / `SectorOffsetZ`, then `SectorSize` and `SectorOverlap`. A missing asset is
not an error: the loader returns a default object, offsets 0, 0, and route requests in sectors whose numbering
depends on the offsets fail (HPD-USE-2). A level that runs bots must ship the asset, matching its path data's own
sector ids (the suite writes it from them: XSMP-AUT-1).

[exe: BFBC2Game.exe path-data set-up 0x00F209F1..0x00F20C8E (strings at 0x014A93D0 / 0x014A93E0, offsets passed from +0x1C / +0x18); probes at 0x00F20AD2, 0x00F20B48, 0x00F20BB0]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:464-471]

### AI-KY-2: One runtime world, and no usable generator

Status: **observed** · Variants: PC

Kynapse's world registry holds one world by default (a kernel parameter set to 1 at 0x00F025EC), and the AI's
runtime world (mode `Minimal`, checking the game's runtime licence string) occupies it. The path-data generator is
compiled into the client (the build driver 0x00F299A0 with its map-, mesh- and graph-building steps) but nothing
calls it, and its map builder asks for a FLEXnet developer licence the game does not have. Path data for a level
without DICE's therefore comes from existing data (campaign, console, Napisal's) or from a generator outside the game
writing the `.hpd` format (NavKit, `transforms/navkit.md`); OPFOR does not bypass the licence.

[exe: BFBC2Game.exe world capacity at 0x00F025EC (C7 46 08 01), registry refusal 0x0109E930; generator driver 0x00F299A0 with no caller in .text; Kaim::CMapBuilder licence request for feature CMapBuilder (error -1,359), 2026-10-14]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:284-292] [doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:348-351]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:363-370] [doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:378-379]

### AI-KY-3: A go-to is computed at once; "true" means the request was taken

Status: **observed** · Variants: PC

BFBC2's pathfinders run in Kynapse's `Flat` data mode (`DataMode` 1, the constructor's default): the go-to
(0x0107FE80) computes the path inside the call (0x0107F620) and returns its result. The caller (0x010B1AB0) reads the
path and sets a state: 1 when the path ends exactly at the destination, 0 when it is empty or ends elsewhere, 2 for no
path. A path that is not followed is asked for again about 9.6 s later. The computation fails on seven exits (start or
destination not resolved to a route node, either id missing from the query's result set, both ids equal, the second
search stage reaching no node); its reason is kept at pathfinder +0x110 (1 start, 2 destination, 3 later stage).
Resolving the points is HPD-RT-1; finding the ids needs each sector's nodes sorted (HPD-NODE-2). The later stage is
the graph search itself: the A* of KYRT-PATH-1 (0x010795E0; another search, 0x0107B790, when the path finder's
+0x474 flag is clear) from the start's route node to the destination's. Reaching the destination's node writes the
path; a search stopped at its 100-node budget returns true with the path still to come; anything else is reason 3.
So "the stage reached no node" means the graph has no route between the two nodes (AI-NAV-1), not a missing list.

[exe: BFBC2Game.exe go-to 0x0107FE80, Flat compute 0x0107F620 (false exits 0x0107F8E1, 0x0107F9B0, 0x0107FABE, 0x0107FB0D, 0x0107FC33, 0x0107FCB2, 0x0107FD6B), caller 0x010B1AB0, reader 0x010B2400; DataMode default 1 in the constructor 0x01080CC0]
[exe: the later stage: `cmp byte [esi+0x474], 0` at 0x0107FCEB, A* 0x010795E0 called at 0x0107FD0A (result 4: the path written, the path finder's +0x64 = 8; result 3: true, resumed later, KYRT-PATH-1's time slicing), the other search 0x0107B790 at 0x0107FD77; reason 3 stored at 0x0107FD3E]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:413-449]

### AI-KY-4: The engine's physics queries

Status: **observed** · Variants: PC

The engine answers physics queries on the level's Havok world through a helper the AI manager holds (+0x130 of
*(0x0189037C)): a sphere cast along a ray (0xB145C0, returning the hit's position and normal; a cast that starts
inside geometry hits at once), a capsule overlap (0xB14640) and a point / sphere proximity test (0xB145A0). The closest
hit's body names what was hit by its shape's vtable (terrain, object, soldier). The game's own path-data generator
takes its geometry from these queries, and OPFOR's in-game sampler uses them to record what NavKit builds from
(`formats/navsample.md`).

[exe: BFBC2Game.exe ray / sphere cast 0xB145C0 (-> 0xB12F60, world vtable +0x34), capsule 0xB14640, point query 0xB145A0 (world vtable +0x3C); ray collector add-hit 0xB0D370, root collidables at +0x50 / +0x54; shape vtables 0x01454D14 terrain, 0x01483B38 objects, 0x01483378 soldiers]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:381-396]

### AI-KY-5: The ground the AI and collision see is the Havok height-field shape

Status: **observed** · Variants: PC

The terrain's collision is one Havok shape (the terrain shape vtable 0x01454D14 of AI-KY-4), built by the physics
world's terrain set-up (0xB0BAB0) from the runtime height field it is given: the sample count and the size give the
sample spacing, size / (count - 1). The AI does not read the height field itself: its physics queries (AI-KY-4) hit
that shape like any other body, and its path data was sampled from the same shape when it was built. The visual
terrain's build (0xA2D690, its jobs `terrainPrepBld`, `terrainBuild`, `terrainPostBld`, `terrainCullView`) is
rendering only. Kynapse's own height-field reader (0xF33620, the `.Heightfield-000` name) belongs to the generator's
steps, which nothing calls (AI-KY-2). So a sculpted height field changes collision and the AI's ground with the shape;
the path data must be built again (TEDIT-NAV-1).

[exe: BFBC2Game.exe terrain shape constructor 0xAC7640 (vtable 0x01454D14 stored at 0xAC7656), called from 0xB0BAB0 (the physics world's vtable slot 0x014575EC; spacing from the height field's +0xE0 / (+0xE8 - 1), shape kept at +0xA3A4); a second terrain-shape constructor 0xAB8800 (0xAB8806); visual terrain build 0xA2D690 (strings 0x0144FEB8-0x0144FF18); generator reader 0xF33620 (string 0x014AA4DC) reached only through 0xF33730 in the table at 0x014AA390]
[inferred: that the object 0xB0BAB0 receives is the level's main height field (its fields are a count and a size, as TerrainHeightfieldData's Resolution and SizeXZ); not followed to the resource loader]

## 3. What a level needs for bots

### AI-NEED-1: Stock multiplayer levels carry no AI

Status: **observed** · Variants: PC, Xbox 360

Every stock multiplayer level has `HasAI` and `HasFlyingAI` false, an empty `PathfindingTypes`, no
`SpawnPointManager`, no path data under `Output\win32\ai\levels\` and no `SectorOffsets`, no area or patrol orders
and no squad entities; MP_002 has one AI spawner, disabled. The levels do carry what bots could use: objectives,
capture points, area geometry (769 points on MP_002), teams and squads. A stock map is a human-only level: bots need
an authored AI layer (XSMP-AUT-1), not a switch.

[data: stock MP levels compared (all launch MP levels, identical on Xbox 360): HasAI false, no PathfindingTypes, no SpawnPointManager; path data only for the 13 campaign levels; MP_002: 1 AI spawner, disabled, 0 AreaOrder, 0 PatrolOrder]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:590-596] [doc: bfo:docs/plans/opfor/STOCK_MP_AI_PLAN.md:55-71]

### AI-NEED-2: What a working AI level has

Status: **observed** · Variants: PC, Xbox 360

Every level on which bots were seen to fight has: `HasAI` true; `PathfindingTypes` naming the infantry and vehicle
settings; path data for both types with its `SectorOffsets`; enabled AI spawners (`SSAiPlayer`) with a real AI
template and a kit; and orders (area, patrol, search-and-destroy, follow) whose soldiers are those spawners, directly
or through squads. DICE names every enabled AI spawner in an order. Squads are optional (adding them changed nothing
in OPFOR's test). On Conquest the spawners need no event logic (they spawn from the beginning); on Rush each stage's
trigger moves the AI on (GAME-RUSH-1).

[data: SSAiPlayer spawners enabled / named by an order: Napisal ONS_MP_005GR 24 / 17, ONS_MP_002 28 / 28, Xbox ONS_MP_002 78 / 78 (54 via squads), SP_004_B 91 / 91; orders 7, 41, 47, 182; SquadEntityData 0, 0, 17, 16]
[doc: bfo:docs/plans/opfor/STOCK_MP_AI_PLAN.md:281-293] [doc: bfo:docs/plans/opfor/STOCK_MP_AI_PLAN.md:330-338]

### AI-NEED-3: Bots need a real AI template

Status: **observed** · Variants: PC

An AI spawner's `Template` decides whether its bots fight: with the `DefaultSoldier` placeholder (combat field of
view 0, no alert range) bots aim and never fire; with DICE's grunt template they fire. The spawner also sets the
bots' team. The template must be in a bundle the level loads: a multiplayer level's bundles load `mp_common`, not
`sp_common`, so a campaign template is packed into the level's own bundles (as on ONS_MP_002).

[data: OPFOR's Onslaught play tests: DefaultSoldier bots aim and do not fire, DICE's grunt template fires; mp_005's package.mft block loads its own bundles and mp_common only]
[doc: bfo:docs/plans/opfor/STOCK_MP_AI_PLAN.md:41-43] [doc: bfo:docs/plans/opfor/STOCK_MP_AI_PLAN.md:312-314]

### AI-NEED-4: The stock SSAiPlayer spawners are the human spawn points

Status: **observed** · Variants: PC

Many stock multiplayer spawners are marked `PlayerType = SSAiPlayer` with a 15 s spawn delay, but they are the
human spawn points: each is listed in a capture point's `Spawners`, carries the ground positions as
`AlternativeSpawnPoints` (the spawner itself 25-92 m above the ground), has no event connection, no template, an
`Amount` of 1 and `SpawnSoldierFromBeginning` false; Team2's carry the US kit. The field does not change human
spawning (Laguna Alta's are `SSHumanPlayer` with delay 0). Whether they spawn bots when the AI runs is untested; the
suite's AI layer adds its own spawners and marks the stock ones `SSHumanPlayer` by default.

[data: SSAiPlayer with SpawnDelay 15: Arica Harbor CQ 8 of 8, White Pass CQ 7 of 7, Nelson Bay Rush 11 of 13, Atacama CQ 7 of 8 (all enabled, Template null)]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:597-600] [doc: bfo:docs/plans/opfor/STOCK_MP_AI_PLAN.md:322-329]

### AI-NEED-5: Kills of bots score nothing offline without the scoring patch

Status: **observed** · Variants: PC

The server's player-killed handler (0x86DC90, from the game-event dispatcher 0x870070, event type 3) returns at once
only for a null victim; a bot is a server player, so its death reaches the kill scoring (0x86C070,
`PointSystemParams.KillScore`) like a human's. What stops the scoring offline is the server's scoring flag (+0x2A),
which stays clear with fewer than 4 clients: nobody scores, humans included (BOT-SCO-3). With OPFOR's patch
(MOD-PATCH-1) a human's kill of a bot scores; the bots themselves never earn score and their deaths are not counted
(BOT-SCO-2). Whether bots count for capturing and for tickets is not known (BOT-GS-3).

[exe: BFBC2Game.exe game-event dispatcher 0x870070 (type at +0x88, 3 = player killed) -> 0x86DC90 (returns for a null victim at 0x86DCAE; scoring flag test 0x86DD97) -> kill scoring 0x86C070 (returns for an AI killer at 0x86C12D) -> award 0x86BE20]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:579-581]

### AI-NAV-1: A bot reaches only what the route graph connects to the main network

Status: **inferred** · Variants: PC

A route request is answered on the path data's route graph (AI-KY-3, HPD-USE-1); the graph falls into islands, the
largest of which carries nearly every route (the AI workspace numbers them, island 0 the main network). A spawn point
whose nearest route node lies on another island, or with no node near it, leaves its bots nowhere to go, and an
objective there is out of the bots' reach. The export's XVAL-PLACE-5 checks the edited spawns, flags and M-COMs
against the path data the AI workspace reads (a node within 15 m in plan and 4 m in height, as the workspace joins
spawns to the graph).

[inferred: Kynapse answers a go-to from the graph of the path data's sectors (AI-KY-3); points not connected to the requester's node have no route; GR2's island and spawn tables show DICE's own campaign files with islands besides the main network]
[doc: rust/BENCHMARKS_ARCHIVE_1.md:5684-5692]

## 4. Crashes and hangs

### AI-CRASH-1: The AI's sphere query walks the static cull tree (0x56C4E0)

Status: **observed** · Variants: PC

The AI's sphere query (0x577390: a position and a radius) collects the objects near a point by walking the level's
static cull tree (0x56C4E0, CULL-TRV-1). OPFOR's console-ported Nelson Bay crashed there at level load on a garbage
child index while walking an object's part tree: a cull tree that does not describe the level's objects is read by
the AI as well as by the renderer (CULL-SLOT-3).

[exe: BFBC2Game.exe AI sphere query 0x577390 -> cull-tree walk 0x56C4E0, fault at level load (console Nelson Bay, 2026-10-29)]
[doc: opfor:src/dll/levelfix.cpp:86-90]

### AI-CRASH-2: The AI start can wait for a resource that never loads

Status: **observed** · Variants: PC

During the AI's start Kynapse waits for each resource it asks for (0x010074E0); a resource that never arrives (level
sub-files under another level's name, LOAD-PART-5) is a hang there rather than a crash.

[exe: BFBC2Game.exe Kynapse wait-ready 0x010074E0 (probe "AI: Kynapse wait-ready"; the crash reader's symbol table)]
[doc: opfor:src/dll/probes.cpp:113] [doc: bfbc2-automated-map-and-bot-test-harness/crash-reader/symbols_795745.txt:47]

### AI-CRASH-3: An AI level needs the server's static-group physics

Status: **observed** · Variants: PC

The AI-side static-model-group set-up (0x8F4720) looks up a physics part for every group member. The PC client's
group physics of Nelson Bay lack the parts it looks up, so a client level is no base for an AI level; the official
Linux dedicated server's level (R34) has them, and OPFOR's console Nelson Bay is built on it (with its
`materials_linux`) and loads to state 3.

[exe: BFBC2Game.exe static-model-group part set-up 0x8F4720 (physics-part lookup 0xA9E6A0)]
[doc: private:opfor-research/research/BFBC2_NAPISAL_RESEARCH.md:269-273] [doc: bfbc2-automated-map-and-bot-test-harness/crash-reader/symbols_795745.txt:30]

### AI-CRASH-4: Unsorted path data crashes the game on exit (0x401090)

Status: **observed** · Variants: PC

At process exit Kynapse frees its containers through its memory callback (0x01046AA0 into the manager at
[0x01B58518], a virtual call). With path data whose sector nodes were not sorted by id, a release reached the manager
after it had been destroyed: a pure virtual call (the handler 0x401090, return 0x1046AB3). It went away when NavKit
wrote each sector's nodes sorted (HPD-NODE-2); DICE's files never hit it. The crash reader shows the hint (restore
the original path data).

[exe: BFBC2Game.exe purecall handler 0x401090 from 0x01046AB3 (Kynapse release 0x01046AA0, from 0x010A248A in the container destructor 0x010A2450), at exit after a NavKit play test, 2026-10-19]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:450-461] [doc: opfor:src/dll/probes.cpp:118-119]
[doc: bfbc2-automated-map-and-bot-test-harness/crash-reader/symbols_795745.txt:48]

## Open questions

- Whether the stock `SSAiPlayer` spawners spawn bots when the AI runs on a stock map (AI-NEED-4), and whether bots
  use objectives without orders.
- Whether bots count for capturing (`MinNrToTakeControl`) and for tickets (AI-NEED-5, BOT-GS-3); where bots' own
  scores are skipped is BOT-SCO-2.
- Answered in round DQ5: the gate before the AI's start is demo playback (AI-START-4); the per-type load is
  observed (AI-START-3); the go-to's later stage is the A* search (AI-KY-3); what reads the height field for
  collision and the AI (AI-KY-5).
- Whether the go-to's caller ever sees the time-sliced "true without a path" (AI-KY-3) on DICE's levels, and which
  path finders clear +0x474.
- Why a go-to needs a route node near the order position (HPD-RT-1). (The tile size is the mesh's resolution field:
  KYN-TILE-1, KYRT-TILE-1; the runtime's loading and searching is `engine/kynapse_runtime.md`.)
- The flying AI's data (`HasFlyingAI`): what 0x59A140 loads; no stock MP level and only Napisal's ONS_MP_005GR set it.
