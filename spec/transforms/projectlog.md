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

A save writes the whole file through a temporary file and removes the autosave; every change writes the autosave
beside it, and an autosave that reads and differs from the saved project is offered for recovery. A file of another
format, from a newer editor, or with an operation of an unknown kind is refused in words (the operation named, not
dropped); an older version is migrated as it loads, one step at a time (versions 2 - 5 only added operation kinds;
5 to 6 gives the base an empty `files`). Round OP1 (project format 6): `version` and `min_reader` are read whole -
a missing, zero, non-numeric or wider-than-32-bit version is refused, never cut or read as 0 (audit AU4 BUG-13); a
file is written with `min_reader`, the newest operation kind in its log and redo stack (6 when it records base
files), and a newer file whose `min_reader` this editor reaches is read. The recorded projects of versions 1, 2, 4
and 5 (synthetic, `tests/data/projects`) load with every operation's JSON unchanged and replay to the partition the
version-5 editor gives (sha1 checked on main at 082f2d2).

[code: rust/bc2-levelwrite/src/project.rs:25-80] [code: rust/bc2-levelwrite/src/project.rs:750-906]
[test: rust/bc2-levelwrite/src/project.rs::log_undo_redo_and_files] [test: rust/bc2-levelwrite/tests/project_versions.rs::canary_projects_load_as_version_1]
[test: rust/bc2-levelwrite/src/ops.rs::json_shape_and_names] [test: rust/bc2-levelwrite/src/project.rs::versions_and_min_reader]
[test: rust/bc2-levelwrite/src/project.rs::old_projects_load_and_replay_the_same]

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

[code: rust/bc2-levelwrite/src/lib.rs:50-62] [code: rust/bc2-levelwrite/src/project.rs:530-539]
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
[code: rust/bc2-levelwrite/src/write.rs:1180-1220] [code: rust/bc2-editor/src/edit.rs:360-410]
[test: rust/bc2-levelwrite/src/files.rs::multi_file_operations_apply_whole_or_not_at_all]
[test: rust/bc2-editor/tests/op1.rs::a_multi_file_operation_is_one_step_whole_or_not_at_all]

### XLOG-FILE-2: A base file is recorded when first edited; the export writes the edited group files

Status: **implemented**

The first operation that edits a level file besides the partition records the file's sha1 and size in the project
(`base.files`, keyed `group:<file>` / `terrain:<file>`). The export reads such a file from the base level's bundles
when the log names it, applies the log to all files at once, writes each edited static group file in place of the
base's root file of the same name (renamed with the level, as the base's are) and reports whether it is the file the
project was made on. A target the export does not write yet (a height field, a paint layer) refuses the export in
words; a project that edits only the partition exports byte for byte as before.

[code: rust/bc2-levelwrite/src/project.rs:766-785] [code: rust/bc2-levelwrite/src/export.rs:585-595]
[code: rust/bc2-levelwrite/src/export.rs:1015-1060] [code: rust/bc2-levelwrite/src/export.rs:1185-1225]

## 2. Apply

### XLOG-APP-1: Operations in order; a failure names the operation

Status: **verified**

Apply reads the base (`EditDbx`, byte-keeping: XDBX-WR-2), applies the operations in log order (organisation
operations change nothing in the partition) and writes it. An operation that cannot be applied (no such instance or
field, a value of the wrong type or range, deleting `LevelData`) stops the apply with its number, its name and why.
A mode choice reads its donor's partition from the game copy through a resolver; without the game copy it fails in
words.

[code: rust/bc2-levelwrite/src/write.rs:1011-1060] [code: rust/bc2-levelwrite/src/write.rs:219-226]
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

[code: rust/bc2-levelwrite/src/write.rs:219-345] [code: rust/bc2-levelwrite/src/write.rs:346-415] [code: rust/bc2-levelwrite/src/write.rs:635-798]
[test: rust/bc2-levelwrite/src/write.rs::move_delete_duplicate_set] [test: rust/bc2-levelwrite/src/write.rs::deep_duplicate_and_place]

### XLOG-APP-5: Field values are typed from the data, never guessed

Status: **verified**

A field set takes its type (kind and size) from the field as the partitions' own data have it (`schema`: the same
rules as the DBX catalogue), and refuses a value outside it (a whole number past 32 bits, text into a number);
placed objects get the defaults of a template instance of the same blueprint.

[code: rust/bc2-levelwrite/src/write.rs:475-500] [test: rust/bc2-levelwrite/src/schema.rs::types_from_the_data] [test: rust/bc2-levelwrite/src/write.rs::move_delete_duplicate_set]

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

[code: rust/bc2-levelwrite/src/gameplay.rs:106-343] [code: rust/bc2-levelwrite/src/write.rs:1115-1135]
[test: rust/bc2-levelwrite/tests/gameplay_xf1.rs::typed_gameplay_operations_on_valparaiso]

### XLOG-APP-7: Paste is a deep duplicate

Status: **verified**

Paste copies what each pasted object owns (a flag's capture area and marker) with it, as Deep duplicate does, each
copy moved by the paste's shift; an object another pasted one owns is copied as that one's. An edit of the copy's
sub-objects (a ring point moved, the marker renamed) leaves the original's instances as they were.

[code: rust/bc2-editor/src/app/tools.rs:1313-1342] [test: rust/bc2-editor/tests/xf1.rs::paste_is_a_deep_duplicate]

## Open questions

- Editing static model-group members in the editor (round SG1): the operation model and the export carry them since
  OP1 (XLOG-FILE-1, XLOG-FILE-2); the views still treat members as read-only.
