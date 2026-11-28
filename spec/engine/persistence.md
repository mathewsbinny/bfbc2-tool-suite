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

When a player joins, the server (0x865D70) creates the player's stats object (0x85F7B0: every known stat at
its default; constructor 0x7E9960, 0x140F8 bytes), asks EA's stats service for the real values, and runs
`Unlocks::compute(player, stats = null)` (0x8639D0), which grants the default items only. Offline the service never
answers, so a stock game offline has default kits and nothing unlocked.

[exe: 0x865D70 join; 0x85F7B0 stats defaults; 0x7E9960 stats constructor; 0x8639D0 Unlocks::compute]
[code: opfor:src/dll/persistence.cpp:1-13] [code: opfor:src/dll/persistence.cpp:47-64]

### PERS-UNLK-1: Unlocks come from kit score buckets and the rank table

Status: **observed** · Variants: PC

When the stats arrive (handler 0x86BEB0) the game copies them into the stats object and runs
`Unlocks::compute(player, stats)`: 11 kit-score buckets (`sc_assault` ...; manager +0x12F8) and the rank table
(manager +0x1324 / +0x1328, stride 0x3C: +0x24 the score threshold, +0x38 the item; an item's id at +0x40, its bit at
+0x4C; a rank's score is the sum of the kit, award and vehicle scores) set the player's unlock bits (state +0x94 bit
words, +0x9C count, +0xA4 changed). The unlock bits follow the order of the Unlock entries in
`Persistence/Weapons.dbx` (rank reward u12 is bit 7, checked against the rank table at run time).

[exe: 0x86BEB0 stats handler; Unlocks manager +0x12F8 buckets, +0x1324 / +0x1328 rank table]
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

Stats finalize (0x84CB90, at round end and other points) hands every changed stats object (+0x140F0) to the stats
listener (`*(0x01566B68)`, vtable slot +0x20), whose job online is the upload to EA.

[exe: 0x84CB90 stats finalize; listener global 0x01566B68 (server singleton 0x01566B18 + 0x50), slot +0x20]
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

Not implemented. The user's decisions (one award engine running BF1943's rules on Pacific War maps, a separate career
file, BF1943's 50 ranks, its 12 hidden Xbox achievements as trophies, OPFOR's own stars, gated on the WW2 install)
are in battlefield-opfor's knowledge base since round KB2: `bfo:docs/decisions/pacific-war-career.md`.

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
[doc: bfo:docs/research/opfor/WW2_AWARDS_COMPARISON_2026-11-15.md:179-188]

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
[doc: bfo:docs/research/opfor/WW2_AWARDS_COMPARISON_2026-11-15.md:198-220]

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

[doc: bfo:docs/research/opfor/WW2_AWARDS_COMPARISON_2026-11-15.md:279-299]
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
awards. The listener and where a finished award is written are PERS-AWD-5 (round SPG2-rest4: one listener object per
player, not one class per `CriteriaType`).

[exe: BFBC2Game.exe 0x86B4B4 (`push 0x1454`, then 0x86A660(server +0x2C, +0xC, byte +0x1E)); 0x86A660 (+0xC the flag; 0x86A796 call 0x86A0F0); 0x86A0F0 ("Persistence/MP_Awards" 0x1433BFC, "Persistence/SP_Awards" 0x1433C14 by the flag; 0x869360); 0x869360 (0x863E70 collects the awards, hash by id, 0x8669B0 per criterion: criterion +0xC key, +0x14 flag); 0x8584D0 (listener lookup 0x844410 by (player, event), call vtable +4); callers of 0x8584D0: 0x85A030, 0x8684F0, 0x86DC90]

### PERS-AWD-5: One award listener per player; a finished award is written into a stats object as two values

Status: **observed** · Variants: PC

The join (0x865D70) registers one listener per player with the server's stat-event dispatcher (0x85AED0, into its
map at +0x20): a 0xBD4-byte object (constructor 0x8531B0, vtable 0x1433780) holding the player's stats object
(+0xBD0, the one the join made: PERS-JOIN-1), the join's record object (+0x08) and 104 lists of 0x1C bytes from
+0x74. Its event handler (vtable +4, 0x85F530, the call PERS-AWD-4's 0x8584D0 makes) walks the criteria listening to
the event: a criterion whose categories match (0x83C170) adds the value (its vtable +0x0C), writes its progress into
the stats object (+0x14) and reports whether it is met (+0x08). For each met criterion the handler tries its award
(0x852CB0): an award not yet done is checked against all its criteria and granted when they are met. A grant:

- writes two stats with the plain set (0x7D26C0, existing names only) into the award object's stats object (+0x18;
  presumably the player's, which the listener hands to its criteria through +0x14): `<Code>_00`, the times awarded,
  and `<Code>_01`, the grant time in days since 1 January 2008 (a 64-bit time less the one made from (2008, 1, 1) at
  0x852FFB, over 86,400);
- by the data's `Repeat` (`AwardType`): `OnceGlobally` (0) and `OncePerRound` (1) mark the award done; `Unlimited`
  (2) resets its criteria; `Limited` (3) marks it done once the count reaches `MaxRepetitions`, else resets them;
- then tries the award object's dependents (its list at +0x3C, 0x18-byte entries: those whose required count the new
  count reaches, or that carry a flag) the same way.

For the event scopes 2 and 0x51 the handler then resets criteria (0x85F3E0 with 1), presumably at a life's or a
round's end. So the game writes a finished award to no file: it is two stats in a stats object, which the stats
commit (PERS-SAVE-1) hands on and OPFOR's commit hook writes to `stats.txt` when their names exist in the object
(PERS-OPF-1).

[exe: BFBC2Game.exe 0x865DE8 (0x85AED0 with the player id, the stats from 0x85F7B0 and the record object); 0x85AED0 (`push 0xBD4`, 0x8531B0, insert 0x854A40 at +0x20); 0x8531B0 (vtable 0x1433780, +0x08, +0xBD0, 0x68 lists from +0x74 at 0x853210 - 0x853227); handler 0x85F530 (0x83C170, criterion vtable +0x0C / +0x14 / +0x08 at 0x85F5EB - 0x85F604; 0x852CB0 at 0x85F671, 0x853240 at 0x85F67F; scopes 2 / 0x51 -> 0x85F3E0 at 0x85F769 - 0x85F777); grant 0x852CB0 ("%s_00" 0x141D918, "%s_01" 0x14333AC with the data's Code +0x90; 0x7D26C0 at 0x85306E and 0x8530B6; 86,400.0 at 0x141DA20; 0x50ED90(2008, 1, 1) into 0x15714C0; Repeat +0x30 through the table 0x853170, MaxRepetitions +0x3C at 0x8530F5; the dependents 0x853124 - 0x85315C); AwardData field descriptors 0x1C32170 - 0x1C322F0 (Code +0x90, Repeat +0x30, MaxRepetitions +0x3C); AwardType 0x1C31D38 - 0x1C31D80]

### PERS-AWD-6: A criterion's CriteriaType decides when its progress is reset

Status: **observed** · Variants: PC

The award listener (PERS-AWD-5) keeps its armed criteria in one map per stat event (the 0x1C-byte maps from +0x74,
the event at criterion +0x0C). When a met criterion's award is granted, 0x853240 takes the criterion off its event's
map and, by its `CriteriaType` (+0x10), moves it from that type's list to the type's parked list: `IAR_InARound` (0)
+0x28 -> +0x4C, `IAD_InADeathStreak` (4) +0x34 -> +0x58, `IAS_InASpawn` (1) +0x40 -> +0x64. `GLOBAL_AllTimeTotal` (2)
and `IfNotTrue` (3) have no lists, so once met they stay off for good. The reset (0x85F3E0 with a type) puts every
parked criterion of that type back into its event's map (0x85F330) and empties the parked list:

- type 0 (in a round): at the listener's round start / end (0x861960, from 0x861EFE and 0x8632B4);
- type 1 (in a life): after the events 2 and 0x51, and at 0x861A4A. The handler's event number is `StatEvent`'s
  (round SPG2-rest6): the kill handler 0x86DC90 sends 0 (`KillOf_X_As_Y`), 1, 2 (`DeathAs_X_By_Y`), 3, 0xB - 0xE
  through 0x8584D0, each the `StatEvent` value of its kill or death, and 0x51 is `StatEvent_Suicide` (the enum's
  0x51st value of 105). The kill handler sends 0x51 in one branch only: when the two players' +0xA8 are equal
  (0x86DECC -> 0x86E262; the same player or the same team, not settled), for the killer when the kill record's byte
  +0x2A is set. So a life's criteria reset on the player's death event and on that event;
- type 4 (in a death streak): never; no call passes 4.

So DICE's award criteria (MP_Awards: 315 all-time, 47 per round, 6 per life; SP_Awards: 30 all-time, 40 per round)
need two resets the game makes offline as online; an `IAD_InADeathStreak` criterion would never reset, and DICE has
none. The criterion objects and their maker are PERS-AWD-8.

[test: core/tests/test_spec_probes.py::persistence]
[data: spec/probes/persistence.py, round SPG2-rest5 2026-11-27: CriteriaType over DICE's MP_Awards (GLOBAL 315, IAR 47, IAS 6) and SP_Awards (GLOBAL 30, IAR 40), Dist/win32/async/startup-00.fbrb]
[exe: BFBC2Game.exe the parking after a grant 0x853240 (called at 0x85F67F; off the event map 0x853270 - 0x8532D1 with 0x8493E0; CriteriaType at criterion +0x10 through the tables 0x853360 / 0x853374: 0 -> +0x28 / +0x4C, 1 -> +0x40 / +0x64, 4 -> +0x34 / +0x58, 2 and 3 none; the append 0x85331E - 0x853355); the reset 0x85F3E0 (the parked lists by the table 0x85F504, each back into its event map by 0x85F330 (lookup 0x844CB0, insert 0x85B610), the type's list by 0x85F518, the parked nodes freed 0x85F4B0 - 0x85F4C6); its callers 0x85F777 (events 2 and 0x51, type 1), 0x86196E (type 0), 0x861A4A (type 1); the listener's vtable 0x1433780 (+0 0x8618B0, +4 0x85F530); CriteriaType's records 0x1C03364]
[exe: BFBC2Game.exe round SPG2-rest6: StatEvent's field descriptors (its record 0x1C03348, the table 0x1C312E8: 105 values, 2 `StatEvent_DeathAs_X_By_Y`, 0x51 `StatEvent_Suicide`); the kill handler 0x86DC90's calls of 0x8584D0 with the pushes 0x86DF3E (0), 0x86DF6E (0xB), 0x86DFA5 (2), 0x86DFD9 (0xD), 0x86E07C (1), 0x86E0D5 (0xC), 0x86E135 (3), 0x86E18E (0xE), 0x86E2D2 (0x51; the branch 0x86DECC - 0x86DEDC, the flag 0x86E2AF)]

### PERS-AWD-8: The award listener makes one criterion object per award criterion, its class by CriteriaType

Status: **verified** · Variants: PC

When the award listener (PERS-AWD-5) is set up (0x866DC0, from 0x86BEB0 and 0x90BC30), it makes a 0x50-byte state
per award (0x863DB0), which copies the award's record and, for each of its criteria, calls the criterion factory
0x8430E0 with the criterion's three words (+0x04, +0x08, +0x0C), its `CriteriaType` (+0x10), its target value
(+0x14, a float) and the award state as the owner. Each criterion is then armed (0x861B90 -> 0x8618D0): unless it is
already met, it goes into the event map of every event its +0x04 object lists (0x85F330; the default list 0x157146C
when +0x04 is empty) and into its type's list. 0x861B90 also makes criteria without an owner (the factory's last
argument 0), and 0x8617B0 / 0x8489A0 call the factory too.

The factory allocates 0x24 bytes and switches on the type: +0x04 / +0x08 / +0x0C the three words, +0x10 the type,
+0x14 the target, +0x18 the progress (0.0), +0x1C the owner, +0x20 a name (empty). The classes:

| CriteriaType | With an owner | Without |
|---|---|---|
| `GLOBAL_AllTimeTotal` (2) | vtable 0x1433058 (built in the factory) | 0x1433250 (0x83BE20) |
| `IAR_InARound` (0) | 0x1433274 (0x83BE90) | 0x1433298 (0x83BF00) |
| `IfNotTrue` (3) | 0x1433304 (0x83C060), either way | |
| `IAS_InASpawn` (1), `IAD_InADeathStreak` (4) | 0x14332BC (0x83BF70) | 0x14332E0 (0x83BFF0) |

Their slots: +0x08 met is progress (slot +0x04) >= the target (0x83B8C0) for a criterion with an owner and always
false (0x83BE70) without one; +0x0C add is progress += the amount (0x83B8F0, all); +0x10 reset is nothing for an
all-time criterion (0x83B910) and progress = 0 for the others (0x83BEE0 / 0x83C0E0, through slot +0x20); +0x14 write
progress writes an all-time criterion's progress into the stats object under its name when the name is not empty
(0x83B920) and nothing for the others (0x83B7D0 / 0x83C0B0). So only all-time progress reaches the career save; a
round's or a life's progress lives in the listener and starts again at 0.

[test: core/tests/test_spec_probes.py::persistence]
[data: spec/probes/persistence.py, round SPG2-rest6 2026-11-27: the factory's type tests and the seven constructors' vtable stores read from the executable; the ownerless met slot's bytes]
[exe: BFBC2Game.exe factory 0x8430E0 (`cmp esi, 2` 0x8430E5, `test esi, esi` 0x843163, `cmp esi, 3` 0x8431BD, 0x24 bytes from 0x420DB0; the stores 0x843142, 0x83BE59, 0x83BECF, 0x83BF39, 0x83C09F, 0x83BFAF, 0x83C029); its callers 0x863E2A (in 0x863DB0, per award, from 0x866E78), 0x861BBA, 0x8617D0, 0x8489C6; the arming 0x8618D0 (slot +0x18 with the stats 0x8618E6, slot +0x08 0x8618EF, the events of +0x04 or 0x157146C into 0x85F330 0x861903 - 0x86192F, the type's list 0x861940 - 0x861956); the slots 0x83B8C0, 0x83BE70, 0x83B8F0, 0x83B910, 0x83BEE0, 0x83C0E0, 0x83B920, 0x83B7D0, 0x83C0B0]

### PERS-AWD-7: Every award code's two stats are Final value templates

Status: **verified** · Variants: PC

A grant writes `<Code>_00` and `<Code>_01` with the plain set, which writes names the stats object already has
(PERS-AWD-5). DICE's templates have them: in `MP_Template` every one of MP_Awards' 380 distinct codes has both
`<Code>_00` and `<Code>_01` among `PersistenceData.Final`'s value templates (1,756 names), and in `SP_Template` every
one of SP_Awards' 70 (317 names); 4 and 3 awards have no `Code` (group headers). So a finished award lands in the
stats object and the commit (PERS-SAVE-1) hands it on; for OPFOR's local career it reaches `stats.txt` (PERS-OPF-1)
when the award evaluator runs (PERS-AWD-4: with the scoring patch). A port that adds awards (the Pacific War career)
must add both templates for each new code, or its grants are dropped.

[test: core/tests/test_spec_probes.py::persistence]
[data: spec/probes/persistence.py, round SPG2-rest5 2026-11-27: DICE's Dist/win32/async/startup-00.fbrb (2010): MP 384 awards, 380 codes, 380 / 380 with _00 / _01 in Final; SP 73 awards, 70 codes, 70 / 70; the game copy's patched Package set (rewritten by OPFOR's install) 446 / 446 of 446 codes]
[exe: BFBC2Game.exe the grant's formats "%s_00" 0x141D918 and "%s_01" 0x14333AC (probe-checked)]

### PERS-SCORE-1: With the scoring patch, bots' deaths are scored in the round

Status: **observed** · Variants: PC

Since the offline-scoring patch (MOD-PATCH-1), the player-killed handler 0x86DC90 tests the server's scoring flag
(`cmp byte [ebp+0x2A], 0` at 0x86DD97) and with it set runs the kill scoring (0x86C070, through the stats add
0x7D27A0) and its award (0x86BE20, `KillScore`): bots' deaths are scored in the round. The career save still writes
score 0 (no kills or score in `stats.txt` over 180 saves): the scoring writes through the record's stats pointer,
which offline is never set (PERS-SCORE-2, round SPG2-rest4). Round AIS-rest settled the contradiction with AI-NEED-5
statically: the handler returns only for a null victim, a bot is a player, and the offline block was the scoring flag;
so with the patch a human's kill of a bot scores, while the bot's own kills, its deaths (no scoring record) and any
award to a bot are skipped by the AI-flag tests at 0x86C12D, 0x865D7F and 0x86A7EE (BOT-SCO-1 to BOT-SCO-3).
PERS-OPF-3's title holds only without the patch. The kill probe in a playtest still has to confirm it (`bot_ai.md`,
play-test plan step 6).

[exe: 0x86DD97 the scoring-flag test in 0x86DC90; 0x7D27A0 the stats add; 0x86DCAE (null victim), 0x86C12D (AI killer), 0x865D7F (no record for an AI player), 0x86A7EE (no award to an AI player)]
[doc: bfo:docs/research/opfor/WW2_AWARDS_COMPARISON_2026-11-15.md:380-386]

### PERS-SCORE-2: The round's scoring writes through the scoring record's stats pointer, which only EA's answer sets

Status: **observed** · Variants: PC

Every stat the round's scoring adds goes through the scoring object's per-player record (0x570 bytes, constructor
0x862A90 at the join, 0x8634E0) and its stats pointer +0x24:

- `deaths` +1 for the victim (0x86C0B5 - 0x86C0C2), `teamkills` +1 (0x86C175 - 0x86C187) and `kills` +1 for the
  killer (0x86D46E - 0x86D486), each skipped when +0x24 is 0;
- the score itself (0x86A7E0, under every score award 0x86BE20): the points go into the record's running score
  (+0x18) and, under the category's stat name (the `sc_*` buckets, presumably), into the stats only when +0x24 is set
  (0x86A9BE - 0x86A9D1).

The constructor sets +0x24 to 0, and the one function that sets it (0x866240, which also feeds the record the stats'
`score` and `veteran`) is called only from the stats-arrived handler 0x86BEB0, when EA's stats service answers
(PERS-JOIN-1, PERS-UNLK-1). Offline the service never answers, and OPFOR's module fills the service's stats object
from `stats.txt` and runs `Unlocks::compute` (PERS-OPF-1) without calling either, so the record's pointer stays 0:
with the scoring patch the round scores in the record (its running score) and sends the awards' stat events, but adds
nothing to the stats object the commit hands on. That is why the career save keeps no kills, deaths or round score
(PERS-SCORE-1): the dropped values were never in the object. The awards' listener is not affected (it holds the join's
stats object directly: PERS-AWD-5). The `score` OPFOR's save log prints is DICE's derived template (`Formula`
`sc_general+sc_team+sc_bonus+sc_squad+...`, PersistentDerivedValueTemplateData in `MP_Template`), so it says 0 even
when `sc_*` values are saved.

The fix that follows (OPFOR's module, not done by this round): after filling the local player's stats, attach them
to the scoring record the way the game does, by 0x866240(player, stats, flag) on the scoring object (the server's
+0x24), or by running the stats-arrived handler itself. Until a playtest with the kill probe confirms it, this stays
observed.

[exe: BFBC2Game.exe record constructor 0x862A90 (+0x24 = 0 at 0x862ABF; 0x570 bytes from 0x8634E0 at 0x863560); 0x866240 (`mov [eax+0x24], esi` at 0x86627B; "score" 0x142B144 and "veteran" 0x1433AD8 read from the stats); its one caller 0x86BF30 in 0x86BEB0; the uses of +0x24: 0x86C0AC, 0x86C175, 0x86D46E, 0x86A9BE; 0x7D27A0 adds only to an existing name (0x7D0C70, a string-keyed map at stats +0x98)]
[code: opfor:src/dll/persistence.cpp:269-277]
[data: game copy's stats.txt and opfor.log, 2026-11-23: saves after the scoring patch name no kills, deaths or score (the file holds two sc_* values whose origin the log does not show); MP_Template (startup-00): kills, deaths, teamkills Increment / Checkpoint, score a derived template, all in PersistenceData.Final (1,756)]

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
[doc: bfo:docs/research/opfor/WW2_AWARDS_COMPARISON_2026-11-15.md:138-157]

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
[doc: bfo:docs/research/opfor/WW2_AWARDS_COMPARISON_2026-11-15.md:71-72]

## Open questions

- **The rank table's sources** (PERS-UNLK-1): which stats the award and vehicle scores sum.
- **BFBC2's award evaluator**: loaded and wired at every level start, driven by the stat events (PERS-AWD-4), one
  listener per player writing `<Code>_00` / `_01` into a stats object (PERS-AWD-5), whose names DICE's `Final`
  templates hold (PERS-AWD-7); resets by `CriteriaType` (PERS-AWD-6), the event 0x51 `StatEvent_Suicide`; the
  criterion objects made per award by the listener's set-up, their class by `CriteriaType`, only all-time progress
  written (PERS-AWD-8, round SPG2-rest6). Open: whether the players' +0xA8 the kill handler compares before sending
  0x51 is the player or the team (a playtest: a suicide and a team kill), what the criteria's three words (+0x04 /
  +0x08 / +0x0C) hold, what 0x8617B0 / 0x8489A0's criteria are for, and a save with an award (playtest).
- **The career save's kills** (PERS-SCORE-2): the scoring record's stats pointer is never set offline; OPFOR's module
  should attach the filled stats with 0x866240. Then a playtest: the kill probe and a save with kills and deaths.
- **Bots' deaths scored or not**: settled statically by round AIS-rest (PERS-SCORE-1 holds with the patch, BOT-SCO-3);
  the kill-probe playtest confirms it.
- The unlock bit space: whether the specializations continue the weapons' bits (PERS-DATA-3).
- The 49 achievement-group awards against `MP_Achievements`; whether BFBC2's awards grid can show BF1943's 8 x 6.
- **The award engine** (section 3): not built; the table (PERS-WW2-1 - PERS-WW2-3) is its input. BF1943 semantics the
  data does not hold (squad-wide awards, vehicle kill assists, best squad offline with AI squads) are open.
- In game since this round: nothing; the module's behaviour was last played before 2026-11-20.
