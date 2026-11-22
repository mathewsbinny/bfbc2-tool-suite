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
`rust/bc2-levelwrite/tests/export_rules.rs::the_registry_and_the_spec_agree`).

What the rules hold off, by crash address: the start-up checksum crash of a level registered with missing bundles
or not registered on both sides (CSUM-CHK-1, CSUM-CHK-2, CSUM-CHK-4), a blueprint that resolves nowhere (0x6EFCA4,
REF-RES-5), a partition that does not parse or streamed wave data in a level bundle (0x49EDF4: LOAD-PART-1,
SND-STRM-1), a null wave (0xFF2AC2, SND-NULL-1), a GameMode that does not resolve (0x74EAB3, LEVEL-DESC-2), a part
with no shader program (0x98BD6E, RNDR-DRAW-6), a group's part counts not matching its members (0xAB4E58,
LEVEL-MG-4); and the black ones: a black loading screen (LVL-DESC-2), black tiled terrain cells (TERR-VT-5).

Round A1 added the site rules (XVAL-PLACE-2 to 4: spawn points over water or inside rocks, a vehicle spawner's room
and domain, gameplay objects outside every combat area) and the path rule (XVAL-PLACE-5: spawns and objectives off the
main route network), each bounded by what DICE's own levels do. Left for later rounds: engine limits on instances,
spawns and event connections (they need reverse engineering first), an exported level's memory against the stock
levels', Xbox 360 meshes in a staged bundle (their big-endian layout is not detected yet), and L1-HT-001 (a flag's
height above the ground around it).

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

[code: rust/bc2-levelwrite/src/rules.rs:147] [code: rust/bc2-levelwrite/src/validate.rs:43]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_build_refusals]

### XVAL-EDIT-2: An operation names an object the level does not have

Status: **verified**

Error (round OP1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal). An edit
names an object the base level no longer has. The export refuses the project until that edit is undone or the object
is back. Rests on XLOG-BASE-1, XLOG-APP-1.

[code: rust/bc2-levelwrite/src/rules.rs:157] [code: rust/bc2-levelwrite/src/export.rs:1022] [code: rust/bc2-levelwrite/src/validate.rs:347]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_build_refusals]

### XVAL-EDIT-3: An operation does not apply

Status: **verified**

Error (round OP1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal). An edit
could not be applied to the level (its reason is given). The export refuses the project until it is undone. Rests on
XLOG-APP-1.

[code: rust/bc2-levelwrite/src/rules.rs:167]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_build_refusals]

### XVAL-EDIT-4: The level partition does not parse

Status: **verified**

Error (round V1); checked live in the editor and by `mapexport validate`; by `mapexport check`. The level's partition
could not be read, so no edit is written to it. A partition the game cannot parse is a null the loader passes on and
the load crashes (0x49EDF4). Rests on LOAD-PART-1.

[code: rust/bc2-levelwrite/src/rules.rs:177] [code: rust/bc2-levelwrite/src/validate.rs:117]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-EDIT-5: Terrain and paint edits are not exported yet

Status: **verified**

Error (round OP1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal). The
project edits the terrain or paints it, and the export does not write terrain yet (round E5): it refuses the project.
Undo the strokes to export the rest. Rests on XLOG-DELTA-1, TERR-VT-5.

[code: rust/bc2-levelwrite/src/rules.rs:187] [code: rust/bc2-levelwrite/src/export.rs:1047] [code: rust/bc2-levelwrite/src/validate.rs:64]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_build_refusals]

### XVAL-EDIT-6: Only the level's own root files are exported

Status: **verified**

Error (round OP1); checked by `mapexport build` (a refusal). The project edits a file the export does not write (only
the level partition and the base's own static group files are written). The export refuses it. Rests on XLOG-FILE-2,
LOAD-PART-5.

[code: rust/bc2-levelwrite/src/rules.rs:197] [code: rust/bc2-levelwrite/src/export.rs:1092]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it]

### XVAL-MODE-1: Only modes the export offers

Status: **verified**

Error (round E3d); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal). Onslaught
needs bots (NavKit path data and the AI layer) and Air Superiority is BF1943's: the export refuses both. Rests on
GAME-MODE-8.

[code: rust/bc2-levelwrite/src/rules.rs:208] [code: rust/bc2-levelwrite/src/export.rs:487] [code: rust/bc2-levelwrite/src/validate.rs:78]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_build_refusals]

### XVAL-MODE-2: The donor is a level of the era's mode

Status: **verified**

Error (round E3b); checked by `mapexport build` (a refusal); by `mapexport check`. The mode layer's donor must be a
stock level of the chosen era and mode; another level's layer would name kits, factions and game mode data the era
does not load. Rests on XEXP-PRE-2.

[code: rust/bc2-levelwrite/src/rules.rs:218] [code: rust/bc2-levelwrite/src/export.rs:161]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-MODE-3: The mode layer is complete

Status: **verified**

Error (round E3d); checked live in the editor and by `mapexport validate`; by `mapexport check`. A multiplayer level
needs its whole mode layer (player, mission, teams, bases, counters, spawns, and per mode its flags, stages or
squads); without it the session waits forever in state 14. Put back what is missing, or choose the mode again. Rests
on XEXP-CHK-2, GAME-LAYER-1, GAME-LAYER-4, LEVEL-MP-1.

[code: rust/bc2-levelwrite/src/rules.rs:228] [code: rust/bc2-levelwrite/src/export.rs:3032] [code: rust/bc2-levelwrite/src/validate.rs:99]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-MODE-4: The donor level changed since the mode was chosen

Status: **verified**

Warning (round V1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal). The
donor's partition is not the one the mode was chosen from (a patch or a rebuilt WW2 level): the layer is grafted from
it as it is now. Choose the mode again to see what changed. Rests on XEXP-PRE-2, XLOG-BASE-1.

[code: rust/bc2-levelwrite/src/rules.rs:239] [code: rust/bc2-levelwrite/src/validate.rs:83]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it]

### XVAL-MODE-5: The export file's combat area is in the base

Status: **verified**

Error (round E3); checked by `mapexport build` (a refusal). An export file without a project mode grafts the donor's
layer into the base's combat area it names; that area must be in the base level. Rests on XEXP-BLD-1.

[code: rust/bc2-levelwrite/src/rules.rs:249] [code: rust/bc2-levelwrite/src/export.rs:1179]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it]

### XVAL-REG-1: One package.mft block and one bundleManifest line per bundle

Status: **verified**

Error (round E3); checked by `mapexport check`. Each of the level's bundles needs its package.mft block and its
bundleManifest line, or the server crashes computing the level checksum at start-up. Rests on REG-NEW-1, CSUM-CHK-1,
CSUM-CHK-2.

[code: rust/bc2-levelwrite/src/rules.rs:260] [code: rust/bc2-levelwrite/src/export.rs:2781]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-REG-2: Every bundle the registration names exists

Status: **verified**

Error (round E3); checked by `mapexport check`. A registered bundle that is missing crashes the server's level
checksum at start-up. Rests on CSUM-CHK-2, REG-MFT-4.

[code: rust/bc2-levelwrite/src/rules.rs:270] [code: rust/bc2-levelwrite/src/export.rs:2795]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-REG-3: A new name, never a level of the game

Status: **verified**

Error (round E3); checked by `mapexport build` (a refusal); by `mapexport check`. An export never replaces a level of
the game copy or one another tool registered: choose another id. Rests on REG-NEW-2, XEXP-PRE-1.

[code: rust/bc2-levelwrite/src/rules.rs:279] [code: rust/bc2-levelwrite/src/export.rs:939]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_build_refusals]

### XVAL-REG-4: Server and client need the same registration

Status: **verified**

Note (round V1); checked by `mapexport check`. Install the level on the server and on every client: both register its
bundles in bundleManifest and package.mft, or the level checksums differ (and a server without them crashes computing
its checksum). Rests on CSUM-CHK-4, CSUM-CHK-1.

[code: rust/bc2-levelwrite/src/rules.rs:288] [code: rust/bc2-levelwrite/src/export.rs:62]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-REG-5: The announcer's registration is the donor's

Status: **verified**

Error (round E3c); checked by `mapexport check`. The level's voice-over blocks and manifest lines must be its donor's
under the new name, with no block whose bundleManifest line is missing. Rests on XEXP-REG-2, SND-VO-3.

[code: rust/bc2-levelwrite/src/rules.rs:298] [code: rust/bc2-levelwrite/src/export.rs:3831]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-NAME-1: The level id's form

Status: **verified**

Error (round EX1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal); by
`mapexport check`. The level id is a file, folder and localisation key: 3 to 40 of A-Z, 0-9 and _, starting with a
letter. Rests on XEXP-PRE-1, LVL-ID-1.

[code: rust/bc2-levelwrite/src/rules.rs:309] [code: rust/bc2-levelwrite/src/export.rs:555] [code: rust/bc2-levelwrite/src/names.rs:382]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-NAME-2: No _NAME in the level id

Status: **verified**

Error (round EX1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal); by
`mapexport check`. The loading screen cuts the name key at its first _NAME to find its picture: an id holding _NAME
gives a black loading screen. Rests on LVL-DESC-2.

[code: rust/bc2-levelwrite/src/rules.rs:318] [code: rust/bc2-levelwrite/src/names.rs:418]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-NAME-3: The level id clashes with nothing the game has

Status: **verified**

Error (round EX1); checked by `mapexport build` (a refusal); by `mapexport check`. The id must not be a level, another
project's export, a loading picture or a text the game already has. Rests on REG-NEW-2, XEXP-PRE-1.

[code: rust/bc2-levelwrite/src/rules.rs:328] [code: rust/bc2-levelwrite/src/names.rs:431]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/tests/export_ex1.rs::names_are_checked_against_the_game_copy]

### XVAL-NAME-4: The folder is the id in lower case

Status: **verified**

Error (round EX1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal); by
`mapexport check`. The game finds a level's bundles in Package\levels\<id>\: the folder cannot differ from the id.
Rests on LVL-ID-1.

[code: rust/bc2-levelwrite/src/rules.rs:337] [code: rust/bc2-levelwrite/src/names.rs:480]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-NAME-5: The server-browser name's form

Status: **verified**

Error (round EX1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal); by
`mapexport check`. maps.ini's name: plain ASCII, no leading or trailing space, no quote, a browser row of at most 159
characters. Rests on XEXP-ID-1.

[code: rust/bc2-levelwrite/src/rules.rs:346] [code: rust/bc2-levelwrite/src/names.rs:492]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-NAME-6: The server-browser name is not another map's

Status: **verified**

Error (round EX1); checked by `mapexport build` (a refusal); by `mapexport check`. Two maps with one name give server
browser rows that look the same. Rests on XEXP-ID-1.

[code: rust/bc2-levelwrite/src/rules.rs:356] [code: rust/bc2-levelwrite/src/names.rs:538]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/tests/export_ex1.rs::names_are_checked_against_the_game_copy]

### XVAL-NAME-7: The on-screen names fit every language

Status: **verified**

Error (round EX1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal); by
`mapexport check`. The on-screen name (64) and description (400) must fit and use only letters each language's texts
have, or they do not show. Rests on XEXP-ID-1.

[code: rust/bc2-levelwrite/src/rules.rs:365] [code: rust/bc2-levelwrite/src/names.rs:553]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-NAME-8: Every place names the level the same

Status: **verified**

Error (round EX1); checked by `mapexport check`. The description, the menus' level list, every package.mft block and
bundleManifest line and the name key's loading picture must name the same level: a mismatch is the start-up checksum
crash or a black loading screen. Rests on XEXP-ID-1, REG-NEW-5, LVL-DESC-2.

[code: rust/bc2-levelwrite/src/rules.rs:375] [code: rust/bc2-levelwrite/src/export.rs:4048]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-NAME-9: The level's own name keys and pictures

Status: **verified**

Error (round E3b); checked by `mapexport check`. With its own name the level's description and partition use its own
keys, every text is stored in each language's encoding, and its pictures are in their templates' form. Rests on XEXP-
ID-1.

[code: rust/bc2-levelwrite/src/rules.rs:386] [code: rust/bc2-levelwrite/src/export.rs:2657]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-FILE-1: The staged files are the ones recorded

Status: **verified**

Error (round E3); checked by `mapexport check`. A staged bundle changed after the export: build the stage again. Rests
on XEXP-CHK-1.

[code: rust/bc2-levelwrite/src/rules.rs:397] [code: rust/bc2-levelwrite/src/export.rs:2677]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-FILE-2: Every staged bundle opens and every partition parses

Status: **verified**

Error (round E3); checked by `mapexport check`. A partition that does not parse is a null the loader passes on: the
load crashes (0x49EDF4). Rests on LOAD-PART-1, XEXP-CHK-1.

[code: rust/bc2-levelwrite/src/rules.rs:406] [code: rust/bc2-levelwrite/src/export.rs:62]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-REF-1: Local references name an instance of their partition

Status: **verified**

Error (round E3); checked by `mapexport check`. A reference to an instance the partition does not have is a null the
game reads. Rests on REF-RES-1, REF-DEL-2.

[code: rust/bc2-levelwrite/src/rules.rs:416] [code: rust/bc2-levelwrite/src/export.rs:2837]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-REF-2: DICE's own dangling references

Status: **verified**

Note (round E3); checked by `mapexport check`. References to partitions no bundle of the game has, made exactly as
DICE's source partitions make them: the stock levels load with them. Rests on REF-RES-3.

[code: rust/bc2-levelwrite/src/rules.rs:425] [code: rust/bc2-levelwrite/src/export.rs:2899]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-REF-3: Every referenced partition is in a bundle the level loads

Status: **verified**

Error (round E3); checked by `mapexport check`. A blueprint that resolves nowhere crashes the server's checksum
(0x6EFCA4). Rests on REF-RES-2, REF-RES-5, CSUM-CHK-3.

[code: rust/bc2-levelwrite/src/rules.rs:435] [code: rust/bc2-levelwrite/src/export.rs:2907]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-REF-4: Every external reference names an instance of its partition

Status: **verified**

Error (round E3); checked by `mapexport check`. A reference to an instance its partition does not have is a null the
game reads. Rests on REF-RES-2.

[code: rust/bc2-levelwrite/src/rules.rs:444] [code: rust/bc2-levelwrite/src/export.rs:2916]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-REF-5: The closure resolves under the profile's bundles

Status: **verified**

Error (round E3d); checked by `mapexport check`. Every partition the level reaches must resolve in the bundles it
loads; the export copies what does not, and never registers it. Rests on XEXP-CLO-1, REF-RES-5.

[code: rust/bc2-levelwrite/src/rules.rs:453] [code: rust/bc2-levelwrite/src/export.rs:1039]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-LVL-1: LevelData.Name is the level's path

Status: **verified**

Error (round E3); checked by `mapexport check`. LevelData.Name must be Levels/<id>, the name every other file uses.
Rests on LEVEL-DATA-2, LVL-ID-1.

[code: rust/bc2-levelwrite/src/rules.rs:464] [code: rust/bc2-levelwrite/src/export.rs:2934]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-LVL-2: The description names the level

Status: **verified**

Error (round E3); checked by `mapexport check`. The description's LevelName must be Levels/<id>, or the menus start
another level. Rests on LVL-DESC-1, LVL-ID-1.

[code: rust/bc2-levelwrite/src/rules.rs:473] [code: rust/bc2-levelwrite/src/export.rs:2946]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-LVL-3: The height field is in a bundle the level loads

Status: **verified**

Error (round E3d); checked by `mapexport check`. Without its height field the map loads without ground: keep the
base's terrain bundles. Rests on TERR-NAME-1, LOAD-PART-6.

[code: rust/bc2-levelwrite/src/rules.rs:482] [code: rust/bc2-levelwrite/src/export.rs:3011]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-PROF-1: Every block loads the era's common bundles

Status: **verified**

Error (round E3b); checked by `mapexport check`. Each package.mft block must load the era's common bundles in their
order, or the era's kits, vehicles and sounds do not resolve. Rests on XEXP-PRE-2, LVL-DESC-1.

[code: rust/bc2-levelwrite/src/rules.rs:492] [code: rust/bc2-levelwrite/src/export.rs:3666]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-PROF-2: The game mode is the donor's and resolves

Status: **verified**

Error (round E3b); checked by `mapexport check`. The description's GameMode must be the donor's and resolve: a
GameMode that does not resolve crashes (0x74EAB3). Rests on LEVEL-DESC-2, LOAD-START-2.

[code: rust/bc2-levelwrite/src/rules.rs:502] [code: rust/bc2-levelwrite/src/export.rs:3727]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-PROF-3: The factions are the donor's

Status: **verified**

Error (round E3b); checked by `mapexport check`. The description's factions must be the donor's, the era's teams.
Rests on XEXP-PRE-2.

[code: rust/bc2-levelwrite/src/rules.rs:512] [code: rust/bc2-levelwrite/src/export.rs:3732]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-PROF-4: The kit pictures are the donor's and resolve

Status: **verified**

Error (round E3b); checked by `mapexport check`. The description's kit pictures must be the donor's, each resolving in
the bundles the level loads. Rests on XEXP-PRE-2, LOAD-PART-4.

[code: rust/bc2-levelwrite/src/rules.rs:521] [code: rust/bc2-levelwrite/src/export.rs:3737]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-PROF-5: The era's level fields

Status: **verified**

Error (round E3b); checked by `mapexport check`. The level fields the era sets (Vietnam's sound states ...) must be
the era's. Rests on XEXP-PRE-2.

[code: rust/bc2-levelwrite/src/rules.rs:530] [code: rust/bc2-levelwrite/src/export.rs:3750]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-PROF-6: The announcer is the era's

Status: **verified**

Error (round E3c); checked by `mapexport check`. Every announcer partition the level names must resolve as on its
donor, all from one era's voices. Rests on XEXP-VO-1, SND-VO-2.

[code: rust/bc2-levelwrite/src/rules.rs:539] [code: rust/bc2-levelwrite/src/export.rs:3937]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-WTHR-1: The weather the export gave the level

Status: **verified**

Error (round VW2); checked by `mapexport check`. The level's weather state, its spawn switch and its copied partitions
must be what the export wrote. Rests on RNDR-WTHR-8, RNDR-WTHR-5.

[code: rust/bc2-levelwrite/src/rules.rs:549] [code: rust/bc2-levelwrite/src/export.rs:2993]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-CULL-1: One cull tree, one object per slot

Status: **verified**

Error (round E2); checked by `mapexport check`. Every slotted static object needs a slot of its own inside the tree: a
shared slot or one outside the tree draws the wrong object or none. Rests on CULL-SLOT-3, LEVEL-SLOT-2, LEVEL-SLOT-3.

[code: rust/bc2-levelwrite/src/rules.rs:559] [code: rust/bc2-levelwrite/src/export.rs:3085]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-CULL-2: A moved or placed object stands in its own blueprint's cull shape

Status: **verified**

Error (round XF1); checked by `mapexport check`. A re-slotted object must stand in leaves of its own blueprint's shape
where it stands, or it pops or vanishes as the camera moves. Rests on CULL-ADD-1, CULL-SPH-1.

[code: rust/bc2-levelwrite/src/rules.rs:569] [code: rust/bc2-levelwrite/src/export.rs:3503]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/tests/export_xf1.rs::a_cross_set_object_gets_its_entries_and_shape]

### XVAL-CULL-3: Every new static object has a cull slot

Status: **verified**

Error (round XF1); checked by `mapexport check`. A new object left at slot 0 is see-through and indestructible: only
objects no level slots may stay without a slot. Rests on CULL-SLOT-3.

[code: rust/bc2-levelwrite/src/rules.rs:579] [code: rust/bc2-levelwrite/src/export.rs:279]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/tests/export_xf1.rs::a_cross_set_object_gets_its_entries_and_shape]

### XVAL-CULL-4: Objects stay within +-2048 m

Status: **verified**

Error (round V1); checked live in the editor and by `mapexport validate`; by `mapexport build` (a refusal). The cull
tree holds centres within +-2048 m on each axis: the export cannot slot an object moved or placed beyond that. Rests
on CULL-SPH-1.

[code: rust/bc2-levelwrite/src/rules.rs:589] [code: rust/bc2-levelwrite/src/validate.rs:214]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_build_refusals]

### XVAL-SHDR-1: Shader entries for every mesh the level draws

Status: **verified**

Error (round XF1); checked by `mapexport check`. Every mesh the level reaches needs entries in its shader databases
(except those DICE's base reaches without them): a part with no program crashes the renderer (0x98BD6E) or draws
nothing. Rests on RNDR-DRAW-6, RNDR-DB-1.

[code: rust/bc2-levelwrite/src/rules.rs:600] [code: rust/bc2-levelwrite/src/export.rs:3585]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/tests/export_xf1.rs::a_cross_set_object_gets_its_entries_and_shape]

### XVAL-SHDR-2: No 40 km/h sign drawn with another level's entries

Status: **verified**

Error (round V1); checked live in the editor and by `mapexport validate`; by `mapexport check`. The 40 km/h road sign
(SpeedSign_40) drawn with another level's shader entries crashed the renderer at 0x98BD6E (OPFOR's Isla Inocentes
test, unsolved). Place it only on a level whose own database draws it. Rests on RNDR-DRAW-6.

[code: rust/bc2-levelwrite/src/rules.rs:610] [code: rust/bc2-levelwrite/src/export.rs:3350] [code: rust/bc2-levelwrite/src/validate.rs:165]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it] [test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-SND-1: No streamed wave data in a level bundle

Status: **verified**

Error (round QW2); checked by `mapexport check`. Streamed wave data in a level bundle is read as a partition and the
load crashes (0x49EDF4). Rests on SND-STRM-1, SND-WAVE-2.

[code: rust/bc2-levelwrite/src/rules.rs:621] [code: rust/bc2-levelwrite/src/export.rs:2515]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/src/export.rs::streamed_waves_named]

### XVAL-SND-2: No deleted marker hides a sound

Status: **verified**

Error (round V1); checked by `mapexport check`. A deleted marker in a level bundle hides the entry of the same name in
the bundles after it: a hidden wave is a null wave in a sampler and the client crashes (0xFF2AC2). Rests on SND-
NULL-1, SND-LOAD-1.

[code: rust/bc2-levelwrite/src/rules.rs:630] [code: rust/bc2-levelwrite/src/export.rs:3232]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/tests/export_rules.rs::console_resources_deleted_sound_markers_and_build_refusals_are_named]

### XVAL-TERR-1: The terrain is the base's own, whole

Status: **verified**

Error (round V1); checked by `mapexport check`. The visual terrain's patch shaders must be drawn with the mask atlas
of their own level: patch shaders from a donor whose atlas has another layout draw every tiled cell black. The level's
terrain bundle must be the base's, entry for entry. Rests on TERR-VT-5, TERR-NAME-1.

[code: rust/bc2-levelwrite/src/rules.rs:641] [code: rust/bc2-levelwrite/src/export.rs:3280]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules]

### XVAL-CONS-1: No Xbox 360 resource in a PC level

Status: **verified**

Error (round V1); checked by `mapexport check`. Xbox 360 textures (big-endian headers, tiled texels) and Havok physics
(layout rules 04 00 00 01) are not the PC's: convert them first (bfbc2-console-convert) or the game reads garbage.
Rests on XCONS-TEX-1, XCONS-HK-1, TEX-HDR-1.

[code: rust/bc2-levelwrite/src/rules.rs:653] [code: rust/bc2-levelwrite/src/export.rs:197]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/tests/export_rules.rs::console_resources_deleted_sound_markers_and_build_refusals_are_named]

### XVAL-GRP-1: An edited static group keeps DICE's layout and its kinds

Status: **verified**

Error (round V1, SG1); checked by `mapexport build` (a refusal); by `mapexport check`. A static group's physics lists
one entry per member and part (0xAB4E58 when they disagree). Members may be moved, deleted and added (round SG1): they
are laid out again as DICE lays groups out (members, leaves, cumulative parts and network ids, physics), every kind a
group holds keeps a member (the last one of a kind is never deleted: move it instead) and no group gains a kind it did
not hold (an added member is a copy of one of its group's; another object is placed loose). Rests on LEVEL-MG-4,
LEVEL-MG-5, LEVEL-MG-8, XLOG-MEM-2, LOAD-MG-1.

[code: rust/bc2-levelwrite/src/rules.rs:664] [code: rust/bc2-levelwrite/src/export.rs:3329]
[test: rust/bc2-levelwrite/tests/export_rules_stage.rs::v1_stage_rules] [test: rust/bc2-levelwrite/src/export.rs::group_member_changes_are_named] [test: rust/bc2-levelwrite/tests/model_group_members.rs::an_export_deletes_moves_and_adds_members]

### XVAL-OBJ-1: No empty entry in the level's object list

Status: **verified**

Error (round GR4); checked live in the editor and by `mapexport validate`; by the DBX inspector's Level > Check.
LevelData.Objects holds an empty (null) entry: the game reads the list without a check and crashes. Rests on LEVEL-
OBJ-2, REF-DEL-2.

[code: rust/bc2-levelwrite/src/rules.rs:678] [code: rust/bc2-formats/src/dbxedit/check.rs:296]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it]

### XVAL-OBJ-2: A world object keeps its physics and mesh

Status: **verified**

Error (round GR4); checked live in the editor and by `mapexport validate`; by the DBX inspector's Level > Check. A
world object without its PhysicsData or MeshEntityType crashes the model-group set-up. Rests on LEVEL-MG-3.

[code: rust/bc2-levelwrite/src/rules.rs:687] [code: rust/bc2-formats/src/dbxedit/check.rs:296]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it]

### XVAL-OBJ-3: World objects are in the level's object list

Status: **verified**

Warning (round GR4); checked live in the editor and by `mapexport validate`; by the DBX inspector's Level > Check. An
object not in LevelData.Objects is not loaded: missing terrain or buildings in the game. Rests on LEVEL-OBJ-3, LEVEL-
OBJ-4.

[code: rust/bc2-levelwrite/src/rules.rs:696] [code: rust/bc2-formats/src/dbxedit/check.rs:296]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it]

### XVAL-CULL-5: No cull slot shared by two objects

Status: **verified**

Warning (round GR4); checked by the DBX inspector's Level > Check. Objects sharing a cull slot are shown and hidden
together (see-through objects). The export gives moved and placed objects slots of their own; the DBX inspector's
Level > Fix slots repairs a hand-edited file. Rests on CULL-SLOT-3, LEVEL-SLOT-2.

[code: rust/bc2-levelwrite/src/rules.rs:705] [code: rust/bc2-levelwrite/src/validate.rs:138] [code: rust/bc2-formats/src/dbxedit/check.rs:296]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it]

### XVAL-CULL-6: Slots agree with the level whose cull trees are used

Status: **verified**

Warning (round GR4); checked by the DBX inspector's Level > Check. Objects with another slot than the same object in
the cull trees' level are see-through: Level > Fix slots in the DBX inspector gives each the slot of its counterpart.
Rests on CULL-SLOT-2, LEVEL-SLOT-3.

[code: rust/bc2-levelwrite/src/rules.rs:715] [code: rust/bc2-levelwrite/src/validate.rs:138] [code: rust/bc2-formats/src/dbxedit/check.rs:296]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it]

### XVAL-PLACE-1: Vehicle spawners do not stand on each other

Status: **verified**

Warning (round V1); checked live in the editor and by `mapexport validate`. Two vehicle spawners of one team stand
within 1 m of each other: their vehicles spawn into one another. Rests on GAME-SPAWN-1.

[code: rust/bc2-levelwrite/src/rules.rs:726] [code: rust/bc2-levelwrite/src/validate.rs:308]
[test: rust/bc2-levelwrite/tests/export_rules.rs::each_edit_rule_has_a_broken_project_that_raises_it]

### XVAL-PLACE-2: Spawn points stand on ground

Status: **verified**

Warning (round A1); checked live in the editor (its ground, water and collision) and by `mapexport validate` (the
ground and water). A spawn point the edits moved or made over water deeper than 1.2 m, more than 4.5 m deep inside a
static object's collision, more than 6 m under the ground or outside the terrain: a soldier spawning there drowns or
is stuck. The bounds lie past what DICE's own 1,475 spawn points do (GAME-SPAWN-3); on the 27 levels measured the
rule finds nothing. Rests on GAME-SPAWN-1, GAME-SPAWN-3.

[code: rust/bc2-levelwrite/src/rules.rs:735] [code: rust/bc2-levelwrite/src/validate.rs:492] [code: rust/bc2-levelwrite/src/site.rs:461]
[test: rust/bc2-levelwrite/tests/auto_layout.rs::site_rules_raise_on_spawns_in_water_and_rock_vehicles_without_room_and_objects_outside]

### XVAL-PLACE-3: A vehicle spawner has its vehicle's room and domain

Status: **verified**

Warning (round A1); checked live in the editor. A vehicle spawner the edits moved or made needs its vehicle's room
(static collision no closer than below DICE's tightest of the class: tanks 2 m, helicopters 3.5 m, cars, boats, IFVs
and quads 0.5 m) and its domain: land vehicles not over deep water, boats on water (where the level's water mesh lies
near the ground), none buried in collision, land and sea vehicles inside the terrain. Otherwise the vehicle spawns into the world or cannot move. Rests on GAME-SPAWN-1,
GAME-VEH-1.

[code: rust/bc2-levelwrite/src/rules.rs:745] [code: rust/bc2-levelwrite/src/validate.rs:346] [code: rust/bc2-levelwrite/src/site.rs:483]
[test: rust/bc2-levelwrite/tests/auto_layout.rs::site_rules_raise_on_spawns_in_water_and_rock_vehicles_without_room_and_objects_outside]

### XVAL-PLACE-4: Gameplay objects stand inside a combat area

Status: **verified**

Warning (round A1); checked live in the editor. An M-COM, a spawn point or a land or sea vehicle spawner the edits
moved or made outside every combat area (a spawn point more than 1 m out, a vehicle more than 150 m out, farther than
DICE's own): a player there is told to return to the area and dies as a deserter. Capture points are L1's (L1-CQ-013,
L1-CQ-014); aircraft and the ambient backdrop are not held to it. Rests on GAME-AREA-1, GAME-AREA-2.

[code: rust/bc2-levelwrite/src/rules.rs:756] [code: rust/bc2-levelwrite/src/validate.rs:415]
[test: rust/bc2-levelwrite/tests/auto_layout.rs::site_rules_raise_on_spawns_in_water_and_rock_vehicles_without_room_and_objects_outside]

### XVAL-PLACE-5: Spawns and objectives have a path for the AI

Status: **verified**

Warning (round A1); checked live in the editor once the AI workspace has read the level's path data (NavKit's build,
else DICE's file). A spawn point, flag or M-COM the edits moved or made with no route node within 15 m in plan and
4 m in height, or whose nearest node is on an island cut off from the main network: a bot spawned there cannot
leave, and bots cannot attack or defend an objective there. Rests on AI-NEED-2, AI-NAV-1.

[code: rust/bc2-levelwrite/src/rules.rs:767] [code: rust/bc2-levelwrite/src/validate.rs:632] [code: rust/bc2-levelwrite/src/site.rs:551]
[test: rust/bc2-levelwrite/tests/auto_layout.rs::path_rules_want_spawns_and_objectives_on_the_main_network]

Round A2 also checks it at the stage (`mapexport check`) on the path data the export's path-data step staged
(XLAY-PATH-1).

### XVAL-PLACE-6: A land vehicle has a drivable way out

Status: **verified**

Warning (round A2); checked live in the editor. A tank, IFV, car or quad bike spawner the edits moved or made that its
vehicle cannot drive 20 m away from (a flood fill on 2 m cells: a step up to 1.2 m, a drop up to 2 m, nothing static
0.6 to 2.5 m above the surface, water up to 1.2 m deep): it spawns and stays. DICE's land vehicles all drive 40 m or
more (GAME-VEH-2); on the 27 levels measured the rule finds nothing. Rests on GAME-VEH-1, GAME-VEH-2.

[code: rust/bc2-levelwrite/src/vehicles.rs:177] [code: rust/bc2-levelwrite/src/site.rs:561] [code: rust/bc2-levelwrite/src/validate.rs:574]
[test: rust/bc2-levelwrite/tests/auto_layout_vehicles.rs::vehicle_rules_raise_on_shut_in_vehicles_and_not_in_the_open]

### XVAL-PLACE-7: A boat has water access

Status: **verified**

Warning (round A2); checked live in the editor. A boat spawner the edits moved or made whose water reaches less than
24 m of open water at least 0.5 m deep with nothing static at the surface (a pond, a pier's piles), judged where the
level's water mesh lies near the ground (as XVAL-PLACE-3): it cannot sail out. DICE's boats reach 30.5 m or more.
Rests on GAME-VEH-1, GAME-VEH-2.

[code: rust/bc2-levelwrite/src/vehicles.rs:177] [code: rust/bc2-levelwrite/src/site.rs:618]
[test: rust/bc2-levelwrite/tests/auto_layout_vehicles.rs::vehicle_rules_raise_on_shut_in_vehicles_and_not_in_the_open]

### XVAL-PLACE-8: An aircraft has an open launch area

Status: **verified**

Warning (round A2); checked live in the editor. A helicopter spawner the edits moved or made with static collision
less than 12 m over it within 3.5 m, or a plane spawner without a straight take-off run of 30 m (its span 10 m: no
step over 1 m, no water, nothing crossing 0.6 to 5 m up) on any of 16 bearings: it cannot take off. DICE's helipads
are open to 60 m, its WW2 planes have runs of 34 m or more. Rests on GAME-VEH-1, GAME-VEH-2.

[code: rust/bc2-levelwrite/src/vehicles.rs:177] [code: rust/bc2-levelwrite/src/site.rs:667] [code: rust/bc2-levelwrite/src/site.rs:693]
[test: rust/bc2-levelwrite/tests/auto_layout_vehicles.rs::vehicle_rules_raise_on_shut_in_vehicles_and_not_in_the_open]

### XVAL-AI-1: The staged path data is the level's and loads

Status: **verified**

Error (round A2); a refusal of `mapexport build` with the path-data step, a line of `mapexport check`. The step stages
path data only where the base's is the level's (the project edits no collision and has no navigation edits), and the
check holds each staged file to the one recorded, to Kynapse path data and to every sector's nodes sorted by id
(HPD-NODE-2: unsorted nodes crash the game when it leaves the level, AI-CRASH-4). Rests on HPD-NODE-2, AI-CRASH-4,
AI-NEED-2.

[code: rust/bc2-levelwrite/src/pathdata.rs:94] [code: rust/bc2-levelwrite/src/pathdata.rs:221] [code: rust/bc2-levelwrite/src/rules.rs:725]
[test: rust/bc2-levelwrite/tests/auto_layout_vehicles.rs::the_path_data_step_finds_refuses_stages_and_checks]

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
(`rust/bc2-levelwrite/tests/export_rules.rs`, `each_placement_rule_has_a_broken_layout_that_raises_it`).

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

## 4. The auto layout (round A1)

### XLAY-AUTO-1: The auto layout proposes DICE's layout fitted to the level, never applied silently

Status: **verified**

`bc2_levelwrite::autolayout::propose` lays a project's mode layer (a stock level's own, brought in by the target:
XVAL-MODE-3) out on the base level's site: the whole layer turned, scaled (0.9 to 1.1 of DICE's own distances) and
placed where the ground suits it - flags, bases, M-COMs and spawn points on open land, not in water or rocks, the
combat areas over the terrain and the level's art - then a flag, base or M-COM with points in a bad spot moved with
what belongs to it (up to 40 m) and a spawn point alone (up to 16 m), heights following the ground. Each of the best 8
candidates is measured with L1's metrics (section 3) and the site rules; the proposal has the fewest warnings, then
the fewest statistics outside DICE's ranges (`l1_a1_params.json`), then the fewest notes, then the best ground score.
It is a list of moves the editor shows and commits as one move operation only when the user accepts it. The same
partition, site and seed give the same proposal.

[code: rust/bc2-levelwrite/src/autolayout.rs:1191]
[test: rust/bc2-levelwrite/tests/auto_layout.rs::the_auto_layout_moves_a_layout_out_of_the_lake_onto_the_town]
[test: rust/bc2-editor/tests/auto_layout_panel.rs::the_auto_layout_is_proposed_drawn_accepted_and_validated]

### XLAY-AUTO-2: The auto layout places the layer's vehicles by class, in DICE's counts

Status: **verified**

Round A2. On the laid-out layer, `bc2_levelwrite::vehicles::plan` gives each capture point DICE's count of vehicles
per mode (GAME-VEH-2: a Conquest base 4, 8 on WW2; a flag 2; Rush's stage-1 attackers' base 3, defenders' 2; Squad
Rush none; Squad Deathmatch one IFV at the layer's neutral capture point). The vehicles are the level's own spawners
that no capture point of the layer owns and that spawn by themselves - DICE's for this level - by team (a flag: a
neutral spawner, else one of each team) and class in DICE's order, each moved once, then copies of them. Each stands
on a spot of rings 12 to 88 m out (boats to 148 m, aircraft from 20 m): its domain, its room (XVAL-PLACE-3), able to
leave (XVAL-PLACE-6 to 8), inside a combat area, its capture point the nearest within 150 m (L1's pool), 6 m or more
from other vehicles and 4 m from soldier spawns, facing the layout's centre (a plane its run). Each joins its capture
point's `Spawners`. The proposal carries moves, copies and links as one operation of the log (one move when there are
no vehicles), measured again with them; what could not be placed is named. A later Rush stage's vehicles are not
placed (DICE enables them by the stage's events, which the layout does not make).

[code: rust/bc2-levelwrite/src/vehicles.rs:501] [code: rust/bc2-levelwrite/src/vehicles.rs:230] [code: rust/bc2-levelwrite/src/autolayout.rs:185]
[test: rust/bc2-levelwrite/tests/auto_layout_vehicles.rs::the_auto_layout_places_vehicles_proposes_several_and_keeps_pins]
[test: rust/bc2-editor/tests/auto_layout_panel.rs::several_proposals_side_by_side_with_vehicles_and_the_areas_after_accept]

### XLAY-AUTO-3: Several proposals; pinned objects stay where they stand

Status: **verified**

Round A2. `autolayout::propose_all` returns the best `k` fitted candidates (distinct places or turns), each with its
vehicles and statistics; the editor shows them side by side and draws one at a time. The objects the user pinned
(`Op::Pin`, the project's organisation, format version 8) stay where they stand: with a pinned flag, base or M-COM the
layer is turned and scaled about the pins (its centre where they stand best, a pin left off its place costing 0.2 a
metre) and its own spawns and markers follow it; a pinned spawn point or vehicle spawner is not moved and a pinned
vehicle counts for its capture point. A version-7 project loads as it is.

[code: rust/bc2-levelwrite/src/autolayout.rs:1191] [code: rust/bc2-levelwrite/src/autolayout.rs:606] [code: rust/bc2-levelwrite/src/ops.rs:304]
[test: rust/bc2-levelwrite/tests/auto_layout_vehicles.rs::the_auto_layout_places_vehicles_proposes_several_and_keeps_pins]
[test: rust/bc2-levelwrite/tests/project_versions.rs::version_7_projects_load_as_8_and_pins_need_8]

### XLAY-PATH-1: The export's path-data step stages the base's build only where it is the level's

Status: **implemented**

Round A2 (opt-in, `ExportSpec.path_data`). The step stages the newest path data of the project's base level -
NavKit's build from in-game samples, else from the offline sampler, else DICE's own file - as
`ai/infantry/pathdata.hpd` (and `ai/vehicle/` when there is one; installed under `Output/win32/ai/levels/<id>/`),
recorded in `stage.json`'s `path_data`. It refuses in words when the project edits the level's collision (a static
object moved, added or deleted, a static group member edited, the terrain sculpted: the level must be sampled again),
when the project has navigation edits (NavKit's apply on a NavKit base is Python), or when the base has none. The
check: XVAL-AI-1 and XVAL-PLACE-5 on the staged file. Not yet (NavKit's port): sampling and building the edited level
in process with the project's objectives as seeds, its navigation edits, the stock-MP AI layer for the staged level.

[code: rust/bc2-levelwrite/src/pathdata.rs:120] [code: rust/bc2-levelwrite/src/pathdata.rs:221] [code: rust/bc2-levelwrite/src/pathdata.rs:60]
[test: rust/bc2-levelwrite/tests/auto_layout_vehicles.rs::the_path_data_step_finds_refuses_stages_and_checks]
