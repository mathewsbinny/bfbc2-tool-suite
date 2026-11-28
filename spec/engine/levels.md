# Levels: ids, mode levels, descriptions at run time

How the game names a level, how one map becomes several levels (one per game mode), what the level data object holds
at run time, and what a level's description declares that the menus and the load act on. The level partition and the
description partition are layer 2 (`model/level.md`: LEVEL-DATA-1 to LEVEL-DATA-3, LEVEL-DESC-1, LEVEL-DESC-2); the
mode layer inside a level is `engine/gameplay.md`; the load itself `engine/loading.md`.

**Sources.** Project OPFOR BFBC2's reverse engineering (`private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md`, build 795745) and its
logged level catalogue, the Pacific War mode work (`bfo:docs/research/opfor/WW2_THIRD_MODE.md`), and the map editor's export study
(`bfbc2-map-editor/EDITOR_ARCHITECTURE.md` 3.12), which compared DICE's own campaign-to-multiplayer conversion
(MP_SP_005CQ from SP_005) and DICE's mode levels with their maps.

### LVL-ID-1: A level is `Levels/<LV>` everywhere

Status: **observed** · Variants: PC, Xbox 360

The game addresses a level by the id `Levels/<LV>`: the menus' host command prefixes its argument with `Levels/`
(NET-SES-1), the level partition is `Levels/<LV>.dbx` with `LevelData.Name` that path (LEVEL-DATA-2), its description
`Levels/<LV>_description.dbx`, its bundles `levels/<lv>/<kind>-00.fbrb` (BNDL-KIND-1) and its path data
`ai/levels/<lv>/` (HPD-USE-1). The suite's exports take an id of letters, digits and `_` (REG-NEW-2).

[exe: BFBC2Game.exe host handler 0x77BEC3 builds "Levels/" + argument]
[data: OPFOR's logged level catalogue: every level id Levels/<LV>; 74 stock levels each with a description in the startup bundles and bundles under levels/<lv>/]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:93] [doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:602-620]

### LVL-MODE-1: Each mode of a map is a level of its own

Status: **observed** · Variants: PC, Xbox 360

A map with several modes ships one level per mode, each with its own id, partition and description naming one game
mode: Atacama Desert is MP_005 (Conquest), MP_005GR (Rush), MP_005SR (Squad Rush). The suffixes follow DICE's naming:
`CQ` Conquest, `GR` Rush (the game mode `goldrush`), `SR` Squad Rush, `SDM` Squad Deathmatch, a bare `MP_<n>` the
map's first mode (Conquest on MP_001, MP_003, MP_005, MP_007; Rush on MP_002, MP_004, MP_006, MP_008); Vietnam's
`R` is Rush. The level, not the session, decides the mode: the suite's exports are likewise one mode per level.

[data: OPFOR's logged level catalogue of the stock levels: 10 BFBC2 maps with 2-4 mode levels each (MP_009 all four), Harvest Day and Oasis four each, Cold War three, Heavy Metal two, five Vietnam maps four each]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:604-620] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1477-1479]

### LVL-MODE-2: A mode level loads its map's bundles after its own

Status: **observed** · Variants: PC

A mode level's `package.mft` blocks target its own bundle first, then its map's level and terrain bundles, then the
common bundles (PREC-ORD-2): MP_005GR loads MP_005's bundles. Its own bundle holds the mode's layout, its own copies of
the files the engine finds by the level's name (PART-NAME-3), and deleted markers over what of the map it replaces
(BNDL-DEL-2); it reads its minimap from the map's (`levels/mp_005/ui/`). No stock level loads another level's bundles
except its map's.

[data: stock package.mft: mode levels' level blocks target the base map's bundles after their own (MP_005GR -> MP_005); MP_005GR holds the shader databases, emitter system, model groups and cull trees under its own name]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1050-1054] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1441-1449]

### LVL-RT-1: The level data object at run time

Status: **observed** · Variants: PC

At run time `GameSharedResources.LevelData` is an object (constructor 0x13A6DA0) whose bytes +0x3AC to +0x3B0 hold the
level's flags (SP_005: 0 0 1 1 1), among them `HasFlyingAI` at +0x3AD and `HasAI` at +0x3AE (AI-START-1); the
`PathfindingTypes` array (`PathfindingSettings` pointers) is at +0x36C (AI-START-3). The server's level load reads them
before any entity is created (LOAD-PH-2).

[exe: BFBC2Game.exe LevelData constructor 0x13A6DA0; HasAI tested at 0x575046 as byte [level+0x3AE]; PathfindingSettings array at LevelData+0x36C (navgen run on SP_009, 2026-10-14)]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:222-223] [doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:345-347]

### LVL-DESC-1: The description declares the level's content set

Status: **observed** · Variants: PC

Beyond its game mode, a level's description declares which content set it belongs to: `MatchmakingModID` (`BC2` on
BFBC2's levels, `VIETNAM` on Vietnam's; OPFOR's Pacific War levels `PACIFIC`), and for the DLC's levels
`Licenses = nam` with `KickIfLicenseMissing = true` and the `KIT_*_NAM` kit images. The offline level lists filter on
the mod id (GAME-MODE-7, NET-SB-3). A level made from another set's description inherits that set's declaration (Wake
Island, installed from NAM_MP_002CQ's description, was `VIETNAM` until OPFOR changed it to `PACIFIC`); the suite's
exports take the target era's donor's (XEXP-PRE-2).

[data: descriptions measured: Vietnam's levels MatchmakingModID VIETNAM, Licenses nam, KickIfLicenseMissing true, KIT_*_NAM; BFBC2's levels MatchmakingModID BC2 and no licence]
[doc: bfo:docs/research/opfor/WW2_THIRD_MODE.md:102-107] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1308-1310]

### LVL-DESC-2: The loading picture is found by the name key

Status: **observed** · Variants: PC

The loading screen takes the description's `Name` loc key, cuts it at `_NAME` and shows
`Awards/images/loadingScreens/<that, lower case>` from the on-demand awards bundle; the main menu's map list shows
`Awards/images/Levels/<level id>`. A name key whose prefix names no picture gives a black loading screen; a new level
under its own name needs its own key (`<LV>_NAME`) and both pictures, or keeps its base's key and so its base's
picture.

[data: OPFOR's records: a _NAME prefix mismatch gave a black loading screen; stock keys such as MP_009_NAME name mp_009's picture]
[code: rust/bc2-levelwrite/src/export.rs:135-150] [code: rust/bc2-levelwrite/src/export.rs:1951-1961]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1255-1261] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1594]

## Open questions

- Whether `KickIfLicenseMissing` acts offline (the in-process host has no entitlement service) and what the
  `Licenses` list gates there.
- The other flags at LevelData +0x3AC, +0x3AF and +0x3B0 (LVL-RT-1).
- Whether a level may name a mode other than its description's at run time (the mode is the description's in every
  level read).
