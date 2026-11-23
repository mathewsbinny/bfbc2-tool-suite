# Options and settings: what exists, where it persists, how the menus edit it

Which settings BFBC2's PC client has, where each kind is kept, how the files are read and written, the defaults and
how the options screens reach them. The input bindings and the controls screen are [input.md](input.md); the
engine-movie events and extern getters in general are [menus.md](menus.md) (MENU-EVT-1 to MENU-EVT-3).

**Sources.** Round SPG3 (2026-11-23) read `BFBC2Game.exe` (PC build 795745) statically on a scratch copy (string and
reference searches, the call index, disassembly of the readers and writers), DICE's partitions of the startup bundle
with `spec/probes/options.py` (whose `checks()` hold the counts below), Project OPFOR BFBC2's research notes
(`research/BFBC2_DECOMP_RESEARCH.md` sections 2, 3 and 10, `DEVELOPMENT_NOTES.md` "User data separation") and the
**key names only** of one `GameSettings.ini` and one `settings.ini` the game wrote (read only, no value read). Nothing
was run; how the game behaves with a damaged or missing file is open.

There are three kinds of settings:

| Kind | Where | Edited by |
|---|---|---|
| The **window, graphics and sound quality** settings | `<Documents>\BFBC2\settings.ini` (SET-INI-1) | the Graphics and Audio options pages, the first start's detection |
| The **game settings** (audio levels, brightness, input sensitivities and schemes, gameplay switches, the player's profile flags, kits, favourites) | `<Documents>\BFBC2\GameSettings.ini` and `GameSettings.bin`, defaults in the `Settings/GameSettings` partition (SET-GS-1) | the options pages through `setSetting` (SET-UI-1) |
| The **engine settings objects** (`Game.*`, `Client.*`, `Server.*`, `Network.*` ...) | in memory only; some from the command line (SET-CLI-1) | nothing in the menus; OPFOR's DLL writes a few (SET-ENG-1) |

The key bindings are a fourth store: `<Documents>\BFBC2\input\<map>.dbx` (INP-FILE-1).

## 1. The files

### SET-FILE-1: One per-user folder under Documents

Status: **observed** · Variants: PC

The game asks Windows once for the Documents folder (`SHGetSpecialFolderPathA(CSIDL_PERSONAL)`, import 0x14057C4) and
keeps its user data in `<Documents>\BFBC2\`, which it also calls `/data/local/` (the string 0x14107C8, used by the
crash report, the screenshots and the input files): `settings.ini`, `GameSettings.ini`, `GameSettings.bin`,
`input\{shared,infantry,land,air}.dbx`, `Screenshots\`, `pb\` (PunkBuster) and `crashreport.xml`. The window position
is not there but in the registry (`HKLM\SOFTWARE\DICE\ROME\Game` `ClientWindowPosX` / `ClientWindowPosY`).

OPFOR's DLL answers the Documents lookup with `<Documents>\Project OPFOR\BFBC2\` instead (or `[paths] userdata=`),
copying `settings.ini`, `GameSettings.ini`, `GameSettings.bin` and `input\` from the base game's folder on first use,
so the base game's settings are never touched (the user-data redirect, MOD-IMP-1).

[exe: BFBC2Game.exe imports SHGetSpecialFolderPathA (IAT 0x14057C4, called at 0x4E25BB and 0x518174); "/data/local/" 0x14107C8, "/data/local/input/" 0x1419E08, "/data/local/GameSettings.ini" 0x1432448, "\settings.ini" 0x142FABC, "/data/local/Screenshots/" 0x147737C, "/data/local/crashreport.xml" 0x1419CF8]
[code: opfor:src/dll/userdata.cpp:170-200]
[doc: opfor:DEVELOPMENT_NOTES.md:187-205] [doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:52-62]

### SET-INI-1: settings.ini: four sections, 29 keys

Status: **verified** · Variants: PC

`settings.ini` is a Windows INI file read and written through `GetPrivateProfileStringA` /
`WritePrivateProfileStringA`. Its section and key names are one static table of 33 strings (12-byte string records
from 0x1570FA8, filled by the static initialisers at 0x1372AA0 - 0x13735C0):

| Section | Keys |
|---|---|
| `WindowSettings` | `Width`, `Height`, `Fullscreen`, `RefreshRate`, `VSync` |
| `Sound` | `Quality`, `VoipEnable`, `SpeakerCount` |
| `Graphics` | `Effects`, `Soldiers`, `Vehicles`, `Overgrowth`, `Undergrowth`, `StaticObjects`, `Terrain`, `Shadows`, `Bloom`, `HSAO`, `MSAA`, `Water`, `MainQuality`, `Texture`, `DxVersion`, `Aniso`, `Detail`, `RenderAheadLimit`, `Fov` |
| `Chat` | `ChatWindowAlpha`, `SmallChatWindowTimer` |

A file the game wrote holds exactly these sections and keys, in this order.

[test: core/tests/test_spec_probes.py::options]
[exe: BFBC2Game.exe key table 0x1570FA8 - 0x1571134 (33 records of {begin, end, capacity}); initialisers 0x1372AA0 - 0x13735C0 ("RefreshRate", "Overgrowth" and "MSAA" through records whose push the probe reads as lengths); GetPrivateProfileStringA IAT 0x14051D8, WritePrivateProfileStringA IAT 0x14051D4]
[data: spec/probes/options.py, 2026-11-23: every section and key name a C string of the exe, the table 33 records]
[data: the key names of one settings.ini the game wrote (read only, no value read): the four sections and 29 keys above, in this order]

### SET-INI-2: settings.ini is read at start-up and written by the options and the recommended settings

Status: **observed** · Variants: PC

The reader 0x7DBD60 runs once from the start-up function 0x5703E0 (called from the entry code at 0x401E2F /
0x401E7A), before a renderer exists: it is what picks the window, the Direct3D version (`DxVersion`, `auto` or a
version) and the quality levels. The writer 0x7DD1F0 writes the `Sound` `Quality` and the per-item `Graphics` levels;
its callers are the start-up (0x570442, writing what the reader resolved), the graphics options (0x9124ED, 0x960FF9)
and `setRecomendedSettings` (0x960946). The item levels are the words `low`, `medium`, `high`, `custom`;
`MainQuality` is the preset, and a change to any single item writes `MainQuality=custom` (0x7D7330, 0x7D73E7).

[exe: BFBC2Game.exe reader 0x7DBD60 (caller 0x570430 in 0x5703E0); writer 0x7DD1F0 (callers 0x56AE02, 0x570442, 0x9124ED, 0x960946, 0x960FF9); "low" / "medium" / "high" / "custom" pushed in 0x7DD100 - 0x7DDA00; "custom" -> MainQuality at 0x7D7339 and 0x7D73F0; "auto" in the reader]
[doc: opfor:research/WW2_DX9_SHADER_PORT_PLAN_2026-11-15.md:179]

### SET-GS-1: The game settings' defaults are the GameSettings partition

Status: **verified** · Variants: PC

The defaults of `GameSettings.ini` are the startup bundle's `Settings/GameSettings` partition: one
`GameSettingsAsset` whose `Categories` are complexes of a `Category` name and four typed lists (`Floats`, `Ints`,
`Strings`, `Binaries`), each setting a `Name`, a `Value` and `UserExposed`. DICE's has eight categories (`GstAudio`,
`GstRender`, `GstAI` (empty), `GstInput`, `GstGameplay`, `GstPlayerProfile`, `GstMenuTracker`, `GstPersistence`)
and 59 settings: 14 floats, 34 ints, 11 strings, no binary. 45 are `UserExposed`; the 14 that are not are
`AimAssist`, `AlternativeHeliControls`, `CameraBobbing`, `Crouch`, `ELO`, `FlashValues`, `highestRankDialog`,
`Scheme1FlipY`, `Scheme1Sensitivity`, `ThemeInstalled` and `GstMenuTracker`'s four `UIMenuTrackerPage_*`.

[test: core/tests/test_spec_probes.py::options]
[data: spec/probes/options.py, 2026-11-23: DICE's startup bundle, Settings/GameSettings: the categories, 14 / 34 / 11 / 0 settings per type, 45 UserExposed, the 14 names above]

### SET-GS-2: Nine categories, one INI section each

Status: **verified** · Variants: PC

A setting's category is an enumeration whose section names the switch at 0x80D970 gives: 0 `GstAudio`, 1
`GstGraphics`, 2 `GstRender`, 3 `GstInput`, 4 `GstAI`, 5 `GstGameplay`, 6 `GstPlayerProfile`, 7 `GstPersistence`, 8
`GstBinary`; any other value names the section `unknown`. The menus' getters use the same numbers (SET-UI-1:
`settingsAudio` 0 ... `settingsInput` 3). `GstMenuTracker`, a category of the partition (SET-GS-1), has no name in
the switch.

[test: core/tests/test_spec_probes.py::options]
[exe: BFBC2Game.exe 0x80D970 (`cmp eax, 8`, jump table 0x80D9E0; "unknown" 0x1431B88)]

### SET-GS-3: GameSettings.ini is read setting by setting, typed, and written whole with GameSettings.bin

Status: **observed** · Variants: PC

The reader 0x831ED0 walks the categories, and for every setting whose byte +0x28 is set (the user-exposed ones,
SET-GS-4) asks `GetPrivateProfileStringA(<category section>, <name>, "", buffer of 4096, "/data/local/GameSettings.ini")`;
a value found is parsed by the setting's type (+0xC): 1 a float (0x512320), 2 an int (0x512360), 3 a string. A
missing key keeps the default. It then reads `GameSettings.bin` (0x815870), the binary settings. The writer 0x833FD0
writes both files whole; it does not go through `WritePrivateProfileStringA`.

The menus' `loadSettings` (0x920470, also run by the `settingsLoad` event at 0x92A8DF) calls the reader; the save
0x9205A0 (which calls the writer at 0x920623) runs on `saveSettings` and right after every controls-screen change
(SET-UI-2).

[exe: BFBC2Game.exe reader 0x831ED0 (0x80D970 section name, flag +0x28 at 0x831FB0, GetPrivateProfileStringA at 0x831FDC, type switch 0x831FEA - 0x832241, "GameSettings.bin" 0x83215F -> 0x815870); writer 0x833FD0 ("/data/local/GameSettings.ini" 0x834330, "GameSettings.bin" 0x83446D; callers 0x8347CF, 0x920623); 0x9205A0 callers 0x932E5C, 0x95993D, 0x95FB65, 0x95FEFD, 0x962D33, 0x962F0D, 0x96316E, 0x963336]
[doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:52-62]

### SET-GS-4: Which keys the file holds

Status: **observed** · Variants: PC

`GameSettings.ini` holds the user-exposed settings of the partition (SET-GS-1) and the settings the code asks for by
name with a default of its own, which the partition does not define: `ToggleCrouch` (0x814723, 0x81ADBC),
`HoldToZoom`, `ColorBlindMode`, `KitConfig_0` - `KitConfig_7` and `CurrentKit` (the kit screen 0x777AA0) in
`GstGameplay`; `ServerFilterSettings` and `ServerDropDownSettings` (the server browser) in `GstPlayerProfile`. The
partition's 14 hidden settings are never written (`Crouch`, `AimAssist` and `CameraBobbing` among them: their
user-facing twins are `ToggleCrouch` and the engine's `Game.AutoAimEnabled`). The typed getters take a category, a
name and a default (0x80D2E0 for an int, about 140 call sites; 0x80D0C0 for a float), so a setting the code asks for
exists from its first use (inferred) and is then saved.

[exe: BFBC2Game.exe getters 0x80D2E0 and 0x80D0C0 (call sites listed by the round's call index); "ToggleCrouch" pushed at 0x814722 before 0x80D2E0; "KitConfig_0" - "_7" and "CurrentKit" at 0x77898A - 0x778AFA and 0x778970 in 0x777AA0; "ServerFilterSettings" / "ServerDropDownSettings" at 0x95D5BA / 0x95D495]
[data: the key names of one GameSettings.ini the game wrote (read only, no value read): GstAudio 10, GstGameplay 19, GstInput 19, GstPersistence 5, GstPlayerProfile 4, GstRender 2 keys; none of the 14 hidden names; the extra names above]

## 2. The engine settings objects and the command line

### SET-ENG-1: Engine settings are reflected objects looked up by name

Status: **observed** · Variants: PC

Besides the game settings there are the engine's settings objects (the type names `CoreSettings`, `GameSettings`,
`ClientSettings`, `ServerSettings`, `NetworkSettings`, `PhysicsSettings`, `DemoSettings`, `MonitoringSettings`
...), whose fields the code reads by the name `<Object>.<Field>` through 0x4F4C40 (`Server.IsAiEnabled`,
`Game.AutoAimEnabled`, `Client.UseGlobalGamePadInput` ...; about 130 such names are strings of the exe). They are not
saved in the user's files and no options page edits them. The `GameSettings` object holds, among others,
`InputConfiguration` (+0x78, the input asset's name, INP-CFG-1), `InputTapMaxTime`, `InputHoldMinTime`,
`ToggleCrouch` (+0x8E) and `HoldToZoom` (+0x90). OPFOR's DLL reads and sets a few of them (`Server.IsAiEnabled`,
`Server.UnrestrictedUnlocks`, the AI-NEED rules).

[exe: BFBC2Game.exe lookup 0x4F4C40 (ECX = *(0x0155CFDC); the boolean type object 0x01BF0354); the GameSettings field table around 0x01C15250 (InputConfiguration +0x78, ToggleCrouch +0x8E, HoldToZoom +0x90, InputTapMaxTime +0x1C, InputHoldMinTime +0x84); type names "CoreSettings" 0x1412FB8, "GameSettings" 0x142CD30, "ClientSettings" 0x142CEB4, "ServerSettings" 0x142D4C0]
[doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:69-86]

### SET-CLI-1: Command-line options

Status: **observed** · Variants: PC

The command line is kept as a list of strings (begin / end at 0x155D008 / 0x155D00C). An option is a word starting
with `-` or `+` followed by its value; 0x4F4CF0(name) finds one by a case-blind compare (0x508160) and returns the next
word. The names the client asks for include `dataPath`, `remoteFileServer`, `secure`, `noCache`, `host`, `hostUser`,
`sessionId`, `displayAsserts`, `displayErrors`, `crashDumpAsserts`, `crashDumpErrors`, `loginToken`, `platform`,
`dedicated`, `networkProtocolId`, `heartBeatInterval`, `stopOnFocus`, `requestState`, `requestStateParams`,
`nosound` (which clears `Client.IsSoundEnabled`), `soundsystem`, `speakerSetup`, `port`, `pingInterval`, `listen`,
`revisionLevel`, `revisionKey`, `maxQueriesPerSecond` and `queryPortOffset`. The entry code tests `-dedicated`,
`-server` and `-runMultipleGameInstances` (0x401F0C - 0x401F65) before it takes the single-instance mutex; the client
adds `-noAsync` and `-host <name>` to its own list (0x501F86, 0x5022A8). The engine settings objects also carry an
`IsCommandLineOption` flag in their option descriptors (the `OptionDescriptor` type: `Name`, `Category`,
`Description`, `ValueType`, `DefaultValue`, `TargetPlatforms`, `IsCommandLineOption`, `IsFlag`); whether a generic
`-<Object>.<Field>` option sets them was not traced.

[exe: BFBC2Game.exe argument list 0x155D008 / 0x155D00C; finder 0x4F4CF0 (`cmp cl, 0x2D` / `0x2B` at 0x4F4D52 / 0x4F4D5B; compare 0x508160); its 40 callers (the names pushed before each); entry tests 0x401F0C, 0x401F29, 0x401F48, 0x401F65; "-noAsync" 0x14123AC at 0x501F86, "-host" 0x1412418 at 0x5022A8; OptionDescriptor 0x1412E24, its fields from 0x01C13438]

## 3. The options screens

### SET-UI-1: The options pages talk to the engine by name: getters per category, commands, events

Status: **verified** · Variants: PC

The options pages read and write the game settings through the menu name table (APT-LOAD-3) only:

| Name | Id | Kind | Engine side |
|---|---|---|---|
| `settingsAudio`, `settingsGraphics`, `settingsRender`, `settingsAI`, `settingsGameplay`, `settingsInput` | 0x9C - 0xA1 | extern | 0x9430C0(category 0, 1, 2, 4, 5, 3): the category's settings |
| `_settings` | 0xA2 | extern | the settings object the pages index |
| `isSettingsDirty`, `isDefaultSettings` | 0xCA, 0x151 | extern | unsaved changes; defaults in use |
| `adapterResolutions` | 0xE7 | extern | the display modes |
| `setSetting` | 0x5B | command | `"<category>,<name>,<value>"`, split at `,` and set through 0x929D30 |
| `resetSettings`, `applySettings` | 0x5C, 0x5D | command | 0x920430, 0x920450 |
| `loadSettings`, `saveSettings` | 0x5E, 0x5F | command | SET-GS-3 |
| `setRecomendedSettings` | 0x60 | command | the detected defaults; writes `settings.ini` (SET-INI-2) and reloads |
| `changeResolution` | 0x99 | command | the display mode (0x960F0D) |

`settingsLoad`, `settingsLoaded` and `settingsSaved` are events. In DICE's movies the names are constants of 11
movies (`main_root`, `hud_root`, `nam_hud_root`, the loading and stats movies: the copies of one shared class), and
the `settings` movie (the in-game options, `Menu/Settings` in each `*_common` in-game bundle) itself names
`_settings`, `resetSettings`, `changeResolution` and `adapterResolutions`.

[test: core/tests/test_spec_probes.py::options]
[exe: BFBC2Game.exe name table 0x0143C898 (ids as above); command dispatcher 0x95E680 (`id - 0xB`, byte table 0x963F24, jump table 0x963DA4: 0x5B -> 0x9607C5 ... 0x60 -> 0x960934); extern switch in 0x95A1B0 (`id - 1`, byte table 0x95DB68, jump table 0x95D98C: 0x9C - 0xA1 -> 0x95B325 - 0x95B36B, each 0x9430C0(n)); setSetting 0x9607F6 (split at ",") -> 0x929D30]
[data: round SPG3's scan of DICE's 91 movies (spec/probes/menus.py's corpus): the settings names in 11 movies' constants; `settings` names _settings, resetSettings, changeResolution, adapterResolutions, confirmKeySwitch and cancelKeySwitch]

### SET-UI-2: Graphics pages set items; the controls pages save at once

Status: **observed** · Variants: PC

The graphics page's item setters (0x961121 - 0x9627AB, one `setSetting`-style call 0x929D30 per item: `Effects`,
`Shadows`, `Texture` ...) feed the quality items of `settings.ini`; a preset choice rewrites every item and a single
change makes the preset `custom` (SET-INI-2). The controls commands (`controlsSliderMoved` 0xCC,
`controlInvertToggle` 0xCD, `resetInputSettings` 0xC9) post their change on the client's message channel 0x2B
(message 0x1B) and save `GameSettings.ini` at once (0x9205A0); key bindings are saved by their own path
(INP-FILE-1).

[exe: BFBC2Game.exe 0x929D30 callers 0x961121 - 0x9627AB (37 sites); 0x962F37 / 0x962D45 / 0x9631BC: `push 0x1B; push 0x2B; call [vtable+0x18]` on 0x7FF060's channel object, then 0x9205A0]

## 4. Open questions

- What the game does with a damaged `GameSettings.ini` or `settings.ini` value (a word where a number is expected, an
  unknown `DxVersion`): the parsers were not traced past the type switch. Needs a test start with a copy of the
  files - in-game tests are paused.
- `GameSettings.bin`'s layout (the `GstBinary` category; DICE's partition defines no binary setting).
- Whether a generic `-<Object>.<Field> <value>` command-line option sets the engine settings objects
  (`IsCommandLineOption`), and which ones.
- `Sensitivity0` - `Sensitivity2` and `Scheme0Sensitivity` / `Scheme0InputType` are named in INP-SET-1 by what reads
  them; which slider of the controls page writes which is inferred.
- `GstMenuTracker` has no section name (SET-GS-2): whether its settings are saved at all (they are not user-exposed).
