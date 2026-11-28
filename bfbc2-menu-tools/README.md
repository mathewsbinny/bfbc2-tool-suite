# Menu tools

Command-line tools for Battlefield: Bad Company 2's menus and HUD (EA APT movies), built on the suite's APT reader
(`bc2_formats::apt` in the Rust core; its Python twin `bfbc2-console-readers\apt\apt.py`). They read your own game copy - straight from its bundles, nothing unpacked - and
never write into it. There is no menu viewer here.

| Tool | For |
|---|---|
| `aptcheck.py` | checking menu patches before a build reaches the game: the rules whose breaking crashes the menu loader |
| `bfbc2 aptrefs` (library `aptrefslib.py`; `aptrefs.py` retired in round RT3) | what the movies ask of the engine (`extern.<name>`), send it (FSCommands) and handle (engine events), with the exe's name / id table |
| `bc2loc.py` | the game's localized texts: look up a key, search the texts, the keys a movie uses; write texts |
| `aptgraft.py` | writing APT movies (round trip, the other byte order) and grafting between movies: a character with what it uses, the Onslaught HUD timer, mode cases and aliases, frame-label aliases, crosshairs; its commands and model are in its docstring (`python aptgraft.py` prints them) |

Since round OR14 no Project OPFOR code imports this folder's Python: its WW2 and Onslaught movie tools edit movies through `bc2_native.AptMovie` (`bc2_menu::graft`), the Pacific War menu through `bc2_native.pacific_main_nav` / `pacific_main_root` (`bc2_menu::pacific`), and the texts through `bc2_native.LocFile` (`bc2_menu::loc`). `aptgraft.py`, `bc2loc.py`, `aptcheck.py` and `menucommon.py` are the Rust's test references until PR1c.

Project OPFOR's menu build steps are `bfbc2.exe` commands with no Python (round OR5): `bfbc2 aptpatch <in.res>
<file.patch> <out.res>` applies one of OPFOR's patch files (the format `aptcheck lint` reads; OPFOR's AptTool.cs gave
the same bytes) and `bfbc2 menuart onslaught <unpacked mainmenu> [--xbox DIR] [--refresh] [--cache FILE]` adds the
Offline Matches panel's ONSLAUGHT mode picture (OPFOR's onslaught_art.py gave the same bytes; spec MENU-ED-7, -8).

Python 3.12 or newer (the suite's; numpy is not needed); with the suite's Rust core built, the movies are decoded and
the rules and scans run in Rust (rounds E3n, E4c), the same answers. Without Python: `bin\bfbc2.exe <tool> ...`
(`aptcheck`, `aptrefs`, `bc2loc`, `aptgraft`; the suite's `bin\`, rounds O2 / O3) takes the same arguments and gives the same output, files and
exit codes; a damaged movie is handed to the Python (the suite's README, "bfbc2.exe"). Movies are named the same way
in all of them:

| You give | Read from |
|---|---|
| a path to a `.res` (or a `.apt` with its `.const` beside it) | that file |
| `main_nav` | `Menu/main_nav.res` of the main menu bundle the game loads (`Package` over `Dist`) |
| `pristine:main_nav` | OPFOR's pristine copy of the main menu bundle (`original\Package\menu\main\mainmenu-00.fbrb`, kept by `build_menus.bat`): before OPFOR's patches |
| `<bundle.fbrb>:main_nav` | any bundle |

Project OPFOR BFBC2's folder is found as the other tools find it (`--opfor`, `OPFOR_ROOT`, or a `project-opfor-bfbc2` folder next
to the toolkit). Outputs made from game data go to `<OPFOR>\work\tools_out\`.

## aptcheck.py - menu patch checks

```
python aptcheck.py lint "<OPFOR>\src\menus\*.patch" --installed --brief
python aptcheck.py verify pristine:main_nav main_nav --patch "<OPFOR>\src\menus\main_nav.patch"
python aptcheck.py check main_root
```

- **`lint <file.patch>...`** reads OPFOR's patch format (`<member> <hex offset> <expected bytes> -> <new bytes> # note`
  and `append <member> <label> "text"`, as `bfbc2 aptpatch` applies them; OPFOR's AptTool.cs until round OR5): syntax, the size kept, lines
  overlapping, labels, a comment on the line or just above it. It then applies the file to the source movie (default
  `pristine:<file name>`) with every expected byte checked, and verifies the result. `--installed` also says whether
  the game's installed movie is that result (if not, the build is stale - or, for main_nav, `bfbc2 menuart onslaught` added
  the Onslaught picture after the patches).
- **`verify <original> <patched>`** names every changed byte range (the instructions before and after and the
  function they are in, a constant's old and new text, or data), checks that every instruction away from the changed
  bytes decodes exactly as before, and that the patched movie breaks none of the rules below where the original did
  not. With `--patch`, every changed byte must be one the patch file names.
- **`check <movie>`** runs the rules on one movie by itself.

The rules (`private:opfor-research/DEVELOPMENT_NOTES.md` section 3, learned from menus that crashed while loading; normative: `spec/formats/apt.md`):

| Finding | Meaning |
|---|---|
| `shared-list` | a PushData item list used by two PushData: the loader converts each list in place, a second user crashes the menus |
| `end-in-function` | an End (00) opcode inside a function body |
| `zero-opcode` | a changed byte that is now opcode 00 (use `59 17`, push 0 / pop, as filler) |
| `body-end` | a function body that does not decode exactly to its end (an operand misaligned, an instruction straddling the end) |
| `branch-target` | a branch that does not land on an instruction of its own stream |
| `const-index` | a constant index past the constant table, or a pool index past the current constant pool |
| `decode-shift` | instructions away from the changed bytes decode differently (alignment or an operand moved) |
| `unknown-op` | an opcode with an argument the reader does not know |

Exit code 1 when there is an error. Every movie of the game copy's movie sets (128: menus, HUDs, loaders) and the
Xbox 360 menus pass the rules by themselves; OPFOR's 12 patch files lint with no error (2026-10-26).

## bfbc2 aptrefs - externs, FSCommands and events

```
bfbc2 aptrefs xref                           every name across every movie set -> work\tools_out\aptrefs.txt / .json
bfbc2 aptrefs find playerStats               one name: its kind, engine id, getter case, movies
bfbc2 aptrefs movie wag                      one movie: externs read, FSCommands sent, events handled
bfbc2 aptrefs table                          the exe's name -> id table
```

- **From the movies:** `extern.<name>` reads (`name$param` when a parameter is added: `unlockWeaponData$<id>`), the
  FSCommands sent (`"FSCommand:<name>"` strings, `fscommand` / `fscommandProxy` calls with a constant name), and the
  engine events handled: `_root.catchEvent(name, handler)` in the HUD, `gameEventListener.<name> = handler` in the
  menus, with the handler's parameter count.
- **From the exe (data only):** the name -> id table (574 names at `0x0143C898` in build 795745, found from its
  `presenceRequestOnline` entry, so other builds work too; getter and FSCommand names share one id list, and ids repeat
  between the two kinds); for build 795745, which getter switch has a case for each id - the main menu getter
  `0x76EC30` (ids 0x35-0x11A and 0x123-0x15D) and the UI manager's generic getter `0x95A1B0` (ids 1-0x15F). The HUD
  getter `0x777B30` is not read, so a name only HUD movies read shows "none - HUD getter not read".
- **"NOT in the exe"** marks a name the exe has no string for: an event the engine probably never sends by that name
  (some are built at run time, so it is a hint).

## bc2loc.py - localized texts

```
python bc2loc.py get MENU_PLAY_NOW PC_SERVER_BROWSER IDK_A
python bc2loc.py search "server browser" --lang english
python bc2loc.py movie main_nav
python bc2loc.py dump --lang russian        -> work\tools_out\loc_russian.txt
python bc2loc.py langs
```

The texts are `Localization/bin/language_<lang>_normal.bin` in `Dist\win32\overlay-00.fbrb`: a table of key hashes
(h = h * 33 + byte from 0xFFFFFFFF) and texts encoded through the language's `_histogram.bin` (one byte per character,
or two for Japanese). PC texts are stored under `WIN32_<key>` (`WIN32_PC_SERVER_BROWSER`, `WIN32_IDK_A`); `get` looks
there first, as the PC game does. Keys themselves are not stored: `search` and `dump` name the ones found among the
exe's strings and the movies' constants (about 840 of the 7,077 English entries). The format is in the script's header.

`set` writes language files with texts set or added (`python bc2loc.py set NAME_X="TEXT" --out <folder> --lang all`):
ASCII only (those bytes are themselves in every language's encoding); the texts already there keep their bytes, new
ones are appended, the table stays sorted by hash; with nothing set the file comes back byte-identical. Nothing is
written into the game folder.

### How a weapon gets its name on screen

A weapon's own `Name` (its `SoldierWeaponData`, e.g. `Garand`, `M1A1 Thompson`, `40mmgl`) is looked up in
`Menu/shared_config.dbx` (a `UISharedConfigAsset`, `Package\async\startup-00.fbrb`): a `PersistentWeapon` entry gives
`NameSID` / `DescriptionSID` / `Abbreviation` and the unlock picture (`Garand` -> `NAME_M1` "WWII M1 GARAND"), a kill
message entry gives `PresentationSID` (`KILL_MSG_M1` "{0:s}<WWII M1 GARAND>"). The stats screens group names by
`Awards/WeaponCategories.dbx` (`ABBR_M1` "M1"). The HUD movies take the text from the engine (`extern.kitWeapons`);
their own weapon-name lists are debug mock data. A weapon whose `Name` has no entry has no text - DICE's Type 5 rifle
grenade (`JAP_riflegrenade`) has none, and the Type 5 rifle shares the Garand's `Name`.

## Tests

`tests\test_menu_tools.py`: a movie, patch files, a language file and an exe built in the test (quick tier); with
`--game`, OPFOR's patch files, every movie of the game copy, the exe's table and getter cases, known events, and the
English, Russian and Japanese texts (game tier). `tests\test_bc2loc_write.py`: every language written back
identical and edited with `set` (game tier). `tests\test_aptgraft.py` (aptgraft's writes and grafts),
`tests\test_apt_e3n.py` / `test_apt_e4c.py` (the Rust decoder, model and scans against their Python),
`tests\test_bc2loc_native.py` and `test_pe_native.py` (the localization and PE readers in Rust); the command lines in
Rust: the suite's `core\tests\test_cli_o2.py` / `test_cli_o3.py`.

Developed for Project OPFOR BFBC2. Battlefield: Bad Company 2 is © Electronic Arts / DICE.
