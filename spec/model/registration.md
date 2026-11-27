# Registration: `package.mft`, `bundleManifest` and the level checksum

The game learns which bundles exist and which bundles each bundle loads from two text files in
`game\Dist\win32\`: `package.mft` (the bundle registry: each source bundle and its stack of targets) and
`bundleManifest` (a flat list of bundles). A level the game can load is checksummed at start-up from what these
files register; a new level that is registered wrongly crashes that checksum. This file states the two formats,
what a new level must add, and what is known of the checksum.

**Corpus.** "Stock" is DICE's files as OPFOR backed them up before its first change (`package.mft.opfor-orig`,
`bundleManifest.opfor-orig`); "current" is OPFOR's game copy on 2026-11-19 (stock plus Onslaught, Vietnam, the
Pacific War and map-editor installs). Structure and counts only.

## 1. package.mft

### REG-MFT-1: File form

Status: **verified**

`package.mft` is ASCII text with CRLF line ends and no byte-order mark. It starts with a head block, the line
`Signature <40 hexadecimal digits>`, the line `Authorative` (no value) and a blank line; then the bundle blocks,
each preceded by a blank line.

[test: core/tests/test_spec_probes.py::registries]
[data: stock: 136,251 bytes, 3,493 lines all CRLF, no BOM; line 1 Signature with a 40-digit hex value, line 2 Authorative, 900 blank lines, 899 BundleSource lines each after a blank line, 1,692 BundleTarget lines; current: 960 blocks, 1,817 targets, same form]

### REG-MFT-2: A block

Status: **verified**

A block is one line `BundleSource <ref>` followed by one or more lines `BundleTarget <ref>`; a blank line ends it.
Each `<ref>` is a registry name (BNDL-LOC-2). A tool parses a block from its `BundleSource` line to the next blank
line and leaves the head block alone; a block with an empty or whitespace-only line inside is malformed.

[code: rust/bc2-editor/src/package.rs:80-100] [code: rust/bc2-levelwrite/src/export.rs:326-339]
[test: rust/bc2-editor/src/package.rs::registry_and_paths] [test: rust/bc2-levelwrite/src/export.rs::root_files_and_names]
[test: opfor:src/tools/opforcore/tests/test_slots.py::test_mft_block_with_blank_line_refused]

### REG-MFT-3: Sources are `game:` names, unique

Status: **verified**

Every `BundleSource` is a `game:` name, also for a bundle that exists only under `game\Package` (the 41
Package-only levels). No source appears twice. The source names the bundle being loaded; the targets say where
its contents come from.

[test: core/tests/test_spec_probes.py::registries]
[data: stock: 899 of 899 sources game:, 0 duplicates; of the level, terrain, ingame and loader sources 33 x 4 have files in Dist and Package, 41 x 4 in Package only]
[code: rust/bc2-editor/src/package.rs:106-114]

### REG-MFT-4: Targets are the bundle's stack

Status: **implemented**

A block's targets, in order, are the bundles the source bundle loads (PREC-ORD-2): for a level kind its own bundle
(`package:` then `game:` when both exist), then, for a mode layout, its base level's bundles, then the common
bundles of the kind (`mp_common`, `sp_common`, `mp1_common`, `mp2_common`, `nam_common`, OPFOR's `ww2_common`). A
block whose targets are only common bundles describes a bundle with no own copy (a voice-over source).

[code: rust/bc2-levelwrite/src/export.rs:288-297] [code: rust/bc2-levelwrite/src/export.rs:563-601]
[test: rust/bc2-editor/src/package.rs::registry_and_paths]
[data: stock: blocks of 1 target 328, 2 targets 349, 3 targets 222; level-kind blocks with an own target: package only 164, package then game 132; most frequent common targets mp_common level, ingame and loader 61 each]

### REG-MFT-5: What is registered

Status: **verified**

Stock registers, per level (74), `level`, `terrain`, `ingame`, `loader` and seven `vo/<lang>` bundles; 50 `async`
bundles; 25 `menu` bundles; `vo-00`, `build_overlay-00`, `streaming_sounds-00` and seven `streaming_vo_<lang>-00`.

[test: core/tests/test_spec_probes.py::registries]
[data: stock source kinds: level, terrain, ingame, loader 74 each; vo/lang 518; async 50; menu 25; 10 root bundles]

### REG-MFT-6: The Signature is not checked against the content

Status: **observed**

The `Signature` line is not a checksum the game verifies against the blocks: OPFOR's current file has 61 more
blocks than stock and the stock `Signature` line unchanged, and the game starts and loads its levels. Tools leave
the head block as it is.

[data: current package.mft: 960 blocks against stock 899, Signature line identical to stock; OPFOR's levels load in game]
[code: opfor:src/tools/opforcore/slots.py:94-97]

## 2. bundleManifest

### REG-BM-1: Line form

Status: **verified**

`bundleManifest` is ASCII text with LF line ends and no byte-order mark: one line per bundle,
`<path>\t<GUID>\t<n>`, where `<path>` is the bundle's path under `game\Dist\win32\` without `-00.fbrb`
(`levels/mp_005/level`, `levels/mp_005/vo/en`, `async/...`), `<GUID>` is `00000000-0000-0000-0000-000000000000` and
`<n>` is `1`. A line missing its third field is a defect (an earlier Onslaught stage wrote one; OPFOR's Wake install
repairs it).

[data: stock: 835 lines, 3 tab-separated fields on all 835, the zero GUID on 835, third field 1 on 835, no CR, no BOM]
[code: rust/bc2-levelwrite/src/export.rs:603-613] [test: rust/bc2-levelwrite/src/export.rs::root_files_and_names]
[code: opfor:src/tools/ww2/wake_install.py:2422-2425]

### REG-BM-2: What stock lists

Status: **verified**

Stock `bundleManifest` lists the Dist install's bundles: the 33 Dist levels' `level`, `terrain`, `ingame`, `loader`
and seven `vo/<lang>` each, 443 `async` bundles (weapons and the campaign's cutscenes among them), 17 `menu`
bundles, and a few root bundles. It lists **no** Package-only level, and 237 of its lines name bundles not in the
game copy (the voice-over bundles). The lines are in no sorted order.

[test: core/tests/test_spec_probes.py::registries]
[data: stock: levels 33, all with Dist bundles, 0 Package-only; 598 lines with a Dist file, 237 without; 397 of 835 lines are also package.mft sources]

### REG-BM-3: The suite adds a line per bundle of a new level

Status: **verified**

For a new level the suite adds one line per bundle it ships, `levels/<lv>/<kind>\t<zero GUID>\t1`, for the kinds
`level`, `terrain`, `ingame` and `loader`, with `<lv>` lower case.

[code: rust/bc2-levelwrite/src/export.rs:603-613] [code: rust/bc2-levelwrite/src/export.rs:1536-1553]
[test: rust/bc2-levelwrite/src/export.rs::root_files_and_names]
[test: rust/bc2-levelwrite/tests/export_sp01.rs::sp_01_exports_as_a_new_conquest_level]

## 3. Registering a new level

### REG-NEW-1: One block and one manifest line per bundle

Status: **verified**

A new level `<lv>` (under `game\Package\levels\<lv>\`) is registered with, for each bundle it ships (`level`,
`terrain`, `ingame`, `loader`):

- a `package.mft` block `BundleSource game:levels/<lv>/<kind>-00.fbrb` whose first target is
  `package:levels/<lv>/<kind>-00.fbrb`; for `level` and `terrain` then the base level's own bundles that exist, then
  the content profile's common bundles of the kind, and for `level` the base's other targets after those; for
  `ingame` and `loader` the profile's common bundles;
- a `bundleManifest` line (REG-BM-3).

Every target other than the level's own bundles must exist in the game copy. The export's check refuses a stage
whose registration misses a kind, names one twice, or targets a missing bundle.

[code: rust/bc2-levelwrite/src/export.rs:563-601] [code: rust/bc2-levelwrite/src/export.rs:1536-1571]
[test: rust/bc2-levelwrite/tests/export_sp01.rs::sp_01_exports_as_a_new_conquest_level]
[test: rust/bc2-levelwrite/tests/export_e3b.rs::a_vietnam_export_with_its_own_name_and_picture]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:843-851]

### REG-NEW-2: A new name only

Status: **implemented**

A new level's name must not be a level of the game copy (no `levels/<lv>/level-00.fbrb` in Dist or Package) and
must not already be a `package.mft` source of another tool's level. Its id is `Levels/<LV>` with letters, digits and
`_`.

[code: rust/bc2-levelwrite/src/export.rs:649-660] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:835-838]

### REG-NEW-3: Voice-over: the donor's blocks, no bundle of its own

Status: **verified**

A new level ships none of the four kinds' `vo/<lang>` bundles of its own making (`export::KINDS`: level, terrain,
ingame, loader). Since round E3c it registers its donor's voice-over blocks under its own name: as on every stock
level, their sources are absent and their targets are the set's common voice-over bundles, which are there
(SND-VO-3); a WW2 donor's own `vo/en-00` bundle is staged byte for byte and installed with the level. (Corrected in
round SP2: the rule first said a new level registers no voice-over block at all, which was E3's behaviour.)

[code: rust/bc2-levelwrite/src/export.rs:40-44] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:855-858]
[test: rust/bc2-levelwrite/tests/export_e3c.rs::a_ww2_export_takes_its_donors_announcer]

### REG-NEW-4: Registration is written last and removed first

Status: **implemented**

An install copies the level's bundles in, patches the shared bundles (startup's description and level list, the
menus), and writes `package.mft` and `bundleManifest` **last**, so that at no point a level is registered whose
bundles are not there; a revert takes the registration out first.

[code: rust/bc2-install/src/mapedit.rs:976-993] [code: rust/bc2-install/src/run.rs:718-731] [code: rust/bc2-install/src/run.rs:419-455]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:900-916]

### REG-NEW-5: The menus need the startup bundle too

Status: **observed**

Registration makes a level loadable, not visible: the level also needs its description partition and two
`LevelListQuery` items in `async/startup-00.fbrb` (LEVEL-DESC-1), and OPFOR's `maps.ini` section for the launcher.

[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:823-827] [code: rust/bc2-levelwrite/src/export.rs:49-50]
[data: OPFOR's SP_005 and Onslaught installs: a level without the description crashed in state 13; without the list items it was in no menu]

## 4. The level checksum

### CSUM-CHK-1: The server checksums every registered level

Status: **observed**

At start-up the server computes a checksum over every level it can load from the two registry files, and client and
server must agree. A new level under a new name that is not registered in both `package.mft` and `bundleManifest`,
on both the server and the client, crashes the server while it computes the level checksum.

[data: the user's report of 2026-11-06 (Wake Island, a new level under a new name); recorded in OPFOR's WW2 port plan, phase F]
[doc: opfor:research/WW2_PORT_PLAN.md:49-55]

### CSUM-CHK-2: A registered bundle that is missing crashes

Status: **observed**

A new level registered while its bundles are not in the game copy crashes the start-up checksum. This is why the
registration is written last (REG-NEW-4) and why a new level registers voice-over blocks only as stock levels do,
with targets that are there (REG-NEW-3).

[data: OPFOR's install audit C2: a failed install that left the level registered with absent bundles is the start-up checksum crash]
[doc: opfor:research/PYTHON_CODE_AUDIT.md:30] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1123]

### CSUM-CHK-3: The checksum dereferences every placement's blueprint

Status: **observed**

The checksum walks the level's placements and reads each one's blueprint; a placement whose blueprint was not
created (its references resolve nowhere, REF-RES-5) is a null read at 0x6EFCA4.

[exe: BFBC2Game.exe 0x6EFCA4] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1124]

### CSUM-CHK-4: Client and server need the same registration

Status: **inferred**

The client must register the same level bundles as the server.

[inferred: the checksum exists to compare the server's level data with the client's; a level the server registers and the client does not cannot match, and the user's ruling (CSUM-CHK-1) requires both sides]

## Open questions

- **What the checksum covers.** Which bundles, entries or partitions feed it, and where it is computed in
  `BFBC2Game.exe` (only the crash address 0x6EFCA4 and the symptoms are known).
- **bundleManifest's role.** Stock lists no Package-only level (41 of them) and those levels load, so a Package level
  may not need a `bundleManifest` line at all; the suite adds one per the 2026-11-06 ruling (CSUM-CHK-1). Which of the
  two files the checksum crash actually follows is not isolated.
- **Missing registered bundles.** Stock registers 518 voice-over bundles (and 237 manifest lines) that are absent,
  and the game runs; the crash for a new level registered with absent bundles (CSUM-CHK-2) is therefore conditional on
  something not yet known (the bundle kind, the level being new, or the server's checksum set).
- **The Signature.** What the 40-digit value is (its length is a SHA-1's) and whether anything checks it.
- **`Authorative`, the GUID and the `1`** of `bundleManifest` lines: their meaning (every stock line has the zero GUID
  and 1).
- Whether the server reads its own copy of the registry (a dedicated server's) in the same way.
