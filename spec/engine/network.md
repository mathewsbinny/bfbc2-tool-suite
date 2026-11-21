# Networking and hosting: the session, the in-process server, the server browser

How BFBC2 starts a match it hosts itself, how the client reaches that server, what the server browser shows and how
it names a row, and what a level or mode must declare so that the menus and the browser treat it right. Only what is
evidenced is here: the PC client's offline paths as Project OPFOR BFBC2 uses them. EA's online services (login,
matchmaking servers, stats) are not reachable and not specified.

**Sources.** Project OPFOR BFBC2's reverse engineering of the PC client (`research/BFBC2_DECOMP_RESEARCH.md` sections
4, 5, 6 and 13, build 795745; the hostMultiplayerLocal call chain was found statically and confirmed in game on
2026-10-12), its DLL's patches and their comments (`src/dll/patches.cpp`, `src/dll/server_browser.cpp`), the Pacific
War mode work (`research/WW2_THIRD_MODE.md`, `research/AIR_SUPERIORITY_RE_2026-11-14.md`). The mode keys, the game
mod and the stats category are rules of `engine/gameplay.md` (GAME-MODE-3 to GAME-MODE-7); this file cites them.

## 1. Starting a session

### NET-SES-1: The menu's host command becomes a host-game message

Status: **observed** · Variants: PC

The menus' commands are names looked up in a table of 574 `{name, id}` pairs (0x143C898): `startSingleplayer` 7,
`joinMultiplayer` 8, `hostMultiplayer` 9, `hostMultiplayerLocal` 0xA. The main-menu dispatcher (0x77BC10) sends ids 9
and 0xA to one handler (0x77BEC3), which queues a start request with `type = (id == 0xA)` and the level
`"Levels/" + argument`. The UI manager's request consumer (0x951E00) turns it into a host-game message (type 0x13,
0x38 bytes: the level at +0x28, `isLocal` at +0x34) on channel 0x28; the campaign's start is message 0x10.

[exe: BFBC2Game.exe command table .rdata 0x143C898 (574 entries); dispatcher 0x77BC10, switch 0x77BC8C, shared handler 0x77BEC3; queue 0x956E10 (UiManager+0x82CD0); consumer 0x951E00; HostGameMessage constructor 0x918D70]
[doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:90-95]

### NET-SES-2: The session hosts the level in process

Status: **observed** · Variants: PC

The session's message handler (0x56A890) takes message 0x13: the level to +0xB8, `isLocal` to +0xD4, +0xD5 = 0, +0xD0 =
1 (multiplayer), and state 5; message 0x17 aborts to the menu. In state 5 the local session start (0x565450) uses
`Persistence/MP_Template` for a multiplayer session, creates the server inside the game's own process (the object at
+0xDC; create 0x54B0B0) and connects the client to `127.0.0.1` (when +0xD4 is set) or `localhost`, port
`Network.ServerPort`. The load then runs through the session states of LOAD-PH-1 to state 3.

[exe: BFBC2Game.exe session onMessage 0x56A890 (jump table 0x56AA48; 0x13 -> 0x56A959); updateState 0x56A6D0 (state 5 -> 0x565450); server object vtable 0x01418140, create 0x54B0B0]
[doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:95-98]

### NET-SES-3: A local multiplayer session needs the in-memory transport

Status: **observed** · Variants: PC

The client's transport is chosen when the client is created (0x54AEE0, at 0x54AF2C):
`isSingleplayer ? 0 : (flagD5 ? 2 : 1)`, where 0 is the in-memory link, 1 sockets and 2 a remote join. A
`hostMultiplayerLocal` session is multiplayer with +0xD5 clear, so it gets sockets, but the in-process server never
listens on PC: the client times out after about 7 s (message 0x17) and the session returns to the menu. The campaign
works because it gets mode 0. OPFOR's patch selects mode 0 whenever the session's local flag (+0xD4) is set (only the
campaign and `hostMultiplayerLocal` set it); with it the user reached a running match (MP_005GR, states 5 to 3 in
about 7 s) with no socket opened. A remote join passes 0 and is unchanged.

[exe: BFBC2Game.exe createClient 0x54AEE0, selection at 0x54AF2C (22 bytes, continuing at 0x54AF42); setTransportMode 0x650E60: 0 in-memory 0x4F1E00, 1 sockets 0x4F1E60, 2 remote 0x5F0F40; play test 2026-10-12]
[code: opfor:src/dll/patches.cpp:3-17] [doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:98]

### NET-SES-4: Offline matches open no sockets

Status: **observed** · Variants: PC

With the in-memory transport a hosted match opens no socket at all. The client otherwise talks to EA's services
(login and matchmaking over WS2_32, web content over WinHTTP, ICMP pings, NetBIOS adapter queries); the game-server
join path (0xDBC440, which drops a player "not approved" at 0xDBA466 or for a "ticket mismatch" at 0xDBA4DE) is not
used offline. OPFOR's network guard keeps every lookup of the network modules on loopback.

[exe: BFBC2Game.exe game-server join 0xDBC440, drops at 0xDBA466 / 0xDBA4DE; imports of WS2_32, WinHTTP, ICMP, NETAPI32]
[doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:625-630] [doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:100]

### NET-SES-5: One game process at a time

Status: **observed** · Variants: PC

The client holds a named mutex for its whole run, so a second copy cannot start while one runs (the server build has
its own). OPFOR's and the suite's install tools separately refuse to write while the game runs.

[exe: BFBC2Game.exe named objects: mutex BFBC2-6A8D82A8-2C27-46db-A39C-96E64C8286B5 (client), BFBC2Server-124DD424-A478-4d95-A026-DA20E544D4E2 (server), SingleMgrMutex<pid>]
[doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:64-66]

## 2. The player

### NET-JOIN-1: The join carries the local profile; offline the player is "Human player"

Status: **observed** · Variants: PC

The client's join (0x55DFB0) is built from the player name at client+0x1604 and the local player profile (its clan
tag cut to 8 characters, letters and digits only by 0x743A70; its rank). Without an EA persona the server names the
local player `"Human player"` with id 1 (0x841930). The server-side join (0x865D70) creates the player's stats object
and asks the stats service for it (0x85AED0), a request never answered offline (GAME-AWARD-1).

[exe: BFBC2Game.exe Client::sendJoin 0x55DFB0, clan tag check 0x743A70, default name at 0x841930 (string 0x1433338); server join 0x865D70, stats request 0x85AED0]
[doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:154-160] [doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:171-173]

## 3. The server browser

### NET-SB-1: A browser row is a server record; its mode is a key

Status: **observed** · Variants: PC

The engine's server list (UiManager+0xC8958) is a vector of 0xF0-byte records: address +0x48 (empty: the row is
greyed), name +0x54, level +0x60, game-mode key +0x6C, players +0x84, maximum +0x88, ping +0x90. The row writer
(0x956A80) builds each row's values from a record; the level catalogue it looks rows up in is built from the
description partitions (MENU-CAT-1). The mode keys are `CONQUEST`, `RUSH`, `SQRUSH`, `SQDM` (GAME-MODE-3), matched
exactly and case-sensitively.

[exe: BFBC2Game.exe server list at UiManager+0xC8958 (record constructor 0x5ECF60), row writer 0x956A80, rows rebuilt by 0x958BF0; level catalogue UiManager+0xC3F34 built by 0x951480]
[doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:127-133]

### NET-SB-2: A key shared by two modes blanks the rows of one of them

Status: **observed** · Variants: PC

The row writer finds a row's catalogue entry in two steps: the name of the record's mode key (0x9293C0, called at
0x956C2F), then the record's level under that name (0x918990). The key-to-name walk returns the **first** mode name of
a key. A mode that shares another's key and comes first in the catalogue therefore takes every row of that key: OPFOR's
Air Superiority shares `CONQUEST` (its `MatchmakingQueryID`, which the stats need, GAME-MODE-4), so every Conquest row
was looked up under `airsuperiority`, where only Coral Sea is, and every other Conquest row (the Pacific War maps and
DICE's alike) showed no map and no mode. A new mode must either take a key of its own or have its rows named by the
catalogue's mode name (OPFOR's DLL does the latter at 0x956C2F).

[exe: BFBC2Game.exe row writer 0x956A80; key -> name walk 0x9293C0 (thiscall(UiManager+0xC3F34), ret 8) called at 0x956C2F; level lookup 0x918990; blank Conquest rows, 2026-11-16]
[code: opfor:src/dll/server_browser.cpp:421-431]

### NET-SB-3: Which levels a menu offers is the level's declaration

Status: **observed** · Variants: PC

What the menus offer is decided by the level's description, not by its bundles: the offline level list keeps only the
levels whose `MatchmakingModID` is the current game mod's tag (GAME-MODE-7), and a row's mode is its
`MatchmakingQueryID`'s key (NET-SB-2). A level for a new content set therefore declares its own `MatchmakingModID`
(OPFOR's `PACIFIC`) and needs a menu that knows the tag; one declared under another set's tag appears in that set's
list.

[exe: BFBC2Game.exe multiplayerMaps builder 0x9421F0 (tag compare at 0x942247 against record+0x30)]
[doc: opfor:research/WW2_THIRD_MODE.md:86] [doc: opfor:research/WW2_THIRD_MODE.md:288]

## Open questions

- Whether a remote join (transport 2) and a dedicated server (the Linux build) need anything of the level that the
  in-process host does not; only the in-process path has been run.
- What the stats service's commit path does offline beyond the unanswered request (where in-round results accumulate
  before the commit).
- Whether `Network.ServerPort` and the socket transport could host a LAN match on PC at all (the in-process server
  never listens; a listening server would need more than the transport choice).
- How the browser's record fields +0xE0 / +0xE4 (ids) and +0xEC (flags) are filled for an offline row.
