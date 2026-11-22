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

## 4. The persistence data (DICE's classes)

Counted on 2026-11-22 (round SPG2) in DICE's own start-up bundle: Package `async/startup-00.fbrb.opfor-orig` (the
patched set, as DICE shipped it; OPFOR's rewrite of the live file changed none of these classes) and Dist
`async/startup-00.fbrb` (before Vietnam).

### PERS-DATA-1: The persistence and award partitions

Status: **observed** · Variants: PC

| Partition | Classes (patched Package set) |
|---|---|
| `Persistence/MP_Awards` | AwardDataTree 1, AwardData 451, CriteriaData 440, AwardStarData 20, CriteriaStarData 20, AwardStarNameData 5, CriteriaStarCategoryData 5 |
| `Persistence/MP_Template` | PersistenceData 1, PersistentValueTemplateData 2,066, UnlockToken 245, UnlockTokenData 1, PersistentDerivedValueTemplateData 2, PersistentPartition 2, EntitlementQuery 5, EntitlementAsset 1 |
| `Persistence/Weapons` | PersistenceWeaponData 1, PersistentWeapon 84 |
| `Persistence/Specializations` | PersistenceWeaponData 1, PersistentSpecialization 41 |
| `Persistence/SP_Weapons`, `SP_Template` | PersistentWeapon 30; PersistentValueTemplateData 324 |
| `Persistence/Ons_Awards`, `Ons_Template` | AwardData 9, CriteriaData 8; PersistentValueTemplateData 170 |
| `Persistence/RichPresence`, `MP_Achievements` | 54 instances; AchievementsDataAsset 1 |
| `Awards/PointSystemParams` | PointSystemParamsAsset 1, DogTagData 3 |
| `Awards/WeaponCategories`, `VehicleCategories`, `LevelCategories` ... | StatsCategoryData 107, 47, 25 ... (the award criteria's categories) |

Dist (before Vietnam) has 384 AwardData, 1,756 value templates, 70 weapons, 40 specializations, plus `SP_Awards`,
`SP_Achievements`, `SP_Specializations` and 15 `Awards/*` category trees.

[data: round SPG2's reads of DICE's two start-up bundles, 2026-11-22: class counts per partition as listed]
[doc: opfor:research/WW2_AWARDS_COMPARISON_2026-11-15.md:170-179]

### PERS-DATA-2: PersistenceData roots the career; value templates say how a stat is written

Status: **observed** · Variants: PC

`MP_Template`'s `PersistenceData` lists `Partitions`, `Defined`, `Derived`, `Final` (all 2,066 value templates),
`Awards` (`persistence/mp_awards`), `Unlocks` (Weapons and Specializations), `UnlockTokens`, `Entitlements`;
`Consumables` and `Collectibles` false. A `PersistentValueTemplateData` (`Name`, `DefaultValue`, `ValueType`,
`WritePolicy`, flags) is High (1,400), Increment (651) or Set (15), written at a Checkpoint (1,426), at Disconnect (638)
or at Finalize (2). `UnlockToken` scores run 30 - 62,100.

[data: round SPG2's read of the patched MP_Template, 2026-11-22: the fields and counts listed]

### PERS-DATA-3: Unlock items take their bit by order

Status: **observed** · Variants: PC

A `PersistentWeapon` (`Name`, `WeaponType`, `Cost`, `Id`, `BitIndex`, `Dependency`, `Licenses`, `AdditionalLicenses`,
`Slot` ...) has `BitIndex` -1 on all 84 of the patched set and no `Dependency`: the unlock bit is the entry's order
(PERS-UNLK-1). 59 are `Unlock`, 25 `Normal`; `Slot` 0 on 57; `Licenses` `nam` on 14 (Vietnam's additions) and
`ka` / `ke` / `km` / `kr` on 4; `AdditionalLicenses` hold the tokens `sa`, `se`, `sr`, `sm`, `le`, `vet`, `gst`, matched
as whole words by the module. `PersistentSpecialization` (41) adds `SpecializationType`, `Modifier`, `KitCategory`.

[data: round SPG2's read of the patched and Dist Weapons / Specializations, 2026-11-22: the values listed]
[code: opfor:src/dll/persistence_rules.h:10-21]

### PERS-RANK-1: BFBC2's 50 ranks are inline in PointSystemParams

Status: **observed** · Variants: PC

`PointSystemParamsAsset` holds `Ranks` inline: 50 entries (`RankName`, `PointsNeeded`, `Unlock`, image, sound and
voice-over fields), strictly ascending from 6,500 to 5,400,000 points; ranks 1 - 22 carry an unlock, 23 - 50 none.
Three `DogTagData` at ranks 0, 21 and 41. Score fields are structs (`UIString`, `Score`, `Bucket`): `KillScore` 50 in
`ScoringBucket_General`. In the executable the rank table (0x92CF70) has 0x3C-byte entries with the points at +0x24;
entry 0 is the start (rank -1), the points for rank r are entry r + 1; a fresh offline profile is rank -1. The same
values in Dist and Package.

[data: round SPG2's read of PointSystemParams in both start-up bundles, 2026-11-22]
[code: opfor:src/dll/stats_model.h:120-136]
[code: opfor:src/dll/stats_screens.cpp:172-179]

### PERS-AWD-1: BFBC2's awards: stars, insignias, pins and achievements

Status: **observed** · Variants: PC

The patched `MP_Awards` has 451 `AwardData`: stars 225 (once) and 75 platinum stars (`Limited`, `MaxRepetitions` 9),
insignias 54 (once), pins 48 (unlimited), achievements 49 (42 hidden). 402 score into the Award bucket, 49 the Bonus
bucket (scores 5,000 x 120, 10,000 x 84, 1,000 x 75, 500 x 75, 100 x 32, 200 x 16, 0 x 49); 417 have criteria, 238
dependencies (meta awards); licences `nam` on 9, each SPECACT kit's on 4. Dist (before Vietnam): 244 stars, 54
insignias, 48 pins, 38 achievements. `Ons_Awards`: 9 achievements (8 hidden, licence `ons`, one criterion each:
complete a level at a difficulty).

[data: round SPG2's read of MP_Awards / Ons_Awards in both start-up bundles, 2026-11-22: the splits listed]
[doc: opfor:research/WW2_AWARDS_COMPARISON_2026-11-15.md:189-211]

### PERS-AWD-2: Criteria count stat events over a scope

Status: **observed** · Variants: PC

A `CriteriaData` (`Value`, `Measuring`, `ParamX`, `ParamY`, `CriteriaType`, scale and visibility flags) counts a stat
event over a scope: all time (`GLOBAL_AllTimeTotal`, 384 of 440), in a round (`IAR_InARound`, 50) or in a life
(`IAS_InASpawn`, 6); 48 distinct `Measuring` events, led by `KillWith_X_As_Y` (326) and `KillOf_X_In_Y` (32); `ParamX` /
`ParamY` name `StatsCategoryData` of the `Awards/*` trees. `ReadableByServer` is false on all 440. The stat keys the
executable builds are `c_` + parts + `_g` (0x843310: `kw` kills with, `ki` kills in, `si` seconds in, `col`
collected); a star's proficiency (0x924070) comes from the `br` / `si` / `go` / `pl` award counters (1 none, 2 bronze,
3 silver, 3 + n gold, 12 + n platinum).

[data: round SPG2's read of the patched MP_Awards, 2026-11-22: 440 criteria as listed]
[exe: 0x843310 the stat-name builder; 0x924070 the proficiency level]
[code: opfor:src/dll/stats_model.h:55-81]

### PERS-AWD-3: BFBC2's award data descends from BF1943's

Status: **observed** · Variants: PC, BF1943

Both games use `AwardDataTree`, `AwardData`, `CriteriaData`, `StatsCategoryData`, `DogTagData`, and
`Awards/PointSystemParams` has the same partition and instance GUIDs in both (the three dog tags too). BFBC2 adds
`AwardData.License`, `UIArgument`, `MaxRepetitions`, the star classes, the criteria's readability flags and the score
structs (BF1943 has flat `StringFor*` / `PointsFor*`); BF1943's ranks are separate `RankLevelData` instances (60 in the
file, 50 referenced). BFBC2's executable has no `AwardGroup_PatchShield`, no `AwardGroup_Trophy`, no
`StatEvent_CaptureCapturePoint` (BFBC2's is `FlagCaptured`) and no `StatEvent_VehicleKillAssist_X`.

[doc: opfor:research/WW2_AWARDS_COMPARISON_2026-11-15.md:270-290]
[data: round SPG2's read: BFBC2's PointSystemParams has 4 instances, no rank instances; BF1943's table (awards.json) 92 flat PointsFor* / StringFor* keys]

### PERS-AWD-4: The award tree is loaded at every level start and driven by the per-player stat events

Status: **observed** · Variants: PC

The server's scoring object (0x1454 bytes, constructor 0x86A660) is made at every level start, offline too
(allocation at 0x86B4B4, no network or player-count test on the way). Its constructor loads the award tree (0x86A0F0):
`Persistence/SP_Awards` when its single-player flag (+0xC, the level start's +0x1E) is set, else
`Persistence/MP_Awards`, and 0x869360 walks the tree: every award is keyed by its id in a hash table, then each
criterion is linked both ways to its award and its stat (0x8669B0: the criterion's stat key and flag). Nothing scores
an award at load. What moves an award is the per-player stat events: 0x8584D0 looks up the listener registered for
(player, event) and calls it (vtable +4) with the two stat categories, the value and the scope. So the evaluator runs
offline as soon as stat events are sent, and those are sent only for human players and only with the server's scoring
flag (BOT-SCO-1): without OPFOR's scoring patch (BOT-SCO-3) no award moves offline, with it a human's events reach the
awards. The listener classes (one per `CriteriaType`: in a round, in a spawn, all-time) and where a finished award is
written were not followed.

[exe: BFBC2Game.exe 0x86B4B4 (`push 0x1454`, then 0x86A660(server +0x2C, +0xC, byte +0x1E)); 0x86A660 (+0xC the flag; 0x86A796 call 0x86A0F0); 0x86A0F0 ("Persistence/MP_Awards" 0x1433BFC, "Persistence/SP_Awards" 0x1433C14 by the flag; 0x869360); 0x869360 (0x863E70 collects the awards, hash by id, 0x8669B0 per criterion: criterion +0xC key, +0x14 flag); 0x8584D0 (listener lookup 0x844410 by (player, event), call vtable +4); callers of 0x8584D0: 0x85A030, 0x8684F0, 0x86DC90]

### PERS-SCORE-1: With the scoring patch, bots' deaths are scored in the round

Status: **observed** · Variants: PC

Since the offline-scoring patch (MOD-PATCH-1), the player-killed handler 0x86DC90 tests the server's scoring flag
(`cmp byte [ebp+0x2A], 0` at 0x86DD97) and with it set runs the kill scoring (0x86C070, through the stats add
0x7D27A0) and its award (0x86BE20, `KillScore`): bots' deaths are scored in the round. The career save still writes
score 0 (no kills or score in `stats.txt` over 180 saves). Round AIS-rest settled the contradiction with AI-NEED-5
statically: the handler returns only for a null victim, a bot is a player, and the offline block was the scoring flag;
so with the patch a human's kill of a bot scores, while the bot's own kills, its deaths (no scoring record) and any
award to a bot are skipped by the AI-flag tests at 0x86C12D, 0x865D7F and 0x86A7EE (BOT-SCO-1 to BOT-SCO-3).
PERS-OPF-3's title holds only without the patch. The kill probe in a playtest still has to confirm it (`bot_ai.md`,
play-test plan step 6).

[exe: 0x86DD97 the scoring-flag test in 0x86DC90; 0x7D27A0 the stats add; 0x86DCAE (null victim), 0x86C12D (AI killer), 0x865D7F (no record for an AI player), 0x86A7EE (no award to an AI player)]
[doc: opfor:research/WW2_AWARDS_COMPARISON_2026-11-15.md:371-377]

## 5. BF1943's awards for the Pacific War career

### PERS-WW2-1: BF1943's award set as OPFOR's table holds it

Status: **verified** · Variants: BF1943

OPFOR's award table (`ww2_awards.py`, from BF1943's `persistence/mp_awards`) sorts BF1943's 60 awards by what the
player sees: 24 postcards (visible, once), 24 stamps (visible, unlimited), 12 trophies (hidden: `ac2` - `ac8`,
`ac10` - `ac12`, `p6`, `pq3`; there is no `ac9`; 2 are patch shields). Their 79 criteria are all-time (49) or in a
round (30), over 15 stat events, two of which BFBC2 lacks (`CaptureCapturePoint` 5, `VehicleKillAssist_X` 1). Stamps
score 10 - 50 points (20 on 17), postcards and trophies 0. The awards grid is 8 x 6: rows 0 - 2 postcards, 3 - 5
stamps.

[test: opfor:src/tools/tests/test_ww2_awards.py::test_the_table_is_bf1943s_set]
[code: opfor:src/tools/ww2/ww2_awards.py:154-211]

### PERS-WW2-2: BF1943's ranks, point scale and dog tags

Status: **verified** · Variants: BF1943

BF1943 has 50 ranks from 50 to 160,000 points, strictly ascending (BFBC2: 6,500 - 5,400,000); a kill is 10 points
(headshot +5, melee 3), a capture 30 (assist 10), an objective destroyed 30, a kill assist 3 (squad 5), repairs 10 /
15, a team kill -15, a suicide -10 (BFBC2's kill: 50). Its dog tags come at ranks 0, 10 and 20 (BFBC2's 0, 21, 41).
These are the Pacific War career's scale (the user's decision of 2026-11-15).

[test: opfor:src/tools/tests/test_ww2_awards.py::test_the_table_is_bf1943s_set]
[code: opfor:src/tools/ww2/ww2_awards.py:212-225]
[doc: opfor:research/WW2_AWARDS_COMPARISON_2026-11-15.md:129-148]

### PERS-WW2-3: BF1943's categories map onto OPFOR's weapons and kits

Status: **verified** · Variants: BF1943

Every BF1943 category member an award criterion uses must map onto a name OPFOR's career reports, or the table's build
refuses; OPFOR weapons with no BF1943 role (the M1897, the flamethrowers) count only for generic awards; BF1943's three
kits map onto four (Assault -> Assault, Support -> Demolition and Support, Specialist -> Recon); the tutorial level is
left out (the ruling of 2026-11-15).

[test: opfor:src/tools/tests/test_ww2_awards.py::test_members_map_onto_opfor_names_and_leave_the_tutorial_out]
[code: opfor:src/tools/ww2/ww2_awards.py:50-97]

### PERS-WW2-4: The award stings on Pacific levels

Status: **implemented** · Variants: PC, BF1943

BF1943 plays a sting per award kind (rank-up `USSting02A`, dog tag `USSting02B`, postcard `JPSting02A`, stamp
`JPSting02B`; the eleven `op*` postcards name `USSting02A`). On a Pacific level the module maps the award sound BFBC2
plays (by name, 0xF909C0) to BF1943's and tells a rank-up apart at the HUD's rank call (MOD-MODE-1). Never heard in
game yet.

[code: opfor:src/dll/gamemode_rules.h:134-139]
[doc: opfor:research/WW2_AWARDS_COMPARISON_2026-11-15.md:62-63]

## Open questions

- **The rank table's sources** (PERS-UNLK-1): which stats the award and vehicle scores sum.
- **BFBC2's award evaluator**: loaded and wired at every level start, driven by the stat events (PERS-AWD-4); the
  listener classes per criteria type and where a finished award is written are open, and so is why the career save
  drops the round's kills (PERS-SCORE-1).
- **Bots' deaths scored or not**: settled statically by round AIS-rest (PERS-SCORE-1 holds with the patch, BOT-SCO-3);
  the kill-probe playtest confirms it.
- The unlock bit space: whether the specializations continue the weapons' bits (PERS-DATA-3).
- The 49 achievement-group awards against `MP_Achievements`; whether BFBC2's awards grid can show BF1943's 8 x 6.
- **The award engine** (section 3): not built; the table (PERS-WW2-1 - PERS-WW2-3) is its input. BF1943 semantics the
  data does not hold (squad-wide awards, vehicle kill assists, best squad offline with AI squads) are open.
- In game since this round: nothing; the module's behaviour was last played before 2026-11-20.
