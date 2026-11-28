# OPFOR's game module: every patch point in BFBC2Game.exe

Project OPFOR BFBC2's game module (`opfor.dll`, loaded through a `dinput8.dll` proxy: `opfor:src/dll/`) changes the
running `BFBC2Game.exe` at fixed addresses: trampoline hooks, byte patches, call-site redirects, vtable and import
slots. This file lists every address the module's source uses, what it changes and why, and how the module checks
it is patching the executable it was written for. Written by round SPG2 (2026-11-22) from a read of all of `src/dll`
(OPFOR at commit 55eeeb6); line numbers are that commit's. Everything here is **implemented** (the module relies on
it and runs in game) unless a rule says otherwise; the module's C++ rule tests (`src/dll/tests/*_test.cpp`,
`int main()` programs) cannot be cited as `[test: ...]`. What the patched features do is specified elsewhere:
the offline host [network.md](network.md), the career [persistence.md](persistence.md), bots [ai.md](ai.md) and
[bot_ai.md](bot_ai.md), the menus [menus.md](menus.md), Onslaught [onslaught.md](onslaught.md), the crashes
[crashes.md](crashes.md).

Addresses are virtual addresses in the image (base 0x400000). Ini keys are `game\opfor.ini` `[section] key=default`.

## 1. The executable and the patch machinery

### MOD-EXE-1: Every address was read on PC client build 795745; only signatures check it at run time

Status: **implemented** · Variants: PC

All addresses are those of `BFBC2Game.exe` PC client build 795745 (file version 1.0.1.0; the retail install, OPFOR's
`game\` copy and Napisal's install are the same file). The module checks no version, size, timestamp or checksum: it
refuses to patch unless the process image is `BFBC2Game.exe` at base 0x400000 and 21 eight-byte signatures (the
functions the offline host and the menus call) match; any mismatch logs `SIGNATURE MISMATCH` and nothing is patched.
The import-table patches of the network guard and the user-data redirect are made earlier, in `DllMain`, and need only
the image check.

[code: opfor:src/dll/bc2.h:1]
[code: opfor:src/dll/bc2.h:136-158]
[code: opfor:src/dll/opfor.cpp:162-188]
[code: opfor:src/dll/opfor.cpp:370]
[doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:17-18]

### MOD-HOOK-1: A hook is a 5 - 27 byte jump to a 32-byte trampoline, after its bytes are checked

Status: **implemented** · Variants: PC

`InstallHook` takes a target, its expected original bytes (whole position-independent instructions, 5 - 27 bytes),
compares them, allocates a 32-byte executable trampoline holding the original bytes and a jump back, and writes
`E9 rel32` plus `90` padding over the target. Bytes that differ: the hook is not installed (logged) and the module's
feature is skipped. The start-request hook of the offline host is hand-rolled the same way (6-byte prologue); the 51
diagnostic probes use 64-byte `pushad` stubs that call the probe and run the relocated bytes.

[code: opfor:src/dll/hook.cpp:236-265]
[code: opfor:src/dll/hook.h:43-49]
[code: opfor:src/dll/opfor.cpp:306-335]
[code: opfor:src/dll/probes.cpp:821-850]

### MOD-HOOK-2: Patches are committed together, with the game's other threads suspended

Status: **implemented** · Variants: PC

The start-up patches are queued (at most 256 writes of at most 32 bytes, the original bytes recorded when queued) and
committed at once: every other thread suspended, a thread whose instruction pointer lies inside a patched range let
run 1 ms at a time (up to 20 tries), the bytes compared again, then written with `VirtualProtect` and
`FlushInstructionCache`. All or nothing: changed bytes or a thread stuck in a range writes none of them. Writes after
start-up (the stats listener's vtable slot, the prompts' platform string) are immediate.

[code: opfor:src/dll/hook.cpp:87-138]
[code: opfor:src/dll/hook.cpp:140-234]
[code: opfor:src/dll/hook.h:30-32]

### MOD-HOOK-3: What is checked before writing, and what is not

Status: **implemented** · Variants: PC

Checked: every trampoline hook (MOD-HOOK-1); every byte patch (its original bytes, skipped on a mismatch); the four
fixed vtable slots (their current value must be the expected function); the "conquest" literal the mode-name hook
relies on; each probe. Not checked: levelfix's four import slots (read and replaced), the persistence stats
listener's run-time vtable slot, the network and user-data import slots (matched by the resolved function's address,
not by place).

[code: opfor:src/dll/patches.cpp:43-46]
[code: opfor:src/dll/profile.cpp:122-127]
[code: opfor:src/dll/boatrelease.cpp:155-160]
[code: opfor:src/dll/levelfix.cpp:801-805]
[code: opfor:src/dll/levelfix.cpp:848-853]
[code: opfor:src/dll/persistence.cpp:210-225]
[code: opfor:src/dll/net.cpp:336-337]

### MOD-CNT-1: 221 addresses in 27 files

Status: **observed** · Variants: PC

The module uses 221 distinct addresses: 92 hooked entry points (42 trampoline hooks, 51 probe stubs, one address in
both), 15 byte-patch sites, 4 call-site redirects (0x956C2F, 0x95178F, 0x81ADA8, 0x7283CB), 9 fixed data-slot writes
(vtable slots 0x142B774, 0x1433B3C, 0x1429784, 0x14A83A8; import slots 0x1405504, 0x1405274, 0x140533C, 0x1405908;
the global 0x18947D4) and one run-time vtable slot: about 120 write sites. The other ~101 are functions it calls,
globals it reads, bytes it only verifies and return addresses it compares. Per file: probes 54, bc2.h 25, gamemode 24,
levelfix 23, server_browser 22, navgen 18, persistence 16, input 16, stats_screens 13, profile 12, navquery 11, the
rest 1 - 9 (horn, padaimer, captureprobe, net, userdata, dinput8_proxy: none fixed).

[data: a read of every file of opfor:src/dll, round SPG2 2026-11-22: addresses the code uses (patched, hooked, called, read or verified), comment-only mentions excluded (about 358 distinct hex values counting comments)]

## 2. The patch points by feature

Each table row: the address, the kind (hook: trampoline; patch: bytes, original -> new; redirect: a call's target;
slot: a vtable or import entry), what it is, and the gate when it is not always on. The source line is in the rule's
evidence.

### MOD-HOST-1: The offline host and the shared engine calls

Status: **implemented** · Variants: PC

| Address | Kind | What, why |
|---|---|---|
| 0x951E00 | hook | `UiManager::processStartRequests`: the offline multiplayer host, the `[dev]` level / autostart / F9 trigger, the Pacific War and Onslaught level start |
| 0x956E10, 0x75DF60, 0x75EAB0 | calls | queue a start request (as the `hostMultiplayerLocal` handler at 0x77BEC3 does), its constructor and destructor |
| 0x4014C0, 0x425080, 0x401470 | calls | the engine's string assign (C string, string) and destructor, everywhere |
| 0x19A4CA8 | global | the UI manager (+0x82CD0 the start-request queue) |
| 0x920360, 0x588910, 0x588940, 0x9299B0, 0xF7C600, 0x12FBB40, 0x929A70 | calls | menu events: acquire, add string / value, queue, make bool / string, send |
| 0x54AF2C (to 0x54AF42) | patch, 22 bytes -> `E9` to a stub + 17 x `90` | `Session::createClient`'s transport choice: with the local flag (+0xD4) the in-memory link (mode 0). `[patches] local_inmemory_transport=1` |

[code: opfor:src/dll/bc2.h:15-27]
[code: opfor:src/dll/bc2.h:52]
[code: opfor:src/dll/bc2.h:96-108]
[code: opfor:src/dll/opfor.cpp:192-211]
[code: opfor:src/dll/patches.cpp:30-72]
[code: opfor:src/dll/patches.cpp:145-149]
[doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:93-98]

### MOD-PATCH-1: Vietnam without sign-in, and scoring with fewer than four players

Status: **implemented** · Variants: PC

| Address | Kind | What, why |
|---|---|---|
| 0x75FCC9 | patch `75 16` -> `EB 23` | the game-mod switch's sign-in gate (in 0x75FCB0) jumps straight to the switch: Vietnam offline. `[patches] vietnam=0` |
| 0x75FE35 | patch `75 10` -> `EB 10` | skips the `showNamRedeemPopup` event. Same gate |
| 0x865F36, 0x86FD12, 0x842E6A, 0x842E9A, 0x83B591 | patch `83 F8 04 73` -> `83 F8 00 73` at each | the "fewer than 4 players: no scoring / stats" tests (`call 0x53F430; cmp eax, 4; jae`); all five or none. `[patches] offline_scoring=1` |

[code: opfor:src/dll/patches.cpp:91-108]
[code: opfor:src/dll/patches.cpp:123-132]
[doc: bfo:docs/research/opfor/WW2_THIRD_MODE.md:29]
[doc: bfo:docs/research/opfor/WW2_AWARDS_COMPARISON_2026-11-15.md:239]

### MOD-MODE-1: The Pacific War as a third game mod

Status: **implemented** · Variants: PC

All gated by `[gamemode] pacific_war=1`, and all after the first row only if that row's hook went in.

| Address | Kind | What, why |
|---|---|---|
| 0x75FCB0 | hook, 8 bytes | the game-mod setter: mode 3 (or 2) becomes engine mod 1 plus OPFOR's Pacific flag; 0x77D87F (the return address after 0x77D87A) tells the menu's call from a replay's |
| 0x75FDB7 | patch, 10 bytes | the `BFBC2Nam` label pushes point at OPFOR's label (`BFBC2Nam` or `BFBC2143`) |
| 0x8FF330 | hook, 6 bytes | the level-mod parser: a `PACIFIC` level is parsed as Vietnam |
| 0x921CF7 (checks 0x921E01, continues at 0x921E29) | hook, 6 bytes | the minimap's combat-area second quad, skipped on Pacific levels (`[debug] map_overlay_bypass=1`) |
| 0x921CDB (checks 0xF82BC0, 0x9214B4; global 0x1572B18) | hook, 6 bytes | the minimap image quad: Wake's ocean shader state (`[debug] map_ocean=1`) |
| 0x751BC9, 0x7776F1 | patch, 5 bytes each | `mov eax, nam_GameTips` (0x142ADCC) -> OPFOR's tips name: the death and loading screens' tips (marker file `ww2_common\loader-00.fbrb.opfor-tips`) |
| 0x75AEB3 | patch, 4 bytes (22 verified from 0x75AEA1) | the menu config push (`nam_menuConfig`) -> OPFOR's: Pacific menu music (marker `mainmenu-00.fbrb.opfor-pacific-music`) |
| 0xF909C0 | hook, 6 bytes | the HUD's play-sound-by-name: BF1943's award stings (marker `ingame-00.fbrb.opfor-stings`) |
| 0x142B774 | slot | the HUD rank-call vtable (0x142B764) slot 4, only if it holds 0x73EB90 with its bytes: the rank-up sting |
| 0xF7C5D0 | call | make an int menu value (`opforPacific`, `opforGameMod`) |
| 0xF88510, 0xF9C9E0 | hooks | HUD icon lookup and icon-set load: diagnostics only (`[debug] icon_probe=0`), installed even without the Pacific War |

[code: opfor:src/dll/gamemode.cpp:47-58]
[code: opfor:src/dll/gamemode.cpp:119-123]
[code: opfor:src/dll/gamemode.cpp:233-260]
[code: opfor:src/dll/gamemode.cpp:291-303]
[code: opfor:src/dll/gamemode.cpp:376-380]
[code: opfor:src/dll/gamemode.cpp:462-512]
[code: opfor:src/dll/gamemode.cpp:636-711]
[doc: bfo:docs/research/opfor/WW2_THIRD_MODE.md:24-75]

### MOD-BRWS-1: The offline server browser and the mode names

Status: **implemented** · Variants: PC

| Address | Kind | What, why |
|---|---|---|
| 0x77BC10 | hook, 6 bytes `81 EC 08 0A 00 00` | the main-menu dispatcher: the offline server list, joins, Onslaught, the profile and redeem commands |
| 0x92A160 | hook, 10 bytes (installed with the first per-frame callback) | the UI manager's flag update: the module's per-frame callback (input, browser, stats, prompts, Onslaught HUD) |
| 0x956C61 | patch, 6 bytes `53 E8 59 59 62 00` -> a jump to a stub | a server row's mode text: "ONSLAUGHT", "AIR SUPERIORITY" |
| 0x956C2F | redirect from 0x9293C0 | a row's mode name: Conquest and Air Superiority share CONQUEST |
| 0x95178F | redirect from 0x6E8650 | the level catalogue keeps level names without a digit (the WW2 levels) |
| 0x9421F0 | hook, 6 bytes | the `multiplayerMaps` builder: Onslaught records, Pacific / Vietnam order |
| 0x5ECF60, 0x5ED370, 0x951480, 0x956A80, 0x958BF0, 0x42C300 | calls | server record, level catalogue, server rows, tree walk |

[code: opfor:src/dll/bc2.h:55-56]
[code: opfor:src/dll/server_browser.cpp:377-416]
[code: opfor:src/dll/server_browser.cpp:429-491]
[code: opfor:src/dll/server_browser.cpp:508-509]
[code: opfor:src/dll/server_browser.cpp:787-808]
[code: opfor:src/dll/ui_frame.cpp:15-35]

### MOD-FIX-1: Engine guards for levels built from other builds' data

Status: **implemented** · Variants: PC

Always on: the console Onslaught ports and the WW2 levels reach engine paths DICE's data never did.

| Address | Kind | Guards |
|---|---|---|
| 0x8F4720 | hook, 6 bytes | static-model-group part set-up: a member with no physics part is skipped (Nelson Bay's null) |
| 0x4304D0 | hook, 6 bytes, only if 0x142ABD8 reads "conquest" | string compare: "onslaught" and "airsuperiority" compare equal to "conquest" (the flag HUD, the loading text: ONS-MODE-3) |
| 0x577390 | hook, 6 bytes | the AI sphere query: a fault inside (0x56C4E0) skips the query |
| 0x4EB4B0 | hook, 8 bytes | the reference reader: a reference with no `ref` attribute reads as null (crash 0x4EB538) |
| 0x5DEB30 | hook, 6 bytes | kit lookup: a null key from 0x7785D7 or 0x752456 gets an empty entry (Wake kits) |
| 0x6EA340 | hook, 7 bytes | the voice-over walk: a null object, or a blueprint with no object, is skipped (crash 0x6EA3F0) |
| 0x9BC68E, 0x9BC661 | hooks, 5 bytes | texture binding (pixel / vertex shader resources): slots holding dead views are emptied (Wake's spawn crash) |
| 0x5A5A00, 0x4AAB60, 0x6DC280 | hooks | log a fault (mesh asset 0x5A5A61, partition load 0x49EDF4, team kit without a soldier) and let it proceed |
| 0x1405504, 0x1405274, 0x140533C, 0x1405908 | import slots | `exit`, `ExitProcess`, `OutputDebugStringA`, `MessageBoxA`: logged with their callers |

A vectored exception handler logs every access violation with its instruction pointer in 0x401000 - 0x1400000,
with detail for 0x6EFCA4 (Md5Buffer), 0xAB4E58 and 0xFF2AC2.

[code: opfor:src/dll/levelfix.cpp:32-94]
[code: opfor:src/dll/levelfix.cpp:192-193]
[code: opfor:src/dll/levelfix.cpp:254-279]
[code: opfor:src/dll/levelfix.cpp:323-393]
[code: opfor:src/dll/levelfix.cpp:492-616]
[code: opfor:src/dll/levelfix.cpp:757-760]
[code: opfor:src/dll/levelfix.cpp:812-854]

### MOD-IN-1: Controller input and on-screen prompts

Status: **implemented** · Variants: PC

| Address | Kind | What, why |
|---|---|---|
| 0x7CC4A0 | hook, 6 bytes | the global pad update: XInput in the console layout (`[input] xinput_pad=1`, XInput present); its only caller returns to 0x7D2509 |
| 0x7CC820, 0x7CC840 | hooks, 6 bytes | the joystick yaw / pitch scale setters (calls from 0x7C4790 - 0x7C4894 only); 0x80D0C0 reads `Sensitivity3/4/5` |
| 0xF82020, 0xF82050, 0x1BE9C5C | calls, global | Apt key input and mouse, the current Apt target: the pad in the menus (`[input] menu_pad=1`) |
| 0x95B7D4 | patch, 4 bytes (the push of "Win32") | the button-prompt platform string, switched to "Xenon" at run time (`[input] prompts=auto`) |
| 0x59F6C0, 0x758800 | hooks, 6 bytes | the interaction-prompt and hint key names |
| 0x81ADA8 | redirect from 0x7ED7A0 | the client input frame's restriction call: the landing gear, the boat release and the tank horn (entry input 35) share it |

[code: opfor:src/dll/input.cpp:61-63]
[code: opfor:src/dll/input.cpp:211-252]
[code: opfor:src/dll/input.cpp:930-986]
[code: opfor:src/dll/prompts.cpp:47-63]
[code: opfor:src/dll/prompts.cpp:169-194]
[code: opfor:src/dll/inputframe.cpp:19-59]
[doc: bfo:docs/plans/opfor/CONTROLLER_PLAN.md:79-104]

### MOD-WW2-1: The WW2 gameplay patches

Status: **implemented** · Variants: PC, BF1943

| Address | Kind | What, why | Gate |
|---|---|---|---|
| 0x729C90, with 0x7283CB redirected from 0x6E0F50 | hook, 6 bytes; redirect | the damage-area trigger's handler: repair stations | `[gamemode] repair_stations=1` |
| 0x59E190 | hook, 7 bytes (or a probe observer when the probes hold it) | the per-frame AI update: the landing gear's ground-height probe (0x610A20, 0x189037C, 0xB145C0) | `[gamemode] landing_gear=1` |
| 0x1429784 (vtable 0x14296D0 slot 45), only if it holds 0x72BCA0 | slot | the vehicle spawner's update: the landing craft's cradle release (raises OnOccupantInputTriggered through 0x1002950) | `[gamemode] boat_release=1` |
| (0x81ADA8, MOD-IN-1) | | the tank horn on entry input 35 | `[gamemode] tank_horn=1` |
| 0x637770 | hook, 6 bytes | the voice-over event raise: the multikill call-out | always |
| 0x636010 | hook, 6 bytes | the voice-over matcher: diagnostics | `[debug] vo_probe=0` |

[code: opfor:src/dll/repair.cpp:33-37]
[code: opfor:src/dll/repair.cpp:147-163]
[code: opfor:src/dll/gear.cpp:41-48]
[code: opfor:src/dll/gear.cpp:192-216]
[code: opfor:src/dll/boatrelease.cpp:34-38]
[code: opfor:src/dll/boatrelease.cpp:148-166]
[code: opfor:src/dll/horn.cpp:46-60]
[code: opfor:src/dll/voprobe.cpp:29-33]
[code: opfor:src/dll/voprobe.cpp:113-121]
[doc: opfor:research/WW2_REPAIR_STATIONS_LANDING_GEAR_RE_2026-11-14.md:186]

### MOD-PERS-1: The offline career, the profile and the stats screens

Status: **implemented** · Variants: PC

| Address | Kind | What, why |
|---|---|---|
| 0x8639D0 | hook, 6 bytes | `Unlocks::compute`: unlocks from the local stats |
| 0x8527D0 | hook, 8 bytes | `Unlocks::applyLicenses`: licences offline (`[patches] vietnam`, profile.ini `[unlocks]`) |
| listener vtable + 0x20 (through 0x1566B68) | run-time slot, at the first join | the stats commit: `stats.txt` |
| 0x1566B64, 0x848AC0, 0x7D26C0, 0x7D6BF0, 0x7E9960, 0x420DB0 | global, calls | the stats service, find, get / set, constructor, the game's allocator |
| 0x1433B3C, only if it holds 0x841930 | slot | the offline identity: the profile's name instead of "Human player" |
| 0x55DFB0 | hook, 6 bytes | `Client::sendJoin`: name and clan tag (0x59EDD0; the online singleton 0x15713D8, +0x54 the profile) |
| 0x76EC30 | hook, 6 bytes | the main menu's extern getter: My Stats, Weapons & Gadgets, `opforPacific` (`[stats] screens=1`) |
| 0x92AB10 and the menu-event calls | calls | the Onslaught stopwatch's HUD events (ONS-END-1) |

[code: opfor:src/dll/persistence.cpp:48-57]
[code: opfor:src/dll/persistence.cpp:210-225]
[code: opfor:src/dll/persistence.cpp:517-520]
[code: opfor:src/dll/profile.cpp:105-130]
[code: opfor:src/dll/profile.cpp:217]
[code: opfor:src/dll/stats_screens.cpp:52-64]
[code: opfor:src/dll/stats_screens.cpp:632-640]
[code: opfor:src/dll/onslaught_hud.cpp:48-75]
[doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:174-185]

### MOD-DEV-1: Diagnostics and AI research, off by default

Status: **implemented** · Variants: PC

`[debug] probes=1` installs 51 entry probes (session and join states, the server level load, AI start-up and the
`HasAI` test, the Kynapse world, the pathfinder's stages, node lookups and the sector-offset loads 0xF20AD2 -
0xF20BB0); each logs its hits and is skipped alone if its bytes differ. On top of them: `[dev] force_ai` (writes
level + 0x3AE = 1 at 0x5958F0 / 0x575046: bots on stock maps), `[dev] sector_offsets=X,Z` (rewrites the loaded
offsets at 0xF20B48 / 0xF20BB0), the pathfinder counters, `[dev] navgen` (the Kynapse world limit 1 -> 2 by the byte at
0xF025EF; the generator's file capture hooks 0xEAA920 / 0xE955D0; the point-query slot 0x14A83A8), `[dev] navquery`,
`[dev] navsample` (hook 0xB0D370), `[debug] kill_probe` (hooks 0x870070, 0x86DC90, 0x86C070, 0x86BE20) and
`[debug] capture_probe` (hardware breakpoints, no fixed address). None of these is on in a play install.

[code: opfor:src/dll/probes.cpp:71-218]
[code: opfor:src/dll/forceai.cpp:19-47]
[code: opfor:src/dll/sectoroffsets.cpp:20-58]
[code: opfor:src/dll/navgen.cpp:396-421]
[code: opfor:src/dll/navsample.cpp:349-398]
[code: opfor:src/dll/killprobe.cpp:124-143]

### MOD-IMP-1: The network guard and the user-data redirect patch imports by function

Status: **implemented** · Variants: PC

In `DllMain`, before the signature check, the module walks the executable's import descriptors and replaces, by the
resolved function's address, Winsock (15 functions), WinHTTP (2), `IcmpSendEcho2`, `Netbios` and `GetProcAddress`
(the network guard: loopback only, NET-*), and Shell32 (3), Kernel32 (17), Advapi32 (7) and `FindWindowA` (user data,
registry and mutex names redirected; `[paths] userdata=shared` makes it trace-only). No fixed address.

[code: opfor:src/dll/net.cpp:321-349]
[code: opfor:src/dll/net.cpp:400-426]
[code: opfor:src/dll/userdata.cpp:416-457]
[code: opfor:src/dll/opfor.cpp:391-396]

## Open questions

- No run-time check ties the addresses to build 795745 beyond 21 signatures and the per-site bytes; a patched
  executable with those bytes intact but other code moved would be patched blindly. A whole-file hash check was never
  added.
- The persistence listener slot and levelfix's import slots are written without checking what they held
  (MOD-HOOK-3).
- The rank-up / award sting patch (MOD-MODE-1, 0xF909C0 / 0x142B774) has never been heard in game.
