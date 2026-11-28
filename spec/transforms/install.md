# Install and revert: changing the game copy and giving it back

Every tool that writes Project OPFOR's game copy (`game\`) is an **install**: input a staged result (bundles, patched
shared files) and the game copy; output the game copy with the result in it and what is needed to take it out
again. Its inverse, **revert**, must give the game copy back byte for byte. Three mechanisms are in use:

- the **stage protocol** (`src\tools\onslaught\console_common.py`; since round DF2 on the install ledger, section 0 -
  before it:) whole-file backups `<file>.opfor-<stage>-orig`,
  the last-writer record `<file>.opfor-last` (`<stage> <sha1>`), `-applied` records; used by the console stages,
  the WW2 install (`ww2\wake_install.py`; its game-copy side `bc2-install` since round OR1) and the map export's
  install (`mapedit\mapedit_install.py`; `bc2_install::mapedit` since round OR6);
- the **ledger** (`bc2_gamefs::ledger` since round OR15; before it also `opforcore\ledger.py`), contributions to
  shared files recorded per owner, which replaces whole-file backups (OPFOR's `research\PACIFIC_BOUNDARY_AUDIT.md`
  6.2);
- the registration order of REG-NEW-4.

The suite's editor writes the game copy only through the map export's install (section 2; round OR6: in process,
before it ran OPFOR's installer: EDITOR_ARCHITECTURE 3.12, E3d).

## 0. The install ledger: one record of every install (round DF2)

The game copy's install ledger (`<game>\opfor\ledger`, format `opfor-ledger/1`) is the one install record of every
tool that writes the game copy: the suite's NavKit and stock-MP installs, OPFOR's stages, the WW2 and map export
installs and the mod manager (sections 1 - 5 below). Since round OR15 it has one implementation, `bc2_gamefs::ledger`
(XINST-LED-2): `bfbc2core.gameledger` and OPFOR's `opforcore.stages` call it (`bc2_native.ledger_call`).
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

[code: rust/bc2-gamefs/src/ledger.rs:863-1249] [code: rust/bc2-install/src/ledgerapi.rs:1-40]
[test: rust/bc2-gamefs/src/ledger_tests.rs::ds1_sequence_keeps_dices_file] [test: rust/bc2-gamefs/src/ledger_tests.rs::every_order_of_two_tools]
[test: rust/bc2-gamefs/src/ledger_tests.rs::a_derived_layer_refuses_its_base_leaving] [test: rust/bc2-gamefs/src/ledger_tests.rs::opfors_records_are_kept]
[test: core/tests/test_gameledger.py::python_twin] [test: core/tests/test_gameledger.py::cross]

### XINST-GL-2: No interruption leaves a half-written game copy

Status: **verified**

Before the first write every file a commit writes or removes (game files and records) is recorded with its pre-image
in the journal (flushed to disk). Each game file is written beside its target, flushed (`sync_all` / fsync) and moved
over it, the move retried while another process holds the target for a moment; the live file is never moved aside.
A failed write rolls the commit back at once; a killed process leaves the journal and a stale lock, and the next
operation (or `bfbc2 gameledger recover`) rolls it back first, so the command can simply be run again.

[code: rust/bc2-gamefs/src/ledger.rs:247-257] [code: rust/bc2-gamefs/src/ledger.rs:628-861]
[test: rust/bc2-gamefs/src/ledger_tests.rs::an_interruption_at_every_write_step] [test: core/tests/test_gameledger.py::interruptions]
[test: core/tests/test_gameledger.py::cross]

### XINST-GL-3: Markers of before the ledger are adopted, or refused in words

Status: **verified**

A suite install finding its own markers of before the ledger beside a file (NavKit's `.opfor-orig` / `.opfor-new`,
stock-MP's `.opfor-stockmp-*`) adopts them into the ledger in one journalled commit (the backup's bytes become the
prior) and removes them; another tool's markers, or markers that do not describe one finished install, are refused
in plain words before anything is written (OPFOR's migration moves the rest).

[code: rust/bc2-gamefs/src/ledger.rs:1340-1500]
[test: rust/bc2-gamefs/src/ledger_tests.rs::legacy_markers_are_adopted] [test: core/tests/test_gameledger.py::legacy]

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

[code: rust/bc2-install/src/stages.rs:280-400] [code: opfor:src/tools/opforcore/stages.py:200-240] [code: opfor:src/tools/onslaught/console_common.py:191-300]
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

[test: opfor:src/tools/opforcore/tests/test_stages.py::test_kill_at_every_write_step_is_resumed]
[test: opfor:src/tools/tests/test_df2_installs.py::test_a_stage_revert_killed_at_every_write_leaves_the_bundle_whole]
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
[test: opfor:src/tools/opforcore/tests/test_migrate.py::test_migrate_is_idempotent_and_a_kill_at_every_step_resumes]
[test: opfor:src/tools/opforcore/tests/test_migrate.py::test_inconsistent_markers_are_refused_and_left]

### XINST-OPF-4: OPFOR and the suite read and stack on each other's records

Status: **verified**

OPFOR's stages and the suite's tools share one store and, since round OR15, one implementation (XINST-LED-2): a
NavKit layer under an OPFOR stage is refused a splice (the stage is derived), and audit AU4's DS-1 sequence through
the Rust command lines and the Python clients ends with DICE's file. Records the retired implementations wrote read
and revert (XINST-LED-3).

[test: core/tests/test_gameledger.py::cross] [test: core/tests/test_gameledger.py::old_disks]
[test: opfor:src/tools/opforcore/tests/test_old_ledgers.py::test_installs_made_by_the_old_code_revert_with_the_new]


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

[code: rust/bc2-install/src/stages.rs:280-400] [code: opfor:src/tools/opforcore/stages.py:200-240]
[test: opfor:src/tools/opforcore/tests/test_stages.py::test_outside_change_is_refused_and_files_created_are_removed]
[test: opfor:src/tools/opforcore/tests/test_stages.py::test_derived_stages_revert_in_order_only]

### XINST-STG-3: The last writer builds from its backup; others build on the current file

Status: **verified**

A stage rebuilds from the file before its own last write only when it is the file's top layer and the file is its
write; otherwise it builds on the file as it is, so a re-run of an early stage keeps every later stage's work - and
since DF2 that re-run is refused while a later layer sits on it (revert that one first). The `.opfor-last` and
forwarded `-applied` records of before DF2 are retired (audit AU4, DS-10: a forwarded record let an earlier tool's revert
put its old backup over a later stage).

[code: rust/bc2-install/src/stages.rs:230-280] [code: opfor:src/tools/onslaught/console_linux_base.py:46-54]
[test: opfor:src/tools/tests/test_df2_installs.py::test_linux_base_redo_builds_on_the_first_stage_and_a_later_stage_refuses]
[test: opfor:src/tools/tests/test_df2_installs.py::test_hud_source_is_the_current_bundle_unless_a_redo]
[test: rust/bc2-install/src/onslaught/hud.rs::never_patches_twice]
[test: opfor:src/tools/tests/test_onslaught.py::test_hud_passes_each_bundles_source_and_writes_the_records_after_the_install]
[data: 2026-10-30, Valparaiso: two stages re-applied from old backups dropped the soldier pass and the Mark V; the rule followed]

### XINST-STG-4: A revert is refused unless the stage wrote last

Status: **verified**

`--revert` of a stage gives its files back as they were before it, and is refused before anything changes while
another stage's layer sits on it (naming that stage) or a file changed outside the ledger since the stage wrote it.
`--force` of the Napisal-level tools is no longer needed (every install they make is recorded).

[code: rust/bc2-install/src/stages.rs:280-400] [code: opfor:src/tools/opforcore/stages.py:200-240]
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

The map editor's exports (`mapexport build`, a stage folder with `stage.json`) are installed by `bc2_install::mapedit`
(round OR6, "OPFOR in Rust": `bfbc2 install mapedit plan | apply | revert | status`, the editor's Install dialog in
process; the stage's check and the shared bundles' patches through `bc2_install::mapedit::Patches`, implemented by
`bc2_levelwrite::installpatch::LevelWrite`). Project OPFOR's `mapedit\mapedit_install.py` runs the same command; its Python twin (OPFOR 706b909) was
retired after the parity proof of XINST-MAP-8.

### XINST-MAP-1: Every check before the first write

Status: **verified**

An apply runs the stage's offline check (`mapexport check`'s rules, XEXP-CHK-1, against the check game copy), refuses
a level folder that is not an earlier install of an export (a stock level is never replaced), builds the shared
files' patch, and refuses before writing anything: while the game runs, when another tool wrote a shared file after
this owner's last install or changed it outside the ledger (revert that tool first), when the level's path data
would be refused, or when a shared file the set no longer needs could not go back.

[code: rust/bc2-install/src/mapedit.rs:663-700] [code: rust/bc2-install/src/mapedit.rs:918-960] [code: rust/bc2-install/src/stages.rs:251-309]
[test: rust/bc2-levelwrite/tests/install_mapedit.rs::the_patches_come_from_each_installs_kept_copy]
[test: rust/bc2-levelwrite/tests/install_mapedit.rs::text_patches_add_and_take_out]

### XINST-MAP-2: The order: the level's record, its kept copy and bundles, its path data, the shared files

Status: **verified**

An install writes, in order: the level's folder `Package\levels\<lv>\` with its record first
(`opfor-mapedit-installed.json`: `stage.json`'s fields in their order, then `stage` - the stage folder - and
`installed`, each bundle's sha256; `json.dumps(indent=1)` with every character past `~` escaped, as OPFOR's Python
wrote it), its kept copy (XINST-MAP-5) and its bundles (each checked against the stage's sha256 after the copy), the
path data through the ledger as owner `mapedit-ai:<lv>`, then the shared files in one journalled commit of owner
`mapedit` in their order - `startup-00`, `overlay-00`, `ondemand_awards-00`, `maps.ini`, then `package.mft` and
`bundleManifest` last (REG-NEW-4) - each built from the ledger's copy from before this owner while it wrote the file
last, so every installed export is in one patch; a shared file no installed export needs any more (the texts and
pictures once no level has its own name) goes back as it was before this owner.

[code: rust/bc2-install/src/mapedit.rs:918-960] [code: rust/bc2-install/src/mapedit.rs:962-999] [code: rust/bc2-install/src/mapedit.rs:579-661]
[code: rust/bc2-install/src/mapedit.rs:1038-1055]
[test: rust/bc2-levelwrite/tests/install_mapedit.rs::install_beside_and_revert_give_every_file_back]
[test: rust/bc2-levelwrite/tests/install_mapedit.rs::path_data_goes_through_the_ledger_under_its_own_owner]

### XINST-MAP-3: Revert gives every file back byte for byte

Status: **verified**

A revert refuses when an installed bundle changed since the install (its sha256), or the ledger would refuse taking
the owner's files out; it writes the patch without the levels taken out (other exports stay) or, for the last level
out, every shared file back as it was before this owner in one journalled commit; the levels' path data back; their
folders removed, each record last. Installing exports one after another on a copy and reverting them gives every
shared file back byte for byte, and the ledger as it was.

[code: rust/bc2-install/src/mapedit.rs:1071-1130] [code: rust/bc2-install/src/mapedit.rs:1132-1146]
[test: rust/bc2-levelwrite/tests/install_mapedit.rs::install_beside_and_revert_give_every_file_back]
[test: opfor:src/tools/tests/test_mapedit_install.py::test_apply_then_revert_on_a_scratch_copy]

### XINST-MAP-4: Text patches add and take out exactly

Status: **verified**

The shared text files are patched by key, never by position: a level's `package.mft` blocks and `bundleManifest`
lines out, then each installed level's at the end (the WW2 install's rules, `bc2_install::text`); `maps.ini` with this
installer's marker line and a section per level (`[<LV>]`, `name=<title>`, `<mode>=Levels/<LV>`); taking a level out
of a patched text gives the text without it. A file is read and written byte for byte (UTF-8 when it is, else
latin-1); what an install adds is ASCII (XEXP-PRE-1, REG-MFT-1, REG-BM-1).

[code: rust/bc2-install/src/mapedit.rs:332-358] [code: rust/bc2-install/src/mapedit.rs:360-373] [code: rust/bc2-install/src/mapedit.rs:375-389]
[test: rust/bc2-levelwrite/tests/install_mapedit.rs::text_patches_add_and_take_out]
[test: opfor:src/tools/opforcore/tests/test_slots.py::test_mft_add_remove_any_order]

### XINST-MAP-5: Each install keeps its own copy of what the shared patches are built from

Status: **verified**

The shared patches of every installed export are built from that export's own kept copy (`Package\levels\<lv>\
opfor-mapedit-stage\`: `stage.json`, the description partition, the pictures, at their paths in the stage), written
by its install, never from the editor's stage folder, which a later export rewrites (audit AU4, DS-12: an install or
revert of another level rebuilt this one's description, texts and pictures from whatever the stage folder held
then). An install made before round OR6 has no kept copy: its stage folder is read, with a line saying so.

[code: rust/bc2-install/src/mapedit.rs:1001-1036] [code: rust/bc2-install/src/mapedit.rs:564-577]
[test: rust/bc2-levelwrite/tests/install_mapedit.rs::the_patches_come_from_each_installs_kept_copy]

### XINST-MAP-6: A kill at any write step is finished by the next run

Status: **verified**

`BC2_INSTALL_CRASH_AT=<n>` ends `bfbc2 install mapedit` before its n-th write step (the level folder, its record, each
kept file and bundle, each ledger step: journal, blobs, files, records; each removal of a revert), as a kill would.
The record is the level folder's first write and a revert's last removal, so a folder left part way is always an
export's: the same command run again rolls an interrupted ledger commit back and finishes with the files and records
of a run never interrupted (a folder holding only the record's temporary files counts as an install's start).

[code: rust/bc2-install/src/stages.rs:82-93] [code: rust/bc2-install/src/stages.rs:133-146] [code: rust/bc2-install/src/mapedit.rs:490-501]
[test: rust/bc2-levelwrite/tests/install_mapedit.rs::a_kill_at_every_write_step_is_finished_by_the_next_run]
[data: round OR6, real exports on scratch copies: an apply of a second export (18 write steps: the folder, its record, 4 kept files, 4 bundles, 8 ledger steps) and a revert of one level of two (17: 8 ledger steps, 9 removals), each killed before every step and run again: 35 of 35 with the files and ledger records of an uninterrupted run]

### XINST-MAP-7: On DICE's package.mft only the levels' blocks change

Status: **verified**

DICE's `package.mft` ends with a blank line (REG-MFT-1: 900 blank lines for 899 blocks). OPFOR's Python wrote the
patched file ending in one CRLF, so an install on DICE's file changed a byte outside the levels' blocks; the file's run
of line ends at its end is kept (a decision against DICE's data, round OR6). The WW2 install's text rules are
unchanged (their oracle holds them).

[code: rust/bc2-install/src/mapedit.rs:332-358]
[test: rust/bc2-levelwrite/tests/install_mapedit.rs::text_patches_add_and_take_out]
[data: DICE's package.mft (the ledger's original): 136,251 bytes ending CRLF CRLF; through the Python's patch with no level: one CRLF fewer; bundleManifest (835 lines, LF) unchanged by its patch]

### XINST-MAP-8: The files, records and lines are the Python installer's

Status: **verified**

On scratch copies of the author's six shared files (migrated into the ledger: owners import, loading, loadset,
minimap-edges, wake), the Python installer (OPFOR 706b909) and `bfbc2 install mapedit` ran plan, apply, revert,
re-apply, a redo, a second export beside the first, that level replaced by its named export, one level out of two,
the named one replaced by a plain one (the texts and pictures back as before this owner) and a revert of all: every
game file and level record identical after each step, every ledger record identical but its transaction id and time
(the Rust writes its keys sorted), every printed line identical but the kept copy's (XINST-MAP-5), and the copy
after each full revert the one before the first apply.

[code: rust/bc2-cli/src/mapedit_cli.rs:1-40]
[data: round OR6, scratch copies of the six shared files migrated into the ledger, two fresh exports and a plain variant (rust/BENCHMARKS.md OR6): 10 of 10 steps identical, 38 to 54 game files and 5 to 6 ledger records a step]
[test: rust/bc2-levelwrite/tests/install_mapedit.rs::install_beside_and_revert_give_every_file_back]

## 3. The ledger

### XINST-LED-1: The slot kinds are read and kept, never taken out

Status: **verified**

Format `opfor-ledger/1` has slot kinds besides whole files: a `package.mft` block, a `bundleManifest` line, an ini
key, a bundle entry, a bundle's container format and a folder (`tree`), each found by key with its prior. Only
OPFOR's retired `opforcore.ledger` (round P1) wrote them, and only its own tests did: every install that shipped
wrote whole files (`Stages.install`'s `file` contributions, the migration, NavKit, stock-MP, the WW2 and map export
installs). The one ledger reads them with the retired reader's rules, shows them (`kept`), keeps them byte for byte
through every other owner's commit (a splice rewrites only `prior` / `below`), refuses a revert of their owner in
words (what to do instead) and a whole-file install over a file they hold, and drops them on request (`forget`, no
file touched: what they wrote stays as if written outside the ledger).

[inferred: OPFOR's history - commit 29a8967 "no callers yet"; every later caller of Ledger.transaction is
stages.py's whole-file install or a test]
[code: rust/bc2-gamefs/src/ledger_check.rs:1-200]
[test: core/tests/test_gameledger.py::damaged] [test: core/tests/test_gameledger.py::legacy]
[test: opfor:src/tools/opforcore/tests/test_old_ledgers.py::test_installs_made_by_the_old_code_revert_with_the_new]

### XINST-LED-2: One implementation reads every ledger the three wrote, and refuses a damaged one in words

Status: **verified**

Until round OR15 three programs read and wrote the store: `opforcore.ledger`, `bfbc2core.gameledger` and
`bc2_gamefs::ledger` (audit AU5, ARC-3). The format did not change, so no ledger on a user's disk is converted: the one
implementation reads each record by the union of the three readers' rules (the format, the owner and its file name,
`txn` and `installed`, each contribution's kind, file, slot, below, extra, the text kinds' text, a whole file's sha,
a prior's blob, a tree's files) and refuses the first that breaks one naming the file, why and how to drop it; the
status lists every damaged record as `corrupt` beside the healthy owners, and the check (`bfbc2 gameledger check`,
`bfbc2 doctor`) also names every revert whose blob is missing or damaged. A record another implementation wrote stays
byte for byte while other owners commit.

[code: rust/bc2-gamefs/src/ledger_check.rs:1-130] [code: rust/bc2-gamefs/src/ledger.rs:380-395]
[test: rust/bc2-gamefs/src/ledger_check.rs::every_reader_rule_is_kept] [test: core/tests/test_gameledger.py::damaged]
[test: core/tests/test_gameledger.py::old_disks]

### XINST-LED-3: Installs made by the retired implementations revert and recover with the one ledger

Status: **verified**

The retired Python implementations, run from their last commits (the suite's `1e92f152`, OPFOR's `f3a70ec`) on
scratch game copies: their installs - derived stages stacked on one bundle, NavKit's and stock-MP's independent
layers, a restated file - revert with the one ledger under the same rules (a derived layer refuses its base leaving,
an independent one is spliced), every file back byte for byte; a commit either left half-written by a kill at any
of its write steps is rolled back by the one ledger's first operation, and the next install and revert go through.

[test: core/tests/test_gameledger.py::old_disks]
[test: opfor:src/tools/opforcore/tests/test_old_ledgers.py::test_installs_made_by_the_old_code_revert_with_the_new]
[test: opfor:src/tools/opforcore/tests/test_old_ledgers.py::test_a_commit_the_old_code_left_half_written_is_rolled_back]

## 4. The WW2 install in the suite (round OR1, "OPFOR in Rust" stage 1)

Project OPFOR's WW2 install (`wake_install.py --plan / --apply / --apply-staged / --revert`) builds the levels'
bundles and the shared patches in Python, then writes the install plan (`bfbc2-install-plan/1`: the staging and
OPFOR's decisions as data) and runs `bfbc2 install` (`bc2-install`, the game-copy installer IN1 builds on).

### XINST-WW2-1: The gates and the guard before the first write; the registration last

Status: **verified**

A plan whose level failed its model-group or icon gate, or a shared file whose copy from before the install holds
the install's own content (the backup guard on the plan's marks: WW2 entries, lines, the pause-map shader, the
level list, the names build's files), writes nothing; then every shared file's ledger check (another install on top,
a change outside the ledger) before the first write. The order is the Python's: each level's bundles and record,
ww2_common, the DLL's markers, the shared bundles (one commit each), opfor.ini and maps.ini, the registration
(package.mft, bundleManifest) last; then each level verified. An interrupted commit is rolled back before any file
is read.

[code: rust/bc2-install/src/run.rs:629-763] [code: rust/bc2-install/src/marks.rs:312-354]
[test: rust/bc2-install/tests/main.rs::plan_apply_revert_on_a_fake_game] [test: rust/bc2-install/tests/main.rs::a_failed_gate_writes_nothing]
[test: opfor:src/tools/tests/test_ww2_installer.py::test_gates_stop_before_the_first_write]
[test: opfor:src/tools/tests/test_ww2_installer.py::test_guard_refuses_a_polluted_backup]
[test: opfor:src/tools/tests/test_ww2_installer.py::test_an_install_on_top_refuses_the_redo]

### XINST-WW2-2: The records and the files are the Python installer's

Status: **verified**

The shared files go through the ledger as owner `wake` the way `opforcore.stages` writes them (`install_keep`: the
owner's other files kept, an unchanged file kept in place, the record's order untouched / kept / new; `restate` for
opfor.ini edited by the launcher), so a game copy installed by either installer reverts with the other. On scratch
copies of the author's game copy's affected files (migrated into the ledger), the Python installer (OPFOR 1b9ec17)
and this one ran revert, apply of the real staging and revert: every file identical (74 after the apply; the DLL's
two markers name their own bundle's write time), every ledger record identical but its id and time, the printed
lines identical, and each copy after the second revert byte for byte the one before the apply.

[code: rust/bc2-gamefs/src/ledger_keep.rs:83-440] [code: rust/bc2-install/src/run.rs:740-828]
[data: round OR1 scratch proof, 2026-11-21: 37 / 74 / 37 files and 5-6 records identical per step (rust/BENCHMARKS.md OR1)]
[test: rust/bc2-install/tests/main.rs::install_keep_orders_and_restate]
[test: opfor:src/tools/tests/test_ww2_installer.py::test_revert_gives_the_files_back]
[test: opfor:src/tools/tests/test_ww2_installer.py::test_a_launcher_edit_is_kept_by_the_opt_out]

### XINST-WW2-3: A kill at any write step is resumed

Status: **verified**

`BC2_INSTALL_CRASH_AT=<n>` ends `bfbc2 install` before its n-th write step (a level bundle, a record, a marker, a
ledger journal, blob, file or record), as a kill would; the next run rolls the interrupted commit back and finishes
with the same files and records as a run never interrupted, for apply and revert alike. The run holds the WW2
rebuild's lock (`work\ww2\_rebuild\rebuild.lock`): a rebuild and an install never run at once.

[code: rust/bc2-install/src/run.rs:214-243] [code: rust/bc2-install/src/lock.rs:60-139]
[test: opfor:src/tools/tests/test_ww2_installer.py::test_a_kill_at_every_write_step_recovers]
[test: opfor:src/tools/tests/test_ww2_installer.py::test_the_rebuild_lock]

## 4b. The WW2 level build in the suite (round OR2, "OPFOR in Rust" stage 2)

Project OPFOR's `wake_install.build` stages each WW2 level's bundles from its builds and the game copy; since round OR2
its parts other than the shader glue are `bc2_install::levelbuild` (OPFOR's Python passes its decisions - the level's
bindings, its roots, remaps and donors - as data).

### XINST-WW2-4: The closure takes what the level reaches and nothing it loads has

Status: **verified**

The walk starts from the level partition, the partitions named after its assets, the masked partitions its root
files name and the partition names our mesh sets hold; it goes in waves, a wave's stock partitions read bundle by
bundle. Our partition of a name wins over a stock one; a stock partition comes from the first stock bundle holding
it (Package before Dist, levels first, never a WW2 level's own, an Onslaught or OPFOR build, an on-demand or global
streaming bundle), a level-local copy by its alias under the name referenced; its resources from its own bundle, then
each one it lacks from the first other bundle that has it, never a longer partition's of the same stem. A partition a
loaded bundle holds (nam_common's deleted markers hiding mp_common's names) is not copied, but its resources nothing
loaded has are. A terrain's water and its decals' materials and roads name no partition (DICE's convention); a shader
name goes to the shader databases; anything else resolving nowhere is reported as ours or stock, as is a reference
naming an instance its partition lacks.

[code: rust/bc2-install/src/levelbuild/closure.rs:297-420] [code: rust/bc2-install/src/levelbuild/index.rs:150-240]
[test: rust/bc2-install/tests/levelbuild.rs::closure_walks_ours_stock_and_loaded] [test: rust/bc2-install/tests/levelbuild.rs::stock_bundles_index_and_our_files]
[data: round OR2, 2026-11-23: a Wake build through bc2_install::levelbuild against OPFOR main's Python build, every one of the 6,260 staged files identical]

### XINST-WW2-5: The staging and the shared patches keep their bundles' order

Status: **verified**

A staging folder's manifest lists its files by lower name with the level partition last (entries load in manifest
order); a shared bundle (startup, overlay, ondemand_awards) and ww2_common (nam_common's three bundles) are unpacked,
a file of an entry's name replaces it in place (its type, flags and, for ww2_common, the bundle's spelling kept) and
the others are added at the end, then packed. A file staged again from the same unchanged source is not written
again.

[code: rust/bc2-install/src/levelbuild/stage.rs:109-160] [code: rust/bc2-install/src/levelbuild/stage.rs:316-420]
[test: rust/bc2-install/tests/levelbuild.rs::staging_patches_and_common]

## 5. The mod manager (round MM1)

`bfbc2 mods` (`bc2_install::mods`) turns Project OPFOR's mods on and off one at a time on the install ledger of
section 0, without a rebuild: `list / status / enable / disable / install / uninstall / restore-original / verify /
baseline / migrate / resume`. Its own folder is `<game>\opfor\mods` (kept copies in `stash\` and `blobs\`, moved level
folders in `folders\`, the operation journal `.op.json`, a lock).

### XINST-MOD-1: A mod is the installs its tools name

Status: **verified**

Each ledger owner belongs to one mod by the name its tool gives it (`opforcore.stages.owner_of`): `wake` and
`minimap-edges` the Pacific War (`ww2`), `pacificmenu` its menus (`pacific-menus`), the console port's 14 stages and
the Napisal-level fixes on a level `onslaught:<level>` (their shared files `onslaught`), `mapedit` / `mapedit-ai:<lv>`
the map editor's exports, `navkit:<lv>` / `stockmp:<lv>` the suite's, `import` and `manifestfix` OPFOR's `base`; a
level folder holding an `opfor-<stage>-installed.json` beside the ledger is the Pacific War's (the map editor's for
`opfor-mapedit-installed.json`); `opfor.ini [patches] vietnam` is the `vietnam` setting, on when missing (the DLL's
default since 2026-11-22). A mod is enabled, disabled or partly so (some of it kept aside). `pacific-menus` needs `ww2`
and `vietnam`.

[code: rust/bc2-install/src/mods/catalog.rs:1-230]
[test: rust/bc2-install/src/mods/catalog.rs::owners_map_to_mods] [test: rust/bc2-install/src/mods/catalog.rs::groups_select_their_levels]
[test: rust/bc2-install/tests/mods.rs::the_list_names_every_mod]

### XINST-MOD-2: Disable keeps what each layer wrote; enable puts it back byte for byte, on what it was built on

Status: **verified**

Disable takes a mod's layers out through the ledger (each file's prior back) after hard-linking what each wrote into
the kept copy, with its write time; its level folders are moved aside (a rename: their write times, which the DLL's
markers compare, are kept). Enable moves the folders back first and installs the kept bytes last-out-first, each file's
write time set back, so a disable / enable round trip leaves every game file as it was, bytes and write time. A kept
layer goes back only on the file it was built on (its prior's sha256): otherwise it is refused in words naming what
to enable first, except a settings file (`.ini`), which gets the keys changed while the mod was out carried onto the
kept copy. An owner's own ini keys (`wake`'s `[dev] level` and `[gamemode] pacific_levels`, the WW2 install's
`ini_refresh` rule) go back on disable with every other edit made since kept.

[code: rust/bc2-install/src/mods/plan.rs:379-580] [code: rust/bc2-install/src/mods/plan.rs:662-820]
[code: rust/bc2-install/src/mods/mod.rs:691-890] [code: rust/bc2-gamefs/src/ledger_part.rs:29-150]
[test: rust/bc2-install/tests/mods.rs::disable_and_enable_ww2_round_trip_byte_for_byte]
[test: rust/bc2-install/tests/mods.rs::a_settings_edit_made_while_a_mod_is_out_is_carried_onto_it]
[test: rust/bc2-install/src/mods/ini.rs::rebase_carries_the_edits_made_since] [test: rust/bc2-install/src/mods/ini.rs::owned_keys_alone_go_back]
[data: round MM1 scratch proof, 2026-11-22: ww2, onslaught, pacific-menus, base and vietnam each disabled and enabled on a scratch copy of the author's game copy's 182 affected files: every game file identical to the copy before, bytes and write times (opfor.ini's write time new after the setting)]

### XINST-MOD-3: What is built on a mod goes first, with a confirmation; it comes back with it

Status: **verified**

Disabling a mod takes first every owner whose layer sits on one of its layers (built on its file) and every mod that
needs it; without `--yes` nothing is written and the command names them (exit 2). Enable brings back the mods disabled
in the same operation (unless `--only`); mods it needs that are off come with `--yes` only.

[code: rust/bc2-install/src/mods/plan.rs:267-313] [code: rust/bc2-install/src/mods/plan.rs:587-660]
[test: rust/bc2-install/tests/mods.rs::disabling_onslaught_takes_what_was_built_on_it_first]
[test: rust/bc2-cli/tests/mods.rs::the_command_lines_and_their_exit_codes]

### XINST-MOD-4: restore-original gives DICE's files back and keeps the installs

Status: **verified**

`restore-original <file> ...` takes every layer off the files given (each owner's other files kept: its mod is then
partly disabled); `--all` disables every mod but the settings. The files are then the bottom layer's prior, DICE's
original kept once by sha256 (none where an install created the file), and what came off is kept: `enable` puts each
back, refused when the layer under it is not back yet.

[code: rust/bc2-install/src/mods/plan.rs:825-912]
[test: rust/bc2-install/tests/mods.rs::restore_original_all_gives_dices_files_and_enable_gives_the_mods_back]
[test: rust/bc2-install/tests/mods.rs::restore_original_of_one_file_keeps_the_owners_other_files]
[data: round MM1 scratch proof: restore-original --all left each of the 22 ledger files with its original sha256 and no WW2 level folder; enable gave every file back, bytes and write times]

### XINST-MOD-5: A setting is set through every state the ledger keeps

Status: **verified**

Turning `vietnam` off or on sets the key in the file, in every layer's prior and value the ledger keeps of it (one
journalled commit, `Ledger::edit_through`) and in every kept copy, so no later revert or enable, in any order, takes
the setting back.

[code: rust/bc2-gamefs/src/ledger_part.rs:213-320] [code: rust/bc2-install/src/mods/mod.rs:891-945]
[test: rust/bc2-install/tests/mods.rs::vietnam_is_set_through_every_layer_and_kept_copy]

### XINST-MOD-6: An operation interrupted at any step is finished by the next command

Status: **verified**

Every operation is planned and checked first, the kept bytes linked, then written as one journal holding every step
with all its data before the first game write; each step can run again with the same result (each ledger step one
journalled ledger commit, XINST-GL-2). A killed process leaves the journal and a stale lock: the next `bfbc2 mods`
command (or `resume`) carries the operation to its end. `BC2_MODS_CRASH_AT=<n>` ends the process before its n-th write
step, its own and the ledger's.

[code: rust/bc2-install/src/mods/mod.rs:600-690]
[test: rust/bc2-install/tests/mods.rs::a_kill_at_every_step_of_a_disable_is_finished]
[test: rust/bc2-install/tests/mods.rs::a_kill_at_every_step_of_an_enable_is_finished]
[test: rust/bc2-install/tests/mods.rs::a_kill_at_every_step_of_restore_original_is_finished]
[test: rust/bc2-install/tests/mods.rs::a_kill_at_every_step_of_a_setting_is_finished]
[test: rust/bc2-cli/tests/mods.rs::a_real_kill_at_every_step_of_a_disable] [test: rust/bc2-cli/tests/mods.rs::a_real_kill_at_every_step_of_an_enable]

### XINST-MOD-7: Uninstall drops the kept copy; the WW2 opt-out keeps the manifest repair

Status: **verified**

`uninstall` is a disable (with its confirmation always) whose kept copies are dropped; the mods built on it stay
disabled and kept. Uninstalling `ww2` keeps the bundleManifest repair as owner `manifestfix`, as the opt-out does.

[code: rust/bc2-install/src/mods/plan.rs:210-265] [code: rust/bc2-install/src/mods/mod.rs:947-990]
[test: rust/bc2-install/tests/mods.rs::uninstall_drops_the_kept_copy_and_keeps_the_manifest_repair]

### XINST-MOD-8: verify checks every file the ledger holds, the level folders and the kept copies

Status: **verified**

Per ledger file: the top layer's write is the file, each layer's write the next one's prior, every prior (DICE's
original at the bottom) kept intact by its sha256; each installed level folder against its own install record; each
kept copy by its sha256; files still carrying markers from before the ledger; with a baseline (`bfbc2 mods baseline`)
every other game file. Exit 1 on any problem.

[code: rust/bc2-install/src/mods/verify.rs:52-280]
[test: rust/bc2-install/tests/mods.rs::verify_finds_a_changed_file_and_a_damaged_kept_copy]

### XINST-MOD-9: The migration in the suite is OPFOR's, and resumes from any interruption

Status: **verified**

`bfbc2 mods migrate` (the plan) and `--apply` are `opforcore\migrate.py` in Rust: the same stacks, refusals, notes and
lines. A run interrupted while archiving the markers resumes: the backups left may be fewer than the records already
written, so a file is refused only when a backup's owner holds no record (before MM1, a kill after the first backup
moved left the file refused for good, in OPFOR's Python too).

[code: rust/bc2-install/src/mods/migrate.rs:123-560]
[test: rust/bc2-install/tests/mods.rs::migrate_plans_applies_resumes_and_reverts_to_the_backups]
[test: rust/bc2-install/tests/mods.rs::a_kill_at_every_step_of_the_migration_is_finished]
[data: round MM1: two scratch copies of the author's 182 affected files, one migrated by OPFOR's migrate.py, one by bfbc2 mods migrate: the plan's 36 lines and the apply's lines identical, 288 files (game files, records less their id and time, blobs, archived markers) identical with their write times]

## Open questions

- Since round DF2 every whole-file install is on the ledger; the game copies with old markers are migrated by
  `opforcore\migrate.py` (XINST-OPF-3). The slot contributions (XINST-LED-1) were never written by a shipped install; moving
  the registration writes (package.mft blocks, bundleManifest lines, ini keys) onto slots is IN1's.
- The installer is in the suite: the WW2 install's game-copy side is `bc2-install` since round OR1 (section 4), the
  map export's install since round OR6 (section 2). Round OR15 added IN1's doctor (`bfbc2 doctor`), the game-folder
  setting (`bfbc2 settings game-folder`, which `bfbc2 install mapedit` and the editor's Install use) and anonymised
  sharing (`bfbc2 project share`); left of IN1: the bug-report bundle, the editor's Help > Diagnostics, a small
  "install this project" tool for players, the old export stages pruned.
- One implementation of the ledger since round OR15 (audit AU5 ARC-3 closed, XINST-LED-2); the slot kinds
  (XINST-LED-1) are kept and forgotten but never taken out: a revert of one would need OPFOR's slot texts and bundle
  entry edits in Rust, for records no shipped install wrote.
