# Options and settings: what exists, where it persists, how the menus edit it

Which settings BFBC2's PC client has, where each kind is kept, how the files are read and written, the defaults and
how the options screens reach them. The input bindings and the controls screen are [input.md](input.md); the
engine-movie events and extern getters in general are [menus.md](menus.md) (MENU-EVT-1 to MENU-EVT-3).

**Sources.** Round SPG3 (2026-11-23) read `BFBC2Game.exe` (PC build 795745) statically on a scratch copy (string and
reference searches, the call index, disassembly of the readers and writers), DICE's partitions of the startup bundle
with `spec/probes/options.py` (whose `checks()` hold the counts below), Project OPFOR BFBC2's research notes
(`research/BFBC2_DECOMP_RESEARCH.md` sections 2, 3 and 10, `DEVELOPMENT_NOTES.md` "User data separation") and the
**key names only** of one `GameSettings.ini` and one `settings.ini` the game wrote (read only, no value read). Nothing
was run. Round SPG3-rest (2026-11-27) read the parsers, `GameSettings.bin`'s reader and writer and the registry's text
path the same way (capstone over a scratch copy; REA's Ghidra provider timed out on the 19 MB executable), and walked
the structure of one `GameSettings.bin` the game wrote (names and sizes only).

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
missing key keeps the default (a value that does not parse: SET-PARSE-1). It then reads `GameSettings.bin`
(0x815870), the settings that are not user-exposed (SET-BIN-1). The writer 0x833FD0
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

### SET-BIN-1: GameSettings.bin holds the settings the INI does not: a typed list per category

Status: **verified** · Variants: PC

`GameSettings.bin` is written by the same save as the INI (0x833FD0 -> 0x815940) and read after it (0x815870), both
through the file system's asynchronous jobs. The serializer 0x80DA10 writes, for each of the 11 categories in
enumeration order (`GstAudio` 0 ... `GstMenuTracker` 9, `GstStatsCache` 10; `GstCount` = 11), a u32 count (a
placeholder written first, then patched) and per setting whose `UserExposed` byte (+0x28) is clear:

| Field | Type |
|---|---|
| type | u32: 1 float, 2 int, 3 string, 4 binary (the setting's +0xC) |
| name length | u32, the NUL included |
| name | bytes, NUL-terminated |
| value (type 4) | u32 size, then the bytes |
| value (types 1 - 3) | u32 length, the NUL included, then the value as text (a float as `%f`, e.g. `0.200000`) |

So the INI holds the user-exposed settings and the `.bin` the rest: SET-GS-1's 14 hidden settings and everything the
code adds unexposed at run time. The reader 0x832270 walks the same 11 categories and sets a binary setting through
0x82FFA0 and any other from its text through 0x82FCF0 (the parsers of SET-PARSE-1); it runs only when the read job
answers with a result of kind 8 whose second word is 0xD (0x834A4D - 0x834A5F), and either way the load posts its
done message on channel 0x2B. A file the game wrote starts with 16 bytes before the list, `08 00 00 00`, `FF FF FF
FF`, `0D 00 00 00` and the list's length, which the job layer's result checks match (inferred).

That file holds: `GstInput` 3 (the hidden `Scheme1Sensitivity`, `Scheme1FlipY`, `AlternativeHeliControls`),
`GstGameplay` 3, `GstPlayerProfile` 5, `GstPersistence` 2,302 strings, `GstBinary` 2 (`header` 50 bytes and `body`:
the campaign save, the two names the single-player code uses, among others 0x55D250, 0x6E5900, 0x75FFA0 near
`onStartLoadingSP`, and 0x951E00), `GstMenuTracker` 4, `GstStatsCache` 701 strings; the other categories 0. The
campaign save's own layout is left to the campaign's rules (SPG4-rest).

[test: core/tests/test_spec_probes.py::options]
[exe: BFBC2Game.exe category enumeration 0x01C38890 - 0x01C38998 (GstAudio 0 ... GstStatsCache 10, GstCount 11); serializer 0x80DA10 (`mov [esp+0x24], 11` at 0x80DA2E, `cmp byte [esi+0x28], 0` at 0x80DA8F, type 4 at 0x80DAE4 -> 0x80D660, else 0x80D4B0); write 0x815940 (caller 0x83449E, job id +0xB18); read 0x815870 (caller 0x832191, job id +0xB14); reader 0x832270 (`mov [esp+0x28], 11` at 0x832292, type 4 at 0x832309 -> 0x82FFA0, else 0x82FCF0; caller 0x834A8F in the job completion 0x8345A0, checks 0x834A48 / 0x834A5B); INI writer 0x833FD0 (`cmp eax, 11` at 0x83431C); "header" 0x141841C / "body" 0x1418414 read by 0x55D250, 0x75FFA0, 0x6E5900 and the binary getter 0x80D660's other callers]
[data: spec/probes/options.py, 2026-11-27: the enumeration (12 names, GstCount 11) and the code bytes cited]
[data: one GameSettings.bin the game wrote (`<Documents>\BFBC2`, 242,795 bytes; a second in OPFOR's user folder, 159,208 bytes), walked by this layout to its last byte; structure, names and sizes only, no value read: the counts above]

### SET-PARSE-1: What a bad value does: a number that does not parse becomes 0

Status: **observed** · Variants: PC

The game settings' text values (the INI's and the `.bin`'s) go through the engine's own type parsers: the float
parser 0x4E5680 and the int parser 0x4E6C90, each first stripping one pair of surrounding `"`:

- an int takes white space, an optional `+` / `-` and decimal digits, stops at the first other character
  (`12abc` is 12) and fails when there is no digit or the value leaves 32 bits;
- a float is `strtod`'s: it fails only when nothing at the start parses (`1.5x` is 1.5, `1e999` is infinity);
- a string setting is taken as written.

The callers 0x512360 (int) and 0x512320 (float) return their second argument when the parse fails, and the INI reader
passes 0 (0x832227) and 0.0 (0x832241): a word where a number is expected sets the setting to 0, not to its default,
and the next save writes the 0. An empty value (`Key=`) keeps the default, because `GetPrivateProfileStringA` then
returns no characters and the reader skips the key (0x831FE4); a missing key also keeps the default. The setters
(0x82FAB0 int, 0x82F860 float) store what they are given: no range is checked here, so a slider value out of range
reaches its reader unclamped.

`settings.ini`'s reader 0x7DBD60 uses the same two callers with 0 as the fallback. `DxVersion` is `auto` (the device
is detected) or else parsed as an int: 9 -> `SelectedRenderer_Dx9` (1), 10 -> `SelectedRenderer_Dx10` (2), 11 ->
`SelectedRenderer_Dx11` (4), anything else, a word included, -> `SelectedRenderer_None` (0), written to
`Render.Renderer` (`GameRenderSettings` +0x6C). What the renderer does with `None` at device creation was not traced.

[exe: BFBC2Game.exe float parse 0x4E5680 (`strtod` IAT 0x14054F0; fails when the end pointer did not move), int parse 0x4E6C90 -> scanner 0x428A40 (`isspace`, sign, digits, 64-bit overflow check) and the 32-bit range check at 0x4E6D07 - 0x4E6D2A; both strip `"..."` at 0x4E569F / 0x4E6CB0; 0x512320 / 0x512360 (default on failure); INI reader type switch 0x831FEA with defaults pushed at 0x832227 (0) and 0x832241 (fldz); skip on 0 characters at 0x831FE4; setters 0x82FAB0, 0x82F860; DxVersion: "auto" 0x142FBE8 at 0x7DCC87, 0x512360 at 0x7DCCF2, map 0x7DCCFA - 0x7DCD1A, "Render.Renderer" 0x142FBD8 set through 0x7DA7E0; enumeration SelectedRenderer_None 0 ... _Ps3 6 (0x01C15370 - 0x01C15400), GameRenderSettings.Renderer +0x6C]

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
`Description`, `ValueType`, `DefaultValue`, `TargetPlatforms`, `IsCommandLineOption`, `IsFlag`); a generic
`-<Object>.<Field>` option does not set them (SET-CLI-2).

[exe: BFBC2Game.exe argument list 0x155D008 / 0x155D00C; finder 0x4F4CF0 (`cmp cl, 0x2D` / `0x2B` at 0x4F4D52 / 0x4F4D5B; compare 0x508160); its 41 callers (the names pushed before each; SET-CLI-2); entry tests 0x401F0C, 0x401F29, 0x401F48, 0x401F65; "-noAsync" 0x14123AC at 0x501F86, "-host" 0x1412418 at 0x5022A8; OptionDescriptor 0x1412E24, its fields from 0x01C13438]

### SET-CLI-2: No command-line option sets an engine setting by name

Status: **verified** · Variants: PC

`-<Object>.<Field> <value>` (for example `-VisualTerrain.UndergrowthDensityScale 2`) changes nothing in the retail
client. The argument list (SET-CLI-1) is read in two places only: the finder 0x4F4CF0, whose 41 call sites each push
a fixed option name of their own (the names of SET-CLI-1 and `extra_account_detail`, `name`), and 0x4FDD10, which
copies it into the start-up log line. Nothing walks it for `<Object>.<Field>` words.

The settings registry (the object at `*(0x155CFDC)` that SET-ENG-1's lookups use) does have a text path for
`<Object>.<Field> <value>` pairs, but nothing reaches it:

- a reader 0x5017C0 of a settings text file: one pair per line, a line starting with `#`, `;` or `/` skipped, a `#`
  later in the line ends it, the key and the value split at the first white space;
- an adder 0x501AB0 of one pair;
- both keep the pairs in a pending list (registry +0x44, 0x18-byte records); each settings object registered later
  takes the pairs whose `<Object>` is its name (0x4F4A00, case-blind), and 0x4F3FA0 sets the field through the
  class's field parser (0x4EB0F0), which logs `Field '%s' not found in class '%s'` or `Parse error '%s.%s', value =
  '%s'` on a bad pair.

The reader and the adder are reached only through two this-adjusting thunks (0x501CF0, 0x501D10) that no call, jump
or stored pointer names, so the retail build never reads such a file or adds such a pair; the `OptionDescriptor`
type (`IsCommandLineOption`) is registered but no code reads one. What does set engine settings by name is the code
itself, through the registry's value setter 0x4F4C00: `settings.ini`'s display values (`DX9DisplaySettings.*` and its
Dx10 / Dx11 twins, `Render.Renderer`: SET-PARSE-1), `Network.Version` (`"ROMEPC795745"`) and
`Core.DiskLayoutEmulation`. OPFOR's DLL writes the objects' fields directly (SET-ENG-1). So VEG-UG-8's render settings,
like every other engine setting, are out of the user's reach without the DLL.

[test: core/tests/test_spec_probes.py::options]
[exe: BFBC2Game.exe finder 0x4F4CF0 (41 E8 sites, each with a `push imm32` naming a string within 16 bytes before it); 0x4FDD10 (one caller, 0x56EF29: the log line); registry text reader 0x5017C0 (`cmp cl, 0x23 / 0x3B / 0x2F` at 0x501859 - 0x50186B, the pending list +0x44, then 0x4F6EC0), adder 0x501AB0, thunks 0x501CF0 / 0x501D10 (no E8 / E9 site, no stored pointer); apply 0x4F4A00 ("." 0x140BA68), 0x4F3FA0; field parser 0x4EB0F0 ("Field '%s' not found in class '%s'" 0x14117D4, "Parse error '%s.%s', value = '%s'" 0x14117B0); OptionDescriptor type record 0x1BF0638 (its only reference the static initialiser at 0x13865D1); value setter thunk 0x4F4C00 -> 0x4F41E0 (callers 0x5595AE and 0x7DA82E - 0x7DA9AE, the latter from the settings.ini reader 0x7DBD60 and 0x7DE0D4); "Network.Version" set through 0x4F4BD0 at 0x5658E2, 0x5659C2, 0x565B29]
[data: spec/probes/options.py, 2026-11-27: 41 finder call sites, 41 with a pushed name; the reader and the adder reached only from their thunks, the thunks from nothing]

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

- Round SPG3-rest (2026-11-27, static only) answered the bad values (SET-PARSE-1), `GameSettings.bin`'s layout
  (SET-BIN-1), the generic `-<Object>.<Field>` option (SET-CLI-2: none) and the sliders (INP-SET-2). Left: what
  the renderer does with `SelectedRenderer_None` (an unknown `DxVersion`); what a damaged `.bin` does (a count or a
  length past the file's end: the stream reader's bounds were not traced); both want a test start with copies of
  the files (in-game tests are paused).
- `Scheme0Sensitivity` / `Scheme0InputType` are named in INP-SET-1 by what reads them; the controls page's slider
  command writes only `Sensitivity0` - `Sensitivity5` (INP-SET-2), so what else writes them is open.
- `GstMenuTracker` has no section name (SET-GS-2): whether its settings are saved at all (they are not user-exposed).
