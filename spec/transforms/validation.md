# Export validation: the rules an exported level must keep

Round V1 gathered every way an exported level can fail in the game, or be refused by the export, into one rule
registry (`bc2_levelwrite::rules`): each rule a code, a severity (error, warning or note), a plain-language message
saying what goes wrong in the game and what to do, the specification rules it rests on, and where it is checked:

- **live in the editor** (`bc2_levelwrite::validate`, the Problems panel; `mapexport validate <project> --game
  <game>`): on the project as edited, after every change - the log's own problems, the mode, the names, the objects
  the edits moved or made, the partition's own consistency (DbxEditGUI's Level > Check), and DICE's placement
  tendencies (section 3);
- **by `mapexport build`** before it writes: a broken rule is a refusal whose words start with the rule's code;
- **by `mapexport check`** on the staged level: every check line carries its rule's code;
- **by the DBX inspector's Level > Check** (round GR4) on a partition opened by hand.

`mapexport rules` lists the registry. A test keeps this file and the registry the same (every XVAL rule below is a
registry rule with the same title, and the other way round:
`rust/bc2-levelwrite/tests/v1_rules.rs::the_registry_and_the_spec_agree`).

What the rules hold off, by crash address: the start-up checksum crash of a level registered with missing bundles
or not registered on both sides (CSUM-CHK-1, CSUM-CHK-2, CSUM-CHK-4), a blueprint that resolves nowhere (0x6EFCA4,
REF-RES-5), a partition that does not parse or streamed wave data in a level bundle (0x49EDF4: LOAD-PART-1,
SND-STRM-1), a null wave (0xFF2AC2, SND-NULL-1), a GameMode that does not resolve (0x74EAB3, LEVEL-DESC-2), a part
with no shader program (0x98BD6E, RNDR-DRAW-6), a group's part counts not matching its members (0xAB4E58,
LEVEL-MG-4); and the black ones: a black loading screen (LVL-DESC-2), black tiled terrain cells (TERR-VT-5).

Left for later rounds: engine limits on instances, spawns and event connections (they need reverse engineering
first), HQ and spawn placement over water or inside rocks, a vehicle spawner's room and domain, an exported level's
memory against the stock levels', Xbox 360 meshes in a staged bundle (their big-endian layout is not detected yet),
and L1-HT-001 (a flag's height above the ground around it, which needs the terrain's height field).

## 1. Severities

An **error** is a refusal of the build, or a stage that fails its check: the game would crash, misbehave, or the
level would not load as made. A **warning** is a level that may play badly or not as meant; the export goes ahead. A
**note** is worth knowing and nothing is wrong (the stage's server-and-client line; a layout inside L1's warning band
but outside DICE's stock range).

## 2. The export's rules (XVAL)

### XVAL-EDIT-1: The base level changed since the project was made

Status: **verified**

Warning (round OP1); checked live in the editor and by `mapexport validate`. The base level's partition is not the one
the edits were made on (a game patch or a rebuilt level). The edits are kept; check that each still does what you
meant. Rests on XLOG-BASE-1.

[code: rust/bc2-levelwrite/src/rules.rs:147] [code: rust/bc2-levelwrite/src/validate.rs:335]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_build_refusals]

### XVAL-EDIT-2: An operation names an object the level does not have

Status: **verified**

Error (round OP1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal). An edit
names an object the base level no longer has. The export refuses the project until that edit is undone or the object
is back. Rests on XLOG-BASE-1, XLOG-APP-1.

[code: rust/bc2-levelwrite/src/rules.rs:157] [code: rust/bc2-levelwrite/src/export.rs:1017] [code: rust/bc2-levelwrite/src/validate.rs:346]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_build_refusals]

### XVAL-EDIT-3: An operation does not apply

Status: **verified**

Error (round OP1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal). An edit
could not be applied to the level (its reason is given). The export refuses the project until it is undone. Rests on
XLOG-APP-1.

[code: rust/bc2-levelwrite/src/rules.rs:167] [code: rust/bc2-levelwrite/src/export.rs:1059] [code: rust/bc2-levelwrite/src/validate.rs:359]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_build_refusals]

### XVAL-EDIT-4: The level partition does not parse

Status: **verified**

Error (round V1); checked live in the editor and by `mapexport validate`; by `mapexport check`. The level's partition
could not be read, so no edit is written to it. A partition the game cannot parse is a null the loader passes on and
the load crashes (0x49EDF4). Rests on LOAD-PART-1.

[code: rust/bc2-levelwrite/src/rules.rs:177] [code: rust/bc2-levelwrite/src/validate.rs:117]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-EDIT-5: Terrain and paint edits are not exported yet

Status: **verified**

Error (round OP1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal). The
project edits the terrain or paints it, and the export does not write terrain yet (round E5): it refuses the project.
Undo the strokes to export the rest. Rests on XLOG-DELTA-1, TERR-VT-5.

[code: rust/bc2-levelwrite/src/rules.rs:187] [code: rust/bc2-levelwrite/src/export.rs:1042] [code: rust/bc2-levelwrite/src/validate.rs:64]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_build_refusals]

### XVAL-EDIT-6: Only the level's own root files are exported

Status: **verified**

Error (round OP1); checked by `mapexport build` (a refusal). The project edits a file the export does not write (only
the level partition and the base's own static group files are written). The export refuses it. Rests on XLOG-FILE-2,
LOAD-PART-5.

[code: rust/bc2-levelwrite/src/rules.rs:197] [code: rust/bc2-levelwrite/src/export.rs:1084]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it]

### XVAL-MODE-1: Only modes the export offers

Status: **verified**

Error (round E3d); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal). Onslaught
needs bots (NavKit path data and the AI layer) and Air Superiority is BF1943's: the export refuses both. Rests on
GAME-MODE-8.

[code: rust/bc2-levelwrite/src/rules.rs:208] [code: rust/bc2-levelwrite/src/export.rs:957] [code: rust/bc2-levelwrite/src/validate.rs:78]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_build_refusals]

### XVAL-MODE-2: The donor is a level of the era's mode

Status: **verified**

Error (round E3b); checked by `mapexport build` (a refusal); by `mapexport check`. The mode layer's donor must be a
stock level of the chosen era and mode; another level's layer would name kits, factions and game mode data the era
does not load. Rests on XEXP-PRE-2.

[code: rust/bc2-levelwrite/src/rules.rs:218] [code: rust/bc2-levelwrite/src/export.rs:964]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-MODE-3: The mode layer is complete

Status: **verified**

Error (round E3d); checked live in the editor and by `mapexport validate`; by `mapexport check`. A multiplayer level
needs its whole mode layer (player, mission, teams, bases, counters, spawns, and per mode its flags, stages or
squads); without it the session waits forever in state 14. Put back what is missing, or choose the mode again. Rests
on XEXP-CHK-2, GAME-LAYER-1, GAME-LAYER-4, LEVEL-MP-1.

[code: rust/bc2-levelwrite/src/rules.rs:228] [code: rust/bc2-levelwrite/src/export.rs:2981] [code: rust/bc2-levelwrite/src/validate.rs:99]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-MODE-4: The donor level changed since the mode was chosen

Status: **verified**

Warning (round V1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal). The
donor's partition is not the one the mode was chosen from (a patch or a rebuilt WW2 level): the layer is grafted from
it as it is now. Choose the mode again to see what changed. Rests on XEXP-PRE-2, XLOG-BASE-1.

[code: rust/bc2-levelwrite/src/rules.rs:239] [code: rust/bc2-levelwrite/src/validate.rs:83]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it]

### XVAL-MODE-5: The export file's combat area is in the base

Status: **verified**

Error (round E3); checked by `mapexport build` (a refusal). An export file without a project mode grafts the donor's
layer into the base's combat area it names; that area must be in the base level. Rests on XEXP-BLD-1.

[code: rust/bc2-levelwrite/src/rules.rs:249] [code: rust/bc2-levelwrite/src/export.rs:1157]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it]

### XVAL-REG-1: One package.mft block and one bundleManifest line per bundle

Status: **verified**

Error (round E3); checked by `mapexport check`. Each of the level's bundles needs its package.mft block and its
bundleManifest line, or the server crashes computing the level checksum at start-up. Rests on REG-NEW-1, CSUM-CHK-1,
CSUM-CHK-2.

[code: rust/bc2-levelwrite/src/rules.rs:260] [code: rust/bc2-levelwrite/src/export.rs:2741]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-REG-2: Every bundle the registration names exists

Status: **verified**

Error (round E3); checked by `mapexport check`. A registered bundle that is missing crashes the server's level
checksum at start-up. Rests on CSUM-CHK-2, REG-MFT-4.

[code: rust/bc2-levelwrite/src/rules.rs:270] [code: rust/bc2-levelwrite/src/export.rs:2755]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-REG-3: A new name, never a level of the game

Status: **verified**

Error (round E3); checked by `mapexport build` (a refusal); by `mapexport check`. An export never replaces a level of
the game copy or one another tool registered: choose another id. Rests on REG-NEW-2, XEXP-PRE-1.

[code: rust/bc2-levelwrite/src/rules.rs:279] [code: rust/bc2-levelwrite/src/export.rs:934]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_build_refusals]

### XVAL-REG-4: Server and client need the same registration

Status: **verified**

Note (round V1); checked by `mapexport check`. Install the level on the server and on every client: both register its
bundles in bundleManifest and package.mft, or the level checksums differ (and a server without them crashes computing
its checksum). Rests on CSUM-CHK-4, CSUM-CHK-1.

[code: rust/bc2-levelwrite/src/rules.rs:288] [code: rust/bc2-levelwrite/src/export.rs:3142]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-REG-5: The announcer's registration is the donor's

Status: **verified**

Error (round E3c); checked by `mapexport check`. The level's voice-over blocks and manifest lines must be its donor's
under the new name, with no block whose bundleManifest line is missing. Rests on XEXP-REG-2, SND-VO-3.

[code: rust/bc2-levelwrite/src/rules.rs:298] [code: rust/bc2-levelwrite/src/export.rs:3722]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-NAME-1: The level id's form

Status: **verified**

Error (round EX1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal); by
`mapexport check`. The level id is a file, folder and localisation key: 3 to 40 of A-Z, 0-9 and _, starting with a
letter. Rests on XEXP-PRE-1, LVL-ID-1.

[code: rust/bc2-levelwrite/src/rules.rs:309] [code: rust/bc2-levelwrite/src/export.rs:3889] [code: rust/bc2-levelwrite/src/names.rs:381]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-NAME-2: No _NAME in the level id

Status: **verified**

Error (round EX1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal); by
`mapexport check`. The loading screen cuts the name key at its first _NAME to find its picture: an id holding _NAME
gives a black loading screen. Rests on LVL-DESC-2.

[code: rust/bc2-levelwrite/src/rules.rs:318] [code: rust/bc2-levelwrite/src/names.rs:417]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-NAME-3: The level id clashes with nothing the game has

Status: **verified**

Error (round EX1); checked by `mapexport build` (a refusal); by `mapexport check`. The id must not be a level, another
project's export, a loading picture or a text the game already has. Rests on REG-NEW-2, XEXP-PRE-1.

[code: rust/bc2-levelwrite/src/rules.rs:328] [code: rust/bc2-levelwrite/src/names.rs:430]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/tests/export_ex1.rs::names_are_checked_against_the_game_copy]

### XVAL-NAME-4: The folder is the id in lower case

Status: **verified**

Error (round EX1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal); by
`mapexport check`. The game finds a level's bundles in Package\levels\<id>\: the folder cannot differ from the id.
Rests on LVL-ID-1.

[code: rust/bc2-levelwrite/src/rules.rs:337] [code: rust/bc2-levelwrite/src/names.rs:479]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-NAME-5: The server-browser name's form

Status: **verified**

Error (round EX1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal); by
`mapexport check`. maps.ini's name: plain ASCII, no leading or trailing space, no quote, a browser row of at most 159
characters. Rests on XEXP-ID-1.

[code: rust/bc2-levelwrite/src/rules.rs:346] [code: rust/bc2-levelwrite/src/names.rs:491]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-NAME-6: The server-browser name is not another map's

Status: **verified**

Error (round EX1); checked by `mapexport build` (a refusal); by `mapexport check`. Two maps with one name give server
browser rows that look the same. Rests on XEXP-ID-1.

[code: rust/bc2-levelwrite/src/rules.rs:356] [code: rust/bc2-levelwrite/src/names.rs:537]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/tests/export_ex1.rs::names_are_checked_against_the_game_copy]

### XVAL-NAME-7: The on-screen names fit every language

Status: **verified**

Error (round EX1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal); by
`mapexport check`. The on-screen name (64) and description (400) must fit and use only letters each language's texts
have, or they do not show. Rests on XEXP-ID-1.

[code: rust/bc2-levelwrite/src/rules.rs:365] [code: rust/bc2-levelwrite/src/names.rs:552]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-NAME-8: Every place names the level the same

Status: **verified**

Error (round EX1); checked by `mapexport check`. The description, the menus' level list, every package.mft block and
bundleManifest line and the name key's loading picture must name the same level: a mismatch is the start-up checksum
crash or a black loading screen. Rests on XEXP-ID-1, REG-NEW-5, LVL-DESC-2.

[code: rust/bc2-levelwrite/src/rules.rs:375] [code: rust/bc2-levelwrite/src/export.rs:3939]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-NAME-9: The level's own name keys and pictures

Status: **verified**

Error (round E3b); checked by `mapexport check`. With its own name the level's description and partition use its own
keys, every text is stored in each language's encoding, and its pictures are in their templates' form. Rests on XEXP-
ID-1.

[code: rust/bc2-levelwrite/src/rules.rs:386] [code: rust/bc2-levelwrite/src/export.rs:3967]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-FILE-1: The staged files are the ones recorded

Status: **verified**

Error (round E3); checked by `mapexport check`. A staged bundle changed after the export: build the stage again. Rests
on XEXP-CHK-1.

[code: rust/bc2-levelwrite/src/rules.rs:397] [code: rust/bc2-levelwrite/src/export.rs:2640]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-FILE-2: Every staged bundle opens and every partition parses

Status: **verified**

Error (round E3); checked by `mapexport check`. A partition that does not parse is a null the loader passes on: the
load crashes (0x49EDF4). Rests on LOAD-PART-1, XEXP-CHK-1.

[code: rust/bc2-levelwrite/src/rules.rs:406] [code: rust/bc2-levelwrite/src/export.rs:2680]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-REF-1: Local references name an instance of their partition

Status: **verified**

Error (round E3); checked by `mapexport check`. A reference to an instance the partition does not have is a null the
game reads. Rests on REF-RES-1, REF-DEL-2.

[code: rust/bc2-levelwrite/src/rules.rs:416] [code: rust/bc2-levelwrite/src/export.rs:2797]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-REF-2: DICE's own dangling references

Status: **verified**

Note (round E3); checked by `mapexport check`. References to partitions no bundle of the game has, made exactly as
DICE's source partitions make them: the stock levels load with them. Rests on REF-RES-3.

[code: rust/bc2-levelwrite/src/rules.rs:425] [code: rust/bc2-levelwrite/src/export.rs:2858]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-REF-3: Every referenced partition is in a bundle the level loads

Status: **verified**

Error (round E3); checked by `mapexport check`. A blueprint that resolves nowhere crashes the server's checksum
(0x6EFCA4). Rests on REF-RES-2, REF-RES-5, CSUM-CHK-3.

[code: rust/bc2-levelwrite/src/rules.rs:435] [code: rust/bc2-levelwrite/src/export.rs:2866]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-REF-4: Every external reference names an instance of its partition

Status: **verified**

Error (round E3); checked by `mapexport check`. A reference to an instance its partition does not have is a null the
game reads. Rests on REF-RES-2.

[code: rust/bc2-levelwrite/src/rules.rs:444] [code: rust/bc2-levelwrite/src/export.rs:2875]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-REF-5: The closure resolves under the profile's bundles

Status: **verified**

Error (round E3d); checked by `mapexport check`. Every partition the level reaches must resolve in the bundles it
loads; the export copies what does not, and never registers it. Rests on XEXP-CLO-1, REF-RES-5.

[code: rust/bc2-levelwrite/src/rules.rs:453] [code: rust/bc2-levelwrite/src/export.rs:2921]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-LVL-1: LevelData.Name is the level's path

Status: **verified**

Error (round E3); checked by `mapexport check`. LevelData.Name must be Levels/<id>, the name every other file uses.
Rests on LEVEL-DATA-2, LVL-ID-1.

[code: rust/bc2-levelwrite/src/rules.rs:464] [code: rust/bc2-levelwrite/src/export.rs:2893]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-LVL-2: The description names the level

Status: **verified**

Error (round E3); checked by `mapexport check`. The description's LevelName must be Levels/<id>, or the menus start
another level. Rests on LVL-DESC-1, LVL-ID-1.

[code: rust/bc2-levelwrite/src/rules.rs:473] [code: rust/bc2-levelwrite/src/export.rs:2905]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-LVL-3: The height field is in a bundle the level loads

Status: **verified**

Error (round E3d); checked by `mapexport check`. Without its height field the map loads without ground: keep the
base's terrain bundles. Rests on TERR-NAME-1, LOAD-PART-6.

[code: rust/bc2-levelwrite/src/rules.rs:482] [code: rust/bc2-levelwrite/src/export.rs:2961]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-PROF-1: Every block loads the era's common bundles

Status: **verified**

Error (round E3b); checked by `mapexport check`. Each package.mft block must load the era's common bundles in their
order, or the era's kits, vehicles and sounds do not resolve. Rests on XEXP-PRE-2, LVL-DESC-1.

[code: rust/bc2-levelwrite/src/rules.rs:492] [code: rust/bc2-levelwrite/src/export.rs:3557]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-PROF-2: The game mode is the donor's and resolves

Status: **verified**

Error (round E3b); checked by `mapexport check`. The description's GameMode must be the donor's and resolve: a
GameMode that does not resolve crashes (0x74EAB3). Rests on LEVEL-DESC-2, LOAD-START-2.

[code: rust/bc2-levelwrite/src/rules.rs:502] [code: rust/bc2-levelwrite/src/export.rs:3618]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-PROF-3: The factions are the donor's

Status: **verified**

Error (round E3b); checked by `mapexport check`. The description's factions must be the donor's, the era's teams.
Rests on XEXP-PRE-2.

[code: rust/bc2-levelwrite/src/rules.rs:512] [code: rust/bc2-levelwrite/src/export.rs:3623]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-PROF-4: The kit pictures are the donor's and resolve

Status: **verified**

Error (round E3b); checked by `mapexport check`. The description's kit pictures must be the donor's, each resolving in
the bundles the level loads. Rests on XEXP-PRE-2, LOAD-PART-4.

[code: rust/bc2-levelwrite/src/rules.rs:521] [code: rust/bc2-levelwrite/src/export.rs:3628]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-PROF-5: The era's level fields

Status: **verified**

Error (round E3b); checked by `mapexport check`. The level fields the era sets (Vietnam's sound states ...) must be
the era's. Rests on XEXP-PRE-2.

[code: rust/bc2-levelwrite/src/rules.rs:530] [code: rust/bc2-levelwrite/src/export.rs:3641]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-PROF-6: The announcer is the era's

Status: **verified**

Error (round E3c); checked by `mapexport check`. Every announcer partition the level names must resolve as on its
donor, all from one era's voices. Rests on XEXP-VO-1, SND-VO-2.

[code: rust/bc2-levelwrite/src/rules.rs:539] [code: rust/bc2-levelwrite/src/export.rs:3828]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-WTHR-1: The weather the export gave the level

Status: **verified**

Error (round VW2); checked by `mapexport check`. The level's weather state, its spawn switch and its copied partitions
must be what the export wrote. Rests on RNDR-WTHR-8, RNDR-WTHR-5.

[code: rust/bc2-levelwrite/src/rules.rs:549] [code: rust/bc2-levelwrite/src/export.rs:2943]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-CULL-1: One cull tree, one object per slot

Status: **verified**

Error (round E2); checked by `mapexport check`. Every slotted static object needs a slot of its own inside the tree: a
shared slot or one outside the tree draws the wrong object or none. Rests on CULL-SLOT-3, LEVEL-SLOT-2, LEVEL-SLOT-3.

[code: rust/bc2-levelwrite/src/rules.rs:559] [code: rust/bc2-levelwrite/src/export.rs:3034]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-CULL-2: A moved or placed object stands in its own blueprint's cull shape

Status: **verified**

Error (round XF1); checked by `mapexport check`. A re-slotted object must stand in leaves of its own blueprint's shape
where it stands, or it pops or vanishes as the camera moves. Rests on CULL-ADD-1, CULL-SPH-1.

[code: rust/bc2-levelwrite/src/rules.rs:569] [code: rust/bc2-levelwrite/src/export.rs:3394]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/tests/export_xf1.rs::a_cross_set_object_gets_its_entries_and_shape]

### XVAL-CULL-3: Every new static object has a cull slot

Status: **verified**

Error (round XF1); checked by `mapexport check`. A new object left at slot 0 is see-through and indestructible: only
objects no level slots may stay without a slot. Rests on CULL-SLOT-3.

[code: rust/bc2-levelwrite/src/rules.rs:579] [code: rust/bc2-levelwrite/src/export.rs:3428]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/tests/export_xf1.rs::a_cross_set_object_gets_its_entries_and_shape]

### XVAL-CULL-4: Objects stay within +-2048 m

Status: **verified**

Error (round V1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal). The cull
tree holds centres within +-2048 m on each axis: the export cannot slot an object moved or placed beyond that. Rests
on CULL-SPH-1.

[code: rust/bc2-levelwrite/src/rules.rs:589] [code: rust/bc2-levelwrite/src/validate.rs:214]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_build_refusals]

### XVAL-SHDR-1: Shader entries for every mesh the level draws

Status: **verified**

Error (round XF1); checked by `mapexport check`. Every mesh the level reaches needs entries in its shader databases
(except those DICE's base reaches without them): a part with no program crashes the renderer (0x98BD6E) or draws
nothing. Rests on RNDR-DRAW-6, RNDR-DB-1.

[code: rust/bc2-levelwrite/src/rules.rs:600] [code: rust/bc2-levelwrite/src/export.rs:3476]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/tests/export_xf1.rs::a_cross_set_object_gets_its_entries_and_shape]

### XVAL-SHDR-2: No 40 km/h sign drawn with another level's entries

Status: **verified**

Error (round V1); checked live in the editor and by `mapexport validate`; by `mapexport check`. The 40 km/h road sign
(SpeedSign_40) drawn with another level's shader entries crashed the renderer at 0x98BD6E (OPFOR's Isla Inocentes
test, unsolved). Place it only on a level whose own database draws it. Rests on RNDR-DRAW-6.

[code: rust/bc2-levelwrite/src/rules.rs:610] [code: rust/bc2-levelwrite/src/export.rs:3287] [code: rust/bc2-levelwrite/src/validate.rs:165]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-SND-1: No streamed wave data in a level bundle

Status: **verified**

Error (round QW2); checked by `mapexport check`. Streamed wave data in a level bundle is read as a partition and the
load crashes (0x49EDF4). Rests on SND-STRM-1, SND-WAVE-2.

[code: rust/bc2-levelwrite/src/rules.rs:621] [code: rust/bc2-levelwrite/src/export.rs:2695]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/src/export.rs::streamed_waves_named]

### XVAL-SND-2: No deleted marker hides a sound

Status: **verified**

Error (round V1); checked by `mapexport check`. A deleted marker in a level bundle hides the entry of the same name in
the bundles after it: a hidden wave is a null wave in a sampler and the client crashes (0xFF2AC2). Rests on SND-
NULL-1, SND-LOAD-1.

[code: rust/bc2-levelwrite/src/rules.rs:630] [code: rust/bc2-levelwrite/src/export.rs:3172]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/tests/v1_rules.rs::console_resources_deleted_sound_markers_and_build_refusals_are_named]

### XVAL-TERR-1: The terrain is the base's own, whole

Status: **verified**

Error (round V1); checked by `mapexport check`. The visual terrain's patch shaders must be drawn with the mask atlas
of their own level: patch shaders from a donor whose atlas has another layout draw every tiled cell black. The level's
terrain bundle must be the base's, entry for entry. Rests on TERR-VT-5, TERR-NAME-1.

[code: rust/bc2-levelwrite/src/rules.rs:641] [code: rust/bc2-levelwrite/src/export.rs:3220]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules]

### XVAL-CONS-1: No Xbox 360 resource in a PC level

Status: **verified**

Error (round V1); checked by `mapexport check`. Xbox 360 textures (big-endian headers, tiled texels) and Havok physics
(layout rules 04 00 00 01) are not the PC's: convert them first (bfbc2-console-convert) or the game reads garbage.
Rests on XCONS-TEX-1, XCONS-HK-1, TEX-HDR-1.

[code: rust/bc2-levelwrite/src/rules.rs:653] [code: rust/bc2-levelwrite/src/export.rs:3187]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/tests/v1_rules.rs::console_resources_deleted_sound_markers_and_build_refusals_are_named]

### XVAL-GRP-1: An edited static group keeps its members

Status: **verified**

Error (round V1); checked by `mapexport check`. A static group's physics lists one entry per member and part: an
edited group file must keep each member's blueprint and the member count, or the load crashes at 0xAB4E58. Rests on
LEVEL-MG-4, LOAD-MG-1.

[code: rust/bc2-levelwrite/src/rules.rs:664] [code: rust/bc2-levelwrite/src/export.rs:3266]
[test: rust/bc2-levelwrite/tests/v1_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/src/export.rs::group_member_changes_are_named]

### XVAL-OBJ-1: No empty entry in the level's object list

Status: **verified**

Error (round GR4); checked live in the editor and by `mapexport validate`; by the DBX inspector's Level > Check.
LevelData.Objects holds an empty (null) entry: the game reads the list without a check and crashes. Rests on LEVEL-
OBJ-2, REF-DEL-2.

[code: rust/bc2-levelwrite/src/rules.rs:675] [code: rust/bc2-formats/src/dbxedit/check.rs:296]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it]

### XVAL-OBJ-2: A world object keeps its physics and mesh

Status: **verified**

Error (round GR4); checked live in the editor and by `mapexport validate`; by the DBX inspector's Level > Check. A
world object without its PhysicsData or MeshEntityType crashes the model-group set-up. Rests on LEVEL-MG-3.

[code: rust/bc2-levelwrite/src/rules.rs:684] [code: rust/bc2-formats/src/dbxedit/check.rs:296]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it]

### XVAL-OBJ-3: World objects are in the level's object list

Status: **verified**

Warning (round GR4); checked live in the editor and by `mapexport validate`; by the DBX inspector's Level > Check. An
object not in LevelData.Objects is not loaded: missing terrain or buildings in the game. Rests on LEVEL-OBJ-3, LEVEL-
OBJ-4.

[code: rust/bc2-levelwrite/src/rules.rs:693] [code: rust/bc2-formats/src/dbxedit/check.rs:296]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it]

### XVAL-CULL-5: No cull slot shared by two objects

Status: **verified**

Warning (round GR4); checked by the DBX inspector's Level > Check. Objects sharing a cull slot are shown and hidden
together (see-through objects). The export gives moved and placed objects slots of their own; the DBX inspector's
Level > Fix slots repairs a hand-edited file. Rests on CULL-SLOT-3, LEVEL-SLOT-2.

[code: rust/bc2-levelwrite/src/rules.rs:702] [code: rust/bc2-levelwrite/src/validate.rs:138] [code: rust/bc2-formats/src/dbxedit/check.rs:296]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it]

### XVAL-CULL-6: Slots agree with the level whose cull trees are used

Status: **verified**

Warning (round GR4); checked by the DBX inspector's Level > Check. Objects with another slot than the same object in
the cull trees' level are see-through: Level > Fix slots in the DBX inspector gives each the slot of its counterpart.
Rests on CULL-SLOT-2, LEVEL-SLOT-3.

[code: rust/bc2-levelwrite/src/rules.rs:712] [code: rust/bc2-levelwrite/src/validate.rs:138] [code: rust/bc2-formats/src/dbxedit/check.rs:296]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it]

### XVAL-PLACE-1: Vehicle spawners do not stand on each other

Status: **verified**

Warning (round V1); checked live in the editor and by `mapexport validate`. Two vehicle spawners of one team stand
within 1 m of each other: their vehicles spawn into one another. Rests on GAME-SPAWN-1.

[code: rust/bc2-levelwrite/src/rules.rs:723] [code: rust/bc2-levelwrite/src/validate.rs:307]
[test: rust/bc2-levelwrite/tests/v1_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it]

## 3. DICE's placement tendencies (L1)

Round L1 measured how DICE laid out gameplay on the 64 stock multiplayer levels of Conquest, Rush, Squad Rush and
Squad Deathmatch (`bfbc2-map-editor/placement/L1_placement_report.md`) and proposed a rule per tendency
(`bfbc2-map-editor/placement/l1_rules.json`, an index of its own: IDs such as L1-RU-004). Round V1 checks them live
in the editor (`bc2_levelwrite::placement`, a Rust port of `l1_measure.py`: on the 64 levels its 3,539 measurements
the rules read are L1's to 0.1). A value outside a rule's warning band is a **warning**, one inside it but outside
DICE's own range a **note**, never an error; each finding quotes the stock range, the number of levels it was
measured on and the rule's ID ("Stage 3 is 40 m from stage 2; DICE's Rush stages are 93-316 m apart (median 174 m).
[L1-RU-004: stock 92.9 to 315.6 over 16 levels]"). On the 64 stock levels they raise no warning; the notes are
exactly L1's own list of DICE's outliers (`l1_rules_check.md`). The engine rules they illustrate are
`engine/gameplay.md`'s GAME-CQ-2 to GAME-CQ-9, GAME-RUSH-3 to GAME-RUSH-8, GAME-SDM-1, GAME-SDM-2, GAME-SPAWN-2,
GAME-AREA-1 and GAME-AREA-2. Each rule has a synthetic broken layout that raises it
(`rust/bc2-levelwrite/tests/v1_rules.rs`, `each_placement_rule_has_a_broken_layout_that_raises_it`).

| ID | metric | scope | warning | note | DICE's range (levels) |
|---|---|---|---|---|---|
| L1-CQ-001 | `cq.flag_count` | CQ; BFBC2, BFBC1, Vietnam | below 1 | outside 3-4 | 3-4 (16) |
| L1-CQ-001w | `cq.flag_count` | CQ; WW2 | below 1 | outside 5-5 | 5-5 (3) |
| L1-CQ-002 | `cq.flag_nn_dist` | CQ; BFBC2, BFBC1, Vietnam | below 50 | outside 70-300 | 58-450.4 (16) |
| L1-CQ-002w | `cq.flag_nn_dist` | CQ; WW2 | below 80 | outside 100-250 | 103-205.2 (3) |
| L1-CQ-003 | `cq.hq_nearest_flag` | CQ; BFBC2, BFBC1, Vietnam | below 80 | outside 100-400 | 85.4-384 (16) |
| L1-CQ-003w | `cq.hq_nearest_flag` | CQ; WW2 | below 150 | outside 200-650 | 202.2-632.7 (3) |
| L1-CQ-004 | `cq.hq_hq_dist` | CQ; BFBC2, BFBC1, Vietnam | below 250 | outside 280-1000 | 285-1358 (16) |
| L1-CQ-005 | `cq.hq_nearest_flag_asymmetry` | CQ; BFBC2, BFBC1, Vietnam, WW2 | - | outside 0-60 | 1.7-94.2 (19) |
| L1-CQ-006 | `cq.flag_axis_t` | CQ; BFBC2, BFBC1, Vietnam, WW2 | outside 0-1 | outside 0.1-0.9 | 0.1-0.9 (19) |
| L1-CQ-007 | `cq.flag_axis_lateral` | CQ; BFBC2, BFBC1, Vietnam | - | outside 0-0.5 | 0-0.4 (16) |
| L1-CQ-008 | `cq.capture_zone` | CQ; BFBC2, BFBC1, Vietnam, WW2 | below 5 | outside 7-40 | 7.5-36.8 (14) |
| L1-CQ-009 | `cq.time_to_capture` | CQ; BFBC2, BFBC1, Vietnam | - | outside 20-25 | 20-25 (16) |
| L1-CQ-009w | `cq.time_to_capture` | CQ; WW2 | - | outside 10-10 | 10-10 (3) |
| L1-CQ-010 | `cq.flag_spawn_points` | CQ; BFBC2, BFBC1, Vietnam, WW2 | below 1 | below 2 | 2-21 (19) |
| L1-CQ-011 | `cq.flag_spawn_dist_max` | CQ; BFBC2, BFBC1, Vietnam, WW2 | above 200 | above 140 | 15.3-137.3 (19) |
| L1-CQ-012 | `cq.hq_spawn_points` | CQ; BFBC2, BFBC1, Vietnam, WW2 | below 1 | below 4 | 4-13 (19) |
| L1-CQ-013 | `cq.flag_margin` | CQ; BFBC2, BFBC1, Vietnam, WW2 | below 0 | below 10 | 10.9-400.5 (19) |
| L1-CQ-014 | `cq.hq_margin` | CQ; BFBC2, BFBC1, Vietnam, WW2 | below 0 | below 1 | 1.2-65.4 (19) |
| L1-CQ-015 | `cq.hq_vs_enemy_area` | CQ; BFBC2, BFBC1, Vietnam, WW2 | above 0 | above -3 | -87.8--3.2 (8) |
| L1-CQ-016 | `cq.tickets` | CQ; BFBC2, BFBC1, Vietnam | - | outside 200-350 | 200-350 (16) |
| L1-CQ-017 | `cq.combat_area_union` | CQ; BFBC2, BFBC1, Vietnam | - | outside 40000-500000 | 43560-478772 (16) |
| L1-RU-001 | `rush.stage_count` | Rush; BFBC2, BFBC1, Vietnam | below 2 | outside 3-5 | 3-5 (16) |
| L1-RU-002 | `rush.mcoms_per_stage` | Rush; BFBC2, BFBC1, Vietnam | not 2 M-COMs | - | two M-COMs a stage (16) |
| L1-RU-003 | `rush.mcom_pair_dist` | Rush; BFBC2, BFBC1, Vietnam | outside 10-120 | outside 25-80 | 11.7-90.9 (16) |
| L1-RU-004 | `rush.stage_step` | Rush; BFBC2, BFBC1, Vietnam | below 75 | outside 100-320 | 92.9-315.6 (16) |
| L1-RU-004s | `sr.stage_step` | SR; BFBC2, BFBC1, Vietnam | below 40 | outside 60-180 | 45.4-169.4 (14) |
| L1-RU-005 | `rush.defender_spawn_to_obj_min` | Rush; BFBC2, BFBC1, Vietnam | above 100 | outside 5-65 | 6-61.1 (16) |
| L1-RU-005s | `sr.defender_spawn_to_obj_min` | SR; BFBC2, BFBC1, Vietnam | above 100 | outside 20-75 | 22.7-72.3 (14) |
| L1-RU-006 | `rush.attacker_spawn_to_obj_min` | Rush; BFBC2, BFBC1, Vietnam | below 40 | outside 60-460 | 71.5-458.9 (16) |
| L1-RU-006s | `sr.attacker_spawn_to_obj_min` | SR; BFBC2, BFBC1, Vietnam | below 40 | outside 50-170 | 54.9-165 (14) |
| L1-RU-007 | `rush.spawn_ratio` | Rush; BFBC2, BFBC1, Vietnam | below 1 | below 1.5 | 1.8-21.5 (16) |
| L1-RU-007s | `sr.spawn_ratio` | SR; BFBC2, BFBC1, Vietnam | below 0.75 | below 1 | 1-4.4 (14) |
| L1-RU-008 | `rush.defender_spawn_depth` | Rush/SR; BFBC2, BFBC1, Vietnam | below -30 | below 0 | -27-73.9 (30) |
| L1-RU-009 | `rush.attacker_spawn_depth` | Rush/SR; BFBC2, BFBC1, Vietnam | above 0 | above -60 | -490.5--63.9 (30) |
| L1-RU-010 | `rush.attacker_spawns_in_enemy_area` | Rush/SR; BFBC2, BFBC1, Vietnam | above 0 | above -1 | -140.8--1.7 (30) |
| L1-RU-011 | `rush.obj_margin_attacker_area` | Rush/SR; BFBC2, BFBC1, Vietnam | below 0 | below 10 | 5.7-96.1 (30) |
| L1-RU-012 | `rush.obj_margin_defender_area` | Rush/SR; BFBC2, BFBC1, Vietnam | below 0 | below 10 | 5.7-125.9 (30) |
| L1-RU-013 | `rush.attacker_area_kept` | Rush/SR; BFBC2, BFBC1, Vietnam | - | below 75 | 38.3-100 (30) |
| L1-RU-014 | `rush.mcom_pair_dist` | Rush; BFBC2, BFBC1, Vietnam | - | above 10 | 0-8.5 (16) |
| L1-RU-015 | `rush.lives` | Rush; BFBC2, BFBC1, Vietnam | - | outside 75-100 | 75-100 (16) |
| L1-SD-001 | `sdm.squad_count` | SDM; BFBC2, BFBC1, Vietnam | below 2 | outside 4-4 | 4-4 (15) |
| L1-SD-002 | `sdm.spawn_points` | SDM; BFBC2, BFBC1, Vietnam | below 8 | outside 12-40 | 14-34 (15) |
| L1-SD-003 | `sdm.spawn_nn_median` | SDM; BFBC2, BFBC1, Vietnam | - | outside 10-40 | 12.8-37.9 (15) |
| L1-SD-004 | `sdm.combat_area_union` | SDM; BFBC2, BFBC1, Vietnam | - | outside 14000-90000 | 15344-83836 (15) |
| L1-SD-005 | `sdm.spawn_margin_min` | SDM; BFBC2, BFBC1, Vietnam | below -5 | below 0 | 2-15.5 (15) |
| L1-SD-006 | `sdm.vehicles` | SDM; BFBC2, BFBC1, Vietnam | - | outside 1-1 | 1-1 (15) |
| L1-AL-001 | `all.spawn_enemy_radius` | every mode; BFBC2, BFBC1, Vietnam, WW2 | - | outside 10-20 | 10-20 (64) |
| L1-AL-002 | `cq.spawn_margin` | CQ; BFBC2, BFBC1, Vietnam, WW2 | below 0 | below 1 | 1.3-421.9 (19) |
| L1-HT-002 | `cq.flag_pair_dist` | CQ; BFBC2, BFBC1, Vietnam | - | above 25 | 0-24.1 (16) |
