# Network feasibility: LAN, VLAN, Direct IP and locally hosted multiplayer

What Project OPFOR BFBC2 would need to let players on other machines into a match, judged against what the executable
shows (`engine/network.md`). This file is a study, not a rule set: it defines no rules and every fact it uses is a
rule of `network.md` or `module.md`, cited by ID. Nothing here was run; in-game tests are paused (2026-11-20), and the
last section says exactly what would prove each step once they resume.

**Sources.** The rules of `engine/network.md` (round NET2's static reading of build 795745, plus the in-process host
run of 2026-10-12) and `engine/module.md` (OPFOR's DLL). Public pages opened for context, checked against the
executable where the executable can say anything:

- Arcadia (github.com/valters-tomsons/arcadia, opened 2026-11-23): an emulator of EA's Plasma back-end (FESL and
  Theater); it lists BFBC2 as co-op only on PS3 and says the Theater address reaches the client in FESL's Hello. The
  executable agrees on the structure: FESL host from `bfbc2-pc` + `.fesl` + `.ea.com`, Theater from FESL's
  `theaterIp` / `theaterPort` (NET-ONL-1).
- PBBans community forum, "BFBC2 server not streaming" (community.pbbans.com/topic/134545, opened 2026-11-23): a
  staff member gives 19567 as the dedicated server's default game port and 48888 as its RCON port. Not confirmed by the
  client executable: neither number appears there as an immediate, the client has no RCON listener (NET-SRV-2), and
  its own port defaults live in settings data that was not read (NET-CFG-1). Treat both as the Linux dedicated server's
  values only.

A search also listed other community back-end emulators (master-server projects on GitHub and SourceForge); their pages
were not opened (one returned 404), so nothing here relies on them.

## 1. Where things stand

- **Works today:** one machine, one process. `hostMultiplayerLocal` runs the game server inside the client
  (NET-SES-2) over the in-memory transport (NET-SES-3, OPFOR's patch, MOD-HOST-1); no socket opens (NET-SES-4), and
  OPFOR's network guard keeps every network call on loopback (MOD-IMP-1).
- **Present in the executable but unused:**
  - a UDP transport (NET-TRN-1, NET-TRN-2) under a connection manager with a challenge handshake (NET-TRN-3);
  - a server network start that listens on `-listen host:port` or `Network.ServerPort`, on any address unless a
    loopback flag is set (NET-SRV-1);
  - a client join by `ip:port` string (NET-SRV-3);
  - a switch for a second process (`-runMultipleGameInstances`, NET-CFG-3).
- **Not present:**
  - a BFBC2 LAN browser: the only LAN discovery code is Plasma's GameManager broadcast, SDK code whose use on PC is
    unproven (NET-ONL-2);
  - the dedicated server's front end (NET-SRV-2).
- **Known blocker:** with the socket transport the in-process server never listened (2026-10-12); why is the first open
  question of `network.md`.

## 2. What every remote option shares

Every option below except the emulator route needs the same core, so it is costed once here:

1. **A listening host.** The host's server must bind a real address. NET-SRV-1 shows the code; the missing piece is
   why it did not run or did not bind. Expect one of: the network start handler not reached with transport 1, the
   loopback flag (+0x29) set by the local session, or a port the guard rewrote. Fix: a DLL patch or a `listen` option
   injected into the option list (NET-CFG-2).
2. **One transport for the process.** The transport is a process-wide factory (NET-TRN-1). A hosting process must run
   on UDP, its own client included (over loopback). The alternative is a composite factory in the DLL that gives the
   local client in-memory sockets and remote peers UDP; that is more code, and there is no evidence it is needed.
   NET-SES-3's patch would then apply only to offline sessions, with a `[net] host=1` switch selecting UDP.
3. **The guard opened deliberately.** MOD-IMP-1 allows loopback only. Hosting needs a `bind` on the chosen adapter and
   `sendto` / `recvfrom` to the peers. This should be an allowlist (`[net] peers=` addresses or subnets), never
   "allow all": WinHTTP, ICMP, NetBIOS and the back-end's lookups stay blocked, so nothing reaches EA's (dead) or
   anyone else's services.
4. **Distinct players.** Without an EA persona the server names its local player "Human player" with id 1
   (NET-JOIN-1). Two humans would collide. The join must carry a per-machine name (OPFOR's profile) and the server
   must hand out distinct ids. The code path for a second human was never run.
5. **Matching content.** The level digest is computed on both load paths and a "missing content" refusal exists
   (NET-VER-4, inferred); the version and title checks read the same settings on both sides (NET-VER-1, NET-VER-3).
   Same OPFOR build and same installed content on every machine; the installer's manifest is the natural check.
6. **No back-end tickets.** The EA join path checks tickets and approval (NET-SES-4, NET-ONL-1). The in-process host
   joins its own client without one. Whether a remote client's Frostbite join meets a ticket check is unknown. If it
   does, the DLL answers it, the way OPFOR already answers the server browser.

## 3. The options

### 3.1 Locally hosted multiplayer (a listen server)

A player starts an OPFOR match as today, but the hosting process also accepts remote players.

- **Needs:** all of section 2. Hooks:
  - the transport choice (0x54AF2C, already patched);
  - the server's listen address (0x8585E0 / 0x86B1E7, or the option list);
  - the guard's allowlist;
  - the player identity at join (0x55DFB0 client side, 0x865D70 / 0x841930 server side).
- **Risks:**
  - **The unknown blocker.** The server never listened (NET-SRV-1's open question). If the server's network start
    is incomplete on PC, e.g. it expects the dedicated server's Linux data path (NET-SRV-2), the cost rises sharply.
  - **Desync and replication.** Ghosts and moves (NET-TRN-3) were built for this and ran online in 2010, but OPFOR's
    additions were only ever seen with a local client:
    - the DLL's server-side patches (bots, Pacific War modes, the air raid, Onslaught) assume one process;
    - client-only patches (HUD, menus, aim assist) must not be required for the server's state.
  - **Bots.** Server-side (BOT-CTL-1), so they replicate like any soldier. Their cost moves onto the host.
  - **Memory.** The executable is large-address-aware, so up to 4 GB on 64-bit Windows. The host already carries
    server and client; each remote player adds connection and ghost state, small next to a level. The 32-bit
    ceiling is not the first risk but should be measured.
  - **NAT.** Internet play needs the host's UDP game port forwarded. LAN and VLAN do not.
  - **Anti-cheat.** None active: PunkBuster loads only from a `pb\` folder the game copy does not have (NET-AC-1).
    Any player can cheat, which matches a friends-only setting.
- **Effort:** large.
  - One diagnostic playtest.
  - About one to two weeks of DLL work: transport switch, listen address, allowlist, identity.
  - Then playtests on two machines for the DLL's server-side features, one by one.
  - Most risk is in the unknown blocker and the identity path.

### 3.2 Direct IP

The joining half of 3.1: a player types `host:port` and joins.

- **Needs:**
  - a hosting process (3.1);
  - a way to start a join by address: the join message 0x14 with an `ip:port` string (NET-SRV-3), posted from the
    DLL (an ini entry or a menu field) on the client;
  - the client's transport set to UDP (mode 1) for that join, not mode 2, whose sockets go through the online
    layer (NET-TRN-1);
  - the guard allowing that one address.
- **Risks:**
  - the join message's other fields (password, server id) and what the session does between "joining" and
    "loading" for a non-back-end server;
  - mode 2 might be required by the join state machine.
- **Effort:**
  - small once 3.1 exists: a few days, mostly the menu entry;
  - an ini-driven join (`[net] join=`) first, the menu after.

### 3.3 VLAN (ZeroTier, Radmin VPN, Tailscale-style virtual LANs)

Direct IP over a virtual network adapter. The players' machines see each other on a private subnet.

- **Needs:** 3.1 and 3.2. In addition, the guard's allowlist must accept the virtual subnet, and the host's server
  must bind either any address or the virtual adapter's (NET-SRV-1 accepts `host:port`).
- **Risks:**
  - binding to the wrong adapter when a host address is given;
  - the virtual network's MTU against MaxServerFrameSize / MaxClientFrameSize (NET-TRN-3; the frame sizes are
    settings, readable at run time);
  - added latency.
  - NAT is the virtual network's problem, which is why this is the easiest remote setting to support.
- **Effort:** trivial after 3.2. A documented setup, an allowlist example, and one test across a real virtual
  network.

### 3.4 LAN, with or without discovery

- **Without discovery** it is Direct IP on a LAN (3.2): nothing extra.
- **With discovery** the executable offers only Plasma's GameManager LAN broadcast (NET-ONL-2). It is SDK code: its
  creation on PC is unproven, it is not tied to the Frostbite server, and the menus have no LAN entry. Reviving it
  means driving an EA SDK subsystem whose messages and state are undocumented.
- **The better route is OPFOR's own:**
  - the host's DLL answers a UDP broadcast (or sends a periodic beacon) on a fixed port, through the allowlist;
  - the client's DLL fills the server browser's rows with the hosts it hears, which OPFOR already does with the
    level catalogue (NET-SB-1). The record has an address field (+0x48);
  - the row's join (`startServerListMatch`) becomes the address join of 3.2.
- **Risks:**
  - broadcasts through the guard must stay on the local subnet;
  - the browser record's unexplained fields (`network.md`'s last open question).
- **Effort:** medium after 3.2, about a week. No new game-side hook beyond the browser's, which OPFOR already owns.

### 3.5 The emulator route (for comparison)

Run a community FESL / Theater emulator and DICE's dedicated server, and let the client use its own online path
(NET-ONL-1).

- **Why not:**
  - it needs the dedicated server, which the game copy does not ship (NET-SRV-2);
  - the client's back-end traffic must be redirected and the guard opened to it, against OPFOR's no-traffic policy;
  - OPFOR's own content (Pacific War, Onslaught, the bots) is not on such servers;
  - it brings back the ticket and persona machinery (NET-SES-4) instead of avoiding it.
- **When it would help:** for joining existing community servers with the stock game. That is a different goal from
  OPFOR's hosted matches.

## 4. Ranking and recommended order

| Rank | Option | Value | Effort after its prerequisites | Main risk |
|---|---|---|---|---|
| 1 | Locally hosted (3.1) | the base of every other option | large | why the server never listened; player identity |
| 2 | Direct IP (3.2) | first remote play, LAN or internet | small | the join's other fields; transport mode 2 |
| 3 | VLAN (3.3) | internet play without port forwarding | trivial | adapter choice, MTU |
| 4 | LAN discovery (3.4) | convenience on a LAN | medium | the browser's record fields |
| 5 | Emulator (3.5) | community servers, not OPFOR matches | large, outside OPFOR | policy, missing dedicated server |

**Recommended order.**

0. One diagnostic run: OPFOR's net trace on, `[patches] local_inmemory_transport=0`, `hostMultiplayerLocal`. It
   answers the listen question, reads the port defaults and shows whether the server's network start runs.
1. 3.1 and 3.2 as one milestone ("two humans in one match"). They are not separable in a test.
2. 3.3 as documentation and an allowlist example.
3. 3.4 as OPFOR's own beacon.
4. 3.5 not at all for OPFOR.

## 5. What would prove each step (once in-game tests resume)

**The process rule.** NET-SES-5 makes one process the default, and the project's test rule is one game process at a
time. The executable allows two (`-runMultipleGameInstances`, NET-CFG-3). A two-process test on one PC also needs:

- separate user data per process (OPFOR's `[paths] userdata=`);
- the guard allowing loopback, which it already does.

Two processes therefore test everything except real adapters and NAT, but they need the user's permission to run two
copies. Two machines need a second OPFOR install with identical content.

**Step 0, the listen question:**

- **Pass:** the net trace shows the server's `bind` on `Network.ServerPort`, or shows that the network start never
  runs.
- **Also recorded:** the defaults of `Network.ServerPort` / `ClientPort` / `Version` read from the settings registry.

**Step 1, hosting:**

- **Host side:** the trace shows a UDP bind on the chosen address and port, and no traffic outside the allowlist.
- **Client side:** a second process with `[net] join=127.0.0.1:<port>` reaches state 3 (NET-SES-2's states). The
  host's scoreboard shows two humans with distinct names and ids, and each player sees the other's soldier move
  and shoot.
- **Duration:** five minutes of play with bots on.
- **Clean ends:** both a disconnect (reason 0) and a kick (reason 4) end cleanly (NET-VER-2).
- **Then on two machines on one LAN**, the same test, plus one round of each mode on a stock map and on a Pacific War
  map.

**Step 2, Direct IP on the internet:** the same test across the internet with the host's UDP port forwarded, and a
note of the latency at which play degrades.

**Step 3, VLAN:** the same test across a ZeroTier or Radmin network with no forwarding, and the host bound to the
virtual adapter.

**Step 4, LAN discovery:** the client's server browser lists the host within a few seconds of the host starting, and
joining from the row reaches state 3.

**What stays unproven until then:** everything in this file beyond the static facts of `network.md`. In particular:

- that the server can listen at all on PC;
- that a second human gets a distinct identity;
- that OPFOR's server-side patches replicate;
- that no back-end ticket check sits on the Frostbite join.
