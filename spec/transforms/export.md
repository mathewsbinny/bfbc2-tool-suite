# Map export: a project built into a new multiplayer level

`mapexport build <project> <export.json> --game <game> --out <stage>` (`bc2_levelwrite::export::build`, also the
editor's Export window) takes a map project (`transforms/projectlog.md`), an export file (level id, name, mode,
profile, pictures) and the game copy, and writes a **stage**: the new level's four bundles (`level`, `terrain`,
`ingame`, `loader`; a WW2 donor's `vo/en` too), `stage.json` (each file's sha1 and entry count, the registration an
install adds, the closure and announcer copies), `report.txt` and `registration.txt`. It reads the game copy and
never writes it. `mapexport check <stage> --game <game>` proves a stage offline; OPFOR's `mapedit_install.py`
installs it (`transforms/install.md`). The design and the rounds that built it: EDITOR_ARCHITECTURE 3.12 (E3 to E3d).

The data-model rules it obeys are layer 2's: registration REG-NEW-1 to REG-NEW-5 and the checksum CSUM-CHK-1 to
CSUM-CHK-4 (`model/registration.md`), hidden partitions BNDL-DEL-4, files found by the level's name PART-NAME-3,
dangling stock references REF-RES-3, the game-mode layer LEVEL-MP-1.

## 1. Preconditions

### XEXP-PRE-1: The name is new and well-formed

Status: **verified**

The level id is 3 to 40 letters, digits or `_`, upper-cased; the export refuses an id that is a level of the game
copy (a `level-00.fbrb` in Dist, or in Package unless it is an earlier install of an export) or that another tool
registered in `package.mft` (REG-NEW-2). The level's display name and description are 1 to a maximum number of
printable ASCII characters (they are written as plain text into every language's table).

[code: rust/bc2-levelwrite/src/export.rs:155-183] [code: rust/bc2-levelwrite/src/export.rs:831-846]
[test: rust/bc2-levelwrite/src/export.rs::root_files_and_names] [test: rust/bc2-levelwrite/tests/export_e3b.rs::a_vietnam_export_with_its_own_name_and_picture]

### XEXP-PRE-2: Only modes an era has; only a donor of that mode

Status: **verified**

The target is an era (BFBC2, BFBC1, Vietnam, WW2), a mode and a donor level. A mode is offered for an era only when
a stock level of that era has it (WW2: Conquest only); Onslaught (needs bots) and Air Superiority (pending OPFOR's
Pacific War work) are refused with the reason; a donor that is not a level of that era and mode is refused with the
list of valid ones. Nothing is invented from another era's layer.

[code: rust/bc2-levelwrite/src/export.rs:859-875] [code: rust/bc2-levelwrite/src/modelayer.rs:97]
[test: rust/bc2-levelwrite/tests/export_e3d.rs::each_eras_modes_and_their_layers] [test: rust/bc2-levelwrite/tests/export_e3d.rs::rush_to_conquest_as_a_project_operation]
[test: rust/bc2-levelwrite/src/modelayer.rs::mode_names_and_keys]

## 2. What the build does

### XEXP-BLD-1: Campaign scripting out, the mode layer in

Status: **verified**

On a campaign base the campaign's scripting (the 60 types of `mpconvert::CAMPAIGN_SCRIPTING`, grouped in the
campaign checklist; the user may keep groups such as campaign vehicles) is deleted with its event connections and
references (REF-DEL-1, LEVEL-EVT-3), and the donor's mode layer comes in with new guids (UUID v5); with no choice in
the log the result is E3's. The level's fields and description come from the donor (game mode, factions, kits,
licences, matchmaking id, sound states), with the base's map, minimap, music and the export's name.

[code: rust/bc2-levelwrite/src/mpconvert.rs:1-40] [test: rust/bc2-levelwrite/tests/export_e3b.rs::the_campaign_checklist]
[test: rust/bc2-levelwrite/tests/export_sp01.rs::sp_01_exports_as_a_new_conquest_level] [test: rust/bc2-levelwrite/src/mpconvert.rs::groups_cover_the_lists]

### XEXP-BLD-2: Files found by the level's name are copied under the new name

Status: **verified**

The root files the engine finds by the level's name (the shader database collection and its databases, the emitter
system and atlases, model groups, static cull trees, the animation sound map; PART-NAME-3) are copied under
`Levels/<LV>/` with their own names re-pointed; the level partition gets a new partition guid and `LevelData.Name`
`Levels/<LV>`; the base's other bundles are read at run time, not copied (the level block targets them, as DICE's
mode levels target their base map's).

[code: rust/bc2-levelwrite/src/export.rs:632-673] [test: rust/bc2-levelwrite/src/export.rs::root_files_and_names]
[test: rust/bc2-levelwrite/src/mpconvert.rs::names_follow_the_level_and_case]

### XEXP-CLO-1: The closure: what does not resolve is copied, never registered

Status: **verified**

From the level's own partitions, every partition named is looked up in the bundles the level block loads; one found
in the level's own or its base's bundles is walked in turn, one in a shared bundle (`_common`, `async`) stops the
walk. A partition that does not resolve (hidden by a deleted marker of an earlier bundle, BNDL-DEL-4; or in no
loaded bundle: another era's commons, another level's object, a mode level's data) is copied into the new level's
bundle with its resources, from, in order: the level block's bundles, the donor's own bundle, the base's bundles,
then any level bundle of the game copy. Another level's or era's bundles are never registered (that would load
their deleted markers). A partition only the base names that the stock base lacks too is DICE's dangling reference
(REF-RES-3): listed, never filled. Every copy is listed in `stage.json` and `closure.txt` with its source and reason.

[code: rust/bc2-levelwrite/src/export.rs:1592-1723]
[test: rust/bc2-levelwrite/tests/export_e3b.rs::a_vietnam_export_with_its_own_name_and_picture] [test: rust/bc2-levelwrite/tests/export_e3c.rs::a_ww2_export_takes_its_donors_announcer]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1206-1229]

### XEXP-VO-1: The announcer is the target era's

Status: **verified**

The donor's own announcer partitions the layer reaches go into the new level's bundle (which loads first), and every
announcer partition whose version in the level's loaded bundles is not the donor's own is replaced by the donor's;
the donor's `vo/<lang>` blocks are registered under the new name and its own voice-over bundles staged byte for
byte. Each set's announcer resolves to the same bytes as on its donor (BFBC2 13 partitions from `mp_common`,
Vietnam 13 from `nam_common`, WW2 12, BFBC1 13).

[code: rust/bc2-levelwrite/src/export.rs:1801-1862] [code: rust/bc2-levelwrite/src/export.rs:2836-2857]
[test: rust/bc2-levelwrite/tests/export_e3c.rs::each_sets_announcer] [test: rust/bc2-levelwrite/tests/export_e3c.rs::a_ww2_export_takes_its_donors_announcer]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1119-1140]

### XEXP-REG-1: The registration: one block and one line per bundle, written for the install

Status: **verified**

The stage records, for each bundle kind, the `package.mft` block and the `bundleManifest` line an install adds
(REG-NEW-1, REG-BM-3): the level's own Package bundle first, then the base's bundles of the kind, then the profile's
common bundles; no other era's commons; for voice-over the donor's blocks (REG-NEW-3 as revised by E3c). The stage
writes no game file.

[code: rust/bc2-levelwrite/src/export.rs:746-800] [test: rust/bc2-levelwrite/src/export.rs::root_files_and_names]
[test: rust/bc2-levelwrite/tests/export_sp01.rs::sp_01_exports_as_a_new_conquest_level]

### XEXP-REG-2: No voice-over block without its bundleManifest line

Status: **verified**

The game copy's `Dist\win32\bundleManifest` is read whole or the export stops: a file that is there but does not
read is an error, never an empty list (round DF1). A donor whose `package.mft` has voice-over
blocks on a game copy without a `bundleManifest` is refused, so a stage never carries `package.mft` blocks whose
bundles `bundleManifest` does not name (REG-BM-3's checksum crash).

[code: rust/bc2-levelwrite/src/export.rs:260-283] [code: rust/bc2-levelwrite/src/profile.rs:106-115]
[test: rust/bc2-levelwrite/tests/df1.rs::a_missing_or_unreadable_bundle_manifest_is_never_read_as_empty]

### XEXP-ID-1: The level's own name and pictures

Status: **verified**

With an own name, the description's and level's `Name` / `Description` keys are `<LV>_NAME` / `<LV>_DESC`, their
texts added in every language of `overlay-00`; the loading picture is `Awards/images/loadingScreens/<lv>` and the
map-list picture `Awards/images/Levels/<lv>`, made from the user's picture in the donor pictures' form (DXT1 1280 x
720; DXT5 512 x 256 with the picture in the alpha's box) and added to `ondemand_awards-00`. The patched overlay and
awards bundles differ from their sources only in the level's keys and entries.

[code: rust/bc2-levelwrite/src/export.rs:3505-3618] [test: rust/bc2-levelwrite/src/export.rs::overlay_and_awards_patches]
[test: rust/bc2-levelwrite/src/awards.rs::pictures_in_a_template_form] [test: rust/bc2-levelwrite/tests/export_e3b.rs::a_vietnam_export_with_its_own_name_and_picture]

### XEXP-RO-1: The game copy is read only

Status: **verified**

Build and check never write the game copy: after a build, check and install plan, all 1,582 files of `game\` keep
their size and write time and the key shared files their sha256.

[test: rust/bc2-levelwrite/tests/export_sp01.rs::sp_01_exports_as_a_new_conquest_level]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:983-985]

## 3. The check

### XEXP-CHK-1: What a stage must pass

Status: **verified**

`mapexport check` passes a stage only when: each staged file is the one recorded and its bundle opens with every
entry read and every partition parsing; the registration has a block and a line for each kind and every target
bundle exists; the name is no level of the game copy; every local reference names an instance of its partition;
every external reference names a partition of a bundle the level loads, and an instance in it (stock references
to partitions no bundle has are allowed only where DICE's own source partition has them); `LevelData.Name` and the
description's `LevelName` are `Levels/<LV>`; the closure resolves; the terrain's height field is loaded; the mode
layer is complete against the donor's; the profile's commons, game mode, factions, kits and announcer are the
target era's; the cull tree check (XCULL-RES-2) passes; each re-slotted object stands in its own blueprint's
shape and no new object is at slot 0 that some level slots (XCULL-RES-3); every mesh the level reaches has entries
in its shader databases (SHDB-LVL-2). Each failing line is stated in words.

[code: rust/bc2-levelwrite/src/export.rs:2225-2659] [code: rust/bc2-levelwrite/src/export.rs:2679-2807]
[test: rust/bc2-levelwrite/tests/export_sp01.rs::sp_01_exports_as_a_new_conquest_level] [test: rust/bc2-levelwrite/tests/export_e3b.rs::a_vietnam_export_with_its_own_name_and_picture]
[test: rust/bc2-levelwrite/tests/export_e3c.rs::a_ww2_export_takes_its_donors_announcer]

### XEXP-CHK-2: A mode layer must be complete for its mode

Status: **verified**

The mode-layer check compares the level's layer with the donor's and names each missing piece: a Conquest team
without a base, a Conquest level without a flag, a Rush stage with fewer M-COMs than the donor's, a Squad
Deathmatch with fewer teams. Conquest may lose flags, never a base.

[code: rust/bc2-levelwrite/src/modelayer.rs:590] [test: rust/bc2-levelwrite/tests/export_e3d.rs::rush_to_conquest_as_a_project_operation]

## Open questions

- The in-game proof: no exported level has yet been loaded in the game by the user (EDITOR_ARCHITECTURE 3.12,
  "Open for the in-game test"); the start-up checksum (CSUM-CHK-1) is proven only through the registration rules.
- Bots: `HasAI` stays false; the NavKit hook after the strip (XNAV) is not wired into the export yet.
