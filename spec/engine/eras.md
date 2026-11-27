# Native eras: a decision for every reader of the game mod

Project OPFOR BFBC2 runs four eras. The user's ruling of 2026-11-27 makes them **native engine eras**, values of the
UI manager's game mod (+0xC8C8C), not a host layer over DICE's two:

| Era | Value | Content set tag | `gameModChange` label |
|---|---|---|---|
| BFBC2 | 0 | `BC2` | `BFBC2` |
| Vietnam | 1 | `VIETNAM` | `BFBC2Nam` |
| BF1943 / Pacific War | 2 | `PACIFIC` | `BFBC2143` |
| BFBC1 | 3 | `BC1` | `BFBC2Beach` |

This file is round GM1's decision table. It starts from `engine/gamemodes.md` (GM0): ERA-MOD-1's 33 instructions on the
game mod and 8 on the requested mod (+0xC8C90), ERA-MOD-2's parser, ERA-RES-1 / ERA-RES-2's per-era resources and
ERA-LIC-1's licence checks. For each instruction, and for each value derived from one, it records:

- what the site controls;
- its exact test;
- what eras 2 and 3 do there: **as 0**, **as 1**, or **own** (their own resource, which needs new data or code);
- why, with the static citation;
- the patch round GM2 applies in battlefield-opfor's host.

ERA-NEW-2's layered design (eras over engine values 0 / 1) is replaced by this one. ERA-NEW-1's list of places is what
it works through.

**Method (round GM1, 2026-11-27).** Static only: capstone over a scratch copy of `BFBC2Game.exe` (PC build 795745,
image base 0x400000); the extern names come from the getters' jump tables (`bfbc2-menu-tools/aptrefslib.py`, plus the
HUD getter 0x777B30's table); the main-menu and HUD movies were read through the suite's APT dump of DICE's Package
main-menu bundle (OPFOR's `work/aptgraft` copy, dated 2026-10-29). Nothing was run, and nothing was written to `game\`.
The probe `spec/probes/gamemodes.py` keeps a digest of every byte range GM2 patches and of every Vietnam check it must
not touch. The statuses: the probe's checks are **verified**, addresses read from the code are **observed**, and the
decisions are **inferred**.

## 1. The contract

### ERA-NAT-1: Four native eras, with tags and labels the engine can carry

Status: **inferred** · Variants: PC

The game mod takes the values 0 to 3 of the table above, both in the engine field and in `extern.gameMod`.

**Tags.** `PACIFIC` is what OPFOR's WW2 levels already carry as their `MatchmakingModID`. BFBC1's tag is `BC1`, not
battlefield-opfor's `BFBC1`. The three fixed-length string builders that write a tag into an engine string
(ERA-SITE-4) copy either 3 bytes (the `BC2` branch) or 7 bytes (the `VIETNAM` branch), with the end pointer hard-coded
to match. `BC1` (3 characters) fits the first shape and `PACIFIC` (7 characters) the second. Each tag site then needs
only a pointer, never a length. `BC1` also mirrors DICE's `BC2`.

**Labels.** `BFBC2143` is DICE's 1943 frame. For BFBC1, GM1 reuses DICE's `BFBC2Beach` instead of adding a label
(battlefield-opfor uses `BFBC2BC1`):

- the only main_nav sprites with four frames (bgBar 466, loggo_mc 477) already have it;
- bgBar's frame 2 places BFBC2's bar art (char 26), a fair fallback for a modern era;
- an unknown label leaves every sprite on its old frame.

DICE's frame order (Beach 2, 1943 3) does not matter, because the engine sends the label, never a frame number
(ERA-UI-1). The setter's label patch pushes a begin / end pair per era (ERA-NAT-5), so the 10-character label needs no
8-byte buffer.

**Menu codes.** `changeGameMod n` sends the era value itself: 2 is the Pacific War and 3 is BFBC1. OPFOR's movie sends 3
for the Pacific War today (DICE's 1943 slot), and battlefield-opfor maps 2 or 3 to BF1943 and 4 to BFBC1. Both change
(ERA-RISK-5).

[inferred: from the fixed lengths at 0x9422C8 - 0x9422FD, 0x77C9B2 - 0x77C9E2 and 0x77D0E1 - 0x77D111 (alloc 8, copy 7, end +7; the `BC2` branches alloc 4, copy 3, end +3), and ERA-UI-1]
[code: opfor:src/tools/ww2/ww2_level_install.py:128] [code: opfor:src/dll/gamemode_rules.h:16]

### ERA-NAT-2: The era value is 0 - 3 everywhere; GM2's thunks keep registers and the stack

Status: **inferred** · Variants: PC

The engine field is written in three places: the setter (ERA-NAT-5), the loading screen and the HUD's start (both from
the parser, ERA-NAT-3), and the constructor (0).

- **The setter must clamp.** `changeGameMod` reaches it through `atoi`, with no range check (ERA-MOD-1). The host's
  detour of the setter (8-byte prologue `83 EC 0C 55 8B 6C 24 14`) turns any value outside 0 - 3 into 0.
- **The parser returns only 0 - 3.** With both in place, no other value reaches the field, and the per-era tables
  below can index by the value after one bounds check.

**Patch shapes.** GM2 patches are in memory only, applied by battlefield-opfor's host DLL (the probe checks that the
file stays DICE's), in four kinds:

- **byte**: an immediate or a branch condition;
- **call-thunk**: a 5-byte `call` into host code, with `90` filling the rest of the range;
- **detour**: a `jmp` to a host function that replaces the original;
- **cave**: a `jmp` to host code that jumps back to one of several addresses.

**The thunk contract.** A thunk keeps every register and EFLAGS except its stated outputs. A "push" thunk ends with the
stack it replaces: for a `push imm32` it leaves one dword. It does this by duplicating its return address, writing the
value under it and returning:

```
push [esp]; push eax; mov eax, <value>; mov [esp+8], eax; pop eax; ret
```

The thunks read the era from `[0x19A4CA8] + 0xC8C8C` themselves, so no site has to supply it.

[inferred: from ERA-MOD-1's stores (0x75FCF0, 0x77332C, 0x775D0A, 0x9563E5) and ERA-NAT-3]
[exe: BFBC2Game.exe 0x75FCB0 (prologue), 0x77D87A (`changeGameMod`: `atoi`, then the setter)]

## 2. The parser and the setter

### ERA-NAT-3: The parser 0x8FF330 learns the era tags

Status: **verified** · Variants: PC

0x8FF330 (49 bytes, then 15 bytes of `int3`) is `cdecl int(eastl::string*)`. It compares the string with `BC2` (0)
and `VIETNAM` (1) through 0x5084F0, which is case-insensitive through the table 0x1502D00, and returns 0 otherwise. Its
two callers store the result in both the game mod and the requested mod:

- the loading screen 0x772DD0 (at 0x77331D), for the next level's `_description`;
- the HUD's start 0x775C00 (at 0x775D02), for the running level's description.

**GM2: a detour at 0x8FF330.** The host function returns `BC2` 0, `VIETNAM` 1, `PACIFIC` 2, `BC1` 3 and anything
else 0. It compares the same way as 0x5084F0 (case-insensitive, whole string), and it reads **the level's string**,
never the active session's era.

**battlefield-opfor's detour is wrong (ERA-OPF-2).** It returns the active era for every call. With native eras that
would load every level in the session's era, whatever its content.

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: patch range `parser` 0x8FF330+49 (digest recorded), on an instruction boundary]
[exe: BFBC2Game.exe 0x8FF330 - 0x8FF360 (`push 0x142B6B4` `BC2`, `push 0x142B6C0` `VIETNAM`, `call 0x5084F0`); 0x5084F0 (bytes through 0x1502D00 on both sides); callers 0x77331D, 0x775D02 (no other call, jump or pointer)]

### ERA-NAT-4: The setter's sign-in gate: eras 2 and 3 switch at once, era 1 is unchanged

Status: **verified** · Variants: PC

**The gate today.** The setter 0x75FCB0 starts with the gate at 0x75FCC0 - 0x75FCCE:

```
cmp [edi+0xC38D4], 0; mov esi, ecx; jne 0x75FCE1; test ebp, ebp; je 0x75FCE5
```

- A non-zero mod with no EA session object reaches 0x75FCCF: `push 1; call 0x920C00`. That stores **1** as the
  requested mod, whatever was asked, posts the online sign-in request (message 0x25, flag +0xC8BAA) and returns without
  switching.
- Mod 0 calls 0x920C00(0) and switches.
- With a session, a non-zero mod goes straight to the switch at 0x75FCEE.

**Unpatched, eras 2 and 3 would break.** Offline, with no session, they would never switch, and they would leave the
requested mod at 1. The presence re-apply 0x94EB3F would later turn that into Vietnam.

**GM2: a cave.** The 15 bytes at 0x75FCC0 (`83 BF D4 38 0C 00 00 8B F1 75 16 85 ED 74 16`) become `E9 <cave>` and ten
`90`. The cave:

```
mov esi, ecx
cmp ebp, 1
je L1
test ebp, ebp
je 0x75FCE5          ; era 0: exactly as before
jmp 0x75FCEE         ; eras 2, 3: switch, no sign-in, no 0x920C00
L1:
cmp dword [edi+0xC38D4], 0
jne 0x75FCEE         ; era 1 with a session: as before
jmp 0x75FCCF         ; era 1 without one: the sign-in, as before
```

**Era 1 is unchanged.** It reaches the same addresses as the original code in both session states. OPFOR's opt-in
`[patches] vietnam` (the user's Vietnam ruling, for owners) patches 0x75FCC9 today (`75 16` -> `EB 23`). Under GM2 it
becomes a host flag that sends era 1 to 0x75FCEE. It stays opt-in and stays era 1's own: no patch for eras 2 and 3
depends on it.

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: patch ranges `setter.entry` 0x75FCB0+8 and `setter.gate` 0x75FCC0+15]
[exe: BFBC2Game.exe 0x75FCC0 - 0x75FCEE; 0x920C00 - 0x920C61 (stores its argument to +0xC8C90 at 0x920C0E, posts message 0x25 when +0xC38D4 is 0); 0x94EB2E - 0x94EB3F]
[code: opfor:src/dll/patches.cpp:87-109]

### ERA-NAT-5: The setter's labels and the redeem pop-up

Status: **verified** · Variants: PC

**The labels today.** After the switch, the setter picks the `gameModChange` label from ebp (the new mod):

- 0 is `BFBC2` (0x75FDA6 / 0x75FDAB);
- 1 is `BFBC2Nam`, through `cmp ebp, 1; jne 0x75FDD2` at 0x75FDB2 and the begin / end pushes at 0x75FDB7;
- any other value gets an empty label.

The label and the event name go to the menus through 0x9299B0. Then `cmp ebp, 1; jne` at 0x75FE32 raises
`showNamRedeemPopup` for 1 only.

**GM2:**

- **0x75FDB2**: `83 FD 01 75 1B` becomes `83 FD 04 73 1B` (`cmp ebp, 4; jae 0x75FDD2`), so eras 1 - 3 take the label
  path.
- **0x75FDB7**: the 10 bytes `68 74 B2 42 01 68 6C B2 42 01` become a push thunk and five `90`. The thunk pushes the
  era's end, then its begin. For era 1 it pushes DICE's own pointers (0x142B274, 0x142B26C). For 2 it pushes
  `BFBC2143` and for 3 `BFBC2Beach`, both host strings. The stack below matches the original, which matters because
  `lea ecx, [esp+0x18]` follows at 0x75FDC1.
- **The redeem pop-up is not patched.** It stays era 1's (ERA-LIC-2).

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: patch ranges `setter.label_test` 0x75FDB2+5, `setter.label_push` 0x75FDB7+10; kept range `redeem` 0x75FE32+5]
[exe: BFBC2Game.exe 0x75FD8C - 0x75FE42]

## 3. The decision table

### ERA-SITE-1: Every reader of the game mod, with its decision and patch

Status: **verified** · Variants: PC

All 33 instructions on +0xC8C8C and 8 on +0xC8C90 (ERA-MOD-1), grouped by function. "Patch" names the probe's range
(each ends on an instruction boundary; its digest is recorded) and the change. **Function** corrects GM0 in one place:
0x93D14F is in 0x93D120, not 0x93CF40.

| Site(s) | Function and callers | Controls | Test | Era 2 | Era 3 | Patch (GM2) |
|---|---|---|---|---|---|---|
| 0x731340, 0x731354, 0x73135A (req) | dead accessors (no call, jump or pointer) | nothing | load / store | n/a | n/a | none |
| 0x9563E5 | UI manager constructor | the start era | store 0 | n/a | n/a | none |
| 0x75FCF0, 0x75FCF6 (req) | setter 0x75FCB0 <- `changeGameMod` 0x77D87A, presence 0x94EB3F, UI message 0x964E4A | the switch | store ebp | own value | own value | clamp at `setter.entry` (ERA-NAT-2) |
| 0x77332C, 0x773332 (req) | loading set-up 0x772DD0 <- loading screen 0x7774A0 (vtable 0x142B21C) | the next level's era | store parser result | own (`PACIFIC`) | own (`BC1`) | `parser` detour (ERA-NAT-3) |
| 0x775D0A, 0x775D10 (req) | HUD start 0x775C00 | the running level's era | store parser result | own | own | `parser` detour |
| 0x920C0E (req) | sign-in request 0x920C00 <- gate 0x75FCD3 / 0x75FCE9 | requested mod while signing in | store arg (1 or 0) | not reached | not reached | `setter.gate` cave (ERA-NAT-4) |
| 0x94EB2E (req), 0x94EB34 | presence connected 0x94E990 | re-apply the requested mod | `cmp` req, mod | as is | as is | none (era-agnostic copy) |
| 0x95FE92, 0x95FE98 / 0x95FEA0 (req) | FSCommand `presenceCancelLogin` (0x95E680) | requested = current | copy | as is | as is | none |
| 0x95D7CE | `extern.gameMod` (generic getter 0x95A1B0) | what the movies read | raw value | 2 | 3 | none: the menus' contract (ERA-UI-3) |
| derived: ebp at 0x75FCCB, 0x75FDB2, 0x75FE32 | setter | sign-in, label, redeem pop-up | `== 0`, `== 1`, `== 1` | no sign-in; `BFBC2143`; no pop-up | no sign-in; `BFBC2Beach`; no pop-up | ERA-NAT-4, ERA-NAT-5 |
| 0x75AEA1 | menu config 0x75AE80 <- reload 0x75F880 <- setter 0x75FCFC, main-menu entry 0x77394F (vtable method 0x773940) | `MenuConfigAsset`: music, intro / credits movies, grids | `== 1` (load, `cmp eax, 1`) | own `pacific_menuConfig` | own `bc1_menuConfig` | `menucfg.*` (ERA-SITE-2) |
| 0x7584A2 | HUD config 0x758480 <- HUD start 0x775D62 | `HudConfigAsset`: vignettes, victory / defeat stings, pickup sounds | `== 1` (load) | own `pacific_hudConfig` | own `bc1_hudConfig` | `hudcfg.*` (ERA-SITE-2) |
| 0x751BC2 | 0x751BA0 <- `extern.gameTip` (HUD getter id 0x81, 0x777C94) | game tips asset | `== 1` | own `Menu/GameTips/pacific_GameTips` | own `Menu/GameTips/bc1_GameTips` | `tips.hud` (ERA-SITE-2) |
| 0x7776EA | loading screen 0x7774A0 | game tips asset | `== 1` | own (as above) | own (as above) | `tips.loading` |
| 0x775D16 | HUD start 0x775C00 | minimap / HUD icon set (atlas + data) | `== 1` | own `icons_pacific0` / `icons_pacific_data` | own `icons_bc10` / `icons_bc1_data` | `icons` (ERA-SITE-2) |
| 0x93D14F (+ `jne` 0x93D198) | 0x93D120 <- 0x944D50 <- `extern.playerStats` (main-menu getter id 0xD2, 0x76F853) | which unlock tree the stats page walks | `== 1` | own (era asset) | own (era asset) | `unlocks.*` (ERA-SITE-3) |
| 0x9208D0 | accessor (no caller, no pointer) | unlock tree | `== 1` | n/a | n/a | none |
| 0x9469D5, 0x946A75, 0x946B35 | `extern.unlockGrid` 0x9469B0, `kitRankData` 0x946A50, `unlockGridChildren` 0x946B10 | the unlock grid's tree | `== 1` | own (era asset) | own (era asset) | `grid.*` (ERA-SITE-3) |
| 0x93C4E1, 0x93C561 | weapon-data lookups 0x93C4D0 / 0x93C550 <- `extern.kitPrimaryWeapon` and the weapon externs (0x77878A, 0x7798B3, 0x778791, 0x95C04B, 0x95C052), 0x941802, 0x93D1F8, 0x9457B9, 0x9465EF, 0x946694, 0x946915 | weapon presentation (names, images) | `== 1` (era map first, then the base one) | own (era asset) | own (era asset) | `weapons.*` (ERA-SITE-3) |
| 0x920CF8, 0x920E45, 0x920F88, 0x9210D5 | new-item tracker (UI +0xC8A38) 0x920CE0, 0x920E30, 0x920F70, 0x9210C0 <- `seenItem` / `seenItemHud` externs and FSCommands (0x95CDEC, 0x95CE0D, 0x96388D, 0x9638AD) | which "new" badges count | `== 1` -> `sete` (derived boolean) | own tag 3 | own tag 4 | `seen.*` (ERA-SITE-5) |
| 0x76FCDE | `extern.playerHasLicenseToNam` (main-menu getter id 0x15D) | the Vietnam licence answer | `== 1`, then token `nam` | as 0 (false) | as 0 (false) | none (ERA-LIC-2) |
| 0x785CEF, 0x786BCD | HUD update 0x785140 | an `OnFire` event and a burn-health timing while the soldier burns (+0x1B0 bit 0) | `== 1` | as 0 | as 0 | none (ERA-SITE-6) |
| 0x78824F | HUD event 0x7875B0 (collectible notifications, `Client.EnableCollectibleNotifications`) | a fallback notification when the event names no listed collectible | `== 1` | as 0 | as 0 | none (ERA-SITE-6) |
| 0x942247 | `extern.multiplayerMaps` 0x9421F0 | the content-set filter on the offline level list | 0 `BC2` / 1 `VIETNAM` / else empty | own `PACIFIC` | own `BC1` | `maps.*` (ERA-SITE-4) |
| 0x9519AC | `extern.levelList` 0x951910 | a tag string built and never compared | 0 / 1 / else | n/a | n/a | none |
| 0x77C994, 0x77D0C3 | server-search FSCommand cases in 0x77BC10 (`startFindServer` and the next) | the online matchmaking attribute `gameMod` | `== 1` `VIETNAM`, else `BC2` | own `PACIFIC` | own `BC1` | `search.*` (ERA-SITE-4) |

The table settles 41 instructions and the derived values of the setter, the tracker and the level list. None of them
indexes memory by the game mod (ERA-RISK-2).

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: 45 patch ranges with their digests, none ragged; the 33 + 8 sites as ERA-MOD-1]
[exe: BFBC2Game.exe every address in the table; 0x93D120 (after `C2 04 00 CC` at 0x93D11C; calls at 0x944EB3, 0x944F93 in 0x944D50, called at 0x76F853, the main-menu getter's case for id 0xD2 `playerStats`)]

### ERA-SITE-2: Named resources: one name table per resource

Status: **verified** · Variants: PC

Four resources are picked by name. Each gets a four-entry host table indexed by the era. Entries 0 and 1 are DICE's
own string pointers, so BFBC2 and Vietnam load exactly what they load today.

- **Menu config, `menucfg`.** At 0x75AEA7, `83 F8 01` becomes `83 F8 00`. At 0x75AEB0, `75 63` becomes `74 63`, so
  era 0 keeps the `menuConfig` branch and eras 1 - 3 take the other one. At 0x75AEB2, `68 64 B1 42 01` becomes a push
  thunk for the era's name; era 1 pushes 0x142B164 `nam_menuConfig`. The thunk keeps ecx (the UI manager, which
  0xF7A310 uses next).
- **HUD config, `hudcfg`.** The same three changes at 0x7584A8 (`83 F8 01`), 0x7584AB (`75 63`) and 0x7584AD
  (`68 40 B0 42 01`); era 1 pushes 0x142B040 `nam_hudConfig`.
- **Tips, `tips.hud` and `tips.loading`.** The 19 bytes `83 BE 8C 8C 0C 00 01 B8 CC AD 42 01 74 05 B8 B4 AD 42 01`
  at 0x751BC2 and 0x7776EA become `8B 86 8C 8C 0C 00` (`mov eax, [esi+0xC8C8C]`), a call to a thunk that turns the era
  in eax into a name pointer, and eight `90`.
- **Icons, `icons`.** The 22 bytes at 0x775D16 become `8B 85 8C 8C 0C 00`, a call to a thunk that returns the data name
  in eax and the atlas name in ecx, `50 51`, and nine `90`, then fall through to 0x775D2C. Era 1 returns the strings
  that 0x150A27C and 0x150A278 point at.

**Defaults.** Until an era's own data exists (round GM3), its entries name the asset the era's levels can load: the
one in the common bundle they stack on (ERA-RISK-1). A name with no loaded asset is not a safe fallback: the tips path
calls through the looked-up object (0x751C10 - 0x751C19) without a null check on the handle found.

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: patch ranges `menucfg.*`, `hudcfg.*`, `tips.hud`, `tips.loading`, `icons`]
[exe: BFBC2Game.exe 0x75AEA1 - 0x75AF15; 0x7584A2 - 0x758510; 0xF7A310 (`lea eax, [ecx+0x823F8]`); 0x751BC2 - 0x751C19; 0x7776EA - 0x77771A; 0x775D16 - 0x775D54 (0x150A270 - 0x150A27C: `icons0`, `icons_data`, `icons_nam0`, `icons_nam_data`)]

### ERA-SITE-3: The one-slot resources: a UISharedConfigAsset per era, chosen by host getters at the slot reads

Status: **verified** · Variants: PC

**What reads the slots.** `UISharedConfigAsset` (272 bytes; one live instance `Menu/shared_config`, returned by
0x920860 and cached at UI +0xC8990) has one Vietnam slot each for the unlock tree (+0x90; BFBC2's at +0xB0) and the
weapon list (+0x80, read through 0x13D34C0; BFBC2's at +0xE0 through 0x13D3440).

- **The unlock-grid model.** It is built once at UI +0xC3F2C (0x94C580 and 0x957140 -> constructor 0x946330). It keeps
  `asset+0xB0` at +0x134 and `asset+0x90` at +0x138. GM0's "lists +0x134 / +0x138" are these two slots.
- **The weapon maps.** They are built once by 0x9381F0, guarded by +0xC89A8: BFBC2's at UI +0xC899C / +0xC89BC, and
  Vietnam's at +0xC89DC / +0xC89FC from +0x80. Every slot reader is in the table above.

**The design.** Each era has its own `UISharedConfigAsset` partition, `Menu/pacific_shared_config` and
`Menu/bc1_shared_config`, in DICE's type. It is a full clone of `shared_config`, with the era's own tree and weapons in
the Vietnam-position slots. The engine's resource manager loads it from the era's startup patch (GM3). Host getters
return it at the slot reads only:

- **`unlocks`.** At 0x93D14F the immediate `01` becomes `00`, and at 0x93D198 `75 0C` becomes `74 0C`. The 10 bytes
  `E8 C1 36 FE FF 05 90 00 00 00` at 0x93D19A become a call to a thunk and five `90`. The thunk returns eax =
  (era 1 ? DICE's asset : the era asset) + 0x90.
- **`grid.*`.** At each site the immediate `01` becomes `00` and `75 0D` becomes `74 0D`. The 6 bytes
  `8B 8D 38 01 00 00` (`8B 8B` at 0x946A83) become a call to a thunk and one `90`. The thunk returns
  ecx = era 1 ? [ebp / ebx + 0x138] : era asset + 0x90.
- **`weapons.*`.** At each site the immediate `01` becomes `00` and `75 20` becomes `74 20`. The 6 bytes
  `8D B7 DC 89 0C 00` (`FC 89` in the second) become a call to a thunk and one `90`. The thunk returns esi = era 1 ?
  edi + 0xC89DC (+0xC89FC) : the host's era map. The host builds each era map from the era asset's
  Vietnam-position weapon list, keyed as 0x9381F0 keys Vietnam's (from 0x9385C0). A miss still falls back to BFBC2's
  map, as Vietnam's does.

**Why this option.** The other two lose:

- **Swapping the contents of DICE's slots on an era change.** It would rewrite reference-counted arrays of a loaded
  asset. It would not reach the weapon maps, which are built once. And Vietnam's data would no longer be exactly
  DICE's between switches.
- **Extra data reached by a code cave.** It would have to imitate the typeinfo arrays that the engine's readers walk.

A typed clone keeps every reader on real engine objects, and keeps the 20 callers of 0x920860 for the other members
(HUD render config, kill messages, input schemes) on DICE's asset.

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: patch ranges `unlocks.*`, `grid.*`, `weapons.*`; ERA-RES-2's Vietnam half 2 + 26 items]
[exe: BFBC2Game.exe 0x920860 (`/shared_config`, cache +0xC8990); 0x13D30C0 / 0x13D3440 (+0xE0) / 0x13D34C0 (+0x80); 0x94C5A3 - 0x94C5C0 and 0x957574 - 0x957588 (`asset+0x90`, `asset+0xB0` to 0x946330); 0x946389 / 0x94638F (+0x134, +0x138); 0x9381F0 (guard +0xC89A8, maps +0xC899C, +0xC89BC at 0x93838F / 0x938536; the +0x80 list from 0x9385C0); 0x93D198 - 0x93D1AB]
[data: spec/engine/typeinfo/game_shared.md, UISharedConfigAsset row: 272 bytes, WeaponDataVietnam @128, UnlocksDataVietnam @144, UnlocksData @176, WeaponData @224]

### ERA-SITE-4: The tag sites: the `BC2` and `VIETNAM` branches carry four tags

Status: **verified** · Variants: PC

Three builders turn the era into an engine string. Each has a `BC2` branch (alloc 4, copy 3, end +3) and a `VIETNAM`
branch (alloc 8, copy 7, end +7). Eras 0 and 3 take the first and eras 1 and 2 the second (ERA-NAT-1). Only the branch
choice and the pushed pointer change.

- **`maps`, `extern.multiplayerMaps`.** The 8 bytes `8B 82 8C 8C 0C 00 3B C3` at 0x942247 become a call to a thunk and
  three `90`. The thunk sets eax to 1 for eras 1 and 2, otherwise 0, and ends with `cmp eax, 0`, so the `jne` at
  0x942266 and the `cmp eax, ecx` (ecx = 1) at 0x9422B3 route as before. The pushes at 0x942291 (`BC2`) and 0x9422E0
  (`VIETNAM`) become push thunks: `BC2` / `BC1` and `VIETNAM` / `PACIFIC`.
- **`search.a` and `search.b`, the online attribute.** The 7-byte compares at 0x77C994 and 0x77D0C3 become a call to a
  thunk and `90 90`. The thunk sets ZF for eras 1 and 2, so the `jne` at 0x77C9AC / 0x77D0DB sends 0 and 3 to the `BC2`
  branch. The pushes at 0x77C9C0 / 0x77D0EF (`VIETNAM`) and 0x77CA63 / 0x77D194 (`BC2`) become push thunks.

After this, the offline list filter compares each record's tag with the era's own. The online attribute matters only
with a network, which this project never opens; it is patched for consistency.

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: patch ranges `maps.*`, `search.*`]
[exe: BFBC2Game.exe 0x942211 (`mov ecx, 1`), 0x942247 - 0x942324 (0x942302's else branch: an empty string, `push 0x14D68C8`); 0x77C994 - 0x77CA2B, 0x77CA55 - 0x77CA77; 0x77D0C3 - 0x77D146, 0x77D186 - 0x77D194]

### ERA-SITE-5: The new-item tracker: tag = era + 1 (the tag's writer still to trace)

Status: **inferred** · Variants: PC

The tracker's entries (map nodes under UI +0xC8A38 +8) carry a tag at node +0x10: 1 means BFBC2 only and 2 means
Vietnam only. The four functions compute `sete` from `mod == 1`, then skip tag-1 entries under Vietnam and tag-2
entries otherwise.

The tags follow era + 1, so the general rule is: show an entry whose tag is 0 or era + 1. Era 2 gets tag 3 and era 3
tag 4.

The writer of the tag was not found: a scan for constant stores to +0x10 in 0x900000 - 0x980000 found only a font
style. So GM2 first traces how entries are inserted, then replaces each function's boolean and tag tests (0x920D24 -
0x920D86 in 0x920CE0, and the same shape in the other three) with one cave test.

Until then eras 2 and 3 get the `sete` 0 path: BFBC2's filter, which hides Vietnam's badges and shows BFBC2-only ones.

[inferred: from the two tag tests (0x920D2B `cmp [eax+0x10], 1`, 0x920D80 `cmp [eax+0x10], 2`) and the boolean at 0x920D05]
[exe: BFBC2Game.exe 0x920CE0 - 0x920D86; 0x963887 (`lea ecx, [ebp+0xC8A38]`)]

### ERA-SITE-6: The sites left as they are

Status: **observed** · Variants: PC

These sites keep their bytes. Eras 2 and 3 behave as 0 at each one.

- **`playerHasLicenseToNam` (0x76FCDE).** It is false outside era 1. It is a licence test and stays one (ERA-LIC-2).
- **The burn sites (0x785CEF, 0x786BCD).** They act only on a soldier whose +0x1B0 bit 0 is set (burning). Neither BF1943
  nor BFBC1 has a fire weapon.
- **The collectible fallback (0x78824F).** It runs only when a collectible event names no listed collectible. Era 3's
  campaign uses listed ones; inferred.
- **The redeem pop-up (0x75FE32).** It belongs to era 1 (ERA-NAT-5).
- **The plain copies.** These are 0x94EB34, 0x95FE92 and the dead accessors.
- **The levelList tag (0x9519AC).** It is never compared.
- **The raw `extern.gameMod` (0x95D7CE).** It is the menus' contract (ERA-UI-3).

**What no era reader selects.** None of the 41 instructions chooses the persistence template, a stats category, the
award tree, a localisation table or the loading music:

- the template follows campaign or multiplayer (GMD-PERS-1);
- the stats category follows the mode's query id (GMD-SW-3);
- the text and music follow the level's and the movies' data (the description's `Music`, the `NAM_` keys).

An era gets its own of these through data and the module, not through a game-mod patch.

[exe: BFBC2Game.exe 0x76FCDE - 0x76FD0D; 0x785CEF - 0x785D42 (`OnFire` 0x142B85C); 0x786BCD - 0x786C30; 0x78812F - 0x78829C (`Client.EnableCollectibleNotifications`, the loop over [edi+0x70]+0x30's 0x14-byte records, 0x923390 when none matched)]

## 4. The menus

### ERA-UI-1: DICE's main_nav was drawn for four eras; the engine sends labels, not numbers

Status: **observed** · Variants: PC

DICE's Package `main_nav` holds these mode frames:

- **bgBar 466 and loggo_mc 477**: frames `BFBC2` (0), `BFBC2Nam` (1), `BFBC2Beach` (2) and `BFBC2143` (3). bgBar's
  frame 2 places BFBC2's bar art (char 26), and its frame 3 adds and removes nothing, so it keeps frame 2's art.
  loggo_mc's frame 2 removes the logo, and its frame 3 adds nothing: both are empty.
- **gameModBackground 438**: frames `BFBC2`, `BFBC2Nam` and an empty `BFBC2143`.
- **gameModSelector 443, sprites 48 / 53 / 58 / 63 and pageHeaders**: two frames, `BFBC2` and `BFBC2Nam`.

Only sprite 471 tests `extern.gameMod` with more than 1. Its `gameModChange` listener moves `home_btn` to its origin
+170 for 1, and to its origin for 2, for 3 and for any other value.

The listeners take the event's label (`gotoAndStop(label)`, `setGameMod(label)`). Only an empty label makes
`setGameMod` choose, and then only between `BFBC2Nam` (`gameMod == 1`) and `BFBC2`. So the numbering DICE's frame order
suggests never reaches the engine. The native contract (ERA-NAT-1) uses DICE's frames by label: 2 for `BFBC2143`, 3 for
`BFBC2Beach`.

[data: scratch dump of DICE's Package main_nav (OPFOR's work/aptgraft copy), 2026-11-27: frame labels of 438 (3), 466 (4), 477 (4); sprite 471's listener at 0x052019 - 0x0520B4 (`EA_PushByte 2`, `EA_PushByte 3`); setGameMod at 0x01AD24]
[doc: opfor:research/WW2_MENU.md:129-138]

### ERA-UI-2: Every movie reads extern.gameMod as "1 or not"

Status: **observed** · Variants: PC

In DICE's 22 Package main-menu movies there are 32 reads of `extern.gameMod`:

| Movie | Reads |
|---|---|
| main_nav | 15 |
| weaponicon | 5 |
| loadingscreen_mp | 3 |
| mp_your_stats | 2 |
| sharedGraphics | 2 |
| main_root | 1 |
| leaderboards | 1 |
| mp_stats | 1 |
| rank_unlock | 1 |
| wag | 1 |

In the HUD movies there are 16 reads in hudlayer_shared and 1 in mp_objectives.

Every read compares with 1, with two exceptions:

- sprite 471's three-way test (ERA-UI-1);
- hudlayer_shared sprite 139 and its twin: frame 0 goes to `BFBC2Nam` when the value is 1, and frame 1 goes back to
  `BFBC2` when it is 0. With 2 or 3 they stay on frame 0, BFBC2's.

Unedited, every DICE movie draws eras 2 and 3 as BFBC2: art, `_NAM` team-name suffix (main_root and loadingscreen_mp
`getGModStringExtensino`), `NAM_ANY` map picture, kit images.

[data: scratch dumps of the 22 movies of DICE's Package main-menu bundle and OPFOR's work/aptgraft/hudlayer_shared_dump.txt and mp_objectives_dump.txt, 2026-11-27: `extern.gameMod` reads per movie as listed; compares with 1 except the cases named]

### ERA-UI-3: The menus' contract for UI0 - UI7

Status: **inferred** · Variants: PC

For the UI rounds and GM3's movies:

1. **`extern.gameMod` is the native era (0 - 3)**, raw from 0x95D7CE. OPFOR's `extern.opforGameMod` (3 for the Pacific
   War on engine 1) retires: its tests become `extern.gameMod == 2`.
2. **`changeGameMod n` sends 0 - 3.** OPFOR's PACIFIC WAR button sends 2 (not 3), and a BFBC1 button sends 3.
3. **`gameModChange(label)` carries `BFBC2` / `BFBC2Nam` / `BFBC2143` / `BFBC2Beach`.** Every mode-aware sprite needs
   a frame per label it should show. DICE's empty `BFBC2143` and `BFBC2Beach` frames are the places for the new art.
4. **Load-time picks** run when the main menu comes back after a level, with no `gameModChange` (ERA-MOD-2). These are
   `setGameMod`'s empty-label branch, the four sprites' frame-0 / 1 scripts, hudlayer_shared's sprite 139 and every
   `== 1` test of ERA-UI-2. Each must handle 2 and 3. Where an era borrows Vietnam's look, test `gameMod != 0` and
   `!= 3`, never `== 1`.
5. **Per-era suffixes and pictures** (`getGModStringExtensino`: `_NAM`; GM3 adds `_PW` and `_BC1` strings) and the
   era's `*_ANY` map picture are data.
6. **The HUD's movies are chosen by the level's mode partition** (each era's own `HudRootName`, as Vietnam's
   `nam_hud_root`), not by the era. Only the shared layers' `gameMod` tests follow the era.

[inferred: from ERA-UI-1, ERA-UI-2, ERA-MOD-2 and GMD-DATA-1]
[code: rust/bc2-menu/src/pacific.rs:230] [code: rust/bc2-menu/src/pacific.rs:875] [code: rust/bc2-menu/src/pacific.rs:882]

## 5. Licence and ownership

### ERA-LIC-2: No native-era patch touches Vietnam's checks

Status: **verified** · Variants: PC

**The kept ranges.** These are the bytes of Vietnam's ownership checks, which stay DICE's under GM2:

- the redeem pop-up's test (0x75FE32);
- `playerHasLicenseToNam`'s test (0x76FCDE, 28 bytes);
- the licence-token test 0x649ED0;
- its licence callers: `applyLicenses` 0x8528E3 / 0x852904, the level list's licence filter 0x951A83 and the
  persistent-weapon checks 0x6523A7 / 0x6523C8 / 0x6524EC;
- the sign-in request 0x920C00.

The probe records a digest of each and checks that no patch range overlaps one.

**How each patch spares era 1:**

- the gate cave reaches the original addresses for era 1 (ERA-NAT-4);
- the label thunk pushes DICE's own `BFBC2Nam` pointers;
- every table, slot or tag thunk returns DICE's own pointer, asset slot or map for era 1;
- every branch change (`== 1` to `!= 0`) only adds eras 2 and 3 to era 1's path, with era 1's data;
- the entitlement queries (`Persistence/MP_Template`'s `EntitlementAsset`) are data, and no patch reads them.

OPFOR's opt-in `[patches] vietnam` (0x75FCC9, 0x75FE35) is the user's separate choice for Vietnam owners. GM2 neither
widens it nor makes eras 2 and 3 depend on it.

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: 10 kept ranges with digests, none ragged, no overlap with the 45 patch ranges]
[exe: BFBC2Game.exe 0x649ED0 and the callers listed (ERA-LIC-1)]

### ERA-LIC-3: Eras 2 and 3 carry no entitlement gate, which is a data rule

Status: **inferred** · Variants: PC

An era's gate is its levels' `Licenses` and `KickIfLicenseMissing`, and its items' licences: tokens that the level
list (0x951A83) and `applyLicenses` test. The era value itself is not a gate (ERA-LIC-1).

So the native eras' descriptions carry no licence token. In particular they do not carry `nam`. OPFOR's WW2 installer
writes `Licenses nam` and `KickIfLicenseMissing true` today, inherited from the layered design, and GM3's data drops
both for era 2.

Vietnam's levels keep `nam`. No check changes, so nothing is bypassed; the new content is simply not gated.

[inferred: from ERA-LIC-1 and GMD-DATA-3]
[code: opfor:src/tools/ww2/ww2_level_install.py:128]

## 6. Risks

### ERA-RISK-1: Sites that assume a boolean, and assets that must exist

Status: **inferred** · Variants: PC

These sites treat the mod as a two-way value:

- every `== 1` site of ERA-SITE-1;
- the tracker's `sete` (ERA-SITE-5);
- the three-way tag builders (ERA-SITE-4);
- the setter's label (ERA-NAT-5);
- the movies' tests (ERA-UI-2).

Each one GM2 does not patch sends eras 2 and 3 down BFBC2's path, silently. ERA-SITE-1 lists the ones meant to.

**Assets must exist.** A table entry must never name an asset the loaded bundles lack: the HUD's start aborts when the
icon set fails to load (0x775D37 - 0x775D44 returns false), and the tips path dereferences what it looked up. Each
era's names stay at their borrowed default (ERA-SITE-2) until GM3 registers the data (REG-NEW-1).

[inferred: from ERA-SITE-1 - ERA-SITE-5 and ERA-UI-2]
[exe: BFBC2Game.exe 0x775D32 - 0x775D44]

### ERA-RISK-2: No array is indexed by the game mod

Status: **observed** · Variants: PC

Of the 33 instructions:

- 20 compare with 1;
- one compares two fields;
- five store;
- seven load. Of these loads, 0x7584A2 and 0x75AEA1 compare with 1, 0x942247 and 0x9519AC compare with 0 and 1,
  0x95D7CE hands the value to `MakeInt` (0xF7C5D0), 0x95FE92 copies it, and 0x731340 is dead.

None scales the value into an address, and no table is sized by it. GM2's own tables must clamp (ERA-NAT-2), because
`changeGameMod` is unchecked.

[exe: BFBC2Game.exe the 33 addresses of ERA-MOD-1, each read to the first use of the loaded value]

### ERA-RISK-3: Network and server strings, saved state

Status: **inferred** · Variants: PC

**Network.**

- The client's matchmaking attribute `gameMod` is ERA-SITE-4.
- The server publishes `gameMod` from its configuration string +0x118, and keeps the levels whose `MatchmakingModID`
  equals its +0xB8 string in its `LevelListQuery` (0x864E50; empty is read as `BC2`). The writers of both were not
  read (GM0-rest). A server hosting an era's levels needs its own tag there, or its level list drops them.
- Offline hosting goes through OPFOR's module (NET-SB-3). Whether the in-process server consults this list offline is
  not known: a playtest.

**Saved state.**

- The exe saves no era (ERA-MOD-2). After a match the era is the level's, and the main menu reloads that era's menu
  configuration (0x773940 -> 0x75F880). A host that remembers the player's era (OPFOR's `profile.ini [gamemode]
  pacific`, battlefield-opfor's hub) stores the era value, and re-applies it through the setter only from the menus.
- The career stays shared (GMD-PERS-1): an era's unlock state lives in the one MP career unless the module keeps its
  own (PERS-OPF-1).

[inferred: from ERA-MM-1, ERA-MOD-2, GMD-PERS-1]
[exe: BFBC2Game.exe 0x84FCA8 - 0x84FD52; 0x865064 - 0x8650D5 (`+0xB8` compared at 0x8650CB); 0x773940 - 0x77394F]

### ERA-RISK-4: What battlefield-opfor's host must change

Status: **inferred** · Variants: PC

- **The parser detour.** It must parse the level's string with the four tags (ERA-NAT-3), not return the active era
  (ERA-OPF-2).
- **The era values.** `ModeId::engine_mode` returns the native value (today 2 and 3 return 1).
- **The menu codes.** `from_menu_code` maps 2 to BF1943 and 3 to BFBC1 (today 2 / 3 -> BF1943, 4 -> BFBC1), and
  `menu_code` returns 2 / 3.
- **The tag.** The BFBC1 tag becomes `BC1`.
- **The label.** The BFBC1 label becomes `BFBC2Beach`. The single 9-byte label buffer and its 8-character rule go,
  replaced by ERA-NAT-5's thunk.
- **The gate.** The unconditional gate patch (0x75FCC9) becomes ERA-NAT-4's cave, with the Vietnam opt-in as a flag.
- **The menu-config buffer.** The re-pointed buffer at 0x75AEB3 becomes ERA-SITE-2's table.
- **The rest.** It adds ERA-SITE-2 - ERA-SITE-5's patches, checking every range's bytes against the probe's digests
  before writing.

[inferred: from the code of battlefield-opfor read on 2026-11-27 (crates/opfor-dll/src/gamemode.rs: `detour_parse_game_mod`, the patch constants; crates/opfor-vfs/src/modes.rs: `ModeId`, `engine_mode`, `from_menu_code`, the mode table's tags and labels) against ERA-NAT-1 - ERA-SITE-5]

### ERA-RISK-5: What Project OPFOR's module and data must change

Status: **inferred** · Variants: PC

- **The rules.** `gamemode_rules.h`'s `MapRequest` (3 or 2 -> engine 1 plus the Pacific flag), `Effective` and
  `MenuMode` give way to the native value.
- **The menus.** The menu edits send `changeGameMod 2` and test `extern.gameMod == 2` (ERA-UI-3).
- **The data.** The WW2 descriptions drop `Licenses nam` / `KickIfLicenseMissing` (ERA-LIC-3), and the Pacific War gets
  its own menu config, HUD config, tips, icon set and `UISharedConfigAsset` (ERA-SITE-2, ERA-SITE-3) in place of
  Vietnam's.
- **The hooks.** The module's own hooks that test "engine mod 1 plus flag" (level filters, menu music, tips) test
  era 2.

[inferred: from ERA-NAT-1, ERA-UI-3 and ERA-LIC-3 against OPFOR's module and installer as read on 2026-11-27]
[code: opfor:src/dll/gamemode_rules.h:44] [code: opfor:src/tools/ww2/ww2_level_install.py:128]

## Pending playtests (in-game tests are paused)

- ERA-NAT-3: a BFBC2, a Vietnam and a Pacific level in one session, each loading in its own era (icon set, HUD sounds).
- ERA-NAT-4: switching to eras 2 and 3 offline with no session (the menu switches; no sign-in request); era 1 unchanged
  with and without `[patches] vietnam`.
- ERA-NAT-5: the `gameModChange` labels reaching DICE's `BFBC2143` and `BFBC2Beach` frames; no redeem pop-up outside
  era 1.
- ERA-SITE-2: each era's menu music, HUD stings, tips and icons from its own table entry.
- ERA-SITE-3: the stats page, unlock grid and weapon names of an era asset; a miss falling back to BFBC2's weapons.
- ERA-SITE-4: the offline level list filtered by `PACIFIC` and `BC1`.
- ERA-UI-2: the main menu after a level of each era (load-time picks with no `gameModChange`).
- ERA-RISK-3: whether the in-process server's level list drops an era's levels when its +0xB8 string is `BC2`.

## Open questions

- The writer of the new-item tracker's entry tag (ERA-SITE-5).
- The writers of the server's +0xB8 / +0x118 strings (ERA-RISK-3; queued in GM0-rest).
- How 0x9381F0 keys its two Vietnam weapon maps (+0xC89DC / +0xC89FC), which the host's era maps must copy.
- Whether a name lookup of a missing asset (0x459530) returns a null handle or an empty one (ERA-RISK-1).
