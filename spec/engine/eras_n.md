# N native eras: every limit on the era count, and a host era table

`engine/eras.md` (round GM1) settles four native eras: 0 BFBC2 (`BC2`, `BFBC2`), 1 Vietnam (`VIETNAM`, `BFBC2Nam`),
2 BF1943 / Pacific War (`PACIFIC`, `BFBC2143`) and 3 BFBC1 (`BC1`, `BFBC2Beach`). This file is round GM4's answer to
the next question: what stops `BFBC2Game.exe` (PC build 795745) from taking a fifth era, and how GM2's patch set can
read a host-owned era table for any number of eras instead of hard-coding 2 and 3.

It covers:

- every place that bounds the count at 2 or 4, with what an N-era table needs there (ERA-N-1 - ERA-N-7);
- the design: the table, the generalised patch set, and what era 4 costs on top of GM2 (ERA-N-8 - ERA-N-10);
- the menus: the labels' contract and a fifth frame (ERA-N-11, ERA-N-12);
- the online and saved-state paths that must never see an unknown era (ERA-N-13).

**Method (round GM4, 2026-11-27).** Static only: capstone over a scratch copy of `BFBC2Game.exe` (image base
0x400000); `bfbc2 apt dump` of DICE's 22 Package main-menu movies (OPFOR's `work/aptgraft` copy) and OPFOR's
hudlayer_shared / mp_objectives dumps; `bfbc2 aptrefs` for extern names. Nothing was run and nothing was written to
`game\`. `spec/probes/gamemodes.py` gained the digests of the nine further ranges (ERA-N-3) and the tracker's page
count. The statuses: the probe's checks are **verified**, addresses read from the code are **observed**, the design is
**inferred**.

## 1. The limits

### ERA-N-1: Every place that bounds the era count

Status: **observed** · Variants: PC

The game mod itself does not bound the count: +0xC8C8C and +0xC8C90 are dwords, and all 11 stores are `mov dword`
(ERA-MOD-1). The bounds are in the code that reads it, in DICE's data shapes and in the movies.

| # | Limit | Where | Bound | What N eras need |
|---|---|---|---|---|
| 1 | no range check on the requested value | `changeGameMod` 0x77D87A (`atoi`), UI message 0x38 (0x964E4A, ERA-N-6) | none | the setter clamp (ERA-NAT-2), unsigned against N |
| 2 | the level-tag parser | 0x8FF330 | 2 tags | the detour compares the table's tags (ERA-NAT-3) |
| 3 | the setter's labels | 0x75FDA4 - 0x75FDC1 (a `jne` on 0x75FD8C's `test ebp, ebp`, `cmp ebp, 1`, two push pairs; any other value skips to 0x75FDD2 with an empty label) | 2 labels | `setter.labels` (ERA-N-3): one push-pair thunk for any row, eras 0 and 1 included, so every label can be renamed |
| 4 | the sign-in gate | 0x75FCC0 | any non-zero is era 1's path | GM1's cave (ERA-NAT-4): only era 1 keeps DICE's gate |
| 5 | the 20 `== 1` content sites | ERA-SITE-1 | 2-way | name tables and slot getters indexed by the row (ERA-SITE-2, ERA-SITE-3); the 7 sites ERA-SITE-6 keeps stay DICE's |
| 6 | `UISharedConfigAsset` | the type: one BFBC2 and one Vietnam slot pair (ERA-RES-2) | 2 | one asset per era (ERA-SITE-3), so N assets |
| 7 | the weapon maps | 0x9381F0, built once: +0xC899C / +0xC89BC (BFBC2), +0xC89DC / +0xC89FC (Vietnam) | 2 | a host map per era row from its asset |
| 8 | the unlock-grid model | UI +0xC3F2C (0x946330): lists +0x134 / +0x138; 0x94D440 picks one by a byte flag | 2 | the era asset's tree at the slot reads (ERA-SITE-3); the tracker writer reads it too (ERA-N-2) |
| 9 | the tag builders | 0x942247 (offline list), 0x77C994 / 0x77D0C3 (online search), 0x9519AC (never compared) | 2 tags; lengths 3 and 7 fixed | ERA-N-5 |
| 10 | the menu tracker | pages 0 - 3 loaded (0x9573D1), pages 0 / 1 / 2 written (0x9573F0), 4 two-way filters | 4 pages, 2 eras | ERA-N-2 |
| 11 | the launcher pipe | `expansion` = `bfbc2vietnam` at 0x5B5108 | 2 | nothing unless a host launches through it (ERA-N-6) |
| 12 | the online attribute and the server strings | ERA-MM-1 | strings | ERA-N-13 |
| 13 | the movies | 32 + 17 reads of `extern.gameMod` (ERA-UI-2), 17 sprites with era frames, the selector, label literals | 2 - 4 frames | ERA-N-12 |

**What does not bound it.** No array is indexed by the mod (ERA-RISK-2). Persistence, awards, stats and text are not
chosen by era (ERA-SITE-6, GMD-PERS-1); the tracker's pages are the one saved value derived from it (ERA-N-2). The
four GM1 found dead or never compared stay so (ERA-N-7).

[exe: BFBC2Game.exe every address in the table; the 33 + 8 field instructions re-read: 5 + 6 stores, all `mov dword`]
[inferred: from ERA-MOD-1, ERA-SITE-1 - ERA-SITE-6, ERA-UI-1, ERA-UI-2 and ERA-N-2 - ERA-N-7]

### ERA-N-2: The menu tracker keeps four pages, and DICE's page 3 is the Store

Status: **verified** · Variants: PC

This is the "new item" tracker of ERA-SITE-5 (UI +0xC8A38). Its outer map's key, node +0x10, is a **page** of the
compiled enum `UIMenuTrackerPage`: Awards 0, MenuUnlocks 1, MenuUnlocksVietnam 2, Store 3, Count 4. Each page holds a
map of item names, each with two "seen" flags (+0x1C, +0x1D).

- **The writer** is 0x9573F0 (from 0x764808; it runs once, when the tracker's state goes from 7 to 15). It calls
  0x957140, the loader, then inserts every item not yet known:
  - into page 0 from 0x9484A0 (inferred: the award list, by the enum);
  - into page 1 from the unlock-grid model's BFBC2 list (0x94D440 with flag 0, +0x134);
  - into page 2 from its Vietnam list (flag 1, +0x138).

  The only callers of the map's insert 0x955520 and of the lookup 0x921200 are in 0x957140 and 0x9573F0. So no code
  writes page 3; it is only read back from the profile.
- **The loader** 0x957140 reads one profile setting per page, for pages 0 to 3 (`cmp eax, 4; jl` at 0x9573D1). It
  turns the page number into its setting name (0x4E61B0 on the converter 0x1C06768) and splits the value on `;`.
- **The saver** 0x932CB0 writes every page in the map, `name:%d:%d;...` per page, through the profile-options setter
  0x82FCF0, then saves the profile (0x9205A0). These are GameSettings.bin's `GstMenuTracker` entries, 4 in DICE's game
  (SET-BIN-1).
- **The filters** (0x920CE0, 0x920E30, 0x920F70, 0x9210C0) skip page 1 under era 1 and page 2 otherwise. Pages 0 and 3
  always count.

**This corrects ERA-SITE-5.** Its "tag = era + 1" gives era 2 page 3, which is the Store page, and era 3 page 4, which
the loader never reads back. The rule for N eras:

- **Page of an era:** era 0 page 1, era 1 page 2, era k >= 2 page k + 2. So Pacific gets 4, BFBC1 5 and era 4 6.
- **The loader bound:** `tracker.bound` at 0x9573D1, `83 F8 04`, becomes `83 F8 <N + 2>`. That allows N <= 125.
- **The writer:** a call-thunk at `tracker.writer` (0x957631, `mov eax, [ebx+0x82CE0]`, reached from both the
  page-2 loop and its empty-list jump). For each era k >= 2 it inserts that era tree's items into page k + 2, as
  0x9573F0 does for page 2: 0x921200 first, then 0x955520 twice with both flags 0. It then replays the instruction
  it replaced. At that point ebx is the UI manager and esi the tracker.
- **The filters:** each function's 13 bytes `seen.*.key` (0x920D24, 0x920E77, 0x920FB4, 0x921107:
  `cmp byte [esp+0x13], 0; je; cmp [eax+0x10], 1; je`) become a `jmp` to one cave and 8 `90`. The cave counts a page
  when it is 0, 3 or the active era's page, and jumps to the function's "count" address (range + 13) or its "skip"
  address (the node step: 0x920D86, 0x920ED6, 0x921016, 0x921166). The boolean `sete` (GM1's `seen.*`) is then dead
  and needs no patch.

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: `tracker_pages` 4 (the immediate at 0x9573D1); ranges `tracker.bound`, `tracker.writer`, `seen.a.key` - `seen.d.key` with their digests, on instruction boundaries]
[data: spec/engine/typeinfo/game_shared.md, UIMenuTrackerPage: Awards 0, MenuUnlocks 1, MenuUnlocksVietnam 2, Store 3, Count 4]
[exe: BFBC2Game.exe 0x9573F0 - 0x957743 (state 7 -> 15 at 0x95741E - 0x957428; page 0 loop 0x957460 - 0x9574BA; page 1 0x9574F1 - 0x95754B; page 2 0x9575CD - 0x957627); 0x957140 - 0x9573EE (0x9571DD - 0x9571F2 the name, 0x95720A the getter 0x80D4B0, `;` 0x140A434, bound 0x9573D1); 0x932CB0 - 0x932E6B (`:%d` 0x14183E0, 0x82FCF0, 0x9205A0); 0x94D440 (byte flag: +0x138 or +0x134); 0x920D24 - 0x920D86; callers of 0x955520 and 0x921200 (all in 0x957140 / 0x9573F0)]

### ERA-N-3: The N-era patch ranges beyond GM1's

Status: **verified** · Variants: PC

The probe records a digest of each of these. Each range ends on an instruction boundary, and none overlaps a range
ERA-LIC-2 keeps.

| Range | Address + bytes | Replaces | Change |
|---|---|---|---|
| `setter.labels` | 0x75FDA4+29 | GM1's `setter.label_test` and `setter.label_push` | a call to a thunk that pushes the row's label end, then its begin (a two-dword push thunk, reading the era itself: the range starts at the `jne` that used 0x75FD8C's flags), and 24 `90`, falling into 0x75FDC1, so every row's label is assigned |
| `maps.builder` | 0x942266+186 | GM1's `maps.tag3` and `maps.tag7`, only for a tag that is not 3 or 7 characters | ERA-N-5's builder cave |
| `tracker.bound` | 0x9573D1+3 | - | the immediate 4 becomes N + 2 |
| `tracker.writer` | 0x957631+6 | - | a call-thunk, then `90` |
| `seen.a.key` - `seen.d.key` | 0x920D24 / 0x920E77 / 0x920FB4 / 0x921107 +13 | GM1's `seen.*` (left as they are) | a `jmp` to the page cave, then 8 `90` |
| `launcher.expansion` | 0x5B5104+25 | - | optional (ERA-N-6) |

GM1's other ranges stand. The online `search.*` ranges are not patched (ERA-N-13).

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: `n_sites` 9 digests, `n_ragged` none, `n_keep_overlap` none]

### ERA-N-4: Two immediates bound N; every other site reads the table

Status: **inferred** · Variants: PC

Only two sites must carry N itself:

- **`tracker.bound`.** It is the imm8 N + 2 (ERA-N-2).
- **GM1's `setter.label_test`** (`cmp ebp, 4; jae`), if the host keeps that shape instead of `setter.labels`.

Both are sign-extended imm8 compares, so N <= 125. Every other patch is a thunk or cave that reads the era and the
table at run time.

**The clamp** at the setter's entry compares unsigned (`cmp ebp, N; jb`). That way a negative `atoi` result also
becomes 0. With the clamp and the parser detour, no value of N or more reaches the field. Each thunk still bounds-checks
its index and uses row 0 past the end, so a table shorter than the field's value can never index past it.

[inferred: from ERA-NAT-2, ERA-N-2, ERA-N-3 and the imm8 encodings at 0x9573D1 (`83 F8 04`) and 0x75FDB2 (`83 FD 01`)]

### ERA-N-5: The tag builders copy 3 or 7 bytes; other lengths need the builder cave

Status: **observed** · Variants: PC

Each builder is a fixed shape. It allocates through 0x420EB0, with arguments (length + 1, name 0x1408D2C, 0, 0, file
0x1408D38, line 0xE9), copies through `[0x1405510]`, sets the string's begin / end / capacity and writes the NUL. The
lengths are immediates:

- `BC2`: alloc 4, copy 3, end + 3;
- `VIETNAM`: alloc 8, copy 7, end + 7;
- empty: the static 0x154C450 / 0x154C451.

**Tags of 3 or 7 characters** need only GM1's thunks: the class thunk picks the branch by the row's tag length, and
the push thunks give the pointer. `BC1` and `PACIFIC` are of this kind.

**A tag of any other length** in the offline list needs `maps.builder` (0x942266 - 0x942320). This is a cave that
allocates length + 1 through 0x420EB0 with DICE's arguments, copies the row's tag, writes begin / end / capacity to
`[esp+0x1C]` / `[esp+0x20]` / `[esp+0x24]` (the frame as at 0x942266), writes the NUL, and jumps to 0x942320 with
eax = end. The string is then freed by DICE's own code, as before.

The builders at 0x77C994 / 0x77D0C3 (online) and 0x9519AC (never compared) need neither (ERA-N-7, ERA-N-13).

[exe: BFBC2Game.exe 0x942266 - 0x942324 (`push 4` 0x942279, `push 3` 0x94228C, `push 8` 0x9422C8, `push 7` 0x9422DB, the empty branch 0x942302); 0x9519B8 - 0x951A5E (the same shape); 0x77C9AC - 0x77CA77]

### ERA-N-6: The launcher pipe sets era 0 or 1 from its `expansion` parameter

Status: **observed** · Variants: PC

The client reads launch requests from the named pipe `\\.\pipe\rome_snowroller`. The requests are `requestState` with
`requestStateParams`, the states `State_ConnectToGameId` / `State_ConnectToUserId` / `State_Matchmaking` /
`State_MenuReady`, and parameters such as `gameid`, `password` and `friendpersonaid`.

One handler, 0x5B5040 (a vtable slot at 0x1419628), looks up the parameter `expansion`. It posts UI message 0x38 with
+0x28 = 1 if the value is `bfbc2vietnam` and 0 otherwise. The UI manager's handler for 0x38 (0x964E38, the jump-table
entry at 0x96531C) passes +0x28 to the setter (0x964E4A). This is the third setter caller GM0 named "a UI message".

- **For N eras.** It yields only 0 or 1, so it can never send an unknown era. The setter clamp covers it anyway.
- **A host that launches into an era through the pipe** replaces the compare at `launcher.expansion` (0x5B5104 -
  0x5B511D) with a cave. The cave maps an `expansion` name per row to the row, and keeps the `push 0x30` the range
  holds.
- **Otherwise it stays DICE's.** Nothing in OPFOR's source names the pipe or `expansion` (searched 2026-11-27).

[exe: BFBC2Game.exe 0x5B5040 - 0x5B5144 (`expansion` 0x141A4F8, `bfbc2vietnam` 0x141A4E8 compared through 0x4304D0 at 0x5B510E; the message: size 0x30, type +0x20 = 0x38, value +0x28, vtable 0x1419640); 0x964A80 - 0x964A96 (`id - 0x12`, byte table 0x965324, dword table 0x9652E4); 0x964E38 - 0x964E4A; the pipe name 0x141A4CC (0x5B4E07)]

### ERA-N-7: The era paths that need nothing

Status: **observed** · Variants: PC

- **0x9201A0** builds `BC2` / `VIETNAM` / empty from an argument. Nothing calls it, jumps to it or holds a pointer to it.
- **The levelList tag (0x9519AC)** is built and never compared (ERA-SITE-6).
- **`extern.seenMenu`** (id 0x143, generic case 0x95CEFA) takes a page number as its parameter (0x91C9E0, then
  0x9127D0). None of the 128 movies `aptrefs` reads names it. A movie that asks about an era's page must pass that era's page from ERA-N-2.
- **`extern.fillColor`** (id 0x15B, generic case 0x95D7E2) is the constant (218, 134, 0) in every era. An era colour
  is a host extern or movie data, not an era site.
- **`BFBC2` compared at 0x662409** is an entitlement-group parse (next to `:ADDSVETRANK` and `ACTIVE`), not the label.

[exe: BFBC2Game.exe 0x9201A0 - 0x9201D9 (no `call` / `jmp` / pointer to it in the image); 0x95CEFA - 0x95CF2E; 0x91C9E0; 0x9127D0; 0x95D7E2 - 0x95D81B; 0x662343 - 0x662419 (`ACTIVE` 0x141C1AC, `:ADDSVETRANK` 0x141DECC)]
[data: bfbc2 aptrefs on DICE's Package and HUD movies, 2026-11-27: `seenMenu` in the exe's table, in none of the 128 movies read; `fillColor` read by 31 movies of the menu, mp2_hud and nam_hud sets]

## 2. The design

### ERA-N-8: A host-owned era table, one row per era value

Status: **inferred** · Variants: PC

The host (battlefield-opfor's mode table, or OPFOR's module) owns an immutable table filled at start-up. Its row index
is the era value, 0 to N - 1. Rows 0 and 1 hold DICE's own pointers and assets, so BFBC2 and Vietnam load exactly what
they load today.

| Field | Read by | Rows 0 / 1 | Row k >= 2 |
|---|---|---|---|
| tag | the parser detour, the offline list (ERA-SITE-4, ERA-N-5) | `BC2` / `VIETNAM` | the era's `MatchmakingModID` (3 or 7 characters, or any with `maps.builder`) |
| label | `setter.labels` | `BFBC2` / `BFBC2Nam` (renameable: ERA-N-11) | the era's label |
| menu config, HUD config, tips, icon atlas + data | the name thunks (ERA-SITE-2) | DICE's strings | the era's names, or a borrowed default until its data is registered (ERA-RISK-1) |
| shared config | the slot getters (ERA-SITE-3) | DICE's one asset (its two halves) | the era's `UISharedConfigAsset` |
| weapon map | `weapons.*` | DICE's maps | built by the host from the era asset |
| tracker page | the tracker cave and writer (ERA-N-2) | 1 / 2 | k + 2 |
| sign-in | the gate cave (ERA-NAT-4) | 0 switches / DICE's gate (+ the `[patches] vietnam` flag) | switches, never signs in |
| redeem pop-up | kept (ERA-LIC-2) | - / DICE's | never |
| launcher `expansion` | optional cave (ERA-N-6) | - / `bfbc2vietnam` | none, unless the host launches through the pipe |

**The invariants:**

- Rows are never added or removed while the game runs.
- Each row's names name assets that the loaded bundles hold.
- Two rows never share a tag, a label or a tracker page.
- N <= 125 (ERA-N-4).
- Every row k >= 2 carries no licence (ERA-LIC-3).

[inferred: from ERA-N-1 - ERA-N-7, ERA-SITE-1 - ERA-SITE-5, ERA-NAT-1 - ERA-NAT-5]

### ERA-N-9: GM2's patch set, read from the table

Status: **inferred** · Variants: PC

Every GM1 patch is already a thunk, a cave or a branch change, so each can read the table. What changes from GM1's
text:

- **Parser detour.** It compares the level's string with each row's tag, case-insensitively and whole-string, and
  returns 0 when none matches.
- **Setter.** The entry clamp is against N. The labels come from `setter.labels`, one thunk for all rows. The gate cave
  is unchanged: era 1 keeps its paths, and every other non-zero era switches.
- **Name tables and slot getters.** They index the row instead of a four-entry array. The branch changes (`== 1` to
  `!= 0`) are the same for any N.
- **Tags.** The class thunk at `maps.class` returns 1 when the row's tag has 7 characters and 0 when it has 3. The push
  thunks return the row's tag. A row with another length uses `maps.builder` (ERA-N-5).
- **The tracker.** `tracker.bound`, `tracker.writer` and the four `seen.*.key` caves follow ERA-N-2. This replaces
  ERA-SITE-5's era + 1.
- **Online search (`search.*`).** Not patched: every era other than 1 sends `BC2` (ERA-N-13). This supersedes
  ERA-SITE-4's `search.*` patches.

The host checks each range's bytes against the probe's digest before writing it, as GM1 requires.

[inferred: from ERA-NAT-2 - ERA-NAT-5, ERA-SITE-2 - ERA-SITE-5, ERA-N-2 - ERA-N-5, ERA-N-13]

### ERA-N-10: What era 4 costs on top of GM2

Status: **inferred** · Variants: PC

**If GM2 is built on the table (ERA-N-9), era 4 is data.**

- **The table row** (ERA-N-8) and N = 5, so `tracker.bound` becomes 7. Its tag is 3 or 7 characters, or the host
  installs `maps.builder` once.
- **The era's data set**, as GM3 lists it for eras 2 and 3:
  - game-mode partitions with its HUD root;
  - menu config, HUD config, tips and icon set;
  - a `UISharedConfigAsset` clone;
  - level descriptions with its tag and no licence;
  - registration (REG-NEW-1).
- **The menu work** of ERA-N-12. This is the largest part.

**If GM2 hard-codes eras 2 and 3**, era 4 also touches every site:

- the clamp, and the label bound 4 -> 5 or a fifth push pair;
- a fifth entry in each name table and slot getter;
- the `maps.class` length test and the push thunks;
- a third compare in the parser detour;
- the tracker bound and caves.

The second path is the GM1 design plus one case at about 15 places, so GM2 should take the first.

[inferred: from ERA-N-8, ERA-N-9, ERA-N-12 and the GM3 row of the ROADMAP]

## 3. The menus

### ERA-N-11: The label is a key the movies match; the engine never reads it back

Status: **observed** · Variants: PC

**The engine side.** The label is a begin / end pair passed with the `gameModChange` event (0x9299B0). The engine has
no length or character limit on it, and never parses a label back:

- the only references to `BFBC2` and `BFBC2Nam` as labels are the setter's pushes (0x75FDA6 / 0x75FDAB,
  0x75FDB7 / 0x75FDBC);
- `changeGameMod` takes a number through `atoi`;
- main_nav's `setGameMod(label)` (0x01AD24) is ActionScript only: frame jumps and `mainMenu.gameModChanged`, with no
  FSCommand.

**The movie side.** The label is matched three ways:

- **As a frame label** (`gotoAndStop(label)`) on the 17 era-labelled sprites (ERA-N-12).
- **As a literal compare.** main_nav's `mainMenu.gameModChanged` (0x03578A) and start-page listener (sprite 118,
  0x03C44B), and rank_unlock's sprite 57 (0x0102AF), compare it with `"BFBC2Nam"`.
- **As emptiness.** `setGameMod` falls back to `gameMod == 1 ? "BFBC2Nam" : "BFBC2"` only for `""` or undefined.

No listener applies a string operation to the label. So DICE's shared `BFBC2` prefix and the label lengths mean
nothing.

**The form a label must keep:**

- not empty, which is the fallback;
- not a numeric string: `gotoAndStop("2")` may be read as a frame number (AVM1 behaviour, unconfirmed in APT);
- unique among the era-labelled sprites' frame labels;
- exactly the string the movies' frames and literals carry.

**Renaming.** A label is a key, not display text. The eras' visible names are art (loggo_mc, bgBar, the selector
buttons) and localised strings, so renaming what the player sees changes those, not the key. Renaming the key means
three changes that must land together:

- the table row;
- every frame label carrying it, in main_nav, hudlayer_shared, sharedGraphics and weaponicon;
- the three `"BFBC2Nam"` literals, plus each movie's load-time `"BFBC2"` / `"BFBC2Nam"` pushes.

For eras 0 and 1 this edits DICE's frames. That is possible through `setter.labels`, but it breaks any unedited DICE
or Vietnam movie, so the recommendation is to keep DICE's two keys.

[exe: BFBC2Game.exe 0x75FDA4 - 0x75FDE3 (`BFBC2` 0x141DEC4 / 0x141DEC9, `BFBC2Nam` 0x142B26C / 0x142B274, the event name 0x142B25C); no other load of 0x142B26C; 0x141DEC4's other load 0x662409 is ERA-N-7's]
[data: scratch `bfbc2 apt dump` of DICE's 22 Package main-menu movies plus OPFOR's hudlayer_shared / mp_objectives dumps, 2026-11-27: 41 `gameModChange` listeners, string operations in none but sprite 471's numeric `_x + 170`; `"BFBC2Nam"` compared at main_nav 0x0357B4, 0x03C474 and rank_unlock 0x0102D8; setGameMod 0x01AD24 - 0x01AE15 has no GetURL]

### ERA-N-12: A fifth frame: 17 sprites, the selector and the load-time picks

Status: **observed** · Variants: PC

These are the sprites whose frame labels name an era, with their DICE frames:

| Movie | Sprites (instance) | Frames |
|---|---|---|
| main_nav | 438 `gameModBackground` | `BFBC2`, `BFBC2Nam`, `BFBC2143` |
| main_nav | 466 `bgBar`, 477 `loggo_mc` | `BFBC2`, `BFBC2Nam`, `BFBC2Beach`, `BFBC2143` |
| main_nav | 443 `gameModSelector`, 444 `pageHeaders`, 48, 53, 58, 63 | `BFBC2`, `BFBC2Nam` |
| hudlayer_shared | 114 `ammoBar`, 118 `grenades`, 139, 145 `bgPlate` | `BFBC2`, `BFBC2Nam` |
| sharedGraphics | 67 `texture`, 95 `shape` | `BFBC2`, `BFBC2Nam` |
| weaponicon | 42 `ammoBackground`, 45 `healthIcon` | `BFBC2`, `BFBC2Nam` |

**A label a sprite lacks leaves it on its current frame**, which is the previous era's art, not BFBC2's (ERA-UI-1). So
for each era, every one of the 17 needs a frame with its label, even if that frame only repeats BFBC2's art. Only 466
and 477 have frames for eras 2 and 3 today. 438 lacks `BFBC2Beach`, and the other 14 have only two frames.

The rest of the menu work for a fifth era:

- **The selector.** gameModSelector 443 is a two-state toggle. Each frame's `buttonPressed` sends the literal
  `changeGameMod "0"` or `"1"` (0x050E20, 0x050ED4). N eras need a frame per era, or a list control, each sending its
  value.
- **The load-time picks**, with no event (ERA-UI-3 point 4):
  - `setGameMod`'s empty-label fallback;
  - the frame-0 / 1 scripts of 48 / 53 / 58 / 63, hudlayer_shared's sprites and weaponicon's `ammoBackground` /
    `healthIcon` (0x00D154: `== 1` -> `BFBC2Nam`, else `BFBC2`);
  - every `== 1` read of ERA-UI-2.

  Each must map each era to its label, for example through a host extern that returns the active era's label.
- **Sprite 471's `home_btn` offset.** It has cases 1 (+170), 2 and 3 (origin) and an else (origin). Era 4 falls into
  the else; give it a case if its bar differs.
- **The literals.** The three `"BFBC2Nam"` compares (ERA-N-11) decide per era whether it borrows Vietnam's layout.
- **Exits.** main_root's `changeFromNamMod` and the redeem dialogs send `changeGameMod 0`. They are era 1's and stay
  so.
- **Per-era art and colour.** The suffix strings, `*_ANY` pictures (ERA-UI-3 point 5) and any era colour
  (`fillColor` is constant: ERA-N-7) are data.

[data: scratch `bfbc2 apt dump` of DICE's 22 Package main-menu movies plus OPFOR's hudlayer_shared dump, 2026-11-27: the 17 sprites with era frame labels and their placements as listed; gameModSelector's `changeGameMod` literals at 0x050E58 / 0x050E6C / 0x050F0C / 0x050F20; main_root 0x0039BC (`changeFromNamMod`), 0x003DCC, 0x003E1C]

## 4. Risks

### ERA-N-13: The online and saved paths, and what they get for an era they do not know

Status: **inferred** · Variants: PC

None of these is opened offline (OPFOR never connects to game services). A general design still must not leak an
unknown era into them:

- **The matchmaking attribute `gameMod`** (0x77C994 / 0x77D0C3). Leave it unpatched, so every era other than 1 sends
  `BC2`, a value EA's matchmaking knows. A third tag would query for servers that cannot exist. This replaces
  ERA-SITE-4's `search.*` decision.
- **The sign-in request** 0x920C00 stores the requested mod 1 and posts presence message 0x25. Only era 1 may reach it
  (the gate cave). Otherwise the presence re-apply 0x94EB3F would later switch the client to Vietnam.
- **The launcher pipe** produces only 0 or 1 (ERA-N-6).
- **The server's strings** (+0x118 published as `gameMod`, +0xB8 the level-list filter). A host server for an era
  sets them to the row's tag, or its `LevelListQuery` drops the era's levels (ERA-RISK-3; the writers are GM0-rest's).
- **Saved state.** The exe saves no era (ERA-MOD-2). The tracker pages k + 2 are saved as extra `GstMenuTracker`
  entries in GameSettings.bin. DICE's own exe on the same profile ignores pages above 3, which is harmless. Page 3 is
  never used for an era, because DICE's exe would count that era's items as new Store items.
- **Licences and stats.** `playerHasLicenseToNam` stays false outside era 1 (ERA-SITE-6). No stats category, award
  tree or persistence template follows the era (GMD-PERS-1).

[inferred: from ERA-N-2, ERA-N-6, ERA-NAT-4, ERA-MM-1, ERA-RISK-3 and SET-BIN-1]
[exe: BFBC2Game.exe 0x77C994 - 0x77CA77, 0x77D0C3 - 0x77D194; 0x920C00 - 0x920C61; 0x94EB2E - 0x94EB3F]

## Pending playtests (in-game tests are paused)

The list (each rule with the playtest that would prove it) moved to `bfo:docs/tasks/spec-task-notes.md` in round KB2.

## Open questions

- Whether battlefield-opfor's launcher uses `\\.\pipe\rome_snowroller` (only OPFOR's source was searched).
- Who, if anyone, fills the Store page (3) on PC: no code inserts it; the loader only reads it back.
- The page-0 source 0x9484A0 (on [UI +0x82B24] +0x1C20) as the award list is inferred from the enum order.
- GM1's open questions stand: 0x9381F0's Vietnam map keys, the server strings' writers, a missing asset's handle.
