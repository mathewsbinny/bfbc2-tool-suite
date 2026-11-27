# Game modes and eras: how new ones are authored

BFBC2 has two meanings of "mode", and this file covers both:

- a **game mode** inside a game (Conquest, Rush, Squad Rush, Squad Deathmatch, OPFOR's Onslaught and Air
  Superiority): a `GameModeData` partition, the level descriptions that name it, and the level's mode layer
  (`engine/gameplay.md`, GAME-MODE-1 - GAME-LAYER-5);
- an **era**, the engine's *game mod*: BFBC2 (0) or Vietnam (1), held by the UI manager, with its own menus, music,
  HUD configuration, icon set, unlock tree, weapon presentation and licence checks. Project OPFOR BFBC2's further
  eras (the WW2 / BF1943 "Pacific War", later BFBC1) ride on these two values today (MOD-MODE-1).

For each field and each value it says what reads it in `BFBC2Game.exe` (PC build 795745, image base 0x400000), what
DICE's data holds, which parts are pure data, and what needs code. Sections 4 and 5 turn this into a design for
authoring a new mode and a new era, and compare it with what OPFOR's game module and battlefield-opfor's host do.

**Method (round GM0, 2026-11-27).** Static only: a linear capstone sweep of a scratch copy of the executable (6.5
million instructions), targeted reading of the functions found, and `spec/probes/gamemodes.py` over DICE's bundles
(files dated before 2013; the Vietnam patch's startup bundle through OPFOR's kept copy `startup-00.fbrb.opfor-orig`
where the live one is OPFOR's). Nothing was run. Counts the probe checks are **verified**; addresses read from the code
are **observed**; readings of purpose and designs are **inferred**. The field offsets are the run-time members of
`spec/engine/typeinfo/game_shared.md` (`GameModeData`: CrosshairsName +0x18, InGameMenuName +0x24, LoaderName +0x30,
AddToIngameBundleRefs +0x3C, AddToLoaderBundleRefs +0x48, SecondaryLogicLayerName +0x54, MatchmakingQueryID +0x60,
ModeID +0x6C, Description +0x78, PrimaryLogicLayerName +0x84, HudRootName +0x90; `LevelDescription.GameMode` +0x20,
`MatchmakingModID` +0x84; `LevelData.LevelDescription` +0x84, so a level's mode is LevelData +0xA4).

## 1. The game mode's data

### GMD-DATA-1: DICE's ten GameModeData partitions

Status: **verified** · Variants: PC

DICE's two startup bundles hold ten `GameSharedResources.GameModeData` partitions with five distinct `ModeID`s and
four non-empty `MatchmakingQueryID`s:

| Partition | ModeID | MatchmakingQueryID | Description (loc key) | HudRootName | SecondaryLogicLayerName | InGameMenu / Loader |
|---|---|---|---|---|---|---|
| `GameModes/Conquest` | `conquest` | `CONQUEST` | `MP_GAMEMODE_CONQUEST_NAME` | `hud_root` | `hudlayer_mp_flagcapture` | `igm_mp` / `loading_mp` |
| `GameModes/GoldRush` | `goldrush` | `RUSH` | `MP_GAMEMODE_RUSH_NAME` | `hud_root` | `hudlayer_mp_goldrush` | `igm_mp` / `loading_mp` |
| `GameModes/SquadRush` | `squadrush` | `SQRUSH` | `MAP_GAMEMODE_SQR_NAME` | `hud_root` | `hudlayer_mp_goldrush` | `igm_mp` / `loading_mp` |
| `GameModes/TeamDeathmatch` | `tdm` | `SQDM` | `MP_GAMEMODE_SQDM_NAME` | `hud_root` | `hudlayer_mp_tdm` | `igm_mp` / `loading_mp` |
| `GameModes/Singleplayer` | `singleplayer` | (empty) | (a sentence) | `hud_root` | `hudlayer_sp` | `igm_sp` / `loading_sp` |
| `GameModes/SingelplayerFirstLevel` | `singleplayer` | (empty) | (a sentence) | `hud_root` | `hudlayer_sp` | `igm_sp` / `loading_sp01` |
| `GameModes/NamConquest`, `NamGoldRush`, `NamSquadRush`, `NamTeamDeathmatch` | as the four above | as above | as above | `nam_hud_root` | as above | `igm_mp` / `loading_mp` |

Every one has `PrimaryLogicLayerName` `hudlayer_shared` and `CrosshairsName` `hud_crosshair`. The bundle refs are
per mode family: `GameModes/<Conquest|GoldRush|Singleplayer>IngameBundle.ref` (Squad Rush shares GoldRush's, Squad
Deathmatch Conquest's; Vietnam's four have `Nam...` refs) and `GameModes/<MP|SP|NamMP>LoadingBundle.ref`. So
Vietnam's modes differ from BFBC2's only in the HUD root and the bundle refs: they keep the same `ModeID`s, which is
what lets every exe test of a mode id (GMD-SW-1) treat them as the stock modes. The code compares the `ModeID`s, not
the partition names: Rush is `goldrush`, Squad Deathmatch `tdm` (GAME-MODE-3 names the descriptions' keys, which
follow the partition names: `teamdeathmatch`).

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: 2 startup bundles (Dist 2010, the Vietnam patch's Package 2012), 10 GameModeData; mode ids conquest, goldrush, singleplayer, squadrush, tdm; query ids CONQUEST, RUSH, SQDM, SQRUSH and empty]

### GMD-DATA-2: Where the mode partitions live; mode levels delete their map's other modes

Status: **verified** · Variants: PC

A mode partition is not only in the startup bundle (LOAD-START-1, LOAD-START-2). DICE ships live copies in the
startup bundles (6 + 4), the common level bundles that a mode level stacks on (`mp_common` 4, `sp_common` 1, `sp_01` 1,
`nam_common` 4: 9 under `*_common`), the main-menu bundles (Dist 6, the Vietnam patch's pristine Package one 4), the
eight language `eula_*` bundles of each (80), and 15 mode-level bundles of the Vietnam patch's Package tree (a level's
own mode, re-shipped with the patch). Every one is the same partition by name, so the bundle stack resolves one
object. The mode levels also carry **deletion markers** for their map's other modes: 168 deleted `GameModes/*`
entries in the level bundles (Conquest 28, GoldRush 29, SquadRush 26, TeamDeathmatch 24, each Nam mode 15,
Singleplayer 1): `mp_005sr` deletes Conquest, GoldRush and TeamDeathmatch. A tool that adds a mode to a map's level
bundles must decide whether to add such a marker; the startup and common copies are what a description's reference
resolves to (LEVEL-DESC-2's crash when none does).

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: live GameModes partitions by place startup 6, common level 9, level 15, main menu 10, eula 80; 168 deletion markers]
[inferred: the deletion markers keep a mode level's bundle stack from carrying the sibling modes' partitions; no exe reader of a marker was looked for]

### GMD-DATA-3: The descriptions name the mode, the content set and the licence

Status: **verified** · Variants: PC

DICE's 74 level descriptions (LEVEL-DESC-1) pair a mode with a content set (`MatchmakingModID`) and a licence: BFBC2
Conquest 8 + 3 `ciab`, Rush 6 + 5 `ciab`, Squad Rush 5 + 3 `ciab` + 1 `sqr`, Squad Deathmatch 5 + 5 `ciab`, all `BC2`;
Vietnam's four modes 5 each, all `VIETNAM` with licence `nam`; the campaign 12 `singleplayer` + 1
`singelplayerfirstlevel`, `BC2`. No DICE description names another content set or a mode partition outside the ten.
The description is the only place that binds a level to a mode partition and to an era, so a new mode or a new era's
level starts with a description (GMD-NEW-1, ERA-NEW-2).

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: 74 descriptions; content sets BC2, VIETNAM; licences ciab, nam, sqr and none]

## 2. What reads a mode, and what switches on it

### GMD-READ-1: Who reads each GameModeData field

Status: **observed** · Variants: PC

The current mode is reached three ways: the UI's mode owner 0x92AB10 (0x920750, the level description: in a level
LevelData's, before one the `<level>_description` asset's, MENU-LOAD-2; then +0x20), a level's LevelData +0xA4, and two
getters on the game's level object (0x53EDD0 ModeID, 0x53EDF0 MatchmakingQueryID). Every read of a field found:

| Field | Reader | What it does |
|---|---|---|
| `ModeID` (+0x6C) | 26 literal compares (GMD-SW-1), the mode-kind object (GMD-SW-2) | the UI's per-mode behaviour |
| | `extern.gameModeID` (HUD getter 0x777B30, case 0x77B3B8) | hands the id to the movies |
| | catalogue builder 0x951480 (at 0x951615) | the key of the mode catalogue (GMD-CAT-1) |
| | 0x53EDD0 -> 0x85746E, 0x865EDC (with the four-cc tags `NUOR` / `GNOC`), 0x86E9DE; 0x575134 | server-side reports and a stat event's string parameter |
| `MatchmakingQueryID` (+0x60) | 0x5680DB (in 0x567D30, called from 0x85F9BE) | the stats category (GMD-SW-3) |
| | catalogue builder (0x95166B) | the mode's key in the catalogue (`CONQUEST`, ...: MENU-CAT-1) |
| | 0x53EDF0 -> 0x84FCB3 (the server's online attribute `gamemode`), 0x86F3D8 | online attributes and reports |
| `Description` (+0x78) | catalogue builder (0x951654) | the mode's display name in the menus |
| `LoaderName` (+0x30) | loading screen's root slot 0x74EA90 | the loading movie (MENU-LOAD-2) |
| `HudRootName` (+0x90) | HUD's root slot 0x74D860 | the HUD movie (MENU-LOAD-2) |
| `PrimaryLogicLayerName`, `SecondaryLogicLayerName`, `CrosshairsName`, `InGameMenuName` (+0x84, +0x54, +0x18, +0x24) | `extern.hudMovies` (HUD getter case 0x77AB2D), four strings in that order | the HUD's layers (MENU-HUD-1, MENU-HUD-2) |
| `AddToIngameBundleRefs`, `AddToLoaderBundleRefs` (+0x3C, +0x48) | none found by the operand scan | (inferred: read by the bundle set-up through another path) |

The field the server's gameplay reads is none: the server reads the ModeID only for reports, the `singleplayer` tests
(GMD-SW-4) and its level list (ERA-MM-1); the rules of play are the level's entities (GAME-MODE-1, ONS-MODE-1).

[exe: BFBC2Game.exe 0x92AB10 (`call 0x920750; mov eax,[eax+0x20]`); 0x53EDD0 / 0x53EDF0 (`[ecx+0x64]` -> `+0xA4` -> `+0x6C` / `+0x60`); 0x77AB2D - 0x77AB64 (+0x84, +0x54, +0x18, +0x24 to 0xF7C5C0); 0x77B3B8 (+0x6C); 0x951615 / 0x951654 / 0x95166B; 0x5680D1 - 0x5680DB]
[data: scratch operand scan, 2026-11-27: every load of `[reg+0xA4]` or call of 0x92AB10 followed within 7 instructions by a GameModeData offset: 28 hits, the false ones (other structures at the same offsets) set aside by reading each]

### GMD-SW-1: The executable compares ModeID with five literals at 26 sites

Status: **verified** · Variants: PC

The exe names five mode ids and never another (`onslaught`, `airsuperiority`: GMD-PERS-1's absent list). Every compare
goes through the eastl-string-equals-C-string 0x4304D0 (or an inline `repe cmpsb`):

| Literal | Sites | Function | What it decides |
|---|---|---|---|
| `conquest` (0x142ABD8) | 9 | 0x749120 (isConquest, called by the HUD externs `spawnPoints` 0x777CE8, `flagStatus` 0x77827A, `getSpawnIndexFromScreenCoord` 0x77B50F, and 0x9330E8), 0x749660, 0x749F50 (a HUD object's set-up, called from the HUD's start: +0xC1), 0x7531D0 (loading text), 0x76D2A0 (`enemyTeamScores` 0x779256), 0x76E430, 0x7773B0, 0x785140 (twice) | the flag HUD, spawn-point and flag-status externs, ticket bars, the loading text `MP_LOADING_GAME_MODE_CQ` |
| `goldrush` (0x142AE7C) | 2 | 0x7531D0 (loading text `MP_LOADING_GAME_MODE_GR`), 0x7875B0 (a HUD event case) | Rush's loading text and HUD event |
| `squadrush` (0x14184A8) | 3 | 0x7531D0 (`PC_MP_LOADING_GAME_MODE_SR`), 0x561530 (mode kind, GMD-SW-2), 0x664940 | Squad Rush's loading text and kind |
| `tdm` (0x14184B4) | 7 | 0x749150 (isTdm, no caller), 0x749F50 (+0xC2), 0x7531D0 (`PC_MP_LOADING_GAME_MODE_TDM`), 0x561530 (kind), 0x775C00 (the HUD's start: an extra object for Squad Deathmatch), 0x959320 / 0x959530 (`extern.eorSquads`, `extern.eorSquadPlayers`) | Squad Deathmatch's HUD, end-of-round squads and kind |
| `singleplayer` (0x1418768) | 5 | 0x567D30 (twice: stats category, start delay), 0x864E50 (server level list), 0x949EB0, 0x9501D0 (`extern.collectiblesGrid`) | GMD-SW-4 |

A mode with a new ModeID answers false at all 26: no flag HUD, no loading text, kind 0, a start delay and stats
from its query id. OPFOR's module answers the 9 `conquest` compares true for `onslaught` and `airsuperiority`
(ONS-MODE-3), which is how both new modes get the flag HUD; nothing answers the `goldrush`, `squadrush` or `tdm`
compares for a new mode yet.

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: push / mov-imm loads of the literals in the code: conquest 9, goldrush 2, squadrush 3, tdm 7, singleplayer 5]
[exe: BFBC2Game.exe 0x749132, 0x749682, 0x749F72, 0x753293, 0x76D575, 0x76E612, 0x7773D4, 0x78642E, 0x786AF0 (conquest); 0x7532B0, 0x787E4F (goldrush); 0x753306, 0x561851, 0x664F7D (squadrush); 0x749162, 0x749F9C, 0x753333, 0x561834, 0x7764A4, 0x959417, 0x95955D (tdm); 0x568177, 0x568640, 0x86509E, 0x949F07, 0x950212 (singleplayer)]

### GMD-SW-2: A level's ModeID picks a mode-kind object: Squad Deathmatch, Squad Rush or the rest

Status: **observed** · Variants: PC

At level set-up 0x561530 compares the level's ModeID (the game context 0x547BF0 -> +0x30 -> +0x64 -> +0xA4 -> +0x6C)
with `tdm` (kind 2) and `squadrush` (kind 1), anything else kind 0, and replaces the object at 0x15666C0 when the kind
changed: kind 0 a 0x410-byte object (constructor 0x555E50, vtable 0x1418048), kinds 1 and 2 an 8-byte object (vtables
0x14169B0, 0x14169C4) holding the kind at +4. Its readers (14 instructions) ask the kind or call its
virtual functions (with no object they read the kind as 3). A new ModeID gets kind 0, the Conquest / Rush object.

[exe: BFBC2Game.exe 0x561834 - 0x5618EC (the compares, the allocations, the store to 0x15666C0); readers 0x532AA0, 0x54F4AF, 0x564CF1, 0x569A1B, 0x573F9B]
[inferred: kind 0's large object holds the flag / objective modes' team and squad rules, the two small ones the squad modes'; their virtual functions were not read]

### GMD-SW-3: The matchmaking id: the stats category and the fixed list of four

Status: **verified** · Variants: PC

`MatchmakingQueryID` is compared with four literals at three places each (12 sites):

- **The stats category** (0x567D30 at 0x5680E6 - 0x568189): `RUSH` or `CONQUEST` 0x20, `SQDM` 0x10, `SQRUSH` 8;
  otherwise, when the ModeID is `singleplayer`, 1; else -1, handed to 0x84F790 on the stats object. GAME-MODE-4's
  rule (a new mode keeps `CONQUEST` to keep score and stats) is this switch.
- **The server search** (main-menu FSCommand `startFindServer`, 0x77C651 - 0x77C8F1): a requested mode of `any` expands
  to the matchmaking attribute `gamemode` = `CONQUEST`, `RUSH`, `SQDM`, `SQRUSH`; another request is passed through.
- **The server** (0x8654A7 - 0x865636): the same four names in a list.

So a query id is free-form data for the catalogue (GMD-CAT-1) and the server's attribute, but only the four stock
ids score; the online "any mode" search never lists a fifth.

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: CONQUEST, RUSH, SQDM, SQRUSH loaded 3 times each]
[exe: BFBC2Game.exe 0x5680C0 - 0x568197; 0x77C657 (`any` 0x142B6D4 via 0x508160), 0x77C686 / 0x77C73C / 0x77C7F2 / 0x77C8A8; 0x8654A7 / 0x86552C / 0x8655B1 / 0x865636]

### GMD-SW-4: The singleplayer ModeID: no start delay, no server level, its stats category

Status: **observed** · Variants: PC

`singleplayer` is the one id the game's own logic tests: the level start copies the description's `StartDelayTime`
(LevelData +0xA8) unless the ModeID is `singleplayer` (0x568640, zero then); the stats category is 1 (GMD-SW-3); the
server's level list leaves such levels out (0x86509E, ERA-MM-1); the menus' level-list walk 0x949EB0 and
`extern.collectiblesGrid` (0x9501D0) separate campaign from multiplayer levels by it. A new multiplayer mode must not
use this id; a new campaign-like mode (co-op) that wants the campaign's behaviour must.

[exe: BFBC2Game.exe 0x56862F - 0x568665 (`repe cmpsb` with 0x1418768, then [ebx+0xA8]); 0x86509E; 0x949F07; 0x950212]

### GMD-CAT-1: The menus' mode catalogue is keyed by ModeID

Status: **observed** · Variants: PC

The catalogue builder 0x951480 (MENU-CAT-1) walks the `_description` partitions and, per description, looks its mode's
`ModeID` up in a table (0x915EC0); the first description of a ModeID adds the entry with that mode's `Description`
(the display name) and `MatchmakingQueryID` (the key). So two partitions with one ModeID (`conquest` and
`namconquest`) are one catalogue mode, named by whichever description came first, and a new mode is a menu mode of its
own exactly when its ModeID is new.

[exe: BFBC2Game.exe 0x95160F - 0x95169C: `+0x38` (the description's GameMode), `lea [ebp+0x6C]` to 0x915EC0, then +0x78 and +0x60 copied]
[inferred: "first wins" from the insert-if-absent shape; the order of the walk is the startup bundle's]

### GMD-PERS-1: The persistence template is chosen by campaign or multiplayer only

Status: **verified** · Variants: PC

The exe names two persistence templates, `Persistence/SP_Template` and `Persistence/MP_Template`, and picks one by a
campaign / multiplayer flag at each use (five `mov eax` pairs: 0x565497, 0x6F076E, 0x775D84, 0x8526CC, 0x853A43; the
MP one also pushed alone 4 times, the SP one 3). DICE's PC startup bundle also ships `Persistence/Ons_Template`
(Onslaught's, 170 value templates), which no code names. A new mode therefore shares the multiplayer career
(PERS-DATA-1); a per-mode or per-era career is not data the engine can select (a separate Pacific War career is the
module's: PERS-OPF-1).

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: MP_Template loaded 9 times, SP_Template 8; Persistence/Ons_Template absent from the exe]
[data: scratch read of DICE's startup bundles, 2026-11-27: PersistenceData MP_Template (2,066 final templates), SP_Template (324), Ons_Template (170)]

### GMD-MM-1: DICE's matchmaking data names the modes by query id, Onslaught included

Status: **verified** · Variants: PC

`Awards/MatchmakerParams` (startup) holds 65 `MatchMakingMap`s keyed by query id and licence (`CONQUEST` 8 + 3 `ciab`
+ 5 `nam`, `RUSH` 6 + 5 + 5, `SQDM` 5 + 5 + 5, `SQRUSH` 5 + 3 + 5 + 1 `sqr`, `ONSLAUGHT` 4 with licence `ons`), nine playlists
(`CONQUEST`, `RUSH`, `SQDM`, `SQRUSH` twice, `ONSLAUGHT` once) and the `gamemode` filter's possible values `RUSH`,
`CONQUEST`, `SQDM`, `SQRUSH`, `ONSLAUGHT`. The PC data was built with the console's Onslaught matchmaking even though
the PC exe never compares `ONSLAUGHT` (GMD-SW-3). This data serves online matchmaking only; offline play never reads it
(inferred).

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: 65 matchmaking maps, modes CONQUEST, ONSLAUGHT, RUSH, SQDM, SQRUSH; playlists CONQUEST 2, ONSLAUGHT 1, RUSH 2, SQDM 2, SQRUSH 2]

## 3. The game mod (era)

### ERA-MOD-1: 33 instructions use the game mod, 8 the requested one

Status: **verified** · Variants: PC

The UI manager (`[0x19A4CA8]`) holds the game mod at +0xC8C8C and the requested one at +0xC8C90. Exactly 33
instructions address +0xC8C8C (battlefield-opfor's "33 mode-reader sites" is this count) and 8 address +0xC8C90:

| Use | Sites (function) | Purpose |
|---|---|---|
| dead accessors | 0x731340 / 0x731354 (no caller, no pointer) | none |
| stores | 0x75FCF0 (the setter 0x75FCB0), 0x77332C (loading screen 0x772DD0), 0x775D0A (HUD start 0x775C00), 0x9563E5 (constructor, 0) | ERA-MOD-2 |
| content `== 1` | 0x751BC2 (0x751BA0, `extern.gameTip` 0x777C94) and 0x7776EA (0x7774A0): `Menu/GameTips/nam_GameTips` vs `GameTips` | ERA-RES-1 |
| | 0x7584A2 (0x758480, from the HUD start 0x775D62 only): `Menu/nam_hudConfig` vs `hudConfig` | |
| | 0x75AEA1 (0x75AE80, from the setter's reload 0x75F897): `Menu/nam_menuConfig` vs `menuConfig` | |
| | 0x775D16 (HUD start): the icon set `menu/minimap/icons_nam0` + `icons_nam_data` vs `icons0` + `icons_data` (0xF9C9E0; a failure aborts the HUD) | |
| | 0x9208D0 (no direct caller) and 0x93D14F (0x93CF40, in-game 0x7866D3): `UISharedConfigAsset.UnlocksDataVietnam` (+0x90) vs `UnlocksData` (+0xB0) | ERA-RES-2 |
| | 0x93C4E1 (0x93C4D0) and 0x93C561 (0x93C550, the weapon-data lookup; callers include `extern.kitPrimaryWeapon` / `kitSpecialization` / `wname` / `weaponData`): Vietnam's map +0xC89DC / +0xC89FC first, then the base map | |
| | 0x9469D5 / 0x946A75 / 0x946B35 (0x9469B0 `extern.unlockGrid`, 0x946A50 `kitRankData`, 0x946B10 `unlockGridChildren`): the list at +0x138 vs +0x134 | |
| | 0x920CF8 / 0x920E45 / 0x920F88 / 0x9210D5 (0x920CE0, 0x920E30, 0x920F70, 0x9210C0: the `seenItem` / `seenItemHud` externs and FSCommands): the new-item tracker's entries tagged 1 (BFBC2 only) or 2 (Vietnam only), mod 1 skipping 1s and others 2s | |
| | 0x76FCDE: `extern.playerHasLicenseToNam` | ERA-LIC-1 |
| | 0x785CEF, 0x786BCD (0x785140, the HUD update), 0x78824F (0x7875B0, a HUD event): Vietnam-only HUD behaviour (an `OnFire` event when burning, a burn health timing, a kill-message variant) | |
| tags | 0x942247 (0x9421F0, `extern.multiplayerMaps`): 0 `BC2`, 1 `VIETNAM`, else empty | GAME-MODE-7 |
| | 0x9519AC (0x951910, `extern.levelList`): the same tag, built and never compared | |
| | 0x77C994, 0x77D0C3 (two server-search FSCommand cases, `startFindServer` and the next): the matchmaking attribute `gameMod` = `VIETNAM` when 1, else `BC2` | ERA-MM-1 |
| plain reads | 0x95D7CE (`extern.gameMod`, the raw value), 0x95FE92 (FSCommand `presenceCancelLogin`: requested = current), 0x94EB34 (on presence connected: re-apply the requested mod through the setter when it differs) | |

The requested mod's 8: the dead setter 0x73135A, the setter 0x75FCF6, the two parsed stores 0x773332 / 0x775D10,
0x920C0E (0x920C00: the request posted with a sign-in, ERA-LIC-1), 0x94EB2E (read), 0x95FE98 / 0x95FEA0. The setter
has three callers: the main-menu FSCommand `changeGameMod` (0x77D87A, `atoi` with no range check), the presence
re-apply 0x94EB3F and a UI message 0x964E4A.

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: +0xC8C8C 33 instructions (cmp 1: 20, cmp reg: 1, load 7, store 5), +0xC8C90 8; the 33 addresses listed]
[exe: BFBC2Game.exe every address in the table; extern names from the getters' jump tables (bfbc2-menu-tools/aptrefslib.py GETTERS, the HUD getter 0x777B30's table 0x77BAB8 / 0x77B880)]
[doc: opfor:research/WW2_THIRD_MODE.md:59-97]

### ERA-MOD-2: The game mod follows the level's MatchmakingModID; the exe keeps no setting

Status: **observed** · Variants: PC

The two stores that bypass the setter do not read a profile setting (as OPFOR's first study and GAME-MODE-6 had it):
both parse a level description's `MatchmakingModID` (+0x84) with 0x8FF330 (`BC2` 0, `VIETNAM` 1, anything else 0):

- the loading screen's set-up 0x772DD0 (called from 0x7774A0) loads the `<level>_description` asset (0x459530, the
  string `_description` 0x1425AC0), keeps its +0x18 (the description) and parses its +0x84 at 0x77331D;
- the HUD's start 0x775C00 (vtable 0x142B7A0's slot 3) parses the running level's `LevelData` +0x84 +0x84
  (0x7FF060 -> +0x38 -> +0x44 -> +0x108, the path 0x920750 uses) at 0x775D02, then builds the icon set and the HUD
  configuration from the result (ERA-RES-1).

The constructor starts the game at 0 (0x9563E5). So the menu's era choice (the setter) lasts until the next level loads,
and every loaded level puts the client in its own description's era: a description whose `MatchmakingModID` is not
`BC2` or `VIETNAM` is played as BFBC2 (the Wake Island icon failure of GAME-MODE-6 before OPFOR's parser hook). The
string `gameMod` is only a matchmaking attribute's name (ERA-MM-1); no exe code saves or restores the era.

[exe: BFBC2Game.exe 0x772DD0 - 0x77333C (0x772E62: 0x459530 with type 0x1BFD69C; 0x772EC2 - 0x772EE5: asset +0x18 kept; 0x773313 - 0x773332); 0x775CE9 - 0x775D10; 0x9563E5]
[data: spec/probes/gamemodes.py, 2026-11-27: the `gameMod` string loaded 5 times, all as a matchmaking attribute key (0x77CA01, 0x77CAA4, 0x77D130, 0x77D1D5, the server's 0x84FD2A)]
[code: opfor:src/dll/gamemode.cpp:485-505]

### ERA-MOD-3: Only 0 and 1 exist in the code

Status: **verified** · Variants: PC

Every reader compares the game mod with 1 (20 sites) or, for the tags, 0 then 1; the parser returns only 0 or 1; the
setter's `gameModChange` label is `BFBC2` for 0, `BFBC2Nam` for 1 and empty otherwise (0x75FDA4 - 0x75FDBC), and it
raises `showNamRedeemPopup` for 1. The exe holds no third era's tag or label: `PACIFIC`, `BFBC2143`, `BFBC2Beach`,
`BC1`, `BFBC1` are absent (DICE's main_nav movie has `BFBC2Beach` / `BFBC2143` frames and tests `extern.gameMod` for 2
and 3: the menus were drawn for four eras, the engine was built for two, MENU-ED-9).

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: the tags BC2 loaded 8 times, VIETNAM 6; the strings PACIFIC, BFBC2143, BFBC2Beach, BC1, BFBC1 absent]
[exe: BFBC2Game.exe 0x8FF330 (two compares, default 0); 0x75FD8C - 0x75FE42]

### ERA-RES-1: Each era's resources, and when they are chosen

Status: **observed** · Variants: PC

| Resource | BFBC2 (0) | Vietnam (1) | Chosen |
|---|---|---|---|
| menu configuration (`MenuConfigAsset`: `BackgroundMusic`, intro / attraction / credits movies, award and vehicle grids) | `Menu/menuConfig` (main menu bundle) | `Menu/nam_menuConfig` (the patch's main menu bundle; its music `sound/music/vietnam/menu_and_loading/...`) | at every era switch (the setter's reload) |
| HUD configuration (`HudConfigAsset`: vignettes, victory / defeat stings per faction, pickup and objective sounds) | `Menu/hudConfig` (`mp_common` / `sp_common` in-game) | `Menu/nam_hudConfig` (`nam_common` in-game: 16 vignettes, the `nam_victory_*` stings, no MEC ones) | at the HUD's start, from the level's era |
| HUD / minimap icon set | `menu/minimap/icons0`, `icons_data` | `menu/minimap/icons_nam0`, `icons_nam_data` | at the HUD's start |
| game tips | `Menu/GameTips/GameTips` | `Menu/GameTips/nam_GameTips` | per tip request |
| HUD root | each mode's `hud_root` | each Nam mode's `nam_hud_root` (GMD-DATA-1) | by the level's mode, not the era |
| unlock tree, weapon presentation | `UISharedConfigAsset.UnlocksData`, `WeaponData` | `UnlocksDataVietnam`, `WeaponDataVietnam` (ERA-RES-2) | per request |
| kit / unlock lists, new-item tags | lists +0x134, tag 1 | +0x138, tag 2 | per request |
| content set of a level | `MatchmakingModID` `BC2` | `VIETNAM`, licence `nam`, `KickIfLicenseMissing`, `KIT_*_NAM` kit images (LVL-DESC-1) | data |
| menu art | main_nav frames `BFBC2` | `BFBC2Nam` (the `gameModChange` label) | at every era switch |

Two eras share everything else: one persistence template (GMD-PERS-1), one award tree and rank table (PERS-AWD-1), one
stats category per mode (GMD-SW-3), one localisation table (the Vietnam strings are `NAM_` / `nam_`-prefixed keys in the
same table: inferred from the asset names), and one `UIMenuTrackerPage` enum whose third value is
`UIMenuTrackerPage_MenuUnlocksVietnam` (typeinfo).

[exe: BFBC2Game.exe 0x75AEB2 / 0x75AF15; 0x7584AD / 0x758510; 0x775D1F - 0x775D54 (pointers 0x150A270 - 0x150A27C); 0x751BC2; 0x9208D0; 0x93C4EF; 0x9469E3 / 0x9469EB]
[data: scratch read, 2026-11-27: MenuConfigAsset in Dist / pristine Package main menu (menuConfig, nam_menuConfig: BackgroundMusic differs), HudConfigAsset hudConfig (mp_common, sp_common) and nam_hudConfig (nam_common), GameTips per level and nam_GameTips in nam_common]

### ERA-RES-2: The Vietnam patch adds the era's half of UISharedConfigAsset

Status: **verified** · Variants: PC

`UISharedConfigAsset` has a fixed pair of fields per era, nothing more: `UnlocksData` / `UnlocksDataVietnam` and
`WeaponData` / `WeaponDataVietnam`. Dist's `Menu/shared_config` (2010) sets only the BFBC2 half (2 unlock groups, 134
weapons); the Vietnam patch's (2012) sets both (2 + 2 groups; 131 + 26 weapons). RTTI-DBX-1's "two members DICE's data
does not set" is true of the census corpus, which leaves the patch's startup bundle out when OPFOR has replaced it. A
third era has no field to hold its unlock tree or weapon presentation: the class is compiled into the type system.

[test: core/tests/test_spec_probes.py::game_modes]
[data: spec/probes/gamemodes.py, 2026-11-27: Dist UnlocksData 2, WeaponData 134, Vietnam fields 0; Package UnlocksData 2, WeaponData 131, UnlocksDataVietnam 2, WeaponDataVietnam 26]

### ERA-LIC-1: The licence, entitlement and sign-in checks tied to the era

Status: **observed** · Variants: PC

Described here, not designed around (the user's ruling: never bypass ownership or entitlement checks; Vietnam
content offline is fine for owners):

- **Sign-in.** The setter 0x75FCB0, asked for any non-zero mod while the client has no EA session object (UI
  +0xC38D4), posts a sign-in request through 0x920C00 (requested mod stored, presence request) and returns without
  switching; mod 0 always switches. It does not test a licence. OPFOR's `[patches] vietnam` changes this gate
  (module.md, the Vietnam ruling) and the redeem pop-up.
- **The Vietnam licence.** `extern.playerHasLicenseToNam` (0x76FCD3) is true when the mod is 1 and the profile's
  licences (+0x1E34) hold the token `nam` (the token test 0x649ED0: space-separated tokens up to 15 characters).
- **Licences on levels.** A description's `Licenses` (`nam`, `ciab`, `sqr`: GMD-DATA-3) and `KickIfLicenseMissing`
  gate the level: the level-list extern 0x951910 filters levels by licence (0x951A83) and rank; no reader of
  `KickIfLicenseMissing` was found by its offsets (inferred: the server's join path, online).
- **Licence items.** `Unlocks::applyLicenses` (0x8527D0, two token tests) unlocks items whose `Licenses` /
  `AdditionalLicenses` the player holds (PERS-LIC-1); the persistent-weapon checks 0x652380 / 0x6524C0 test the same
  tokens. Offline nobody holds a licence; OPFOR grants the `nam` items on Vietnam levels (PERS-OPF-2).
- **Entitlements.** `Persistence/MP_Template`'s `EntitlementAsset` has five `EntitlementQuery`s (groups
  `BFBC2%PLATFORM%`, `AddsVetRank`, `BattlefieldBadCompany2` with tag `BFBC2:COMMON:GAMESTOP`, `BFBC2PC` with
  `BFBC2:PC:LimitedEdition`, `NoVetRank`): EA's account items become the licences above (online).

The era value itself carries no entitlement: the checks are per licence token, so an era layered over mod 1 inherits
the Vietnam licence tests (`playerHasLicenseToNam`, the `nam` level licence) wherever it reuses Vietnam's data.

[exe: BFBC2Game.exe 0x75FCC0 - 0x75FCE9; 0x920C00 (stores +0xC8C90 at 0x920C0E); 0x76FCDE - 0x76FCF5; 0x649ED0 and its 10 call sites (0x6523A7, 0x6523C8, 0x6524EC, 0x76FCF5, 0x7785FB, 0x8528E3, 0x852904, 0x866E57, 0x951A83, 0x951CC5); 0x75FE32 - 0x75FE42]
[data: scratch read of DICE's startup bundles, 2026-11-27: EntitlementAsset 1, EntitlementQuery 5 (group names and tags as listed)]

### ERA-MM-1: The era online: matchmaking attributes and the server's level list

Status: **observed** · Variants: PC

Online, the era is a string: the server-search FSCommands set the attribute `gameMod` = `VIETNAM` or `BC2` from the
client's mod (ERA-MOD-1); the server publishes `level`, `gamemode` (the level's query id), `gameMod` (a settings
string at +0x118 of its configuration) and `elo` (0x84FCA8 - 0x84FD4B); and its `LevelListQuery` builder 0x864E50 keeps
the descriptions whose ModeID is not `singleplayer`, whose `Revision` is at most the server's, and whose
`MatchmakingModID` (empty read as `BC2`) equals the server's own mod string (+0xB8). These are strings, so a third era's
tag passes through them unchanged (an unpatched client simply never matches it); offline, OPFOR's module filters the
level lists itself (GAME-MODE-7, NET-SB-3).

[exe: BFBC2Game.exe 0x77C9A6 - 0x77CA2D, 0x77D0C3 - 0x77D20E (the attribute map 0x775930, posted as message 0x25 at 0x77D24E); 0x84FCA8 - 0x84FD52; 0x864E50 (0x864EAB `LevelListQuery`; 0x865064 - 0x8650F2)]
[inferred: the server's +0xB8 / +0x118 strings come from its settings; their writers were not read]

## 4. Authoring a new game mode

### GMD-NEW-1: The minimum data set of a new mode

Status: **inferred** · Variants: PC

A new multiplayer mode is data, in this order:

1. **The mode partition** `GameModes/<Name>.dbx`, cloned from the closest stock one (GMD-DATA-1) under new GUIDs:
   a new `ModeID` (lower case; it becomes the menus' mode, GMD-CAT-1), `MatchmakingQueryID` one of `CONQUEST`, `RUSH`,
   `SQDM`, `SQRUSH` (the stats category, GMD-SW-3), a new `Description` loc key, the HUD names (stock movies, or new
   movies registered like any menu movie), `LoaderName` (`loading_mp`) and the bundle refs of the family it plays like.
2. **Where it goes**: the shared startup bundle patch (LOAD-START-2; a missing one crashes at 0x74EAB3) and the level's
   own bundles (OPFOR's precedent for Onslaught and Air Superiority); optionally a deletion marker in sibling mode
   levels (GMD-DATA-2).
3. **The descriptions**: each level's `_description` names it (`GameMode`), with the era's `MatchmakingModID`,
   licence and kit images (GMD-DATA-3), plus the two `LevelListQuery` items (LOAD-START-1, MENU-CAT-1).
4. **Registration**: the level's bundles in `package.mft` and `bundleManifest` (REG-NEW-1 - REG-NEW-5), and OPFOR's
   `maps.ini` row with the mode's display name.
5. **The rules of play**: the level's mode layer (GAME-LAYER-2): teams, ticket / life / kill counters, capture points,
   objectives, combat areas, spawns, messages and voice-over, built from stock entity classes. No class outside
   BFBC2's catalogue can be loaded (Air Superiority's DMZ, repair stations and hot zones are the module's:
   `opfor:research/AIR_SUPERIORITY_RE_2026-11-14.md` section 3.B).
6. **Text**: the `Description` and loading strings in the localisation table OPFOR patches.

What stays shared and cannot be per mode: the persistence template and career (GMD-PERS-1), the award tree, the stats
category beyond the four (GMD-SW-3), the online "any" search list.

[inferred: from GMD-DATA-1 - GMD-DATA-3, GMD-SW-1 - GMD-SW-4, GMD-CAT-1, GMD-PERS-1; OPFOR's two new modes followed it (ONS-MODE-2, LOAD-START-2)]
[code: rust/bc2-install/src/onslaught/mode.rs:35-43] [code: opfor:src/tools/ww2/wake_install.py:2675-2676]

### GMD-NEW-2: What a new ModeID needs in code

Status: **inferred** · Variants: PC

Data alone gives a new mode the stock behaviour of "no known id" at the 26 compares (GMD-SW-1): kind 0, no flag HUD,
no loading text, no Rush or Squad HUD events. The choices:

- **Reuse a stock ModeID** (as Vietnam's four partitions do): no code at all, every UI behaviour of that mode, but the
  menus show one catalogue mode for both (GMD-CAT-1) and the loading text is the stock mode's.
- **A new ModeID plus a compare answer**: OPFOR's hook on 0x4304D0 answers chosen literals true for chosen ids (today
  `conquest` for `onslaught` and `airsuperiority`, ONS-MODE-3). A mode that should behave like Rush, Squad Rush or Squad
  Deathmatch needs the same answer for `goldrush` / `squadrush` / `tdm`; the inline `repe cmpsb` sites (`squadrush`
  0x664F7D, `tdm` 0x959417 / 0x95955D, `singleplayer` 0x568640) do not go through 0x4304D0 and need their own patch if
  they matter for the mode.
- **Its own loading text or HUD event**: a patch at 0x7531D0 (the loading text) or the HUD event's case (0x787E4F).

Scoring, tickets, spawns and the end of the round need no code: the server never reads the mode's name (GMD-READ-1).

[inferred: from GMD-SW-1, GMD-SW-2, GMD-CAT-1 and OPFOR's levelfix hook]
[code: opfor:src/dll/levelfix.cpp:57-84]

## 5. Authoring a new era

### ERA-NEW-1: What a third game-mod value would touch

Status: **inferred** · Variants: PC

A native value 2 (or 3) is not data: no data names the era (only the level's string, which 0x8FF330 folds to 0), and
every site tests `== 1`. Each place a third value would touch, with the work it needs:

| # | Place | Work |
|---|---|---|
| 1 | parser 0x8FF330 | a third tag compare (a detour or code cave); without it every level of the era loads as BFBC2 (ERA-MOD-2) |
| 2 | setter 0x75FCB0 | a third `gameModChange` label (a branch at 0x75FDB2); the sign-in gate already treats it like 1 (ERA-LIC-1); the redeem pop-up stays 1-only |
| 3 | tags 0x942247 (offline list), 0x77C994 / 0x77D0C3 (online attribute), 0x9519AC (unused) | a third tag at the first two |
| 4 | the 20 `== 1` content sites (ERA-MOD-1) | per site: "like Vietnam" (a one-byte condition change) or its own resource |
| 5 | own resources with a name: menu configuration, HUD configuration, game tips, icon set | a string per era at 0x75AEB2, 0x7584AD, 0x751BC2 / 0x7776EA, 0x775D1F (data: new partitions and textures) |
| 6 | own resources with no slot: unlock tree, weapon presentation (ERA-RES-2), the +0x134 / +0x138 lists, the new-item tag | module-held data behind detours of 0x9208D0, 0x93CF40, 0x93C4D0, 0x93C550, 0x9469B0 / 0x946A50 / 0x946B10, 0x920CE0 / 0x920E30 / 0x920F70 / 0x9210C0 (the type system cannot grow a field) |
| 7 | Vietnam-only HUD behaviour 0x785CEF, 0x786BCD, 0x78824F; `playerHasLicenseToNam` 0x76FCDE | decided per site; the licence extern must stay a licence test (ERA-LIC-1) |
| 8 | the menus | every ActionScript test of `extern.gameMod` (main_nav and the backend pages) |
| 9 | persistence, awards, stats | nothing selects them by era (GMD-PERS-1): a separate career is the module's |

So the engine cannot support game mod 2, 3 natively by data. It can by patches: rows 1 - 5 and 7 - 8 are small,
documented patches; rows 4 - 7 are 30 sites whose miss falls back silently to BFBC2's content, so each needs its
decision; row 6 has no data slot anywhere and needs module-held data whatever the engine value. The user chose
native eras 2 and 3 on 2026-11-27 (ROADMAP GM1 - GM3: a decision per site, then the patch set and the data);
ERA-NEW-2 is the layered design those rounds compare against, and its steps are needed by both.

[inferred: from ERA-MOD-1 - ERA-MOD-3, ERA-RES-1, ERA-RES-2, ERA-LIC-1, GMD-PERS-1]
[doc: opfor:research/WW2_THIRD_MODE.md:257-311]

### ERA-NEW-2: An era as a host layer over 0 / 1 with per-site own content

Status: **inferred** · Variants: PC

The design that keeps every engine reader valid:

- **Data**: the era's levels carry their own `MatchmakingModID` (`PACIFIC`, a BFBC1 tag) and licence; its modes are
  `GameModes/<Era><Mode>` partitions with the era's HUD root (as `nam_hud_root`) and stock or new `ModeID`s
  (GMD-NEW-2); its menu configuration, HUD configuration, game tips and icon set are new partitions named per era.
- **The engine value**: the era picks 0 or 1 as its base (the base whose content it reuses where it has none); the
  host keeps the era (OPFOR's Pacific flag, battlefield-opfor's `ModeId`).
- **The minimal patch set**, each justified by a site above: (1) the parser 0x8FF330 maps the era's tag to its base
  (ERA-MOD-2: else its levels load as BFBC2); (2) the setter's label buffer at 0x75FDB7 / 0x75FDBC for the era's menu
  frames (ERA-MOD-3); (3) the level lists (0x9421F0) re-tag the era's records (GAME-MODE-7); (4) the string sites of
  ERA-NEW-1 row 5 point at the era's partitions while the era is active (0x75AEB2 menu music, 0x751BC2 / 0x7776EA tips,
  and, not done yet anywhere, 0x7584AD / 0x758510 the HUD configuration and 0x775D1F / 0x775D47 the icon set); (5) row 6
  only where the era needs its own tree or weapon names, one accessor at a time; (6) the menus' `extern.gameMod` tests
  answered by host externs (`opforGameMod`).
- **Not patched**: the licence and sign-in tests (ERA-LIC-1). An era built on base 1 inherits Vietnam's licence tests;
  that is right for content that is Vietnam's and must be stated to the user where an era only borrows Vietnam's
  slots.

[inferred: from ERA-NEW-1; OPFOR's module implements (1) - (4) for the Pacific War except the HUD configuration and icon set]
[doc: opfor:research/WW2_THIRD_MODE.md:125-174]

### ERA-OPF-1: OPFOR today, and its gaps

Status: **implemented** · Variants: PC

OPFOR's module (MOD-MODE-1) runs the Pacific War as base 1 plus its flag: the setter hook, the label buffer, the parser
hook (a `PACIFIC` level parsed as 1), the level-list filters, Pacific menu music and per-map tips; Onslaught and Air
Superiority are new ModeIDs answered as `conquest`. Measured against this file, the gaps:

- The HUD configuration and the icon set of a Pacific level are Vietnam's (`nam_hudConfig`: Vietnam's victory stings
  and vignettes; `icons_nam0`): ERA-RES-1's "at the HUD's start" sites are not era-aware.
- The unlock tree, weapon presentation, kit lists and new-item tags are Vietnam's (row 6), by design.
- `playerHasLicenseToNam` and the `nam` level licence apply to Pacific War levels (they carry `Licenses nam`).
- The comments in `src/dll/gamemode.cpp`'s header still call 0x775D0A / 0x77332C a profile setting (ERA-MOD-2; the
  parser hook's own comment has it right).
- Onslaught and Air Superiority get Conquest's flag HUD but not its loading text's other branches, and their catalogue
  entries are their own (GMD-CAT-1): their `Description` keys must exist in the string table.

[code: opfor:src/dll/gamemode.cpp:485-505] [code: opfor:src/dll/levelfix.cpp:57-84]
[code: opfor:src/dll/gamemode_rules.h:235-243]

### ERA-OPF-2: battlefield-opfor's host, and its gaps

Status: **inferred** · Variants: PC

battlefield-opfor (`crates/opfor-dll/src/gamemode.rs`, `crates/opfor-vfs/src/modes.rs`) maps four eras onto the engine
value (BFBC2 0; Vietnam, BF1943, BFBC1 1), repoints the label to `BFBC2Nam` / `BFBC2143` / `BFBC2BC1` and the menu
configuration push, and detours the setter and 0x8FF330. Its 0x8FF330 detour returns the active era's engine value
for every call, believing the function a start-up profile parser; it is the level's era parser (ERA-MOD-2), so while an
era is active every level loads in that era's base: a BFBC2 level in a Vietnam / BF1943 / BFBC1 session gets mod 1
(Vietnam's HUD configuration and icon set on a BFBC2 common bundle: GAME-MODE-6's empty icon table, reversed), and a
Vietnam level in a BFBC2 session gets 0. The fix is OPFOR's rule: parse the level's string, map the era tags to their
base. BFBC1 on base 1 also inherits Vietnam's licence tests (ERA-NEW-2). Its "33 mode-reader sites" are ERA-MOD-1's
33 instructions, of which 27 are readers.

[inferred: from the code of battlefield-opfor read on 2026-11-27 (gamemode.rs detour_parse_game_mod returns the active mode's engine_mode) against ERA-MOD-2]

## Pending playtests (in-game tests are paused)

- ERA-MOD-2: after a Vietnam match the main menu comes back in Vietnam's era (music, art); a fresh start is BFBC2.
- ERA-OPF-2: battlefield-opfor in an era session hosting a level of another era (the icon set and HUD sounds).
- ERA-OPF-1: a Pacific level's victory stings and vignettes (Vietnam's today).
- GMD-SW-2: a new ModeID's team / squad behaviour (kind 0) on a Squad-style layer.
- GMD-CAT-1: the menus' mode name for Air Superiority and Onslaught (their own catalogue entries).
- GMD-NEW-2: a mode reusing `conquest` as its ModeID (one catalogue entry with Conquest).

## What the UI rounds (UI0 - UI7) need from this file

- **UI0** (static RE): the extern names this file ties to era and mode sites (`gameMod`, `gameModeID`, `hudMovies`,
  `playerHasLicenseToNam`, `levelList`, `multiplayerMaps`, `unlockGrid`, `kitRankData`, `seenItem`, `seenItemHud`,
  `eorSquads`) and the HUD getter's jump table (0x777B30: ids 4 - 0x158, byte map 0x77BAB8, jumps 0x77B880).
- **UI3 / UI4** (the editor's export, battlefield-opfor's registries): a new mode's movies are the five names of its
  GameModeData (MENU-HUD-1); a new era's menu is a `MenuConfigAsset` per era chosen at the setter (ERA-RES-1).
- **UI5 / UI6** (the hub, era switch without restart): the era switch re-runs only the menu configuration; the HUD
  configuration and icon set follow the level at the HUD's start (ERA-MOD-2), so no restart is needed for them, but an
  era that wants its own HUD configuration needs ERA-NEW-2's step (4) first.
