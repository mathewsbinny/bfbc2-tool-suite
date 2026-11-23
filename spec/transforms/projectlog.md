# The editor's project log and its apply

A map project (`<name>.mapedit.json`, `bc2_levelwrite::project::MapProject`) is data only: the base level, its
partition's name and sha1, the content profile, and a log of operations (move, rotate, delete, duplicate, typed field
sets, placements, organisation, the campaign checklist, the era / mode target, navigation edits), each with its
number, author and time. **Apply** (`bc2_levelwrite::apply` / `apply_with`) is the transformation: the base
partition's bytes and the log in, the edited partition's bytes out. The editor applies each new operation to the
partition it keeps; the export applies the whole log to the base read from the game copy (`transforms/export.md`).
Design: EDITOR_ARCHITECTURE 3.1 and section 6, ruling 2 (a project holds no game content).

## 1. The log

### XLOG-LOG-1: Undo, redo and amend

Status: **verified**

Adding an operation clears the redo stack; undo moves the last operation to the redo stack and redo moves it back;
"go to" undoes or redoes to a given count. A continuing gesture (a drag, a wheel burst) amends the last operation in
place, keeping its number: one gesture is one undo step.

[code: rust/bc2-levelwrite/src/project.rs:541-589] [test: rust/bc2-levelwrite/src/project.rs::log_undo_redo_and_files]

### XLOG-LOG-3: Undo replays from a checkpoint, not the whole log

Status: **verified**

The editor keeps the document's state (its entities, outlines, collision placements, deleted set and edited
partition) after each mode choice and after every 16 operations, at most 8 of them (a mode choice's kept longest).
Undo, redo and History start from the newest checkpoint the log still passes through (the operation number at its
place unchanged: an amend or a new branch after an undo drops the ones it passes) and apply the rest; a mode choice's
donor level is read from the game copy once per document. The result equals the whole log applied from the base in
a fresh document: the partition byte for byte, every transform, the deleted set. On SP_01 as WW2 Conquest with 40
moves (release): undo 9.1 - 10.9 s before, 78 - 98 ms after; History jumps 1.4 s before, 73 - 98 ms after.

[code: rust/bc2-editor/src/edit.rs:222-333] [test: rust/bc2-editor/tests/xf1.rs::undo_on_a_mode_project]

### XLOG-LOG-2: The file: whole writes, an autosave, versions

Status: **verified**

A save writes the whole file through a temporary file and removes the autosave; every change goes into the autosave
beside it (XLOG-AUTO-1), and an autosave that reads and differs from the saved project is offered for recovery. A file of another
format, from a newer editor, or with an operation of an unknown kind is refused in words (the operation named, not
dropped); an older version is migrated as it loads, one step at a time (versions 2 - 5 only added operation kinds;
5 to 6 gives the base an empty `files`; 6 to 7, round SG1, only added the member delete and add, so a version-6 file
loads as it is and is still written with `min_reader` 6 until it holds one). Round OP1 (project format 6): `version` and `min_reader` are read whole -
a missing, zero, non-numeric or wider-than-32-bit version is refused, never cut or read as 0 (audit AU4 BUG-13); a
file is written with `min_reader`, the newest operation kind in its log and redo stack (6 when it records base
files), and a newer file whose `min_reader` this editor reaches is read. The recorded projects of versions 1, 2, 4
and 5 (synthetic, `tests/data/projects`) load with every operation's JSON unchanged and replay to the partition the
version-5 editor gives (sha1 checked on main at 082f2d2).

[code: rust/bc2-levelwrite/src/project.rs:25-80] [code: rust/bc2-levelwrite/src/project.rs:750-906]
[test: rust/bc2-levelwrite/src/project.rs::log_undo_redo_and_files] [test: rust/bc2-levelwrite/tests/project_versions.rs::canary_projects_load_as_version_1]
[test: rust/bc2-levelwrite/src/ops.rs::json_shape_and_names] [test: rust/bc2-levelwrite/src/project.rs::versions_and_min_reader]
[test: rust/bc2-levelwrite/src/project.rs::old_projects_load_and_replay_the_same]
[test: rust/bc2-levelwrite/src/project.rs::a_version_6_project_loads_and_its_member_edit_applies]

### XLOG-AUTO-1: The autosave is a journal: crash-safe, resumable, compacted

Status: **verified**

Since round OP1 (audit AU1 R17 / P3) the autosave `<project>.mapedit.json.autosave` is an append-only journal: a
first line holding the project as it was at the first change since the last save, then one line per change (an
operation added or amended, undo, redo, a History jump, a base file recorded). Each line is the JSON's CRC-32 in 8
hex digits, a space, the JSON and an end of line; it is written and flushed at once and synced to the disk at most
once a second. Reading takes lines up to the first one that is cut short, fails its checksum or does not follow (an
operation number out of order, an undo with nothing to undo) and guesses nothing after it; resuming cuts that end off
and appends after the last good line; after 4,096 lines or 8 MB it is rewritten as one first line, whole or not at
all. A journal cut at every byte gives the project after its last whole line; a writer killed mid-write (six kills,
three of them inside a line) is recovered to operations 1 ... k and resumed. An autosave of the whole project, as
editors before OP1 wrote it, still reads. Project and lock files are written through a synced temporary file moved
over the file with retries, the temporary removed on any failure (audit AU4 DS-20 / BUG-12).

[code: rust/bc2-levelwrite/src/journal.rs:1-290] [code: rust/bc2-levelwrite/src/project.rs:880-959]
[code: rust/bc2-editor/src/edit.rs:775-800]
[test: rust/bc2-levelwrite/src/journal.rs::a_journal_cut_anywhere_gives_its_last_whole_line]
[test: rust/bc2-levelwrite/src/journal.rs::written_resumed_and_compacted]
[test: rust/bc2-levelwrite/tests/journal_kill.rs::journal_survives_its_writer_killed]
[test: rust/bc2-editor/tests/editor_core.rs::strokes_are_tiles_and_the_autosave_is_a_journal]

### XLOG-DELTA-1: Brush strokes are tiles beside the project, named by sha1

Status: **verified**

A `stroke` operation (round OP1, audit AU1 R24 / D19) changes a raster - a height field's stored values or a paint
layer's weights - and names only the 64 x 64 tiles it changed: their places and the sha1s of their files before and
after. A tile's file (`BC2TILE1`, width, height, little-endian `u16` samples) is kept once in
`<project>.mapedit.json.tiles\<sha1>.tile`; a file whose sha1 does not match is never read as the tile. A stroke
applies only where every tile is still its `before` (else nothing is written and it is refused in words); a paint
layer starts at 0 at the stroke's size. Rasters keep their tiles shared with the editor's checkpoints, so undo and
redo cost only the tiles touched. The export does not write rasters yet (round E5) and refuses such a project in words.

[code: rust/bc2-levelwrite/src/delta.rs:1-430] [code: rust/bc2-levelwrite/src/files.rs:270-330]
[code: rust/bc2-editor/src/edit.rs:800-870]
[test: rust/bc2-levelwrite/src/delta.rs::rasters_tiles_and_strokes] [test: rust/bc2-levelwrite/src/delta.rs::tiles_in_a_folder]
[test: rust/bc2-levelwrite/src/files.rs::strokes_apply_from_their_tiles]
[test: rust/bc2-editor/tests/editor_core.rs::strokes_are_tiles_and_the_autosave_is_a_journal] [test: rust/bc2-editor/tests/editor_core.rs::stroke_budgets]

### XLOG-LOG-4: One editor per project; a save never overwrites another writer's file

Status: **implemented**

An editor that opens or creates a project holds `<project>.mapedit.json.lock` (the PC, the process, the time) until
the project closes; another editor refuses the project in words while the holder runs, and a lock whose process is
gone from this PC (or that does not read) is taken over. Before a save the file's modified time and length must be
the ones this editor last read or wrote; otherwise nothing is written and the refusal says so (the changes stay in
the autosave). Round QW2, audit AU1 R4.

[code: rust/bc2-levelwrite/src/project.rs:272-382] [code: rust/bc2-levelwrite/src/project.rs:755-768]
[test: rust/bc2-levelwrite/src/project.rs::lock_and_stamp]

### XLOG-GUID-1: New guids are UUID v5 of the project, the operation and the source

Status: **verified**

An entity an operation creates gets `uuid5("opfor:mapedit", project id, operation number, source guid)`, upper case
(INST-GUID-5), so applying the same log again gives the same guids, and two copies of one source in two operations
differ.

[code: rust/bc2-levelwrite/src/lib.rs:52-65] [code: rust/bc2-levelwrite/src/project.rs:530-539]
[test: rust/bc2-levelwrite/src/lib.rs::guids_are_deterministic]

### XLOG-BASE-1: The base-changed report names, never drops

Status: **verified**

When the base partition's bytes no longer have the project's sha1, the report says so and names every operation
whose target is absent from the base as the log would have it at that point (objects created by earlier operations
count as there, deleted ones and those a mode change removed as gone). The log itself is left as it is.

[code: rust/bc2-levelwrite/src/project.rs:641-672] [test: rust/bc2-levelwrite/src/project.rs::log_undo_redo_and_files]

### XLOG-FILE-1: An operation names the files it changes; a multi-file operation is all or nothing

Status: **verified**

Every operation has targets (round OP1, audit AU1 R18): the level partition, a static group file
(`Levels/<level>/<Name>_HavokAsset.dbx`, by its name in lower case), a height field, a paint layer, the navigation
side file, or the project itself (organisation and the export's choices). The kinds of versions 1 - 5 keep their
JSON and have a fixed target. The `files` operation holds edits of several files - partition operations and static
group members' transforms (16 numbers per member in the group's `Transforms`, each row's fourth word kept) - and
applies them on copies of the files they name: the copies replace the files only when every edit applied, so a
failing edit leaves the partition, the group files and the editor's scene as they were. The editor keeps one writer
per file in its checkpoints, so undo, redo and History jumps bring every file back as one step. A log of the earlier
kinds applies to the same partition bytes as before and to no other file.

[code: rust/bc2-levelwrite/src/files.rs:20-360] [code: rust/bc2-levelwrite/src/ops.rs:410-460]
[code: rust/bc2-levelwrite/src/write.rs:1190-1230] [code: rust/bc2-editor/src/edit.rs:360-410]
[test: rust/bc2-levelwrite/src/files.rs::multi_file_operations_apply_whole_or_not_at_all]
[test: rust/bc2-editor/tests/editor_core.rs::a_multi_file_operation_is_one_step_whole_or_not_at_all]

### XLOG-FILE-2: A base file is recorded when first edited; the export writes the edited group files

Status: **verified**

The first operation that edits a level file besides the partition records the file's sha1 and size in the project
(`base.files`, keyed `group:<file>` / `terrain:<file>`). The export reads such a file from the base level's bundles
when the log names it, applies the log to all files at once, writes each edited static group file in place of the
base's root file of the same name (renamed with the level, as the base's are) and reports whether it is the file the
project was made on. A target the export does not write yet (a height field, a paint layer) refuses the export in
words; a project that edits only the partition exports byte for byte as before.

[code: rust/bc2-levelwrite/src/project.rs:766-785] [code: rust/bc2-levelwrite/src/export.rs:585-600]
[code: rust/bc2-levelwrite/src/export.rs:1032-1083] [code: rust/bc2-levelwrite/src/export.rs:1215-1255]
[test: rust/bc2-levelwrite/tests/model_group_members.rs::an_export_deletes_moves_and_adds_members]

### XLOG-MEM-1: Static group members are named by identity, never by place

Status: **verified**

Round SG1. A member of a model group is named `<group instance guid>#<k>` by its place `k` in the base's group file
for the whole project, and a member the project adds `<group instance guid>#+<n>` (`n` one more than any added to
that instance in the log and its redo stack). The log's member edits name members so (JSON: the number for a base
member, as round OP1 wrote it, `{"added": n}` for an added one); the project's files keep each edited group
instance's members in their current order, and an edit finds a member's place through it. Deleting a member removes
it from the files in DICE's layout (no tombstone, no gap: LEVEL-MG-5, LEVEL-MG-6), so later places shift while every
name stays. The views' operations (move, turn, delete, duplicate, paste, place into a group from the asset browser)
naming members become multi-file operations of the member's group file and, for a delete or an add, its member type
in the partition; an operation naming no member is written as it always was.

[code: rust/bc2-levelwrite/src/members.rs:36-86] [code: rust/bc2-editor/src/memberedit.rs:1-30]
[test: rust/bc2-levelwrite/src/members.rs::identities_read_back]
[test: rust/bc2-editor/tests/group_members.rs::members_are_edited_like_objects]

### XLOG-MEM-2: A member's delete and add keep DICE's layout; the last of a kind stays

Status: **verified**

A delete takes the member's `Assets` item, its 16 transform numbers and its leaf out of the group file, its
`SpawnDatas` record out of its member type, lowers `InstanceCount` and makes the group's cumulative part and network
numbers again. An add copies a member of the kind (its `Assets` reference and `SpawnDatas` record) to the end of
its kind's run at its own transform, its leaf 0 until the export gives one. The last member of a kind in its group is
refused in words before the operation enters the log: DICE's groups never have a kind without one, and its
collision stays in the group's physics. A kind the group does not hold cannot be added to it (its collision is not
in the group's physics); the asset browser places it as a loose object and says so. Members are never scaled: none
of DICE's 23,780 member transforms read on six levels is scaled, and a kind's collision is one shape.

[code: rust/bc2-levelwrite/src/members.rs:405-575]
[test: rust/bc2-levelwrite/src/members.rs::members_are_deleted_and_added_in_dices_layout]
[test: rust/bc2-editor/tests/group_members.rs::members_are_edited_like_objects]
[data: 23,780 of 23,780 member transforms of six levels (four WW2 and BFBC2 groups' kinds) with unit rows]

### XLOG-MEM-3: The export makes the member edits whole: leaves, the client tree, the physics

Status: **verified**

For a log with member edits the export (`groupexport::stage`, after the log is applied): a moved member keeps its
client-tree leaf, moved where it now stands (taken from under its parent and inserted where the tree's spheres hold
it; its sphere is its base leaf carried by its old and new transforms); a deleted member's leaf is freed; an added
member takes a freed leaf, else a new leaf at the client tree's end, its sphere the leaf of the base member it copies
carried to it; its leaf is written to `Enums` and its `SpawnDatas` record. New client leaves move the exclusive
tree's `EnumOffset` and every placed object's slot up by as many, after the exclusive tree's reslot (CULL-SLOT-2).
Each edited group's physics resource is laid out again for the partition's member types (HAVOK-RUN-3); the packfile
is kept. `mapexport check` holds every model group of the staged level to LEVEL-MG-5 - LEVEL-MG-7 and HAVOK-RUN-3.
Wake Island and Atacama Desert exported with a delete, a move and two adds: every check line passes, the deleted
member's leaf is reused by the first add and the second gets leaf L + 1, the other groups' files are the same bytes as
an export without the member edits. A log without member edits runs none of this.

[code: rust/bc2-levelwrite/src/groupexport.rs:1-30] [code: rust/bc2-levelwrite/src/culltree.rs:448-500]
[test: rust/bc2-levelwrite/tests/model_group_members.rs::an_export_deletes_moves_and_adds_members]

## 2. Apply

### XLOG-APP-1: Operations in order; a failure names the operation

Status: **verified**

Apply reads the base (`EditDbx`, byte-keeping: XDBX-WR-2), applies the operations in log order (organisation
operations change nothing in the partition) and writes it. An operation that cannot be applied (no such instance or
field, a value of the wrong type or range, deleting `LevelData`) stops the apply with its number, its name and why.
A mode choice reads its donor's partition from the game copy through a resolver; without the game copy it fails in
words.

[code: rust/bc2-levelwrite/src/write.rs:1021-1070] [code: rust/bc2-levelwrite/src/write.rs:225-232]
[test: rust/bc2-levelwrite/src/write.rs::move_delete_duplicate_set] [test: rust/bc2-levelwrite/tests/export_e3d.rs::rush_to_conquest_as_a_project_operation]

### XLOG-APP-2: The result differs from the base exactly where the operations say

Status: **verified**

On real partitions (MP_005, SP_01, Guadalcanal) a log of moves, rotates, duplicates, deletes and typed field sets
gives a partition that parses, differs from the base in exactly the expected fields (a field-level diff), writes back
byte for byte through the byte-keeping writer, and loads in the scene with every moved object at its new transform,
copies at theirs and deleted ones gone. An empty log gives the base's bytes.

[test: rust/bc2-levelwrite/tests/real_levels.rs::edits_on_real_levels] [test: rust/bc2-levelwrite/src/diff.rs::finds_what_changed]

### XLOG-APP-3: Edits agree with the reference tools byte for byte

Status: **verified**

A delete without event connections is DbxEdit's `drop` byte for byte (with connections, the same but that the
connections are removed, LEVEL-EVT-3, where `drop` leaves them pointing at null); text and number field sets are
DbxEdit's `setfield`; transforms, duplicates and field sets are the Python `dbxbin` reference's bytes.

[test: rust/bc2-levelwrite/tests/real_levels.rs::python_reference] [test: rust/bc2-levelwrite/tests/real_levels.rs::edits_on_real_levels]

### XLOG-APP-4: A duplicate is wired in; a delete takes its wiring out

Status: **verified**

A duplicate gets an `Objects` item (LEVEL-OBJ-4) and copies of its event connections re-pointed to it; a deep
duplicate also copies the instances it owns (a flag's capture area), each with its own new guid. A delete removes
the instance's event connections, then the instance, list items naming it and single references to it (null);
deleting many is one pass with the same result in any order.

[code: rust/bc2-levelwrite/src/write.rs:225-351] [code: rust/bc2-levelwrite/src/write.rs:352-421] [code: rust/bc2-levelwrite/src/write.rs:645-808]
[test: rust/bc2-levelwrite/src/write.rs::move_delete_duplicate_set] [test: rust/bc2-levelwrite/src/write.rs::deep_duplicate_and_place]

### XLOG-APP-5: Field values are typed from the data, never guessed

Status: **verified**

A field set takes its type (kind and size) from the field as the partitions' own data have it (`schema`: the same
rules as the DBX catalogue), and refuses a value outside it (a whole number past 32 bits, text into a number);
placed objects get the defaults of a template instance of the same blueprint.

[code: rust/bc2-levelwrite/src/write.rs:481-506] [test: rust/bc2-levelwrite/src/schema.rs::types_from_the_data] [test: rust/bc2-levelwrite/src/write.rs::move_delete_duplicate_set]

### XLOG-APP-6: Typed gameplay operations check the data's rules

Status: **verified**

Five operations (project format 5) edit gameplay in the data's own terms. `set-team` sets a capture point's
`InitialOwnerTeam` or another object's `Team` to `Team1`, `Team2` or `TeamNeutral` (LEVEL-ENT-1). `link-spawner`
adds a soldier spawner to a capture point's `Spawners` or takes it out (GAME-SPAWN-1). `set-tickets` sets a ticket
counter's `InitialTicketCount` and `TicketLossPerMin` in the types the data gives them (GAME-TKT-2). `insert-point`
puts a copy of an outline point between it and its `Next`, setting `Next` and `Previous` of all three; `remove-point`
links its neighbours to each other, makes a list that named it as its outline's point (an area's `Geometry`) name its
`Next`, and deletes it with its wiring (LEVEL-ENT-2: an outline's points are a chain of `Next` and `Previous`, the
area names one point per outline). Each refuses in words: an object of another type, a link already there or not
there, a negative ticket count, a closed outline left with fewer than three points. On Valparaiso an insert then a
remove gives the partition's content back.

[code: rust/bc2-levelwrite/src/gameplay.rs:106-343] [code: rust/bc2-levelwrite/src/write.rs:1125-1145]
[test: rust/bc2-levelwrite/tests/gameplay_xf1.rs::typed_gameplay_operations_on_valparaiso]

### XLOG-APP-7: Paste is a deep duplicate

Status: **verified**

Paste copies what each pasted object owns (a flag's capture area and marker) with it, as Deep duplicate does, each
copy moved by the paste's shift; an object another pasted one owns is copied as that one's. An edit of the copy's
sub-objects (a ring point moved, the marker renamed) leaves the original's instances as they were.

[code: rust/bc2-editor/src/app/tools.rs:1314-1343] [test: rust/bc2-editor/tests/xf1.rs::paste_is_a_deep_duplicate]

## 4. The gameplay tools (round E4)

### XLOG-GAME-1: A gameplay object is added and deleted with its unit, wired as its template

Status: **verified**

A flag, spawner or M-COM's unit is the object, what moves with it and what it owns through references and its events
(GAME-CQ-11). `place-unit` (project format 9) copies a template's unit to a new place - each member carried with the
root, the root turned as its template - and copies every event connection from or to any member with both ends
remapped, so the copy is wired to the level's shared logic (a Rush stage's counters, shared voice-over) as the
template is. A new flag copies the level's smallest flag, takes the next letter (its name, its marker's `Sid`), the
chosen team and the highest `Index` plus one; a new M-COM copies its stage's first and every counter of the stage
counts it (GAME-RUSH-10); a new spawn point copies one of the spawner's and `link-point` puts it in the spawner's
`AlternativeSpawnPoints`. Each is one operation. A delete takes the unit less what another object's unit holds, and
refuses a team's last base, the level's last Conquest flag and a stage's last M-COM. Carry moves a unit's members as
far as its root, each keeping its height above the ground. On Valparaiso a flag added then deleted gives back every
instance and connection; on Port Valdez an M-COM added keeps the play order; Aurora as Conquest with a new flag, spawn
point and rules exports through the export's own path and passes its stage check, the staged partition reading back
byte for byte. A number field of a grafted layer the base's schema has no evidence for takes the type its own value
shows (the same rule, XLOG-APP-5).

[code: rust/bc2-levelwrite/src/gameplay.rs:938-1347] [code: rust/bc2-levelwrite/src/write.rs:935-975]
[code: rust/bc2-editor/src/tool.rs:347-500]
[test: rust/bc2-levelwrite/tests/gameplay_tools.rs::the_tools_on_a_synthetic_level]
[test: rust/bc2-levelwrite/tests/gameplay_tools.rs::a_spawn_point_added_and_deleted]
[test: rust/bc2-levelwrite/tests/gameplay_tools.rs::aurora_conquest_with_the_tools_edits_exports]

### XLOG-GAME-2: The tools in the editor: one operation, one undo step

Status: **verified**

The Gameplay workspace's tool (`ViewTool`) adds a flag, spawn point or M-COM where the pointer meets the ground and
selects it, or carries a unit by a click and puts it down at the next (Esc drops it); Delete there takes the
selection with its units; the Gameplay panel lists the capture points and Rush stages, sets the selection's team and
applies the round's rules (`set-rules`, only the values changed). Each is one operation of the log, undone in one
step; the views follow (new entities, their zones, team colours).

[code: rust/bc2-editor/src/gameplay.rs:295-530] [code: rust/bc2-editor/src/app/gameplayui.rs:1-60]
[test: rust/bc2-editor/tests/gameplay_tools.rs::gameplay_tools_through_the_interface]

## Open questions

- The gameplay tools (round E4) in game: whether the HUD shows a fifth letter's flag (its capture messages keep the
  template's `HUD_MP_CQ_TOOK_BASE_<letter>` keys: new letters need their own texts), whether a copied flag's
  `Index` matters, whether an M-COM added to a stage arms, alarms and ends its stage, and the rules' effect on a round
  (in-game tests paused, 2026-11-20).

- Static group members (round SG1, XLOG-MEM-1 - XLOG-MEM-3): whether the game accepts a member type with no
  instance (the editor refuses the last of a kind until it is known), whether a member whose leaf sits far from its
  old place culls well (its old ancestors keep their spheres), and everything in game: a moved member's collision
  following its transform (LEVEL-MG-8 is inferred from the files), the new client leaves and the moved slots
  (in-game tests paused, 2026-11-20).
