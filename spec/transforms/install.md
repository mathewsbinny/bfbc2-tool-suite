# Install and revert: changing the game copy and giving it back

Every tool that writes Project OPFOR's game copy (`game\`) is an **install**: input a staged result (bundles, patched
shared files) and the game copy; output the game copy with the result in it and what is needed to take it out
again. Its inverse, **revert**, must give the game copy back byte for byte. Three mechanisms are in use:

- the **stage protocol** (`src\tools\onslaught\console_common.py`; since round DF2 on the install ledger, section 0 -
  before it:) whole-file backups `<file>.opfor-<stage>-orig`,
  the last-writer record `<file>.opfor-last` (`<stage> <sha1>`), `-applied` records; used by the console stages,
  the WW2 install (`ww2\wake_install.py`) and the map export's install (`mapedit\mapedit_install.py`);
- the **ledger** (`opforcore\ledger.py`), per-slot contributions to shared files, which replaces whole-file backups
  where adopted (OPFOR's `research\PACIFIC_BOUNDARY_AUDIT.md` 6.2);
- the registration order of REG-NEW-4.

The suite's editor never writes the game copy; it runs OPFOR's installer (EDITOR_ARCHITECTURE 3.12, E3d).

## 0. The install ledger: one record of every install (round DF2)

The game copy's install ledger (`<game>\opfor\ledger`, OPFOR's `opforcore.ledger` format `opfor-ledger/1`) is the one
install record of every tool that writes the game copy: the suite's NavKit and stock-MP installs (whole files,
`bc2_formats::gameledger` and its Python twin `bfbc2core.gameledger`) and OPFOR's stages (section 1 and 4 below).
One record per owner (`navkit:<lv>`, `stockmp:<lv>`, OPFOR's stages), the bytes each install replaced kept once by
sha256 in `blobs\`, a lock while an operation runs, a journal while one writes.

### XINST-GL-1: Whole files stack; a revert gives back what was there before that tool, in any order

Status: **verified**

Per game file the owners holding it whole form a stack: the bottom one's prior is what was there before any owner
(DICE's original, kept once by sha256, or none), each one's value the sha256 it wrote. Install puts a new layer on
top, refused when the file changed outside the ledger since the top owner wrote it; a re-install of the top owner
keeps its prior. Uninstall of the top owner writes its prior back (or removes a file it added); uninstall of a buried
owner splices it out - the owner above inherits its prior and the file is untouched - unless the owner above is
derived (built from the file it found), then it is refused, naming what to revert first. So audit AU4 DS-1's sequence
(install NavKit, apply stock-MP, restore NavKit, revert stock-MP) ends with DICE's file, and every order of two
independent tools applied and reverted gives every file back byte for byte. Every check comes before the first write;
other owners' records of other kinds are kept as they are, and a whole-file install over a file another owner holds
slots in is refused.

[code: rust/bc2-formats/src/gameledger.rs:949-1335] [code: core/bfbc2core/gameledger.py:560-760]
[test: rust/bc2-formats/src/gameledger_tests.rs::ds1_sequence_keeps_dices_file] [test: rust/bc2-formats/src/gameledger_tests.rs::every_order_of_two_tools]
[test: rust/bc2-formats/src/gameledger_tests.rs::a_derived_layer_refuses_its_base_leaving] [test: rust/bc2-formats/src/gameledger_tests.rs::opfors_records_are_kept]
[test: core/tests/test_gameledger.py::python_twin] [test: core/tests/test_gameledger.py::cross]

### XINST-GL-2: No interruption leaves a half-written game copy

Status: **verified**

Before the first write every file a commit writes or removes (game files and records) is recorded with its pre-image
in the journal (flushed to disk). Each game file is written beside its target, flushed (`sync_all` / fsync) and moved
over it, the move retried while another process holds the target for a moment; the live file is never moved aside.
A failed write rolls the commit back at once; a killed process leaves the journal and a stale lock, and the next
operation (or `bfbc2 gameledger recover`) rolls it back first, so the command can simply be run again.

[code: rust/bc2-formats/src/gameledger.rs:257-280] [code: rust/bc2-formats/src/gameledger.rs:715-947]
[test: rust/bc2-formats/src/gameledger_tests.rs::an_interruption_at_every_write_step] [test: core/tests/test_gameledger.py::interruptions]
[test: core/tests/test_gameledger.py::cross]

### XINST-GL-3: Markers of before the ledger are adopted, or refused in words

Status: **verified**

A suite install finding its own markers of before the ledger beside a file (NavKit's `.opfor-orig` / `.opfor-new`,
stock-MP's `.opfor-stockmp-*`) adopts them into the ledger in one journalled commit (the backup's bytes become the
prior) and removes them; another tool's markers, or markers that do not describe one finished install, are refused
in plain words before anything is written (OPFOR's migration moves the rest).

[code: rust/bc2-formats/src/gameledger.rs:1504-1617] [code: core/bfbc2core/gameledger.py:856-918]
[test: rust/bc2-formats/src/gameledger_tests.rs::legacy_markers_are_adopted] [test: core/tests/test_gameledger.py::legacy]

### XINST-OPF-1: OPFOR's stages are layers of the ledger, built on the file below

Status: **verified**

Every whole-file write of Project OPFOR's installers - the Onslaught `console_*` stages and Napisal-level `apply_*`
fixes (owner `<stage>:<level>`), the Pacific War install (`wake`) and menu mode (`pacificmenu`), the map export's
install (`mapedit`) and the import scripts (`import`) - is one journalled commit of `opforcore.stages` on the ledger
of XINST-GL-1, with `derived` set: the bytes were built from the file as the stage found it. A redo (the stage on
top, the file its write) builds on the ledger's copy from before it; re-applying a stage under a later layer, or
reverting it while a later layer sits on it, is refused in words naming the stage to revert first
(`console_port.py --from N` reverts N..14 first). A file changed outside the ledger since its top owner wrote it is
refused before anything is written.

[code: opfor:src/tools/opforcore/stages.py:240-312] [code: opfor:src/tools/onslaught/console_common.py:191-300]
[test: opfor:src/tools/opforcore/tests/test_stages.py::test_derived_stages_revert_in_order_only]
[test: opfor:src/tools/opforcore/tests/test_stages.py::test_ds1_sequence_keeps_dices_file]
[test: opfor:src/tools/opforcore/tests/test_stages.py::test_every_order_of_two_independent_tools]
[test: opfor:src/tools/tests/test_df2_installs.py::test_an_earlier_tools_revert_never_goes_under_a_later_stage]
[test: opfor:src/tools/tests/test_df2_installs.py::test_loading_startup_layers_per_level_revert_in_order]

### XINST-OPF-2: No interruption leaves a broken file, and the next run resumes

Status: **verified**

An error at any write of a stage's commit (a file the game holds, a full disk) rolls the commit back from the
journal; a killed process leaves the lock and the journal, and the next run of any OPFOR install rolls it back
before it starts. After each, every file is the start or the expected stage, never truncated. No OPFOR tool writes a
game file in place (a lint test).

[test: opfor:src/tools/opforcore/tests/test_stages.py::test_exception_at_every_write_step_rolls_back]
[test: opfor:src/tools/opforcore/tests/test_stages.py::test_kill_at_every_write_step_is_resumed]
[test: opfor:src/tools/tests/test_df2_installs.py::test_a_stage_revert_interrupted_at_every_write_leaves_the_bundle_whole]
[test: opfor:src/tools/tests/test_df2_installs.py::test_no_tool_writes_the_game_copy_outside_the_ledger]

### XINST-OPF-3: The old markers migrate into the ledger without losing an original

Status: **verified**

`opforcore\migrate.py` reads every file's old markers (`.opfor-<stage>-orig`, `.opfor-last`, `-applied`, NavKit's and
stock-MP's), orders the backups by their write time into a stack (each backup the prior of its stage, the next one
its write), checks the stack against `.opfor-last` and stock-MP's record, and with `--apply` hard-links each backup
into a blob, writes the records and moves the markers to `game\opfor\legacy-markers`. It is idempotent, resumes an
interrupted run, and refuses an inconsistent file in words, leaving it as it was. On a scratch copy of the 23 marked
files of the author's game copy (2026-11-21), every one migrated and the 45 owners reverted top-down each gave back
its old backup byte for byte.

[code: opfor:src/tools/opforcore/migrate.py:1-40]
[test: opfor:src/tools/opforcore/tests/test_migrate.py::test_migrate_then_revert_gives_every_backup_back]
[test: opfor:src/tools/opforcore/tests/test_migrate.py::test_migrate_is_idempotent_and_resumes]
[test: opfor:src/tools/opforcore/tests/test_migrate.py::test_inconsistent_markers_are_refused_and_left]

### XINST-OPF-4: OPFOR and the suite read and stack on each other's records

Status: **verified**

OPFOR's `opforcore.ledger` and the suite's store (`bc2_formats::gameledger`, `bfbc2core.gameledger`) read each
other's records and blobs: a NavKit layer under an OPFOR stage is refused a splice (the stage is derived), and
audit AU4's DS-1 sequence across the three implementations ends with DICE's file.

[test: opfor:src/tools/opforcore/tests/test_suite_ledger_compat.py::test_opfor_and_the_suites_python_twin_share_one_store]
[test: opfor:src/tools/opforcore/tests/test_suite_ledger_compat.py::test_ds1_sequence_across_python_and_rust]


## 1. The stage protocol

### XINST-STG-1: Writes are atomic

Status: **verified**

Every write to the game copy goes to `<file>.opfor-tmp-<pid>` beside the target, is flushed to disk and moved over
the target; a failed write leaves the old file and no temporary file. A failed pack stops the run before anything
is installed.

[code: opfor:src/tools/onslaught/console_common.py:162-188]
[test: opfor:src/tools/tests/test_console_common.py::test_write_atomic_replaces_and_leaves_no_temp] [test: opfor:src/tools/tests/test_console_common.py::test_write_atomic_failure_keeps_the_old_file]
[test: opfor:src/tools/tests/test_console_common.py::test_pack_many_failure_stops_the_run]

### XINST-STG-2: The backup is the file before the stage, taken once

Status: **verified**

**Superseded by XINST-GL-1 / XINST-OPF-1 (round DF2).** Before DF2 a stage kept `<file>.opfor-<stage>-orig`; now the
file before each stage is the ledger's prior, kept once by sha, and a redo keeps it. A file that matches no recorded
state (truncated, or changed by a tool that keeps no record) is refused before anything is written.

[code: opfor:src/tools/opforcore/stages.py:240-312]
[test: opfor:src/tools/opforcore/tests/test_stages.py::test_outside_change_is_refused_and_files_created_are_removed]
[test: opfor:src/tools/opforcore/tests/test_stages.py::test_derived_stages_revert_in_order_only]

### XINST-STG-3: The last writer builds from its backup; others build on the current file

Status: **verified**

A stage rebuilds from the file before its own last write only when it is the file's top layer and the file is its
write; otherwise it builds on the file as it is, so a re-run of an early stage keeps every later stage's work - and
since DF2 that re-run is refused while a later layer sits on it (revert that one first). The `.opfor-last` and
forwarded `-applied` records of before DF2 are retired (audit AU4, DS-10: a forwarded record let an earlier tool's revert
put its old backup over a later stage).

[code: opfor:src/tools/opforcore/stages.py:220-238] [code: opfor:src/tools/onslaught/console_linux_base.py:134-143]
[test: opfor:src/tools/tests/test_df2_installs.py::test_linux_base_redo_builds_on_the_first_stage_and_a_later_stage_refuses]
[test: opfor:src/tools/tests/test_df2_installs.py::test_hud_source_is_the_current_bundle_unless_a_redo]
[test: opfor:src/tools/tests/test_onslaught.py::test_hud_builds_on_the_current_movie_and_never_patches_twice]
[data: 2026-10-30, Valparaiso: two stages re-applied from old backups dropped the soldier pass and the Mark V; the rule followed]

### XINST-STG-4: A revert is refused unless the stage wrote last

Status: **verified**

`--revert` of a stage gives its files back as they were before it, and is refused before anything changes while
another stage's layer sits on it (naming that stage) or a file changed outside the ledger since the stage wrote it.
`--force` of the Napisal-level tools is no longer needed (every install they make is recorded).

[code: opfor:src/tools/opforcore/stages.py:240-312]
[test: opfor:src/tools/tests/test_df2_installs.py::test_combat_squads_revert_refused_after_an_outside_change]
[test: opfor:src/tools/tests/test_df2_installs.py::test_an_earlier_tools_revert_never_goes_under_a_later_stage]
[test: opfor:src/tools/tests/test_console_common.py::test_install_and_revert_wrappers_refuse_in_words]

### XINST-STG-5: Scratch is deleted on success, never a backup

Status: **verified**

A stage deletes its working folder under `work\` when it ends without an error (`--keep` keeps it; a failed run
leaves it). A path outside `work\`, a link or junction, or a path that is or holds a backup or original is never
deleted as scratch.

[code: opfor:src/tools/onslaught/console_common.py:351-400] [test: opfor:src/tools/tests/test_scratch.py::test_drop_scratch_rules] [test: opfor:src/tools/tests/test_scratch.py::test_take_keep]

## 2. The map export's install

### XINST-MAP-1: Every check before the first write

Status: **verified**

`mapedit_install.py --apply <stage>` runs the stage's offline check (`mapexport check`, XEXP-CHK-1), builds the
shared files' patch, and refuses before writing anything: while the game runs, when another tool wrote a shared file
after this tool's last install (revert that tool first), or when a backup would be refused (XINST-STG-2).

[code: opfor:src/tools/mapedit/mapedit_install.py:274-290] [code: opfor:src/tools/mapedit/mapedit_install.py:356-366]
[test: opfor:src/tools/tests/test_mapedit_install.py::test_needed_shared_files]

### XINST-MAP-2: The order: bundles, record, startup and menus, registration last

Status: **verified**

An install copies the level's bundles into `Package\levels\<lv>\` with a record of their sha256
(`opfor-mapedit-installed.json`), then writes the shared files in their order, `package.mft` and `bundleManifest`
last (REG-NEW-4). The shared patch is always built from this tool's backup while it wrote the file last, so every
installed export is in one patch, and a shared file no installed export needs any more goes back from its backup.

[code: opfor:src/tools/mapedit/mapedit_install.py:346-378] [code: opfor:src/tools/mapedit/mapedit_install.py:234-275]
[test: opfor:src/tools/tests/test_mapedit_install.py::test_apply_then_revert_on_a_scratch_copy] [test: opfor:src/tools/tests/test_ww2_install.py::test_install_registers_last]

### XINST-MAP-3: Revert gives every file back byte for byte

Status: **verified**

`--revert <LV>` refuses when an installed bundle changed since the install (its sha256) or another tool wrote a
shared file after this one; it takes the registration out first, then rebuilds the patch without the level (other
exports stay) or, for the last level out, restores each shared file from its backup **with the last-writer record it
had before** (`.opfor-mapedit-prevlast`), and removes the level's folder last. Installing every staged export one
after another on a scratch copy and reverting them gives every shared file back byte for byte, with its record.

[code: opfor:src/tools/mapedit/mapedit_install.py:346-354] [code: opfor:src/tools/mapedit/mapedit_install.py:380-403]
[test: opfor:src/tools/tests/test_mapedit_install.py::test_apply_then_revert_on_a_scratch_copy]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:924-936]

### XINST-MAP-4: Text patches add and take out exactly

Status: **verified**

The shared text files are patched by key, never by position: a level's `package.mft` blocks and `bundleManifest`
lines out, then each installed level's at the end; `maps.ini` with this tool's marker and a section per level (one
line per mode); taking a level out of a patched text gives the text without it.

[code: opfor:src/tools/mapedit/mapedit_install.py:162-224]
[test: opfor:src/tools/tests/test_mapedit_install.py::test_text_patches_add_and_take_out] [test: opfor:src/tools/tests/test_mapedit_install.py::test_any_mode_gets_its_maps_line]
[test: opfor:src/tools/opforcore/tests/test_slots.py::test_mft_add_remove_any_order]

## 3. The ledger

### XINST-LED-1: Any order of installs and uninstalls gives the files back

Status: **verified**

Each owner's contribution is to one slot of one file found by key (a `package.mft` block, a `bundleManifest` line,
an ini key, a bundle entry, a whole file, a folder), recorded with the slot's prior value. Uninstall gives each slot
its prior, or splices when another owner's contribution sits on top; so any order of installs and uninstalls of two
owners gives the files back byte for byte, and a foreign writer's change to other slots survives. A conflict (a slot
a foreign edit changed) is reported and nothing is written.

[code: opfor:src/tools/opforcore/ledger.py:1-40]
[test: opfor:src/tools/opforcore/tests/test_ledger.py::test_six_orders_and_foreign_write] [test: opfor:src/tools/opforcore/tests/test_ledger.py::test_stacking_and_splice_records]
[test: opfor:src/tools/opforcore/tests/test_ledger.py::test_foreign_edit_of_owned_slot_refused] [test: opfor:src/tools/opforcore/tests/test_ledger.py::test_dry_run_and_read_only]

## Open questions

- Since round DF2 every whole-file install is on the ledger; the game copies with old markers are migrated by
  `opforcore\migrate.py` (XINST-OPF-3). The slot contributions (XINST-LED-1) are used by OPFOR's add-ons only; moving
  the registration writes (package.mft blocks, bundleManifest lines, ini keys) onto slots is IN1's.
- The installer is planned to move into the suite (EDITOR_ARCHITECTURE 6, ruling 3); until then these rules cite
  OPFOR's code.
