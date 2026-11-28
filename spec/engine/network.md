# Networking and hosting: the session, the in-process server, the server browser

How BFBC2 starts a match it hosts itself, how the client reaches that server, what the server browser shows and how
it names a row, and what a level or mode must declare so that the menus and the browser treat it right. Only what is
evidenced is here: the PC client's offline paths as Project OPFOR BFBC2 uses them, and (sections 4 to 8, round NET2)
the network layer underneath them as the executable shows it statically: the transports, the settings and start-up
options, the version and anti-cheat checks, the server's listen path and the client side of EA's back-end. EA's
services themselves (login, matchmaking servers, stats) are not reachable and are not specified. What the network
layer would need for LAN, Direct IP or hosted play is studied in `engine/network_feasibility.md`.

**Sources.** Project OPFOR BFBC2's reverse engineering of the PC client (`private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md` sections
4, 5, 6 and 13, build 795745; the hostMultiplayerLocal call chain was found statically and confirmed in game on
2026-10-12), its DLL's patches and their comments (`src/dll/patches.cpp`, `src/dll/server_browser.cpp`), the Pacific
War mode work (`bfo:docs/research/opfor/WW2_THIRD_MODE.md`, `bfo:docs/research/opfor/AIR_SUPERIORITY_RE_2026-11-14.md`). The mode keys, the game
mod and the stats category are rules of `engine/gameplay.md` (GAME-MODE-3 to GAME-MODE-7); this file cites them.
Round NET2 (2026-11-23) read a scratch copy of the same executable (sha256 c603f686...0162) with pefile and capstone:
import tables, string cross-references, jump tables and short disassemblies, no run. Nothing it adds was seen at run
time; the statuses say so.

## 1. Starting a session

### NET-SES-1: The menu's host command becomes a host-game message

Status: **observed** · Variants: PC

The menus' commands are names looked up in a table of 574 `{name, id}` pairs (0x143C898): `startSingleplayer` 7,
`joinMultiplayer` 8, `hostMultiplayer` 9, `hostMultiplayerLocal` 0xA. The main-menu dispatcher (0x77BC10) sends ids 9
and 0xA to one handler (0x77BEC3), which queues a start request with `type = (id == 0xA)` and the level
`"Levels/" + argument`. The UI manager's request consumer (0x951E00) turns it into a host-game message (type 0x13,
0x38 bytes: the level at +0x28, `isLocal` at +0x34) on channel 0x28; the campaign's start is message 0x10.

[exe: BFBC2Game.exe command table .rdata 0x143C898 (574 entries); dispatcher 0x77BC10, switch 0x77BC8C, shared handler 0x77BEC3; queue 0x956E10 (UiManager+0x82CD0); consumer 0x951E00; HostGameMessage constructor 0x918D70]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:90-95]

### NET-SES-2: The session hosts the level in process

Status: **observed** · Variants: PC

The session's message handler (0x56A890) takes message 0x13: the level to +0xB8, `isLocal` to +0xD4, +0xD5 = 0, +0xD0 =
1 (multiplayer), and state 5; message 0x17 aborts to the menu. In state 5 the local session start (0x565450) uses
`Persistence/MP_Template` for a multiplayer session, creates the server inside the game's own process (the object at
+0xDC; create 0x54B0B0) and connects the client to `127.0.0.1` (when +0xD4 is set) or `localhost`, port
`Network.ServerPort`. The load then runs through the session states of LOAD-PH-1 to state 3.

[exe: BFBC2Game.exe session onMessage 0x56A890 (jump table 0x56AA48; 0x13 -> 0x56A959); updateState 0x56A6D0 (state 5 -> 0x565450); server object vtable 0x01418140, create 0x54B0B0]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:95-98]

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
[code: opfor:src/dll/patches.cpp:3-17] [doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:98]

### NET-SES-4: Offline matches open no sockets

Status: **observed** · Variants: PC

With the in-memory transport a hosted match opens no socket at all. The client otherwise talks to EA's services
(login and matchmaking over WS2_32, web content over WinHTTP, ICMP pings, NetBIOS adapter queries); the game-server
join path (0xDBC440, which drops a player "not approved" at 0xDBA466 or for a "ticket mismatch" at 0xDBA4DE) is not
used offline. OPFOR's network guard keeps every lookup of the network modules on loopback.

[exe: BFBC2Game.exe game-server join 0xDBC440, drops at 0xDBA466 / 0xDBA4DE; imports of WS2_32, WinHTTP, ICMP, NETAPI32]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:625-630] [doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:100]

### NET-SES-5: One game process at a time

Status: **observed** · Variants: PC

The client holds a named mutex for its whole run, so a second copy cannot start while one runs (the server build has
its own). OPFOR's and the suite's install tools separately refuse to write while the game runs. The executable's own
switches lift this (NET-CFG-3): `-runMultipleGameInstances` or `-dedicated` skips the mutex, `-server` takes the
server's name. One process is therefore the default, not a limit of the executable; OPFOR's user-data redirect also
renames the mutex (`-OPFOR`), which is why OPFOR and the base game already run side by side.

[exe: BFBC2Game.exe named objects: mutex BFBC2-6A8D82A8-2C27-46db-A39C-96E64C8286B5 (client), BFBC2Server-124DD424-A478-4d95-A026-DA20E544D4E2 (server), SingleMgrMutex<pid>]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:64-66]

## 2. The player

### NET-JOIN-1: The join carries the local profile; offline the player is "Human player"

Status: **observed** · Variants: PC

The client's join (0x55DFB0) is built from the player name at client+0x1604 and the local player profile (its clan
tag cut to 8 characters, letters and digits only by 0x743A70; its rank). Without an EA persona the server names the
local player `"Human player"` with id 1 (0x841930). The server-side join (0x865D70) creates the player's stats object
and asks the stats service for it (0x85AED0), a request never answered offline (GAME-AWARD-1).

[exe: BFBC2Game.exe Client::sendJoin 0x55DFB0, clan tag check 0x743A70, default name at 0x841930 (string 0x1433338); server join 0x865D70, stats request 0x85AED0]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:154-160] [doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:171-173]

## 3. The server browser

### NET-SB-1: A browser row is a server record; its mode is a key

Status: **observed** · Variants: PC

The engine's server list (UiManager+0xC8958) is a vector of 0xF0-byte records: address +0x48 (empty: the row is
greyed), name +0x54, level +0x60, game-mode key +0x6C, players +0x84, maximum +0x88, ping +0x90. The row writer
(0x956A80) builds each row's values from a record; the level catalogue it looks rows up in is built from the
description partitions (MENU-CAT-1). The mode keys are `CONQUEST`, `RUSH`, `SQRUSH`, `SQDM` (GAME-MODE-3), matched
exactly and case-sensitively.

[exe: BFBC2Game.exe server list at UiManager+0xC8958 (record constructor 0x5ECF60), row writer 0x956A80, rows rebuilt by 0x958BF0; level catalogue UiManager+0xC3F34 built by 0x951480]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:127-133]

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
[doc: bfo:docs/research/opfor/WW2_THIRD_MODE.md:88] [doc: bfo:docs/research/opfor/WW2_THIRD_MODE.md:290]

## 4. The transports

### NET-TRN-1: The transport is a socket factory installed for the whole process

Status: **observed** · Variants: PC

`setTransportMode` (0x650E60, NET-SES-3) does not open anything itself: it allocates a small factory object and
installs it as the process's socket creator (for modes 0 and 1 it also hands a new object to channel 0x25 of the
message manager). Mode 0 installs the
in-memory factory (constructor 0x4F1E00, vtable 0x1411934), whose sockets are queues inside the process (0x4F1D60);
mode 1 the UDP factory (0x4F1E60, vtable 0x1411960), whose create methods (0x4F5280, 0x4F5380, 0x4F54A0) build
Winsock UDP sockets (0x4F1EC0); mode 2 a third factory (0x5F0D50, vtable 0x141BE04, registered at 0x156D510) whose
sockets (vtable 0x141B5B8) go through the online client's code around 0x5D0000-0x608000. All three share one base
(vtable 0x141128C), so the client and the in-process server use whichever factory is installed: the transport is one
choice for the whole process, not one per connection. Swapping it is therefore a matter of which factory is live
when the server and the client make their sockets.

[exe: BFBC2Game.exe setTransportMode 0x650E60; factories 0x4F1E00 (vtable 0x1411934), 0x4F1E60 (vtable 0x1411960), 0x5F0F40 -> 0x5F0D50 (vtable 0x141BE04); shared base vtable 0x141128C; UDP create methods 0x4F5280 / 0x4F5380 / 0x4F54A0 -> 0x4F1EC0; in-memory create 0x4F5020 -> 0x4F1D60]
[doc: private:opfor-research/research/notes/srv8.txt:1]

### NET-TRN-2: The game's own sockets are UDP only

Status: **observed** · Variants: PC

The socket the UDP factory builds is a Winsock datagram socket (`socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP)`). It opens
two ways: unbound for a client (0x4E3290 binds `INADDR_ANY` port 0, an ephemeral port) or bound to a given address and
port (0x4E3350, the server's side). Datagrams go out with `sendto` (0x4E2FC0) and come in with `recvfrom` (0x4EF520);
the game transport calls no `listen`, `accept` or `connect` (its address helper 0x4F7830 only resolves names,
`localhost` among them). The other sockets in the executable are not the game's: DICE's development links over TCP
(the remote file system "RemoteFs" of the `remoteFileServer` / `host` start-up options, 0x4C4600 called from
0x4D13FB, and the TCP helpers 0x507B90 / 0x50E650 / 0x515CA0 beside it, not classified further), Havok's Visual
Debugger listener (`hkBsdSocket`, TCP `listen` / `accept` at 0xC6A3C0, "Listening on host[...] port"),
EA's DirtySock / Plasma layer (its own thunks to Winsock at 0xC55604-0xC5568E, used from 0xD7D000-0xD80000: the
back-end's TCP and SSL, QoS probes over HTTP, UPnP, `prototunnel`, the LAN broadcast of NET-ONL-2), WinHTTP for EA's
web content, and the licensing library's sockets (0x1212C5C, FLEXnet strings at 0x15261A0 onwards). OPFOR's network
guard (MOD-IMP-1) wraps the executable's Winsock imports, which the UDP transport and the Havok socket call directly;
DirtySock reaches Winsock through its own thunks, which call the same import slots.

[exe: BFBC2Game.exe UDP socket methods 0x4E3290 (socket 0x4E32E9, bind 0x4E3301), 0x4E3350 (socket 0x4E337F, bind 0x4E33B6), sendto 0x4E3025, recvfrom 0x4EF52F; Havok socket 0xC6A3C0 (string 0x1481F80); DirtySock thunks 0xC55604-0xC5568E called from 0xD7DB7A-0xD7F963; WS2_32 IAT 0x14059AC-0x1405A4C]
[code: opfor:src/dll/net.cpp:321-349]

### NET-TRN-3: The connection layer: a challenge handshake, ghosts, moves and voice

Status: **observed** · Variants: PC

Above the sockets, a connection manager ("ConnectionManager") names its states in a table at 0xC02AF0: Connecting,
ConnectProbe, Send Challenge / Send Custom Challenge, Challenge Sent - No reply, Send Connect, Send Accept, Accepting,
Send Accept Confirmation, Send Connection Confirmed, Connected, and the ends Rejected, No Reply, Accept failed,
Connection failed, Timed out, Disconnected by Remote. Its traffic is split into named managers: Ghost (replicated
entities), MoveClient / MoveServer (inputs and corrections) and Voice; a recorder sits on the same layer (Recording,
Playback, Passthru, FastForward). Its sizes and rates are settings (NET-CFG-1): MaxServerFrameSize,
MaxClientFrameSize, MaxGhostCount, Client.IncomingFrequency, Server.OutgoingFrequency, and the timeouts
Client.LoadingTimeout / LoadedTimeout / IngameTimeout and Server.LoadingTimeout / IngameTimeout. Values are bit-packed
by type (CompressedFloat, CompressedUnsignedFloat, CompressedVector, CompressedUnitVector, SparseCompressedVector). No
encryption of this layer was found: the only "encrypted" flags in the executable belong to Plasma's GameManager
(NET-ONL-2), not to the connection manager.

[exe: BFBC2Game.exe connection state names 0xC02AF0-0xC02B5F (strings 0x147D360-0x147D4D4); "ConnectionManager" 0x147D888; managers Ghost / MoveClient / MoveServer / Voice 0x147DA6C-0x147DAA8; recorder states 0x147D2CC-0x147D310; packing types 0x140BB2C-0x140BBC8; frame / timeout setting names 0x1417DE0-0x1418394, 0x141D810-0x141D8D8]

## 5. Settings and start-up options

### NET-CFG-1: The network settings the executable reads

Status: **observed** · Variants: PC

The settings registry (`0x4F4C40`, the engine settings of `private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md` section 3) holds the
network's knobs under four groups. Network: ServerPort, ClientPort, TitleId, ProtocolVersion, Version, MaxClientCount,
MaxGhostCount, MaxServerFrameSize, MaxClientFrameSize, MaxQueriesPerSecond, QueryPortOffset, UseFrameManager.
Client: ServerIp, SecondaryServerIp (NET-SRV-3), IsMultiplayer, Level, IncomingFrequency and the timeouts. Server:
Level, RemoteAdministration, RemoteAdminPassword, DedicatedServerCpu, IsReconfigurable, OutgoingFrequency, the timeouts
and the game rules (FriendlyFireEnabled, IsHardcore, Playlist ...). Game: OnlineListenPort, RetailServers,
svpunkbuster / clpunkbuster (NET-AC-1). `Network.ServerPort` is read by the local session start (0x565654), the
secondary-server join (0x55D0EF), a constructor of the client side (0x561D9B) and the server's listen-address parser
(0x8587CC); `Network.ClientPort` once (0x6561A1). Their default values live in the registry's data and were not read
statically; a run-time read is the way to get them.

[exe: BFBC2Game.exe field names 0x142CC84-0x142CE80 (network / core), 0x142D100-0x142D714 (client / server), 0x142DC00-0x142DDB0 (game); setting strings 0x14183E4, 0x141D9FC, 0x1418624, 0x141D8F4, 0x141D9EC; readers 0x565654, 0x55D0EF, 0x561D9B, 0x8587CC, 0x6561A1]
[doc: private:opfor-research/research/notes/srv5.txt:1-10]

### NET-CFG-2: Start-up options are looked up by name

Status: **observed** · Variants: PC

`0x4F4CF0(name, default, ...)` returns the value of a named start-up option from a list of 12-byte entries
(0x155D008-0x155D00C), and 41 call sites ask for one. The network ones: `listen` (the server's address, NET-SRV-1),
`dedicated` (NET-SRV-2), `port`, `pingInterval`, `heartBeatInterval`, `maxQueriesPerSecond`, `queryPortOffset`,
`networkProtocolId` (NET-VER-1), `loginToken` (five sites), `requestState` / `requestStateParams` (the online client's
start state, NET-ONL-1), `name`, `platform`; others are the file system's (`dataPath`, `remoteFileServer`, `host`,
`secure`) and the sound's. How the list is filled from the command line was not traced; the names match what a
launcher would pass.

[exe: BFBC2Game.exe option lookup 0x4F4CF0 over 0x155D008; call sites listen 0x86B19F, dedicated 0x55D52B, port 0x5FC429, pingInterval 0x5FC374, heartBeatInterval 0x570488, maxQueriesPerSecond 0x856992, queryPortOffset 0x8569B5, networkProtocolId 0x565A07, loginToken 0x5623C6 / 0x56B7E1 / 0x580010 / 0x5889D9 / 0x5B104A, requestState 0x5B4EF5, requestStateParams 0x5B4F36]

### NET-CFG-3: Three switches in WinMain decide the instance mutex

Status: **observed** · Variants: PC

Before anything else, 0x401EE0 walks the command line: `-runMultipleGameInstances` or `-dedicated` sets "many
instances", `-server` or `-dedicated` sets "server". With many instances no mutex is made at all; otherwise the mutex
is `BFBC2Server-...` for a server and `BFBC2-...` for a client, and a name already held (`ERROR_ALREADY_EXISTS` or
access denied) ends the start-up. Two client processes on one PC are therefore possible with
`-runMultipleGameInstances` (NET-SES-5); nothing else in the start-up was seen to refuse a second copy, but the two
would share the user-data folder and the settings unless separated.

[exe: BFBC2Game.exe WinMain helper 0x401EE0 (called from 0xC51FBB): compares at 0x401F11 / 0x401F2E / 0x401F4D / 0x401F6A against strings 0x1409838, 0x140982C, 0x1409824; CreateMutexA 0x401FB7; error checks 0xB7 and 5 at 0x401FC7 / 0x401FD4]

## 6. Versions, checks and anti-cheat

### NET-VER-1: Network.Version is the build's string unless an option overrides it

Status: **observed** · Variants: PC

0x5657B0 (called from 0x566BC8) writes `Network.Version` through the settings setter (0x4F4BD0) as `"ROMEPC795745"`
(this executable's build) and then as `"ROMEPC784592"`, with no branch between the two writes; when the start-up
option `networkProtocolId` is given it writes a third value built from `"ROMEPC` and the option. Which of the first two
stands, and whether the setter keeps the first, was not proven statically. Network.Version is read again on the online
paths (0x5FE3BD, 0x60010E, 0x600522, 0x600689) and by the server (0x853F0C). Two copies of this one executable write
the same strings, so a version mismatch between two copies of the same build cannot arise from this.

[exe: BFBC2Game.exe 0x5657B0 (strings 0x1418614 "ROMEPC795745", 0x1418604 "ROMEPC784592", 0x14185E8, option 0x14185F0); setter 0x4F4BD0; readers of 0x1418624 at 0x565869, 0x565949, 0x565A43, 0x5FE3BD, 0x60010E, 0x600522, 0x600689, 0x853F0C]

### NET-VER-2: A refused or dropped client gets one of fourteen reasons

Status: **observed** · Variants: PC

The client connection's disconnect handler (0x54BBA0, in the vtable at 0x14182D0) logs "Disconnected from remote
host: %s" with the reason and tells its listeners (+0x1798). The reasons, by value: 0 OK, 1 wrong protocol version,
2 wrong title version, 3 server is full, 4 kicked out, 5 banned, 6 generic error, 7 wrong password, 8 kicked out demo
over, 9 rank restricted, 10 configuration not allowed, 11 server reclaimed, 12 missing content, 13 punkbuster kick;
anything else is "unknown reason". The menus show their own texts for some (ERR_SERVERCONNECT_BANNED,
ERR_SERVERCONNECT_WRONGPASSWORD, ERR_SERVER_KICKED, onConnectErrorKicked, onConnectErrorWrongPassword). The checks
that produce 1, 2, 9, 10 and 12 on the server were not located; the inputs they would compare are on both sides
(NET-VER-3, NET-VER-4).

[exe: BFBC2Game.exe 0x54BBA0 (jump table 0x54BC70, 14 entries), strings 0x1417960-0x1417A4C, log 0x141793C; second user 0x558FA4; menu strings 0x141A2C4-0x141A32C, 0x1418CA0-0x1418CB8]

### NET-VER-3: Both ends read the title id and the protocol version

Status: **observed** · Variants: PC

The client side reads `Network.ProtocolVersion` when it builds its network object (0x650C50, at 0x650CBB) and
`Network.TitleId` with `Network.ClientPort` (0x656160); the server's network start (NET-SRV-1) reads `Network.TitleId`
(0x86B267) and `Network.ProtocolVersion` (0x86B345). Both are settings of the same executable, so two copies of one
build agree unless a settings file changes one; the comparison itself (reasons 1 and 2 of NET-VER-2) was not found.

[exe: BFBC2Game.exe 0x650CBB, 0x65620E, 0x6561A2, 0x86B267, 0x86B345]

### NET-VER-4: The level checksum is computed on both load paths

Status: **inferred** · Variants: PC

The level MD5 of LOAD-CSUM-1 (0x6EFBC0) has two callers: the session's level load (0x5681AB) and the "Loading level"
step at 0x829D7D, which stores the 16 bytes at +0x34..+0x40 of its level object. Together with the setting
`EnableDumpLevelChecksumOnDisconnect` and the disconnect reason "missing content" (12, NET-VER-2), this reads as a
client-server comparison of the level's content: a client whose level differs from the server's is dropped. The
comparison was not found, so this is inferred. For remote play it means both machines need the same installed level
content (the same OPFOR install), which LVL / registration rules already require of one machine.

[inferred: two callers of the checksum at 0x5681AB and 0x829D7D, a stored 16-byte digest, the setting name at 0x142CD70 and reason 12 at 0x14179D0; no compare site found]

### NET-AC-1: PunkBuster loads only from its folder

Status: **observed** · Variants: PC

The executable carries PunkBuster's client and server loaders: it looks for `pbcl.dll` (and `pbclold.dll` /
`pbclnew.dll`) and `pbsv.dll` (`pbsvold.dll` / `pbsvnew.dll`, `pbag.dll`) under `pb\`, and reports "PB Error: Client
DLL Load Failure" otherwise. PunkBuster is switched by `Game.clpunkbuster` / `Game.svpunkbuster` (and the commands
`set_cl_punkbuster` / `set_sv_punkbuster`); a PunkBuster kick is reason 13 (NET-VER-2). The game copy has no `pb\`
folder, so neither side loads (DEVELOPMENT_NOTES (private research notes), network guard). No other anti-cheat was found.

[exe: BFBC2Game.exe loader 0x5B83EC-0x5B89BA (strings 0x141AC90-0x141ADAC); settings 0x143C22C, 0x141B750; commands 0x141AE3C, 0x141AF4C]
[doc: private:opfor-research/DEVELOPMENT_NOTES.md:182-183]

## 7. The server side

### NET-SRV-1: The server listens where `-listen` says, else on Network.ServerPort

Status: **observed** · Variants: PC

The server's network start is a handler of message category 0x27, types 0x10 to 0x13 (0x86B0D0, jump table
0x86B8F8). It builds the server's network objects (0x8674D0 when the message's mode at +0x2C is 0, 0xBED680,
0x64F4E0), then asks for the start-up option `listen` and parses it (0x8585E0): `host:port` or `host:port-port`; the
host goes to +0xB0 and the port to +0xBC, and without a `:` the port is `Network.ServerPort`. With no host, the server
uses `127.0.0.1` when the message's flag at +0x29 is set and the empty host (any address) otherwise. It also reads
`Network.TitleId` and `Network.ProtocolVersion` (NET-VER-3). So the code for a server that listens on a real address
is in the client executable; the run of 2026-10-12 (NET-SES-3) saw the in-process server never listen in mode 1, and
why (whether this handler ran, which flag it got) is not known.

[exe: BFBC2Game.exe 0x86B0D0 (category compare at 0x86B0D6, types 0x86B0EE); listen option 0x86B19F; parser 0x8585E0 (':' at 0x858607, '-' at 0x858690, atoi 0x85869D); host default at 0x86B1E7-0x86B1F8 (string 0x141240C); TitleId 0x86B267, ProtocolVersion 0x86B345]

### NET-SRV-2: The client executable has the server, not the dedicated server's front end

Status: **inferred** · Variants: PC

The PC client contains the whole game server (the in-process host of NET-SES-2), PunkBuster's server loader
(NET-AC-1), the server settings (remote administration, `DedicatedServerCpu`, `IsReconfigurable`) and a `-dedicated`
switch: it skips the mutex (NET-CFG-3) and, on one path (0x55D51C, when the object's flag +0x9C is set), makes the data
platform `linux` instead of `win32`, the layout of DICE's Linux dedicated server. It does not contain that server's
configuration front end: no `ServerOptions`, no `RemoteAdminPort`, no RCON listener (the only TCP listener is Havok's,
NET-TRN-2). A client started with `-dedicated` would look for Linux data the PC install does not have. A listen server
(a player's client hosting remote players) therefore needs no foreign code: the server and its listen path are here
(NET-SRV-1); what is missing is the start that hands the server a real address with the UDP transport.

[inferred: 0x55D502-0x55D545 (strings 0x1416738 "win32", 0x1418490 "dedicated", 0x1416724 "linux"); no string "ServerOptions" / "RemoteAdminPort" in the executable; setting names 0x142D568-0x142D688]

### NET-SRV-3: A client can join a server by address

Status: **observed** · Variants: PC

The session's connection-event handler (0x55CE50, events 3 to 15, jump tables 0x55D198 / 0x55D1A8) handles event 10
by reading `Client.SecondaryServerIp`, appending `:` and `Network.ServerPort` when it has no port, and posting a join
message (type 0x14, constructor 0x551480, the address set by 0x55C7A0) on channel 0x28. The same message constructor
is used by the UI manager's join sites (0x952413, 0x95E5C2, 0x95F267). A join by a plain `ip:port` string is therefore
part of the client; it is how the client moves to a secondary server, and it is the hook a Direct IP join needs
(`engine/network_feasibility.md`).

[exe: BFBC2Game.exe 0x55CE50 (event 10 -> 0x55D040); setting 0x14183F8 read at 0x55D07F, ':' search 0x55D0C6, ServerPort 0x55D0EF, format ":%d" 0x14183E0; message 0x551480 (type 0x14 at 0x551483, vtable 0x1417D20); post 0x55D17F; other constructors' callers 0x5B3FF5, 0x5B43C4, 0x952413, 0x95E5C2, 0x95F267]
[doc: private:opfor-research/research/notes/srv6.txt:1-60]

## 8. EA's back-end, seen from the client

### NET-ONL-1: The back-end is Plasma: FESL first, Theater from FESL

Status: **observed** · Variants: PC

The online client is EA's Plasma SDK on DirtySock. The login server's host is built from the client string
`bfbc2-pc` (console: `bfbc2-ps3`, `bfbc2-360`), `.fesl` and `.ea.com` (0xDDC31E / 0xDDC39C, also `fesl.ea.com`
0xDD6EC9); the game-lobby server (Theater) is reached at the `theaterIp` / `theaterPort` FESL hands back, with
override strings for both (FeslHostOverride, TheaterHostOverride, TheaterPortOverride) in the SDK's settings dump. The
client's online state machine names its states State_Starting, State_NotLoggedIn, State_MenuReady, State_Matchmaking,
State_ConnectToGameId, State_ConnectToUserId, State_Connecting, State_InQueue, State_GameLoading, State_Game,
State_GameLeaving, and a launcher can start it in one with `requestState` / `requestStateParams` (NET-CFG-2). The
server browser's rows (NET-SB-1) come from this layer online (`postGetServers` -> the browser list), and a join from
the browser is `startServerListMatch` (OPFOR's DLL answers both offline). The join a back-end hands out carries a
ticket that the hosted game checks (NET-SES-4: "ticket mismatch", "not approved"). Public server emulators replace
exactly this layer (FESL + Theater) by redirecting its host names (`engine/network_feasibility.md`, sources).

[exe: BFBC2Game.exe client strings 0x141C2BC-0x141C2FC; host parts 0x149D080 / 0x149D088 used at 0xDDC39C / 0xDDC31E; 0x149C7D0 at 0xDD6EC9; theaterIp / theaterPort 0x149B5E0 / 0x149B5FC; overrides 0x149A235-0x149A2E5; states 0x1505214-0x1505394; requestState 0x5B4EF5]

### NET-ONL-2: Plasma's GameManager carries a LAN broadcast mode

Status: **observed** · Variants: PC

Linked with the back-end is Plasma's GameManager (strings "GM:"), a peer game layer with hosted games, rosters, host
migration, peer mesh, voice and per-packet "reliable" / "encrypted" flags. It has a LAN mode ("GMLAN"): 0xDBAB60
opens a DirtySock UDP socket, binds it to the configured `LanBroadcastPort` (read through the SDK's settings object at
0x1880F64) and keeps the broadcast address 255.255.255.255 with that port at 0x1880F04; a LAN packet handler
(0xDBF0E5, "Received unknown LAN packet type") reads what arrives. The LAN socket is opened by the GameManager's init
(0xDBAD00), which is called from 0xE01070, a method of a Plasma object (vtable 0x14A022C). The matchmaker's session
types include AssignHost (a player hosting) beside FindServer / ListServers / ResetServer. Whether BFBC2 PC ever
creates the GameManager, and whether its LAN discovery is wired to the Frostbite server at all, was not traced: it is
SDK code shipped with the game, not a LAN browser of BFBC2's own. The menus have no LAN entry.

[exe: BFBC2Game.exe 0xDBAB60 (socket 0xDBABED, bind 0xDBAC6F, broadcast address 0xDBACA9-0xDBACED); init 0xDBAD00 (object 0x4954 bytes, constructor 0xDB7110) called at 0xE0110E; caller 0xE01070 in vtable 0x14A022C; strings 0x1498578, 0x1498C74, 0x1498C9C, 0x1498DC4, 0x149A8E5; session types 0x146E970-0x146E9F4]

## Open questions

- Whether a remote join (transport 2) and a dedicated server (the Linux build) need anything of the level that the
  in-process host does not; only the in-process path has been run.
- What the stats service's commit path does offline beyond the unanswered request (where in-round results accumulate
  before the commit).
- Why the in-process server never listened with the socket transport (2026-10-12): whether the server's network start
  (NET-SRV-1) ran, with which mode and loopback flag, and which ports `Network.ServerPort` / `Network.ClientPort` hold
  by default (the run saw 127.0.0.1:1001 sending to :1003). One run with OPFOR's net trace would answer it.
- What transport 2's sockets send through (plain UDP via Winsock, or EA's `prototunnel`); the static walk ends in
  virtual calls.
- Where the server compares the title id, protocol version and level digest (reasons 1, 2 and 12 of NET-VER-2), and
  what a "configuration not allowed" (10) or "rank restricted" (9) refusal checks.
- Which `Network.Version` stands after 0x5657B0's two writes (NET-VER-1).
- How the start-up option list (NET-CFG-2) is filled from the command line, and whether `-listen` reaches it as
  written.
- Whether Plasma's GameManager and its LAN mode (NET-ONL-2) are ever created on PC.
- How the browser's record fields +0xE0 / +0xE4 (ids) and +0xEC (flags) are filled for an offline row.
