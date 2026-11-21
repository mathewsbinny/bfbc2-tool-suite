# Install and revert: changing the game copy and giving it back

Every tool that writes Project OPFOR's game copy (`game\`) is an **install**: input a staged result (bundles, patched
shared files) and the game copy; output the game copy with the result in it and what is needed to take it out
again. Its inverse, **revert**, must give the game copy back byte for byte. Three mechanisms are in use:

- the **stage protocol** (`src\tools\onslaught\console_common.py`): whole-file backups `<file>.opfor-<stage>-orig`,
  the last-writer record `<file>.opfor-last` (`<stage> <sha1>`), `-applied` records; used by the console stages,
  the WW2 install (`ww2\wake_install.py`) and the map export's install (`mapedit\mapedit_install.py`);
- the **ledger** (`opforcore\ledger.py`), per-slot contributions to shared files, which replaces whole-file backups
  where adopted (OPFOR's `research\PACIFIC_BOUNDARY_AUDIT.md` 6.2);
- the registration order of REG-NEW-4.

The suite's editor never writes the game copy; it runs OPFOR's installer (EDITOR_ARCHITECTURE 3.12, E3d).

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

Before a stage writes a file, its backup `<file>.opfor-<stage>-orig` becomes the file as it is, except when the
stage redoes its own last write (it is the last writer and the file is as it left it): then the backup from before
that write stays. A backup is refused, and nothing installed, when the stage has one and the file matches no
recorded state (not the last-writer record, not the backup, no tool's `-applied` record): the file was truncated or
changed by a tool that keeps no record, and the backup may be the only good copy.

[code: opfor:src/tools/onslaught/console_common.py:235-279]
[test: opfor:src/tools/tests/test_console_common.py::test_take_backup_first_install_copies] [test: opfor:src/tools/tests/test_console_common.py::test_take_backup_redo_keeps_the_backup]
[test: opfor:src/tools/tests/test_console_common.py::test_take_backup_after_a_recorded_stage_refreshes] [test: opfor:src/tools/tests/test_console_common.py::test_take_backup_refuses_a_truncated_file]
[test: opfor:src/tools/tests/test_console_common.py::test_take_backup_accepts_an_applied_record]

### XINST-STG-3: The last writer builds from its backup; others build on the current file

Status: **verified**

Every install records itself as the file's last writer (`<file>.opfor-last`: the stage and the file's sha1). A stage
rebuilds from its own backup only when it is the last writer; otherwise it builds on the file as it is, so a re-run
of an early stage keeps every later stage's work. A tool's `-applied` records that named the file before a rewrite
are moved to the new sha1 (its changes are still in it).

[code: opfor:src/tools/onslaught/console_common.py:191-233]
[test: opfor:src/tools/tests/test_onslaught.py::test_linux_base_ignores_a_stale_backup] [test: opfor:src/tools/tests/test_onslaught.py::test_hud_source_is_the_current_bundle_unless_a_redo]
[test: opfor:src/tools/tests/test_onslaught.py::test_hud_builds_on_the_current_movie_and_never_patches_twice]
[data: 2026-10-30, Valparaiso: two stages re-applied from old backups dropped the soldier pass and the Mark V; the rule followed]

### XINST-STG-4: A revert is refused unless the stage wrote last

Status: **verified**

`--revert` of a stage puts its backup back only when the stage is the file's last writer (its record names the file
as it is now); a tool keeping `-applied` records reverts only when the record exists and matches (`--force` for an
install from before the record was kept, still refused when a record differs). Otherwise it is refused before
anything changes: reverting would undo another stage's work.

[code: opfor:src/tools/onslaught/console_common.py:281-301]
[test: opfor:src/tools/tests/test_console_common.py::test_check_revert_needs_the_record] [test: opfor:src/tools/tests/test_console_common.py::test_check_applied]
[test: opfor:src/tools/tests/test_onslaught.py::test_reverts_are_refused_without_a_record] [test: opfor:src/tools/tests/test_onslaught.py::test_sector_offsets_and_ai_template_revert_need_a_record]

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

[code: opfor:src/tools/mapedit/mapedit_install.py:341-345] [code: opfor:src/tools/mapedit/mapedit_install.py:378-385]
[test: opfor:src/tools/tests/test_mapedit_install.py::test_needed_shared_files]

### XINST-MAP-2: The order: bundles, record, startup and menus, registration last

Status: **verified**

An install copies the level's bundles into `Package\levels\<lv>\` with a record of their sha256
(`opfor-mapedit-installed.json`), then writes the shared files in their order, `package.mft` and `bundleManifest`
last (REG-NEW-4). The shared patch is always built from this tool's backup while it wrote the file last, so every
installed export is in one patch, and a shared file no installed export needs any more goes back from its backup.

[code: opfor:src/tools/mapedit/mapedit_install.py:386-403] [code: opfor:src/tools/mapedit/mapedit_install.py:234-275]
[test: opfor:src/tools/tests/test_mapedit_install.py::test_apply_then_revert_on_a_scratch_copy] [test: opfor:src/tools/tests/test_ww2_install.py::test_install_registers_last]

### XINST-MAP-3: Revert gives every file back byte for byte

Status: **verified**

`--revert <LV>` refuses when an installed bundle changed since the install (its sha256) or another tool wrote a
shared file after this one; it takes the registration out first, then rebuilds the patch without the level (other
exports stay) or, for the last level out, restores each shared file from its backup **with the last-writer record it
had before** (`.opfor-mapedit-prevlast`), and removes the level's folder last. Installing every staged export one
after another on a scratch copy and reverting them gives every shared file back byte for byte, with its record.

[code: opfor:src/tools/mapedit/mapedit_install.py:361-376] [code: opfor:src/tools/mapedit/mapedit_install.py:405-436]
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

- The stage protocol and the ledger coexist; which tools still use whole-file backups, and the ledger's adoption of
  them (`adopt`), are OPFOR's migration, not yet complete.
- The installer is planned to move into the suite (EDITOR_ARCHITECTURE 6, ruling 3); until then these rules cite
  OPFOR's code.
