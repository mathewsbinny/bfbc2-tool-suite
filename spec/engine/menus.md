# Menus: the APT runtime as the game uses it

What the game does with its menu movies at run time: the level catalogue the menus read, how the engine and the
movies talk, and what the movie loader and unloader require. The movie format is layer 1 (`formats/apt.md`, with the
loader's own rules APT-LOAD-1 to APT-LOAD-3); the commands that start a match are NET-SES-1; the server browser's rows
are NET-SB-1 and NET-SB-2.

**Sources.** Project OPFOR BFBC2's reverse engineering of the PC client (`private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md` section
5, build 795745), its menu installs (`bfo:docs/research/opfor/WW2_MENU.md`), and the suite's movie patcher (`bfbc2-menu-tools`,
`aptgraft.py`). Round SPG2-rest added the action script as DICE ships it (MENU-AS-*), measured by
`spec/probes/menus.py` over DICE's bundles of the game copy (`game/**/*.fbrb` dated before 2013, plus OPFOR's pristine
copy of the Package main menu; each distinct movie once), the interpreter read statically from `BFBC2Game.exe`
(MENU-RT-*), the event and extern paths (MENU-EVT-2, MENU-EVT-3), the HUD (MENU-HUD-*) and what a menu edit must keep
(MENU-ED-*). `checks()` in the probe recomputes the counts those rules cite.

### MENU-CAT-1: The level catalogue is built from the description partitions

Status: **observed** · Variants: PC

The UI manager builds its level catalogue (UiManager+0xC3F34, by 0x951480) from the levels' description partitions
`Levels/<LV>_description.dbx` in the startup bundle (LOAD-START-1): per level its map-name key, its mode name and its
mode key. The menus' level lists, the server browser's rows (NET-SB-1) and the match selector read it; a level that
is in no `LevelListQuery` item is in no menu (LEVEL-DESC-1).

[exe: BFBC2Game.exe catalogue builder 0x951480 into UiManager+0xC3F34; mode keys CONQUEST, RUSH, SQRUSH, SQDM]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:131-133] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:849-851]

### MENU-EVT-1: The engine talks to the movies by events and extern getters

Status: **observed** · Variants: PC

The engine sends a menu event by acquiring one (0x920360), adding strings (0x588910) and values (0x588940) and queueing
it (0x9299B0); simple events go through 0x929A70; the movie receives them as `_root.gameEvent`. The movies ask the
engine through `extern.*` properties answered by one switch (case 0x95B316: `isLive`, `canEnterMultiplayer`,
`hasMultiplayerPermissions`, `controllerConnected`, `hasConquest`). `extern.isLive` means "running inside the engine":
false switches the movies to their authoring behaviour (raw localisation keys, mock data). Commands from a movie reach
the engine through the generic command entry (0x753540) and the main-menu dispatcher (NET-SES-1).

[exe: BFBC2Game.exe event acquire 0x920360, add string 0x588910, add value 0x588940, queue 0x9299B0, simple event 0x929A70; extern switch case 0x95B316; command entry 0x753540 -> 0x95E680]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:110-125] [doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:584-585]

### MENU-DLG-1: A class defined in several movies is the first-loaded movie's

Status: **verified** · Variants: PC

`dice.Dialog` is defined in `main_root`, `main_nav` and `main_popups`; whichever movie registers it first defines it
for all (every class registration is guarded, MENU-AS-5). A change to the dialog must be made in all three movies.
(Corrected in round SPG2-rest: the three are the main menu's root and the two movies it loads itself, MENU-AS-8;
dice.Dialog is copied into 23 of DICE's 91 movies in all, 13 of them in the main-menu bundles - pages loaded later,
which find it defined - and 10 HUD, in-game and loading movies. The rule for every shared class is MENU-ED-1.)

[test: core/tests/test_spec_probes.py::menus]
[data: dice.Dialog's definition found in main_root, main_nav and main_popups; OPFOR's dialog patches]
[data: spec/probes/menus.py, 2026-11-22: __Packages.dice.Dialog exported by 23 of the 91 distinct movies]
[doc: private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md:146-147]

### MENU-UNL-1: A sprite's frames pointer must be 0 when it has no frames (0x13046DA)

Status: **observed** · Variants: PC

When a movie is unloaded (0x1304690) the engine reads a first frame through every sprite's frames pointer that is
not 0. A sprite without frames must therefore keep the pointer at 0: an empty frames node written for it (a pointer
to the next node) crashed the menu unload at 0x13046DA (the Pacific War `main_nav`, 23 such sprites). The suite's
movie patcher attaches a frames node only when it adds frames.

[exe: BFBC2Game.exe movie unload 0x1304690, fault at 0x13046DA (the first Pacific War menu install, 2026-11-06)]
[code: bfbc2-menu-tools/aptgraft.py:1094-1104] [doc: bfo:docs/research/opfor/WW2_MENU.md:7-10]

## The action script DICE ships

Counts are over the 91 distinct movies of DICE's bundles (each once, whatever the number of bundles holding it); "a
movie" is a movie name, its versions together. Identifiers only: opcode, class, property, event and movie names.

### MENU-AS-1: 91 distinct movies under 55 names, each with a UI asset

Status: **verified** · Variants: PC

DICE's bundles of the game copy hold 898 `SwfMovie` entries in 93 bundles: 91 distinct movies under 55 names (36 names
in two versions: the Dist and Package copies, or mp_common's and mp2_common's). Every movie has one `UI.UIAsset`
partition `Menu/<name>.dbx` (55) whose `SourceFile` is `Menu/<name>.swf`, except nam_hud_root's, which names
hud_root's (nam_hud_root was built from hud_root's source). Three of these partitions also hold a `UI.UIConfig`:
main_root's (with `UI.UIMenuConfig` and `UI.UIMovieData`) and hud_root's and nam_hud_root's (with `UI.UIHudConfig`
and the nametag list) - the root movies the engine starts itself (MENU-AS-8, MENU-HUD-1). The main-menu bundles hold
32 of the movies, among them all nine library movies (MENU-AS-6).

[test: core/tests/test_spec_probes.py::menus]
[data: spec/probes/menus.py, 2026-11-22: 898 entries in 93 DICE bundles, 91 distinct movies, 55 names; 55 UI.UIAsset partitions, 54 with their own source file; UI.UIConfig in main_root, hud_root, nam_hud_root]

### MENU-AS-2: Opcode frequencies

Status: **verified** · Variants: PC

The 91 movies decode to 1,854,963 instructions with 91 distinct opcodes and none unknown. The ten most frequent:
PushData (0x96) 320,102, EA_GetNamedMember (0xAF) 241,870, EA_PushConstant (0xA2) 178,923, SetMember (0x4F) 87,505,
EA_PushValueOfVar (0xAE) 82,381, Not (0x12) 67,947, BranchIfTrue (0x9D) 63,778, Pop (0x17) 59,574, EA_PushByte (0xB5)
58,773, EA_PushOne (0x5A) 57,795. The rarest: Or (0x11) 3, RemoveSprite (0x25) 8, StringAdd (0x21) 10, RandomNumber
(0x30) 12, SetProperty (0x23) 18. There are 33,500 function definitions: 28,108 DefineFunction2, 5,392
DefineFunction. Of the opcodes the suite names (`apt.NAMES`), 36 never occur: 04 05 0A 0E 0F 10 13 14 20 24 27 28 29
2A 2C 3B 45 46 4D 56 57 58 5C 62 65 68 8A 8B 8D 8F 94 9E A7 B8 B9 - among them EA_BranchIfFalse (0xB8: DICE's compiler
branches with Not and BranchIfTrue) and EA_PushRegister (0xB9: registers are read through PushData, MENU-AS-4).

[test: core/tests/test_spec_probes.py::menus]
[data: spec/probes/menus.py, 2026-11-22: 1,854,963 instructions, 91 opcodes, 0 unknown; DefineFunction 5,392, DefineFunction2 28,108; per-opcode counts in the report]

### MENU-AS-3: Constants and constant pools

Status: **verified** · Variants: PC

The movies' `.const` tables hold 480,628 entries: 160,526 strings and 320,102 registers, no other type (APT-CONST-3).
There are 3,563 ConstantPool instructions; pool sizes have a median of 19 and a maximum of 804, and 64 pools have more
than 256 entries, past the reach of the u8 pool forms (0xA2, 0xAE, 0xAF, 0xB0 - 0xB3): the u16 form EA_PushWordConstant
(0xA3) occurs 27,247 times. Strings are otherwise pushed by EA_PushString (0xA1, an inline pointer, 18,886 times).

[test: core/tests/test_spec_probes.py::menus]
[data: spec/probes/menus.py, 2026-11-22: const entries 160,526 string and 320,102 register; 3,563 ConstantPool, largest 804; 27,247 EA_PushWordConstant]

### MENU-AS-4: In DICE's movies a PushData reads one register

Status: **verified** · Variants: PC

Each of the 320,102 PushData instructions has a list of exactly one item, every item is a register constant (type 4),
and every register constant is the item of exactly one PushData: the 320,102 register constants and the 320,102
PushData pair one to one. DICE's compiler uses PushData only to read a DefineFunction2 register, never to push a
string. APT-LOAD-2 (one PushData per list) therefore holds in DICE's movies by construction; a writer that adds a
register read adds a constant and a list of its own (the suite's model copies, APT-LOAD-2), and OPFOR's in-place
patches re-point a list's one entry at another register constant rather than share a list.

[test: core/tests/test_spec_probes.py::menus]
[data: spec/probes/menus.py, 2026-11-22: PushData lists of more than one item 0; register items 320,102; register constants used by other than exactly one PushData 0]
[doc: opfor:src/menus/main_root.patch:14]

### MENU-AS-5: Classes are `__Packages` exports registered once, guarded

Status: **verified** · Variants: PC

An ActionScript 2 class is a sprite exported as `__Packages.<package>.<Class>` whose init action (APT-FRM-1, item type
8) defines it. The 91 movies define 53 classes: 22 under `dice` (dice.Dialog, dice.EventManager, dice.ImageManager,
dice.CrossHair ...), 18 under `se.river.rome` (the menu framework: MainMenu, PageHandler, backend.Engine ...), 8 under
`caurina.transitions` (the Tweener library), 2 under `mx`, and `ExternalTools`, `dicecomponents.TextManager` and
`extern`. All 1,285 registrations start with the guard `if (!_global.<package>.<Class>)` (the class's name read, Not,
Not, BranchIfTrue over the definition), creating missing packages on the way: within one `_global`, the first movie to
run a class's init action defines it, and later copies leave it alone. Classes are copied widely: dice.WeakRefFrameUpdate
into 38 movies, dice.UISound 37, each caurina class 35, dice.ImageManager 34, se.river.rome.CommonUtils 32,
dice.EventDispatcher and dice.Joystick 26, dice.Dialog 23, se.river.rome.backend.Engine 11, dice.EventManager 8.

[test: core/tests/test_spec_probes.py::menus]
[data: spec/probes/menus.py, 2026-11-22: 53 classes; 1,285 registrations guarded, 0 unguarded; dice.Dialog in 23 movies]

### MENU-AS-6: Imports name nine library movies

Status: **verified** · Variants: PC

Movies share characters by import (APT-MOV-1) from nine library movies: textmanagercomponent (its one symbol, the
TextManager text component, imported by 47 movies), sharedgraphics (23 symbols, 32 movies), weaponicon (6 symbols, 15
movies), menutextures_shared (2, 4), ranktextures_shared (2, 3), bartextures_shared (6, 2), platform_buttons (1, 2),
insignias and pins (1 each, mp_awards). Every import names a symbol its source movie exports under that name. Import
names differ in case from the resource names (`TextManagerComponent`, `sharedGraphics`), so the runtime matches them
without case (inferred), and an importing movie needs its library loaded where it runs; the main-menu bundles hold all
nine.

[test: core/tests/test_spec_probes.py::menus]
[data: spec/probes/menus.py, 2026-11-22: 9 import sources; imports naming a symbol the source does not export: none]

### MENU-AS-7: The engine names the movies use

Status: **verified** · Variants: PC

By the suite's aptrefs scan the movies read 198 `extern.<name>` properties, 15 parameterised ones (`<name>$<param>`),
write 2 (`chId` and `chZA`, hud_crosshair), send 133 FSCommands and handle 148 engine events (`gameEventListener.<name>`
in the root movies, `_root.catchEvent` in the HUD layers, MENU-EVT-2). Most names occur in the 11 movies that define
se.river.rome.backend.Engine, the class that wraps the externs and FSCommands for the pages, whether or not the movie's
own code uses them. Against the executable's name -> id table (569 distinct names, APT-LOAD-3), 9 extern names are
missing (EORWasAttacker, EORWasWinning, deathLocalPlayerMessage, eulaOS, eulaPunkBuster, getSlice,
levelList$singleplayer, popupElorank, popupRankup) and 6 FSCommands (assert, clearLocalDeathMessages, g, loadTexture,
playSound, unloadTexture).

[test: core/tests/test_spec_probes.py::menus]
[data: spec/probes/menus.py, 2026-11-22: 198 externs, 15 parameterised, writes chId and chZA, 133 FSCommands, 148 events; names missing from the exe table: 9 extern, 0 parameterised, 6 FSCommand]
[code: bfbc2-menu-tools/aptrefslib.py:105-177]

### MENU-AS-8: Which movie loads which

Status: **verified** · Variants: PC

By literal name: main_root loads main_nav.swf into `menuMC` and holds main_popups.swf (loaded into `popupsMC`);
main_nav names its 12 pages (add_friend, compare, devmenu, leaderboards, mp_stats, mp_your_stats, rank_unlock,
server_browser, settings, sp_collect, sp_my_campaign, wag) and mp_stats its 4 (mp_awards, mp_dog_tags, mp_vehicle_stats,
mp_your_stats). In game igm_mp loads mp_top_menu, which loads mp_objectives, settings, eor_scoreboard and
eor_squad_ranking; igm_sp names sp_objectives and sp_flynnscrate; loading_mp loads eor and loadingscreen_mp; eor loads
loadingscreen_mp, eor_scoreboard and eor_squad_ranking; hud_root and nam_hud_root load hud_popups.swf into `popupsMC`
and the four layers the engine names (MENU-HUD-2). Every `<name>.swf` constant names a movie of the corpus. Call sites:
loadMovie 50 (the rest of the targets computed), loadClip 57, unloadMovie 40, attachMovie 796, createEmptyMovieClip
241, removeMovieClip 117, duplicateMovieClip 2. loadClip occurs in exactly the 34 movies that define dice.ImageManager,
the same 34 that send the loadTexture / unloadTexture FSCommands and handle the asyncF event. So the load order is: the
engine starts a root (main_root, the mode's HUD root, a loading root), the root loads its children, children load
their pages when opened; library movies come with the bundles.

[test: core/tests/test_spec_probes.py::menus]
[data: spec/probes/menus.py, 2026-11-22: <name>.swf constants all name corpus movies; loadMovie literal targets per movie; loadClip, dice.ImageManager, loadTexture and asyncF in the same 34 movies]
[inferred: dice.ImageManager asks the engine for a texture by FSCommand, waits for asyncF and loads it with a MovieClipLoader: the four always occur together]

### MENU-LOAD-1: A level's loading picture is a Bink video its description names

Status: **verified** · Variants: PC

`LevelData.LevelDescription` carries the loading screen's video: `LoadingMovie` (+0x100, a reference to a
`Render.BinkMemoryAsset` in the level's own `Levels/<level>/UI/` folder: `MP05_CQ`, `MP05_Rush` ... per mode, or
`LoadingMovie` for the campaign), beside `LoadingScreens` (an array), `LoadingMovieLoopBeginFrame` / `-EndFrame`,
`LoadingTime` and `Music` (the type information also lists `LoadingMovieTopLeft` / `-Dimensions` there, at offsets that
suggest a nested structure). 40 of DICE's
74 level partitions set it, every one inside its level's UI folder; the 34 that do not are Vietnam's, BC1's, the
`mp_sp_*` levels' and SP_01. So the "loading" a level names is a video, not the Flash movie that frames it: the
`loading_mp` / `loading_sp` / `loading_sp01` roots of MENU-AS-8 are named by the level's game mode (MENU-LOAD-2,
round SPG2-rest5). The one executable test on a movie's name is the movie object's constructor (0xFDC780 -> 0xFDBE30):
a movie whose name contains `_loading` gets one extra call on its player at creation (vtable +0x20 with 0), which no
other movie gets. A screen names its root through a virtual (MENU-LOAD-2), and the loading screens are told to start by
the events `onStartLoading` / `onStartLoadingSP` (0x7373A0 picks one by a flag; also sent at 0x77BDEB, 0x788D44 and
0x9523D9) through the root's `gameEvent` (MENU-EVT-2).

[test: core/tests/test_spec_probes.py::menus]
[exe: BFBC2Game.exe 0x7373A0 ("onStartLoading" 0x142A71C, "onStartLoadingSP" 0x142A708, sent through 0x929A70)]
[data: spec/probes/menus.py, 2026-11-22: LoadingMovie set on 40 of 74 DICE level partitions, all under Levels/<level>/UI/; the two read by hand (MP05_CQ, SP_02's LoadingMovie) are Render.BinkMemoryAsset with a BinkMemory resource]
[exe: BFBC2Game.exe LevelDescription's field descriptors 0x1C23758 - 0x1C23848 (Name +0xF4, Description +0xE8, LoadingScreens +0x104, LoadingMovie +0x100, LoadingMovieTopLeft / LoadingMovieDimensions +0x8 / +0x0, LoopBeginFrame +0xD4, LoopEndFrame +0xD0, AttackerTeam +0xE4, LoadingTime +0xD8, Music +0x114); 0xFDBE30 (substring test 0x50E8E0 with "_loading" 0x14B2710)]

### MENU-LOAD-2: A screen's root is a virtual; the loading and HUD roots are the game mode's LoaderName and HudRootName

Status: **verified** · Variants: PC

The UI's screens are objects with several bases; slot 4 (+0x10) of a screen's main vtable returns its root movie's
name and slot 5 (+0x14) the level it loads into (`_level0`). Three screen classes have it:

| Screen | Main vtable | Root name |
|---|---|---|
| main menu | 0x142B4E8 | `main_root`, a constant (0x753660) |
| loading screen | 0x142B210 | the current level's `GameModeData.LoaderName` (0x74EA90) |
| HUD | 0x142B7A0 | the current level's `GameModeData.HudRootName` (0x74D860) |

The level's game mode comes from 0x920750: in a running level its `LevelData.LevelDescription`, before one the
`<level>_description` partition's description; the root slots read its +0x20, `GameMode`. DICE's ten `GameModeData`
name the loader `loading_mp` (the eight multiplayer modes, Vietnam's included), `loading_sp` (`SinglePlayer`) and
`loading_sp01` (`SingelPlayerFirstLevel`, DICE's spelling), all movies of the corpus. So a new mode picks its loading
movie by `LoaderName` as it picks its HUD by `HudRootName` (MENU-HUD-1), and an Onslaught-style mode needs no code for
either. Round SPG2-rest4's "slot 12 of vtable 0x142B4C8" is the same function seen through the main menu's third
vtable (0x142B4C8 is the object's +0x30 base; the main vtable 0x142B4E8 is at +0).

[test: core/tests/test_spec_probes.py::menus]
[data: spec/probes/menus.py, round SPG2-rest5 2026-11-27: LoaderName of the 10 DICE GameModeData (loading_mp 8, loading_sp 1, loading_sp01 1), all movie names of the corpus; the three root slots' bytes and the vtables' slot 4]
[exe: BFBC2Game.exe vtables 0x142B4E8 (+0x10 0x753660 `push 0x142AEA0` "main_root"; set at 0x77334B with 0x142B4D4 at +0x2C and 0x142B4C8 at +0x30), 0x142B210 (+0x10 0x74EA90; set at 0x75F627), 0x142B7A0 (+0x10 0x74D860; set at 0x782E98); 0x74EA90 / 0x74D860: 0x920750, then +0x20 (GameMode), then +0x30 / +0x90 (GameModeData's LoaderName / HudRootName descriptors); 0x920750 (in a level 0x7FF060 -> +0x38 -> +0x44 -> +0x84, LevelData's LevelDescription; else "_description" 0x1425AC0 -> the asset's +0x18); the "_level0" slots 0x753690, 0x753510, 0x751B70]

## The interpreter

### MENU-RT-1: One opcode byte, a 185-entry handler table, no bound check

Status: **verified** · Variants: PC

The action interpreter (0x1310F90) runs a stream through a context holding the program counter: it reads the opcode
byte (0x1311041), steps past it and calls `handler(vm, &context)` from the table at 0x1545B00 (0x1311056) with no
bound check. The table has 185 entries, opcodes 0x00 - 0xB8. A handler reads its own operands and advances the counter,
so APT-CODE-2's layouts are the handlers'. End (0x00, handler 0x13043E0) sets the context's stop flag; a top-level
stream runs until the flag is set, a function body also stops when its next opcode would be fetched past its end.

[test: core/tests/test_spec_probes.py::menus]
[exe: BFBC2Game.exe build 795745: interpreter 0x1310F90; movzx of the opcode at 0x1311041; mov edx, dword (edi*4 + 0x1545B00) at 0x1311056, call edx at 0x1311063; End handler 0x13043E0 writes context +0x14]
[data: spec/probes/menus.py, 2026-11-22: the dispatch bytes at 0x1311041 and 0x1311056 as recorded; 185 table entries before the first word that is neither 0 nor code]

### MENU-RT-2: 132 opcodes have a handler; no other may run

Status: **verified** · Variants: PC

132 of the 185 entries hold a handler; 53 hold 0: 01 - 03, 16, 19 - 1B, 1E, 1F, 2D - 2F, 38, 39, 57, 5F, 68, 6A - 6F,
78 - 80, 82, 84 - 86, 89, 8D, 90 - 93, 95, 97, 98, 9C, A0, A8 - AD. Executing an opcode whose entry is 0, or one at or
above 0xB9 (past the table, into the data after it), calls a bad address: no such byte may be executed. Every opcode
of DICE's movies has a handler. Against the suite's names (`apt.NAMES`): EA_PushGlobal (0x57), StringGreater (0x68),
WaitForFrame2 (0x8D) and EA_PushRegister (0xB9) have no handler; 08, 09, 31 - 33, 35 - 37 (Flash's ToggleQuality,
StopSounds and multibyte string opcodes), 5E and 77 have handlers and no suite name; Try (0x8F) and CallFrame (0x9E)
have handlers and no operand layout in the suite's reader.

[test: core/tests/test_spec_probes.py::menus]
[exe: BFBC2Game.exe build 795745: handler table 0x1545B00, 185 entries, 53 of them 0]
[data: spec/probes/menus.py, 2026-11-22: 132 opcodes with a handler; opcodes DICE's movies use without one: none]

### MENU-RT-3: The handlers read the operands as the suite does

Status: **observed** · Variants: PC

GetURL (0x83, 0x131F5A0) aligns the counter to 4 and reads two string pointers; GetURL2 (0x9A, 0x131F660) reads no
operand and takes URL and target from the stack; GotoFrame2 (0x9F, 0x1314B40) aligns and reads 4 bytes;
EA_PushWordConstant (0xA3, 0x130D200) reads 2 unaligned bytes as an index into the VM's current constant pool (VM
+0x44); EA_PushShort (0xB6, 0x130D2E0) reads 2 unaligned bytes as a signed value. These are APT-CODE-2, APT-CODE-3 and
APT-CODE-6 as the PC runtime reads them.

[exe: BFBC2Game.exe build 795745: handlers 0x131F5A0 (GetURL), 0x131F660 (GetURL2), 0x1314B40 (GotoFrame2), 0x130D200 (EA_PushWordConstant, pool at VM +0x44), 0x130D2E0 (EA_PushShort)]

### MENU-RT-4: FSCommands and movie loads go through GetURL and GetURL2

Status: **verified** · Variants: PC

Both handlers compare the start of the URL with the prefix the pointer at 0x1545AA4 names (`FSCommand:`, 0x14CE540;
a byte compare of its 10 characters, so the case matters). On a match the rest of the URL (the command name) and the
target (its argument) go to the host's FSCommand callback (the pointer at 0x1BEAAF8); otherwise a URL ending in `.swf`
(0x14CF31C) is loaded into the target as a movie (0x131F160). DICE's movies send FSCommands both ways: GetURL with a
constant `FSCommand:<name>` (2,270 instructions) and GetURL2 with the name pushed (1,921).

[test: core/tests/test_spec_probes.py::menus]
[exe: BFBC2Game.exe build 795745: prefix pointer 0x1545AA4 -> 0x14CE540; FSCommand callback pointer 0x1BEAAF8; ".swf" 0x14CF31C; movie load 0x131F160]
[data: spec/probes/menus.py, 2026-11-22: the prefix as read is 10 characters; GetURL 2,270 and GetURL2 1,921 instructions]

### MENU-RT-5: The live display tree

Status: **implemented** · Variants: PC

The current Apt target is at 0x1BE9C5C, its level item at target +0x94. A display item holds its name at +0x08 (u16
length at +2, the text at +8, not terminated), its previous and next sibling at +0x14 / +0x18, its parent at +0x1C
and its character instance at +0x20. The instance's render item (+0x04) holds the character data (+0x04: the type
first, a shape's bounds at +8), the matrix (+0x08: 6 floats, 0 = identity) and the flags (+0x18, bit 0 visible); a
movie's children start at instance +0x60, a sprite's at +0x58. main_nav's root `menuMC` is [[level +0x20] +0x60].
OPFOR's pad reads this tree for its focus ring, every read guarded: the tree changes while the menus run.

[code: opfor:src/dll/menutree.cpp:1-25] [code: opfor:src/dll/menutree.cpp:112-118]

### MENU-RT-6: The FSCommand callback answers ten commands itself and passes the rest to the screen

Status: **verified** · Variants: PC

The UI's start-up (0xFA6BC0) installs the host callback MENU-RT-4 calls (the pointer at 0x1BEAAF8 := 0xFA5380) and
fills its command table, a map at UiManager+0x82418 from a name to a number: `loadTexture` 0, `unloadTexture` 1,
`bind3D` 2, `load3D` 3, `unload3D` 4, `pop3D` 5, `setInputRoot` 6, `assert` 7, `quitToMenu` 8, `playSound` 9. The
callback (0xFA5380 -> 0xFA4930, the UiManager from 0x19A4CA8) looks the command up:

- not in the table: the current screen (UiManager+0x14) gets it, its vtable +0x30 with the command and its argument;
  so every game FSCommand of the movies (MENU-AS-7's 133) is the screen's to answer;
- `loadTexture` (0): the argument is split at commas and must give two parts; unless the second starts with `!`, the
  screen's texture loader 0xFA3FF0 gets both;
- `unloadTexture` (1): one part, to 0xF9E5A0;
- `setInputRoot` (6): the argument, or 0 for `void`, to 0x1317610;
- `playSound` (9): the argument to the sound 0xF909C0;
- `bind3D`, `load3D`, `unload3D`, `pop3D`, `assert` and `quitToMenu` (2 - 5, 7, 8): nothing (the switch's empty case).

This answers four of the six FSCommands MENU-AS-7 found missing from the name table (`assert`, `loadTexture`,
`playSound`, `unloadTexture`): they are the callback's, not the screens'. The other two (`clearLocalDeathMessages`,
`g`) reach the screen like any other.

[test: core/tests/test_spec_probes.py::menus]
[data: spec/probes/menus.py, 2026-11-23: the callback's install bytes at 0xFA71AD; its table as the start-up's code gives it (ten names and numbers); 4 of the 6 FSCommands missing from the name table are in it]
[exe: BFBC2Game.exe start-up 0xFA6BC0 (`mov [0x1BEAAF8], 0xFA5380` at 0xFA71AD; the names 0x14AF4F8 - 0x14AF560 pushed and their numbers stored 0xFA6C27 - 0xFA7045, insert 0xFA23F0); callback 0xFA5380 -> 0xFA4930 (lookup 0xF8B6D0 in UiManager+0x82418, number at node +0x1C, table 0xFA4B4C; the screen's vtable +0x30 at 0xFA4B3E; the comma 0x140EB9C)]

### MENU-RT-7: Each screen answers its own FSCommands; the rest go to the UI manager's shared handler

Status: **verified** · Variants: PC

A command the callback does not take (MENU-RT-6) goes to the current screen's slot 12 (+0x30). The handler turns the
command into its id with the UI manager's name table (0x929C40, the table of APT-LOAD-3) and switches on it; an id its
switch does not take goes on to the UI manager's shared handler 0x95E680, which switches again:

| Handler | Ids | DICE's FSCommands it answers |
|---|---|---|
| main menu 0x77BC10 | 7 - 0xE4 | 28 (the front end: `startSingleplayer`, `hostMultiplayer`, `startFindServer`, `quitgame`, `redeemCode` ...) |
| HUD 0x7764F0 | 1 - 0xBB | 29 (in game: `doSpawn`, `selectKit`, `selectSpawnPoint`, `suicide`, `switchTeam`, `joinSquad`, `enterMenu` ...) |
| loading screen 0x753540 | none | passes every id but 0 (unknown) and 0xF (`startLoading`) on |
| shared 0x95E680 | 0xB - 0xE2 | 69 (settings, persona, text boxes, joining, end of round ...) |

Of the movies' 133 FSCommands, 125 are answered by a handler (one, in both the main menu's and the shared, by the
main menu's), 4 by the callback (MENU-RT-6) and 4 by nothing: `cancelJoinServer`, `clearLocalDeathMessages`, `g` and
`showAchivementBlade` go through the chain and are dropped. A menu edit that sends a new FSCommand needs a handler
case for it, which DICE's handlers do not have.

[test: core/tests/test_spec_probes.py::menus]
[data: spec/probes/menus.py, round SPG2-rest5 2026-11-27: the handlers' switch maps against the name table and the movies' 133 FSCommands: main menu 28, HUD 29, shared 69; answered by none: cancelJoinServer, clearLocalDeathMessages, g, showAchivementBlade]
[exe: BFBC2Game.exe the callback's screen call 0xFA4B30 - 0xFA4B3E (UiManager +0x14, vtable +0x30); main menu 0x77BC10 (0x929C40, id - 7 against 0xDD, byte map 0x77D964, jumps 0x77D8B0, default 0x77D88C -> 0x95E680); HUD 0x7764F0 (id - 1 against 0xBA, 0x7772F4, 0x777234, default 0x777210 -> 0x95E680); loading 0x753540 (0x929C40, ids 0 and 0xF dropped, else 0x95E680 at 0x7535AD); shared 0x95E680 (id - 0xB against 0xD7, 0x963F24, 0x963DA4, default 0x963D97)]

### MENU-RT-8: What each FSCommand case does

Status: **observed** · Variants: PC

Round SPG2-rest6 read the 125 cases MENU-RT-7 maps (static reading: a case's first branch and its callees two deep).
The argument arrives as one string; a case that needs several splits it at commas (0x5175F0 with 0x140EB9C) and
converts parts with the C runtime's `atoi` / `atof` (the imports 0x14054FC / 0x14054EC). The words below:

- **client**: the object 0x7FF060 returns (one static instance at 0x15713D8); its +0x58 is the network connection the
  connect code makes (0x656403 - 0x65644B: 0x1900 bytes, 0x5585A0), its +0x18 a dispatcher (vtable +0x18 / +0x20 /
  +0x24);
- **msg N**: a client message (0x10 - 0x40 bytes from 0x420DB0) whose vtable +0x08 returns the id N, appended to the
  connection's queue +0x1E0 (0xC0A390) for the server;
- **post**: an object handed to the dispatcher (client +0x18, vtable +0x24) with the number 0x25 and the delay 0.0:
  a server-browser job (0x1B0 bytes, vtable 0x141C048, its kind 0 - 4 at +0x28), a presence (account) request
  (0x128 bytes, 0x5963D0), a friends request (0xE90 bytes, 0x5E8300);
- **event E**: `_root.gameEvent(E)` sent at once (0x929A70) or queued (0x9299B0; MENU-EVT-2, 4);
- **load**: a level-load request (0x75DF60, from the arguments) handed to the UI manager (0x956E10 -> 0x954CA0) and
  started (0x75EAB0);
- **UI +x**: a field of the UI manager (the object at 0x19A4CA8).

HUD 0x7764F0 (the HUD screen in `esi`):

| Command | Case | Action |
|---|---|---|
| `selectSpawnPoint` | 0x776613 | int -> 0x76DDA0: HUD +0x271F4 = the point, +0x271F8 = -1; looks up the `ClientSpectateEntity`, `ClientCapturePointEntity` and `ClientSoldierSpawnEntity` entities for the spawn camera |
| `selectSquadSpawn` | 0x776639 | int -> 0x772710: HUD +0x271F8 = the squad mate, +0x271F4 = -1; the spectate camera on him |
| `doSpawn` | 0x77665F | 0x764020: the kit's configuration and the spawn choice into msgs 0x23 and 0x19 |
| `selectKit` | 0x7766CB | int -> 0x740A80: HUD +0x271F0 = the kit, after checking the local player |
| `enableGameInput` | 0x7766F1 | int -> 0x7489B0 -> 0x748970 (the input actions 0x42 - 0x4C) |
| `doExitLevel` | 0x77671C | the UI state (0x8FEE20 with 1, 1); 0x75EDD0 (a load request, 0x5ECF60 / 0x956E10); the dispatcher's vtable +0x18 with (0x1B, 0x35, 0) |
| `doRestartLevel`, `doLastSavePoint` | 0x77674F, 0x776763 | 0x75EE80 / 0x75EF20: a load request each |
| `addEvent`, `removeEvent` | 0x7767A9, 0x776824 | numbers (`atof`) -> 0x771EC0 (append to a HUD list); 0x763700 (take off) |
| `activateMission` | 0x7769B5 | int -> 0x74DCD0: HUD +0xBF0 = the mission; the UI's ids 0x58 and 0x57 looked up (0x919120, UI +0x82C64) |
| `enableMinimap`, `zoomMinimap` | 0x776A06, 0x776570 | int -> HUD byte +0x27B31; two ints -> 0x92BCF0 (the minimap's zoom, event `minimapZoomDone`) |
| `selectWeapon`, `selectGadget` | 0x776A2F, 0x776AB4 | two ints (slot, item) -> 0x7644C0 / 0x764540: the kit configuration's slot |
| `clearKitConfiguration` | 0x776B36 | 0x749710: both configuration lists emptied |
| `suicide` | 0x776B77 | 0x7329C0: client +0x58 -> 0x537560 (its +0x14's vtable +0x80 with 0xD) |
| `enterMenu` | 0x776BA3 | int -> 0x73F9C0: the in-game menu (UI byte +0xC3F2A cleared) |
| `setVignette` | 0x776BE6 | int, float -> 0x9347B0 |
| `dequeueHudStates` | 0x776C8E | HUD byte +0x272BC = 1 |
| `skipCutscene` | 0x776CBA | 0x73FCD0: the dispatcher's vtable +0x20 with (0x12, 0x1B, 0, 0.0) |
| `selectFlynnCrateWeapon`, `leavingFlynnCrateScreen` | 0x776CF2, 0x776D72 | two ints -> msg 0x24; msg 0x25 |
| `switchTeam`, `SQDMSwitchTeam` | 0x776D9E, 0x776DB9 | msg 0x1E (`SQDMSwitchTeam` takes the team from its argument) |
| `joinSquad`, `leaveSquad`, `kickPlayer` | 0x776E4B, 0x776F01, 0x776F19 | msg 0x1D; client +0x58 -> 0x537810 (msg 0x1A); 0x900630 (msg 0x1B) |
| `setSquadFilter` | 0x7770CC | int -> 0x900700 (msg 0x22) |

Main menu 0x77BC10 (the main menu screen in `edi`):

| Command | Case | Action |
|---|---|---|
| `startSingleplayer` | 0x77BC93 | event `onStartLoadingSP`; load (the level from the arguments, a number by `atoi`) |
| `hostMultiplayer` | 0x77BEC3 | event `onStartLoading`; load of `Levels/` + the first argument; the request's local flag set when the id is 0xA (one case serves both ids) |
| `continueSP`, `newSP` | 0x77C135, 0x77C149 | 0x75FFA0 (the save's `body`, event `onStartLoadingSP`, load); int -> 0x7600F0 (event `onStartLoadingSP`, the `Client.Level` setting, load) |
| `skipMovie`, `startBackgroundMovie` | 0x77BFCF, 0x77D625 | the `SplashMoviesPlayed` option, the next splash movie or event `attractionEnd` (0x743A30); 0x753740 (the menu music) |
| `startPressed`, `legalDone` | 0x77C0BC, 0x77C0A2 | 0x743970 (input, the dispatcher's vtable +0x18 with (0x1B, 0x34), UI byte +0xC3FA1 = 0); UI byte +0x82CCC = 1 |
| `quitgame` | 0x77C11E | the global 0x155CFEC = 1 |
| `setCreditsRow` | 0x77C040 | int -> screen +0x1AC; the UI's ids 0x98 and 0x99 looked up (0x919120) |
| `requestLBList`, `requestFLList` | 0x77C16F, 0x77C20E | two ints -> 0x928E50 / 0x928F80: a leaderboard / friends-list stats query (UI +0x2F2C / +0x2F30) |
| `requestFriendStats`, `requestDetailedFriendStats` | 0x77C2AD, 0x77C330 | the name stored; 0x92CFF0 (a stats query for one player) |
| `requestDogTags` | 0x77D584 | two ints -> 0x7580A0 (a dog-tag query, event `DogTagsRequest`) |
| `startFindServer`, `cancelFindServer` | 0x77C4AA, 0x77CC24 | the server-browser filter (`any`, `levels/any` by default; 0x5F7EF0, 0x6E8650); post a job of kind 2 (0x775B00) |
| `startQuickMatch`, `postGetServers` | 0x77CD51, 0x77CF1D | post a job (0x6012F0, kind 0); the arguments parsed into the browser (0x5F7EF0) |
| `redeemCode`, `redeemCodeNam` | 0x77D2F0, 0x77D261 | screen byte +0x1D4C = 0 / 1; post a 0x48-byte code request (0x757A00) |
| `requestInputString`, `setInputInfo` | 0x77D37E, 0x77D4A8 | the on-screen keyboard's text request (0x908BA0); its info strings (0xF96F90) |
| `configTosFormatting`, `loadEulaFonts`, `unloadEulaFonts` | 0x77D639, 0x77D800, 0x77D818 | the terms text's styles (0x932260); the fonts per language loaded (0x92CC50) / released (0x90BA80) |
| `transitionTo` | 0x77D661 | 0x932E70: the screen's transition (event `doMenuTransition`) |
| `changeGameMod` | 0x77D866 | int -> 0x75FCB0 -> 0x920C00: UI +0xC8C90 = the game mod, a presence request (+0x2C = 2) posted, UI byte +0xC8BAA = 1 (GAME-MODE-6) |

Shared 0x95E680 (the UI manager in `ebp`):

| Command | Case | Action |
|---|---|---|
| `transitionTo` | 0x962C30 | event `doMenuTransition` (the main menu's own case answers it first) |
| `quickUpdateServerList`, `startServerListMatch`, `lookupServer`, `lookupCurrentServer` | 0x95EF2D, 0x95EC9C, 0x95EDFD, 0x95EED2 | post a job of kind 1 / 3 (UI byte +0xC898C = 1; the password `SERVER_PASSWORD_UNDEFINED` unless given) / 4 / 4 (UI +0xC8BA0 = the job) |
| `addFavoriteGame`, `removeFavoriteGame` | 0x960CFB, 0x960DC2 | post a job of kind 4 (UI +0xC8BA4, byte +0xC8BA8 = 1); post a 0x120-byte request (0x5ED860) |
| `joinWithServerPassword`, `leaveQueue`, `joinSession` | 0x95F17D, 0x95F2D2, 0x962BA4 | the server banner (0x9235E0, event `serverBannerIsReady`) and a 0xA0-byte join (0x551480); post 0x59EA50; the session list (UI +0xC5E90, 0x24-byte entries) searched by the argument, then post 0x928980 |
| `sortServerList`, `svSetKey`, `svSetValue` | 0x962828, 0x962A02, 0x9627E1 | two ints -> 0x95DCE0; the browser's filter key; its value (0x9208F0) |
| `serverDropDownSettings`, `serverFilterSettings` | 0x963B7E, 0x963C42 | the profile options `ServerDropDownSettings` / `ServerFilterSettings` set (0x82FCF0), the profile saved (0x9205A0, `PROF_SAVE`) |
| `presenceRequestOnline`, `presenceAcceptTos`, `presencePrepareCreateAccount`, `presenceRestartLogin` | 0x95F300, 0x95F3A3, 0x95F8FC, 0x95FE64 | post a presence request (0x59A5C0, then UI byte +0xC8BAA = 1; 0x91F8F0; 0x91F830; 0x91F7F0) |
| `presenceCreateAccount`, `presenceLoginAccount` | 0x95F3EA, 0x95F734 | the fields (ints and strings) into a presence request (0x9286F0 for the login) |
| `presenceSelectCountry`, `presenceEntitleGame`, `presenceCancelLogin` | 0x95FCF4, 0x95FE06, 0x95FE92 | post a presence request (0x928840 with the country, a default for `undefined`; 0x928880 with 0xC); UI +0xC8C90, the `AutoLogin` option cleared, the profile saved, post 0x91F810 |
| `createPersona`, `disablePersona`, `loginPersona` | 0x95FBD3, 0x95FC31, 0x95F98E | post a presence request with 0xA / 0xB (0x9287C0 / 0x928800); the persona and clan tag (`UNDEFINED_CLANTAG` by default) into 0x59EDD0, profile options (0x82FCF0) |
| `spawnCreatePersona`, `spawnLoginPersona`, `spawnDisablePersona` | 0x95F92A, 0x95F95C, 0x95FC92 | queued event `presenceCreatePersona` / `presenceLoginPersona` / `spawnDisablePersona` |
| `searchFriend`, `removeFriendFromList`, `pcrequestFriend`, `pcAcceptFriendsRequest`, `pcDeclineFriendsRequest` | 0x95EBF2, 0x95E86E, 0x96020E, 0x96039A, 0x96047F | post a friends request (0x937D20, 0x937D70, 0x7D6BF0, 0x937C50 with 0xD, 0x937C90); the `pc` ones only with client +0x54 set |
| `pcGetMessages`, `messages`, `checkPendingFriendRequest` | 0x9601B4, 0x962B50, 0x963CF5 | post a 0x30-byte message request (0x5B7180); post 0x937BD0; queued event `friendsRequestReceived` when one is pending |
| `inGameJoinSquadAccept`, `exitGameFromEOR`, `enterInGame` | 0x960580, 0x960618, 0x960C47 | UI +0xC3EFC (the UI state), 0x9120F0, 0x784EE0; the same, then load and the dispatcher's vtable +0x18 with (0x1B, 0x35); the `autoLookForSquad` option read, load |
| `setKeyboardUI`, `setMouseUI`, `enableAnalog` | 0x9606B2, 0x9606DB, 0x9609AF | int -> the input object at 0x1570E90 (0x7B8CF0 / 0x7B8D10); UI byte +0xC3F00, events `leftAnalog`, `rightAnalog` |
| `createTextBox`, `setTextBoxFocus`, `executeTextBox`, `clearTextBox` | 0x960704 (one case) | 0x93EBA0, which switches on the id again |
| `setSetting`, `resetSettings`, `applySettings`, `saveSettings`, `setRecomendedSettings`, `loadGfx` | 0x9607C5, 0x9608AF, 0x9608F0, 0x960973, 0x960934, 0x961084 | the `GstGraphics` settings: one set (0x929D30), all reset (0x920430), applied (0x920450), UI byte +0xC38D1 = 1, the recommended ones (0x7C4A80, 0x7BBE90, 0x7DD1F0), each option's value into the movie (`Shadows`, `Effects` ...) |
| `changeResolution`, `screenshot` | 0x960F0D, 0x96094B | int -> the display object at 0x1572A60's modes, compared with 1024 x 720; 0x8FED70 |
| `controlInvertToggle`, `controlsSliderMoved`, `resetInputSettings`, `resetActionMap`, `confirmKeySwitch`, `cancelKeySwitch`, `requestEditInput` | 0x962D45, 0x962F37, 0x9631BC, 0x963360, 0x9633DA, 0x9633F1, 0x963408 | the input profile options (`Scheme0FlipY` / `Scheme1FlipY`, `Sensitivity0` / `Sensitivity1` ..., reset to 0.5), the action map's reset, a key binding's confirm (0x5B1810) / cancel (0x5B1920) / edit |
| `pauseGame`, `unpauseGame` | 0x960987, 0x96099B | UI byte +0xC3F2A = 1 / 0 |
| `numDialogs`, `setCurrentWeaponName`, `clearBeforeUI`, `memCardResponse` | 0x962B08, 0x960EED, 0x963A93, 0x9609DA | int -> UI +0xC4008; the string into UI +0xC3D44; `YES` -> UI byte +0x82AE4 = 1; int -> 0xF7B9D0 |

So the menus drive the game four ways: client messages to the server (spawn, kit, team, squad), objects posted to
the client's dispatcher (the server browser, presence, friends), level-load requests, and the UI manager's own fields
(pause, settings, state). An offline menu keeps DICE's movies and these cases; a command that posts an online job
(presence, friends, the browser) waits for an answer nothing offline gives.

[data: scratch reading of 2026-11-27 (round SPG2-rest6): the 125 answered FSCommands' cases from the three switch tables, each case's calls followed two deep; the message ids from each message's vtable +0x08 (`mov al, id`); the entity types from the type registrations (`push` the type's record, `mov ecx` its run-time record, `call 0x500660`)]
[exe: BFBC2Game.exe the client 0x7FF060 (0x15713D8); the connection stored at 0x65644B (client +0x58); a message's post 0x537980 (0x420DB0; 0x63CB30: vtable 0x141CF88, +0x08 0x63CB50 `mov al, 0x1E`; 0xC0A390 on +0x1E0); a post 0x95EF3F - 0x95EF4D (client +0x18, vtable +0x24, 0x25); the cases at the addresses in the tables]

## Events and extern getters

### MENU-EVT-2: An event is a call of the root's gameEvent, passed to the listeners

Status: **verified** · Variants: PC

The engine's event record names the function `_root.gameEvent` (the string at 0x143C1E0, stored by the queue 0x9299B0
and by the simple send 0x929A70) and the frame it was queued in (UiManager+0x82CF8, the drain's frame counter:
MENU-EVT-4; read as a target movie until round SPG2-rest4); records wait on UiManager +0x82CEC. Four movies define the root `gameEvent`: main_root and loading_mp with 6 parameters, hud_root and
nam_hud_root with 8 (the event name and up to 5 or 7 values); each hands them to `_root.gameEvents.fireEvent`,
`_root.gameEvents` being a dice.EventManager the root makes when it starts. The menus handle an event as
`gameEventListener.<name> = function`; the HUD layers register `_root.catchEvent(name, handler)`, which hud_root defines
with one handler per name (a second registration replaces the first, with a trace message).

[test: core/tests/test_spec_probes.py::menus]
[exe: BFBC2Game.exe build 795745: 0x9299C3 and 0x929AB7 store the name 0x143C1E0 - 0x143C1EF; the frame stamp from UiManager+0x82CF8 into record +4 (0x9299D9, 0x929AC9); queue UiManager+0x82CEC (0x929A39)]
[data: spec/probes/menus.py, 2026-11-22: gameEvent defined by hud_root (8 params), nam_hud_root (8), main_root (6), loading_mp (6); the event function string as read]

### MENU-EVT-3: An extern read reaches the screen's getter by name

Status: **implemented** · Variants: PC

A movie's `extern.<name>` read is answered by its screen's getter - the main menu's 0x76EC30 (thiscall(menu, const
char* name), `name$param` carrying one parameter), the HUD's 0x777B30, a third at 0x76E780 - each falling through to the UI
manager's generic getter 0x95A1B0, with ids from the name table (APT-LOAD-3). Because the getter is given the name, a
hook on it can answer names the table lacks: OPFOR's DLL answers `opforPacific` and `opforGameMod` (the Pacific War
menu, MOD-MODE-1) and, offline, the stats screens' `playerStats`, `unlockGrid`, `vehicleGrid` and
`weaponProficiency$<id>` (MOD-PERS-1). An extern nothing answers reads as undefined (MENU-ED-3).

[exe: BFBC2Game.exe build 795745: getters 0x76EC30, 0x777B30, 0x76E780, 0x95A1B0]
[code: opfor:src/dll/stats_screens.cpp:604-606] [code: opfor:src/dll/gamemode.cpp:623-634]
[doc: bfbc2-menu-tools/aptrefslib.py:11-14]

### MENU-EVT-4: Queued events are delivered once a frame, delayed ones deduplicated

Status: **verified** · Variants: PC

An event record is 0x68 bytes from a reusable pool (UiManager+0x82CE0; 0x9202D0 takes a free one or makes one): +0x00
in use, +0x01 delayed, +0x02 the frame parity it waits for, +0x04 the frame it was queued in, +0x08 the frames to
wait, +0x0C the function (`_root.gameEvent`), +0x18 the event name, +0x24 the value count, +0x28 the values (the
name first). The senders (0x929A70, 0x9299B0) stamp the frame and append the record to the queue UiManager+0x82CEC;
the values are typed when the engine adds them; over the 47 functions that queue events, 45 values are strings
(0xF7C5C0), 11 integers (0xF7C5D0), 7 floats (0xF7C5E0) and 5 one shared constant (0xF7C620). The drain 0x918E50
runs from the screens' updates (0x743640, 0x764790, 0x785140) and only while a movie is loaded (0xF7A320):

1. every record with no values, and every record with values that is not delayed, is called at once (0xF82070
   without values, 0xF820D0 with them) and freed;
2. a delayed record waits until the frame counter has run its frames past the stamp and the counter's parity matches
   +0x02; then, of all waiting records with the same event name, one is called and every one is freed;
3. the frame counter (UiManager+0x82CF8) goes up by one, also when no movie is loaded.

So an event is seen by the movie in the frame it was sent, and a burst of the same delayed event reaches it once.
MENU-EVT-2's "target movie" at +0x82CF8 is this counter.

[test: core/tests/test_spec_probes.py::menus]
[data: spec/probes/menus.py, 2026-11-23: the counter's increments as read, `add dword [ebx+0x82CF8], 1` at 0x918FC2 and 0x918FCE]
[exe: BFBC2Game.exe drain 0x918E50 (callers 0x74364A, 0x7647ED, 0x7851BB; 0xF7A320; first pass 0x918E66 - 0x918ED4, the delayed pass 0x918ED6 - 0x918F80 with the name compare 0x42EA40, the free pass 0x918F86 - 0x918FC0); pool 0x9202D0 (`push 0x68`, the fields cleared 0x9202E5 - 0x92030F); senders 0x929A70, 0x9299B0; the value constructors 0xF7C5C0 -> 0x12F1F90, 0xF7C5D0, 0xF7C5E0 -> 0x12F1EE0 (a float at +8), 0xF7C620 (the value at 0x1BE9234), counted over the 84 call sites of 0x9299B0]

### MENU-EVT-5: No code sets an event's delay: every queued event is delivered in the frame it is sent

Status: **observed** · Variants: PC

The record's delay fields (+0x01 the flag, +0x02 the parity, +0x08 the frames: MENU-EVT-4) are cleared by the pool
when it makes a record (0x9202D0) and written by nothing else: the free-record getter (0x920360) sets only +0x00, the
two senders (0x9299B0, 0x929A70) the function, the frame stamp and the queue link, and none of the 92 call sites of
the getter, nor any function that calls a sender, writes +0x01, +0x02 or +0x08 of a record. So the drain's delayed
pass (MENU-EVT-4 step 2) never runs in this build: an event is called in the frame it was queued, and a burst of the
same event reaches the movie as many times as it was sent.

[exe: BFBC2Game.exe pool 0x9202D0 (+0x01, +0x02, +0x08 cleared at 0x9202E7 - 0x9202F0); getter 0x920360 (`mov byte [eax], 1` at 0x920384); round SPG2-rest5's scans: the 92 calls of 0x920360 (80 instructions after each) and every function calling 0x9299B0 or 0x929A70 (252 sites) for a store at +1, +2 or +8 of a register other than esp: only stores to other objects (0x94E9EA, 0x95B01D, 0x9644D9 ...)]

## The HUD

### MENU-HUD-1: The game mode names the HUD's five movies

Status: **verified** · Variants: PC

`GameSharedResources.GameModeData` (`GameModes/<mode>.dbx`, 10 in DICE's bundles) names the HUD in five name fields,
each beside a reference to the movie's UI asset: `HudRootName`, `PrimaryLogicLayerName`, `SecondaryLogicLayerName`,
`CrosshairsName`, `InGameMenuName`. DICE's values: root hud_root (6 modes) or nam_hud_root (the 4 Vietnam modes);
primary layer hudlayer_shared always; secondary layer hudlayer_mp_flagcapture (Conquest), hudlayer_mp_goldrush (Rush
and Squad Rush), hudlayer_mp_tdm (the deathmatch modes) or hudlayer_sp (single player); crosshairs hud_crosshair
always; in-game menu igm_mp or igm_sp. Every name is a movie of the corpus.

[test: core/tests/test_spec_probes.py::menus]
[data: spec/probes/menus.py, 2026-11-22: 10 GameModeData partitions; every HUD name a corpus movie]

### MENU-HUD-2: The HUD root loads the four layers the engine lists

Status: **verified** · Variants: PC

hud_root and nam_hud_root define MOVIE_HUD_SHARED = 0, MOVIE_HUD_GAMEMODE = 1, MOVIE_HUD_CROSSHAIR = 2 and
MOVIE_HUD_MENU = 3; their `getMovieResource(i)` returns `extern.hudMovies[i] + ".swf"`, and the root loads the four
into `_root.mcS`, `_root.mcG`, `_root.crosshair` and `_root.mcMenu`, then hud_popups.swf into `popupsMC`. The engine
answers extern.hudMovies through the HUD getter (not read): by the ids' names it lists the mode's primary layer,
secondary layer, crosshairs and in-game menu, so a HUD layer is chosen by the mode's GameModeData and needs no code to
load it.

[test: core/tests/test_spec_probes.py::menus]
[data: spec/probes/menus.py, 2026-11-22: MOVIE_HUD_SHARED 0, MOVIE_HUD_GAMEMODE 1, MOVIE_HUD_CROSSHAIR 2, MOVIE_HUD_MENU 3; hud_root's getMovieResource and its four loadMovie calls (apt.py dump)]
[inferred: the four ids pair one to one with GameModeData's four layer fields; the root itself is HudRootName]

### MENU-HUD-3: The HUD states

Status: **verified** · Variants: PC

hud_root defines 16 HUD states, ids 2 to 19: SPAWN_MENU 2, RESULTS_MENU 3, PLAYING 4, SPECTATE 5, KILLED_MENU 6,
END_OF_ROUND 7, ZOOM_MAP 8, PRE_SPAWN 9, POST_SPAWN 10, DEATH 11, VICTORY_SCREEN 12, LETTER_BOX 13, MOVIE 14,
MISSION_FAILED 15, VOID 16, PLAYING_NO_MINIMAP 19 (each `HUD_STATE_<name>`). The engine gives the state as
extern.hudState (read by 14 movies), and the layers move their timelines to frame labels by it (hudlayer_shared's
playing, killed, menu ..., which aptgraft's Onslaught timer places itself on).

[test: core/tests/test_spec_probes.py::menus]
[data: spec/probes/menus.py, 2026-11-22: 16 HUD_STATE ids from 2 to 19; extern.hudState read by 14 movies]

### MENU-HUD-4: What OPFOR's DLL changes in the HUD at run time

Status: **implemented** · Variants: PC

The DLL patches no movie bytes. In the HUD it sends the console stopwatch's events coopRoundTime(time, running) and
coopOldBestRoundTime(best) once a second during an Onslaught round through the event path of MENU-EVT-1 and
MENU-EVT-2 (ONS-END-1), for the onslaught_timer aptgraft grafts into hudlayer_shared (MENU-ED-5); it wraps the HUD's
play-sound-by-name (0xF909C0) and the rank-call slot (0x142B774) for BF1943's award stings; it makes the UI's string
compare (0x4304D0) take "onslaught" for "conquest", so hudlayer_mp_flagcapture serves Onslaught; and it hooks the HUD
icon lookup and icon-set load (0xF88510, 0xF9C9E0) for diagnostics only (MOD-MODE-1, MOD-FIX-1, MOD-PERS-1 list the
addresses).

[code: opfor:src/dll/onslaught_hud.cpp:62-75] [code: opfor:src/dll/gamemode.cpp:369-377] [code: opfor:src/dll/levelfix.cpp:57-70]
[code: opfor:src/dll/gamemode.cpp:462] [code: opfor:src/dll/gamemode.cpp:511]

### MENU-HUD-5: What OPFOR's DLL changes in the menus at run time

Status: **implemented** · Variants: PC

The DLL re-points the setter's `BFBC2Nam` label pushes (0x75FDB7) at its own buffer (`BFBC2Nam` or `BFBC2143`), so the
gameModChange listeners go to DICE's empty BFBC2143 frames for the Pacific War; answers the extern names of
MENU-EVT-3; takes the main-menu commands at the dispatcher 0x77BC10 (NET-SES-1); and drives the menus with a pad, Apt
key events through 0xF82020 and the mouse through 0xF82050 on the Apt manager, aiming by the live tree (MENU-RT-5).
The movie edits themselves are OPFOR's build (MENU-ED-6).

[code: opfor:src/dll/gamemode.cpp:16-19] [code: opfor:src/dll/gamemode.cpp:52] [code: opfor:src/dll/input.cpp:638]
[code: opfor:src/dll/input.cpp:679]

## What a menu edit must keep

The suite's aptgraft and OPFOR's menu build (patch files applied in place, the Pacific War install) rely on these
beyond the format's rules (APT-CODE-5 - APT-CODE-7, APT-LOAD-2, APT-WR-2, MENU-UNL-1).

### MENU-ED-1: A change to a shared class goes into every copy that may register first

Status: **implemented** · Variants: PC

A class registers once, from the first movie to run its init action (MENU-AS-5), so a class patched in one movie runs
only when that movie registers first. OPFOR patches dice.Dialog in main_root, main_nav and main_popups alike, and
se.river.rome.backend.Engine (the mock branch of getWeaponUnlockGrid) in all seven main-menu movies that define it
(main_root, leaderboards, mp_dog_tags, mp_stats, mp_vehicle_stats, mp_your_stats, wag); the four HUD and loading copies
(hud_root, nam_hud_root, loading_mp, loading_sp) are not patched and the main menu does not run them. In the main menu
main_root, the root, registers first (inferred from MENU-AS-8); patching every copy keeps the result independent of
the order.

[code: opfor:src/menus/main_root.patch:9-13] [code: opfor:src/menus/main_root.patch:24-30] [code: opfor:src/menus/wag.patch:7-13]
[code: opfor:src/menus/main_nav.patch:88]

### MENU-ED-2: Written code uses only opcodes with a handler

Status: **verified** · Variants: PC

Code an edit writes may hold only opcodes with a handler (MENU-RT-2): OPFOR's filler `59 17` (EA_PushZero, Pop) is.
Code translated from another build or game (the Xbox 360 HUD, BF1943) must be checked against the PC's table: the
suite's readers decode WaitForFrame2 (0x8D) and EA_PushRegister (0xB9), which the PC cannot run, without complaint,
and aptcheck flags an operand opcode the reader does not know and a written 0x00 (APT-CODE-5), not an opcode without
a handler.

[test: core/tests/test_spec_probes.py::menus]
[exe: BFBC2Game.exe build 795745: handler table 0x1545B00 (MENU-RT-1)]
[data: spec/probes/menus.py, 2026-11-22: opcodes DICE's movies use without a handler: none]

### MENU-ED-3: Grafted extern reads must be names the PC engine answers

Status: **verified** · Variants: PC

aptgraft refuses grafted code that reads an extern no code of the target movie reads, and replaces the two the
console HUD reads that the PC does not answer (coopOldBestRoundTime by undefined, isScreenHD by true). The first timer
graft read extern.coopOldBestRoundTime and crashed when the undefined value was dereferenced in the handler at
0x1311570 - by the handler table (MENU-RT-1) GetMember's (0x4E), not a binary operator's as aptgraft's comment says.

[code: bfbc2-menu-tools/aptgraft.py:2231-2238] [code: bfbc2-menu-tools/aptgraft.py:2962-2972]
[test: bfbc2-menu-tools/tests/test_aptgraft.py::quick]
[exe: BFBC2Game.exe build 795745: handler table 0x1545B00 entry 0x4E = 0x1311570]

### MENU-ED-4: Imported characters stay imports of the same movie and symbol

Status: **verified** · Variants: PC

A grafted character that its source movie imports is mapped to the target's import of the same movie (compared without
case) and symbol, or a new import is added; it is never copied in. The target must then run where that library movie
is loaded (MENU-AS-6).

[code: bfbc2-menu-tools/aptgraft.py:1949-1957] [code: bfbc2-menu-tools/aptgraft.py:1081-1089]
[test: bfbc2-menu-tools/tests/test_aptgraft.py::quick]

### MENU-ED-5: Event handlers are registered by code that runs, one per name

Status: **implemented** · Variants: PC

An event reaches a layer only if the handler is registered before it comes, by code the layer runs. The console
registers the stopwatch's handlers on hudlayer_shared's frame 1, which the PC movie never runs (its frame-0 script
ends with a GotoLabel and Stop), so aptgraft puts the registrations on frame 0 ahead of that script. catchEvent keeps
one handler per name (MENU-EVT-2), so a graft must not register a name the layer already handles. And the engine must
send the name: coopRoundTime and coopOldBestRoundTime are not in the PC executable, OPFOR's DLL sends them
(MENU-HUD-4).

[code: bfbc2-menu-tools/aptgraft.py:2912-2919] [code: opfor:src/dll/onslaught_hud.cpp:62-75]

### MENU-ED-6: OPFOR's menus are rebuilt from DICE's pristine bundle, patches first

Status: **implemented** · Variants: PC

OPFOR's menu build unpacks its pristine copy of the Package main menu (`original/Package/menu/main/mainmenu-00.fbrb`),
applies each `src/menus/<movie>.patch` to `Menu/<movie>.res` in place (every expected byte checked), then runs the
Onslaught mode picture and, when enabled, the Pacific War menu install (aptgraft's keep layout), each on the result of
the step before. The byte-offset patches therefore always meet DICE's bytes; the later steps move only what they grow
(APT-WR-2).

[code: opfor:build_menus.bat:27-42] [code: opfor:build_menus.bat:49-50]

### MENU-ED-7: A menu patch is applied whole or not at all, every expected byte checked

Status: **verified** · Variants: PC

`bfbc2 aptpatch <in.res> <file.patch> <out.res>` (OPFOR's `AptTool.cs patch` until round OR5) reads the patch
format `aptcheck lint` reads, checks every expected byte, appends each `append` string NUL-terminated at its
member's 4-aligned end, writes `@label` as the string's little-endian offset, and rebuilds the BIGF archive with
the members in their index order, 4-byte aligned (APT-WR-1's layout). AptTool accepted a few inputs that can only
be mistakes; they are refused, nothing written: a line `aptcheck lint` finds malformed, a label defined twice
(AptTool kept the last, so the earlier string was unreferenced), a member named twice in the archive's index
(DICE's movies have none: the loader finds members by name). OPFOR's 12 patch files have none of these: their
movies are AptTool's byte for byte.

[code: rust/bc2-menu/src/aptpatch.rs:36-70]
[test: rust/bc2-menu/src/aptpatch.rs::refusals]
[test: rust/bc2-menu/src/aptpatch.rs::patches_and_appends]
[data: round OR5, 2026-11-23: OPFOR's 12 patch files on DICE's pristine main menu, 12 of 12 movies identical to AptTool.exe's]

### MENU-ED-8: The ONSLAUGHT mode picture is new data appended, its pixels Pillow's

Status: **verified** · Variants: PC, Xbox 360 (the source)

`bfbc2 menuart onslaught` (OPFOR's `onslaught_art.py` until round OR5) adds main_nav's picture sprite a frame
`ONSLAUGHT` placing a new shape and image (copies of the first mode's, the geometry's part re-pointed), each table it
extends copied with the new entries and the old copies left unreferenced (MENU-UNL-1, APT-WR-2), and puts the
picture into free DXT5 space of the menu atlas with a new `UITextureAtlasInfo` (APT-ATL-1). The picture's pixels
are the retired Python's: Pillow 12's Lanczos resampling through premultiplied alpha, paste and alpha_composite,
its DXT5 decoding, numpy's float32 luminance (green first, red and blue fused, as OpenBLAS' kernel sums it). The
cache `work\import\onslaught_mode.png` is compared by its pixels: Pillow's and the png crate's file bytes differ.

[code: rust/bc2-menu/src/modeart.rs:481-704] [code: rust/bc2-menu/src/pil.rs:1-20]
[test: rust/bc2-menu/src/pil.rs::pillow_answers]
[data: round OR5, 2026-11-23: rust/parity_menuart.py, 200 random cases of each of 9 operations identical to Pillow / numpy; the menu bundle built from the cache, the user's Xbox 360 package and the fallback identical to onslaught_art.py's]

### MENU-ED-9: The Pacific War mode is main_nav's third mode, built on Vietnam's

Status: **verified** · Variants: PC

OPFOR's Pacific War main-menu mode (`menu_install.py`; its movie edits the Rust core's since round OR14:
`bc2_menu::pacific`) gives `main_nav` three image characters and textured quads (the background at Vietnam's base's
bounds, the label beside Vietnam's, the logo at Vietnam's logo's corner), places the background in
`gameModBackground`'s empty frame `BFBC2143` at depth 1, adds `gameModSelector` a PACIFIC WAR button in frames
`BFBC2` / `BFBC2Nam` and a new frame `BFBC2143` with buttons to the two other modes (each frame's `buttonPressed`
sends `changeGameMod 3 / 1 / 0` by the button's name), shows Vietnam's art in `bgBar`'s and the new logo in
`loggo_mc`'s `BFBC2143` frames, labels every other `BFBC2Nam` frame `BFBC2143`, lets each `if (v == "BFBC2Nam")`
switch take the alias, and makes `setGameMod`'s empty-label branch pick `BFBC2143` when `extern.opforGameMod` is 3;
`main_root`'s `getGModStringExtensino` returns `_PW` then. The movie is written in the graft layout (`.apt` / `.geo`
kept in place, `.const` packed). The Rust makes each node, constant and instruction as the Python did, so the
movies are the same bytes; a movie of another shape is refused with the Python's words.

[code: rust/bc2-menu/src/pacific.rs:1-60]
[test: rust/parity_pacific_menu.py]
[data: round OR14: rust/parity_pacific_menu.py - the menu step's movies at the step's and 40 random label / logo sizes, damaged movies and the dumps identical to menu_install.py's Python on aptgraft.py; menu_install.build old against new: every file and printed line identical]

### MENU-ED-10: The localization file keeps DICE's texts where they were and appends new ones

Status: **verified** · Variants: PC

`language_<lang>_normal.bin` as OPFOR writes it (`names.py`'s WW2 texts; `bc2loc set`; since round OR14 through
`bc2_native.LocFile`, `bc2_menu::loc`): the header (`0x39000`, size, count, table and strings offsets, the 16-byte
name), the texts it had kept at their offsets, each new or changed text appended NUL-terminated, the entry table
sorted by hash (`khash`: `h = h * 33 + c` from `0xFFFFFFFF`). A key is looked up as the PC text `WIN32_<key>` first,
then the key; a text set takes the `WIN32_` key where the file has one. Only ASCII without NUL is written (bytes
under 0x80 are themselves in every language's encoding); anything else is refused. Unchanged, the file is written
as it was read.

[code: rust/bc2-menu/src/loc.rs:30-150]
[test: rust/parity_pacific_menu.py]
[data: round OR14: rust/parity_pacific_menu.py - every language of the game copy: names.py's texts set, written and read back, every lookup, damaged files and random keys and texts identical to bc2loc.py's]

## Open questions

- Which movies load in which order beyond the literal loads of MENU-AS-8 and the roots of MENU-LOAD-2.
- Whether the HUD, the main menu and the loading screens share one `_global` (MENU-AS-5's "first registration wins"
  across them).
- How `loadMovie("<name>.swf")` and an import's movie name find the resource: by the UI asset's name or its
  `SourceFile` (nam_hud_root's names hud_root's), and with or without case.
- extern.hudMovies: the HUD getter (0x777B30) is not read; MENU-HUD-2's order is inferred from the ids' names.
- The 9 extern names missing from the name table (MENU-AS-7): dead code, mock paths, or answered by another route;
  of the 6 FSCommands, 4 are the callback's own (MENU-RT-6) and `clearLocalDeathMessages` and `g` reach no handler
  (MENU-RT-7).
- MENU-RT-8 reads each case's first branch two calls deep: the message ids have no names (no type records), the
  dispatcher's numbers (0x25, 0x1B / 0x34, 0x1B / 0x35, 0x12 / 0x1B) are not decoded, and the online jobs' answers
  were not followed.
- MENU-EVT-5 is a static negative: a write through a pointer the scans did not see would make the delay live; a
  playtest log of a burst of one delayed event would settle it.
- The handlers at 08, 09, 31 - 33, 35 - 37, 5E and 77 (MENU-RT-2) and the operands of Try (0x8F) and CallFrame
  (0x9E): unused by DICE, unread; the suite's names of EA's no-operand opcodes 0x56 - 0x5E and 0x70 - 0x77 are
  OpenSAGE's, and the PC table disagrees with them at 0x57 (no handler) and 0x5E, 0x77 (handlers).
- A check in aptcheck for opcodes without a PC handler (MENU-ED-2) would make that rule implemented.
- What else the unloader reads through non-null pointers (MENU-UNL-1 is the one case hit).
- Whether the catalogue is rebuilt when the game mod changes (GAME-MODE-6) or built once at start-up.
