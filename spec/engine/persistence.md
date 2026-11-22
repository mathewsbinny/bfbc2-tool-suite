# Persistence: stats, ranks, unlocks and licences offline

How BFBC2 decides what a player has unlocked, where its stats live, and what Project OPFOR BFBC2's game module does
so that this works with no EA service: the join's stats object, the unlock computation (kit score buckets and the
rank table), licence items, the stats commit at round end, and the WW2 / Vietnam licence rulings. The menus' stats
screens offline are GAME-AWARD-1; the menu runtime is `engine/menus.md`.

**Corpus.** Static reverse engineering of `BFBC2Game.exe` (PC retail, build 795745) recorded in Project OPFOR
BFBC2's game module (`opfor:src/dll/persistence.cpp`, `stats_screens.cpp`), which hooks these functions and has run
in game since October 2026 (before the in-game pause of 2026-11-20). The module's C++ rule tests are `int main()`
programs that `trace.py` cannot cite, so the rules cite the module's code. Nothing new was run in game this round.

## 1. The game's own path

### PERS-JOIN-1: The join makes a stats object and asks EA for its values

Status: **observed** · Variants: PC

When a player joins, the server (FUN_00865D70) creates the player's stats object (FUN_0085F7B0: every known stat at
its default; constructor 0x7E9960, 0x140F8 bytes), asks EA's stats service for the real values, and runs
`Unlocks::compute(player, stats = null)` (0x8639D0), which grants the default items only. Offline the service never
answers, so a stock game offline has default kits and nothing unlocked.

[exe: FUN_00865D70 join; FUN_0085F7B0 stats defaults; 0x7E9960 stats constructor; 0x8639D0 Unlocks::compute]
[code: opfor:src/dll/persistence.cpp:1-13] [code: opfor:src/dll/persistence.cpp:47-64]

### PERS-UNLK-1: Unlocks come from kit score buckets and the rank table

Status: **observed** · Variants: PC

When the stats arrive (handler FUN_0086BEB0) the game copies them into the stats object and runs
`Unlocks::compute(player, stats)`: 11 kit-score buckets (`sc_assault` ...; manager +0x12F8) and the rank table
(manager +0x1324 / +0x1328, stride 0x3C: +0x24 the score threshold, +0x38 the item; an item's id at +0x40, its bit at
+0x4C; a rank's score is the sum of the kit, award and vehicle scores) set the player's unlock bits (state +0x94 bit
words, +0x9C count, +0xA4 changed). The unlock bits follow the order of the Unlock entries in
`Persistence/Weapons.dbx` (rank reward u12 is bit 7, checked against the rank table at run time).

[exe: FUN_0086BEB0 stats handler; Unlocks manager +0x12F8 buckets, +0x1324 / +0x1328 rank table]
[code: opfor:src/dll/persistence.cpp:60-64] [code: opfor:src/dll/persistence.cpp:248-265]

### PERS-LIC-1: Licence items are added after the computation

Status: **observed** · Variants: PC

`Unlocks::applyLicenses` (0x8527D0) then walks the unlock table's categories and items: an item is unlocked when the
player holds a licence named in its `Licenses` or `AdditionalLicenses` (space-separated tokens: SPECACT, the kit
shortcut packs, Limited Edition, Veteran, Vietnam). Licences are account items from EA; offline nobody holds one.

[exe: 0x8527D0 Unlocks::applyLicenses]
[code: opfor:src/dll/persistence.cpp:345-356]

### PERS-SAVE-1: The round's stats are committed through the stats listener

Status: **observed** · Variants: PC

Stats finalize (FUN_0084CB90, at round end and other points) hands every changed stats object (+0x140F0) to the stats
listener (`*(0x01566B68)`, vtable slot +0x20), whose job online is the upload to EA.

[exe: FUN_0084CB90 stats finalize; listener global 0x01566B68 (server singleton 0x01566B18 + 0x50), slot +0x20]
[code: opfor:src/dll/persistence.cpp:120-130]

## 2. What OPFOR's module does

### PERS-OPF-1: Local stats in game\stats.txt drive the game's own computation

Status: **implemented** · Variants: PC

OPFOR's module fills the joining player's stats object from `game\stats.txt` (one `name = value` per line, the parser
the stats screens share) and runs the game's `Unlocks::compute` with it, so the game's own rules decide what is
unlocked; it replaces the listener's commit slot so that the local player's committed stats are written back to
`stats.txt` before the original runs. `[unlocks] all_progression=1` in `profile.ini` runs the computation once more
with maximum kit scores (every rank and kit-score reward, never a licence item except PERS-OPF-2); `[redeemed]` holds
the five public promo codes' weapons.

[code: opfor:src/dll/persistence.cpp:104-119] [code: opfor:src/dll/persistence.cpp:210-215]
[code: opfor:src/dll/persistence.cpp:15-20]

### PERS-OPF-2: Licence items: none on stock levels, Vietnam's on Vietnam, all on WW2 levels

Status: **implemented** · Variants: PC

After the game's `applyLicenses`, OPFOR grants licence items per hosted level: on a Vietnam level the items of the
`nam` token; on a WW2 (Pacific War) level every licence item (the user's ruling of 2026-11-06: the WW2 kits reuse
BFBC2's unlock entries - the WW2 Garand is the M1's, Veteran; the M1911 and Thompson Limited Edition - which were
otherwise locked); on BFBC2's stock levels none. The flag is set per hosted level, so a stock level after a WW2 one
grants nothing extra.

[code: opfor:src/dll/persistence.cpp:348-357] [code: opfor:src/dll/persistence.cpp:401-410]

### PERS-OPF-3: Kills of bots score nothing offline

Status: **observed** · Variants: PC

Scoring and stats run only with 4 or more clients connected (`call 0x53F430; cmp eax, 4; jae`) in five places
(0x865F36, 0x86FD12, 0x842E6A, 0x842E9A, 0x83B591); offline there is one client, so AI kills score nothing. OPFOR's
module patches the five tests. See AI-NEED-5.

[exe: 0x53F430 client count; the five tests as listed]
[code: opfor:src/dll/patches.cpp:115-123]

## 3. Planned: the Pacific War career (the user's decisions, 2026-11-15)

Not implemented; recorded so the award work starts from them (OPFOR's `research/WW2_AWARDS_COMPARISON_2026-11-15.md`,
option C): one award engine in the game module running BF1943's rules on Pacific War maps; a separate Pacific War
career file (not `stats.txt`); BF1943's 50 ranks and point scale there; BF1943's 12 hidden Xbox achievement trackers
as trophies; a stars system of OPFOR's own in BFBC2's style for the WW2 weapons and vehicles; awards that need content
not yet in shown and earnable once it lands; all gated on the WW2 install.

## Open questions

- **The rank table's sources** (PERS-UNLK-1): which stats the award and vehicle scores sum, from the executable or the
  `Persistence/*.dbx` partitions; a spec of `Persistence/Weapons.dbx` and the award partitions as DBX classes.
- **The award engine** (section 3): BF1943's award rules decoded from its data and executable.
- In game since this round: nothing; the module's behaviour was last played before 2026-11-20.
