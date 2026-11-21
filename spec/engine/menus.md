# Menus: the APT runtime as the game uses it

What the game does with its menu movies at run time: the level catalogue the menus read, how the engine and the
movies talk, and what the movie loader and unloader require. The movie format is layer 1 (`formats/apt.md`, with the
loader's own rules APT-LOAD-1 to APT-LOAD-3); the commands that start a match are NET-SES-1; the server browser's rows
are NET-SB-1 and NET-SB-2.

**Sources.** Project OPFOR BFBC2's reverse engineering of the PC client (`research/BFBC2_DECOMP_RESEARCH.md` section
5, build 795745), its menu installs (`research/WW2_MENU.md`), and the suite's movie patcher (`bfbc2-menu-tools`,
`aptgraft.py`).

### MENU-CAT-1: The level catalogue is built from the description partitions

Status: **observed** · Variants: PC

The UI manager builds its level catalogue (UiManager+0xC3F34, by 0x951480) from the levels' description partitions
`Levels/<LV>_description.dbx` in the startup bundle (LOAD-START-1): per level its map-name key, its mode name and its
mode key. The menus' level lists, the server browser's rows (NET-SB-1) and the match selector read it; a level that
is in no `LevelListQuery` item is in no menu (LEVEL-DESC-1).

[exe: BFBC2Game.exe catalogue builder 0x951480 into UiManager+0xC3F34; mode keys CONQUEST, RUSH, SQRUSH, SQDM]
[doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:131-133] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:849-851]

### MENU-EVT-1: The engine talks to the movies by events and extern getters

Status: **observed** · Variants: PC

The engine sends a menu event by acquiring one (0x920360), adding strings (0x588910) and values (0x588940) and queueing
it (0x9299B0); simple events go through 0x929A70; the movie receives them as `_root.gameEvent`. The movies ask the
engine through `extern.*` properties answered by one switch (case 0x95B316: `isLive`, `canEnterMultiplayer`,
`hasMultiplayerPermissions`, `controllerConnected`, `hasConquest`). `extern.isLive` means "running inside the engine":
false switches the movies to their authoring behaviour (raw localisation keys, mock data). Commands from a movie reach
the engine through the generic command entry (0x753540) and the main-menu dispatcher (NET-SES-1).

[exe: BFBC2Game.exe event acquire 0x920360, add string 0x588910, add value 0x588940, queue 0x9299B0, simple event 0x929A70; extern switch case 0x95B316; command entry 0x753540 -> 0x95E680]
[doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:110-125] [doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:584-585]

### MENU-DLG-1: A class defined in several movies is the first-loaded movie's

Status: **observed** · Variants: PC

`dice.Dialog` is defined in `main_root`, `main_nav` and `main_popups`; whichever movie loads first defines it for all.
A change to the dialog must be made in all three movies.

[data: dice.Dialog's definition found in main_root, main_nav and main_popups; OPFOR's dialog patches]
[doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:146-147]

### MENU-UNL-1: A sprite's frames pointer must be 0 when it has no frames (0x13046DA)

Status: **observed** · Variants: PC

When a movie is unloaded (0x1304690) the engine reads a first frame through every sprite's frames pointer that is
not 0. A sprite without frames must therefore keep the pointer at 0: an empty frames node written for it (a pointer
to the next node) crashed the menu unload at 0x13046DA (the Pacific War `main_nav`, 23 such sprites). The suite's
movie patcher attaches a frames node only when it adds frames.

[exe: BFBC2Game.exe movie unload 0x1304690, fault at 0x13046DA (the first Pacific War menu install, 2026-11-06)]
[code: bfbc2-menu-tools/aptgraft.py:1094-1104] [doc: opfor:research/WW2_MENU.md:5-8]

## Open questions

- Which movies load in which order (MENU-DLG-1's "first" is observed for `dice.Dialog` only).
- What else the unloader reads through non-null pointers (MENU-UNL-1 is the one case hit).
- Whether the catalogue is rebuilt when the game mod changes (GAME-MODE-6) or built once at start-up.
