# Instances, GUIDs and references

A partition (`model/bundles.md`) holds instances; each instance has a GUID; members point at instances by
reference, either in the same partition or in another one by its path. This file states the identity rules, the
reference forms and how a reference resolves, and what the game does with one that does not resolve.

**Corpus.** "The level corpus" is the 41 `level-00.fbrb` bundles of OPFOR's `game\Dist\win32\levels` (79,526
binary partitions, 52,213 distinct partition names, 2,313,497 instances; `model/types.md`), read 2026-11-19.

## 1. Instances and GUIDs

### INST-GUID-1: GUID text form

Status: **observed**

An instance's `guid` attribute, and the partition's `guid` and `primaryInstance` attributes, are GUIDs written as
36 characters, `8-4-4-4-12` hexadecimal digits with hyphens, **upper case**.

[data: level corpus: 2,313,497 instance and 79,526 partition guid attributes all 36-character GUIDs, all upper case; primaryInstance upper case on the 25,565 partitions of the mp_0* bundles checked]

### INST-GUID-2: Unique within a partition

Status: **observed**

No two instances of one partition have the same GUID (compared without case).

[data: level corpus: duplicate instance GUIDs in 0 of 79,526 partitions]

### INST-GUID-3: Not unique across partitions

Status: **observed**

An instance GUID is not a global identity: the same GUID occurs in different partitions (copies of a partition
under other names, mode layouts made from a base level, assets duplicated per level). An instance is identified by
(partition name, GUID). Partition GUIDs are not unique either: 52,213 distinct partitions carry 26,890 distinct
partition GUIDs.

[data: level corpus, one copy per partition name: 439,450 distinct instance GUIDs, 79,802 of them in more than one partition name; 1,567 partition GUIDs shared by partitions of different names]

### INST-GUID-4: GUIDs compare without case

Status: **implemented**

Every comparison of GUIDs ignores case: instance GUIDs are stored upper case and local references lower case
(REF-FORM-2), so a case-sensitive comparison finds nothing.

[code: rust/bc2-formats/src/dbx/model.rs:185-190] [code: rust/bc2-formats/src/dbxedit/editor.rs:51-53]
[code: rust/bc2-levelwrite/src/export.rs:1403-1408] [test: rust/bc2-formats/src/dbx/model.rs::lookups]

### INST-GUID-5: New GUIDs the suite makes

Status: **verified**

The suite gives new instances and partitions name-based UUID version 5 GUIDs (SHA-1 over the URL namespace and
`<namespace>:<part>:<part>...`, version nibble 5, variant bits 10), written upper case, so a rebuild makes the same
GUIDs. A level export renames every instance GUID this way and re-points its local references (lower case) and
`primaryInstance` (upper case).

[code: rust/bc2-formats/src/guid.rs:17-41] [code: rust/bc2-levelwrite/src/export.rs:499-525]
[test: rust/bc2-formats/src/guid.rs::python_uuid5]

## 2. The primary instance

### INST-PRIM-1: Every partition names its primary instance

Status: **observed**

The partition's `primaryInstance` attribute names one of its instances: the asset the partition stands for (a
level's `LevelData`, a blueprint's object data). It is not necessarily the first instance.

[data: level corpus: primaryInstance names an instance of the file in 79,524 of 79,526 partitions; it is the first instance in 30,958, a later one in 48,566]

### INST-PRIM-2: Two partitions name no instance

Status: **observed**

Two partitions of the level corpus have a `primaryInstance` that is none of their instances (each has 2
instances). The game ships them; nothing is known to read their primary instance.

[data: level corpus: 2 partitions, primaryInstance unmatched, no id attribute in either]

### INST-PRIM-3: The id attribute marks the primary instance

Status: **observed**

Exactly the primary instance carries a third attribute, `id`, whose value is the partition's file name without the
folder and extension (`Levels/MP_005.dbx` -> `MP_005`), in the entry name's case or another case.

[data: level corpus: id on 79,524 instances, 1 per partition, always the primary instance; equal to the entry's stem in 33,062, equal ignoring case in 46,462, other 0]

## 3. Reference forms

### REF-FORM-1: References are `ref` attributes

Status: **implemented**

A reference is an element (a `field` or an `item`) with a `ref` attribute. The element is text with empty text;
the attribute's value is the target. Tools find references by the attribute name `ref`, wherever it is.

[code: rust/bc2-levelwrite/src/export.rs:1389-1401] [code: rust/bc2-formats/src/dbxedit/check.rs:310-316]
[data: level corpus: 5,907,174 field and 2,015,781 item elements with ref, all kind 2 with empty text]

### REF-FORM-2: Three forms

Status: **observed**

A `ref` value is one of:

| Form | Value | Meaning |
|---|---|---|
| null | `null` | no target |
| local | a 36-character GUID, **lower case** | an instance of the same partition |
| external | `<partition path>/<guid>` | an instance of another partition; the path **lower case**, the GUID lower case |

No other form occurs (no empty `ref`).

[data: level corpus: 3,836,345 local (all lower case), 3,286,524 null, 800,086 external (all lower-case paths, each ending in a 36-character GUID), 0 other]

### REF-FORM-3: Telling local from external

Status: **implemented**

A value is local when it is 36 characters long and has no `/`; external when it has a `/` after its first
character. The partition path is everything before the **last** `/`, the GUID everything after it (paths have
slashes of their own: `objects/vehicles/m1a2/m1a2/<guid>`).

[code: rust/bc2-formats/src/dbxedit/check.rs:315] [code: rust/bc2-formats/src/dbxedit/check.rs:345-347]
[code: rust/bc2-levelwrite/src/export.rs:1591-1603] [test: core/tests/test_dbxedit.py::cases]

### REF-FORM-4: The partition path is the entry name

Status: **implemented**

An external reference's partition path is the bundle entry name of the target partition without `.dbx`, compared
without case: `levels/mp_005/foo/<guid>` names the entry `Levels/MP_005/Foo.dbx`. There is no other partition
name; a partition does not name itself (TYPE-ATTR-1).

[code: rust/bc2-levelwrite/src/export.rs:1610-1615] [code: rust/bc2-scene/src/scene.rs:674-710]
[test: rust/bc2-levelwrite/tests/export_sp01.rs::sp_01_exports_as_a_new_conquest_level]

## 4. Resolution

### REF-RES-1: Local references resolve in the partition

Status: **verified**

A local reference names an instance of its own partition. The game reads a local reference without checking it
(LEVEL-OBJ-2); a dangling one is a defect.

[code: rust/bc2-formats/src/dbxedit/check.rs:333-343] [test: core/tests/test_dbxedit.py::cases]

### REF-RES-2: External references resolve through the bundle stack

Status: **verified**

An external reference resolves to the instance with that GUID in the partition of that path, found in the bundles
the referring level loads, in their precedence order (PREC-ORD): the first bundle with a live entry of that name
decides, and a `*deleted*` marker first hides the name. The suite's export check proves this for every reference of
a new level (8,578 references into 777 partitions on SP_01).

[code: rust/bc2-levelwrite/src/export.rs:1572-1631] [code: rust/bc2-levelwrite/src/export.rs:254-273]
[test: rust/bc2-levelwrite/tests/export_sp01.rs::sp_01_exports_as_a_new_conquest_level]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:892-896]

### REF-RES-3: Stock data has dangling external references

Status: **observed**

DICE's own partitions contain external references to partitions that no bundle of the game has. They load
(nothing dereferences them on those paths). The suite allows such a reference in a derived partition only when
the stock source partition has the same reference.

[data: level corpus: 89,974 external references name a partition path outside the 41 level bundles (they may resolve in ingame, loader, startup or async bundles, or nowhere); SP_01 export: 2 partitions no bundle of the game copy has, both named by DICE's own source]
[code: rust/bc2-levelwrite/src/export.rs:1631-1671] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:894-896]

### REF-RES-4: Two-step blueprint references

Status: **implemented**

A placed object (`ReferenceObjectData`) names its blueprint by `ReferencedObject`, an external reference to the
blueprint partition's primary object; that object's `Object` field names the object data, locally (same
blueprint partition) or externally. The blueprint name is the referenced partition path; group members strip a
trailing `_havok` from theirs.

[code: rust/bc2-scene/src/scene.rs:674-725] [code: rust/bc2-scene/src/scene.rs:559]
[test: rust/bc2-scene/tests/scene.rs::loads_a_level]

### REF-RES-5: A blueprint that resolves nowhere crashes the checksum

Status: **observed**

When a placed object's blueprint partition is in a loaded bundle but its own references resolve nowhere, the game
does not create it, and the placement reads a null blueprint during the level checksum: crash at 0x6EFCA4 (Wake
Island's first load).

[exe: BFBC2Game.exe 0x6EFCA4, null blueprint read in the level checksum]
[code: opfor:src/tools/ww2/wake_install.py:1276-1280] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:93-94]

## 5. Removing a target

### REF-DEL-1: DbxEdit's drop

Status: **verified**

Removing instance G from a partition (DbxEdit `drop=G`, the editor's delete) walks the tree last to first: the
instance with GUID G is removed; an `item` whose `ref` is G or ends in `/G` (any case) is removed from its array;
any other element with such a `ref` gets `null` in every `ref` attribute it has. Before that, the level writer
removes every event connection whose `Source` or `Target` names G (LEVEL-EVT-3).

[code: rust/bc2-formats/src/dbxedit/editor.rs:51-86] [code: rust/bc2-levelwrite/src/write.rs:225-244]
[test: rust/bc2-levelwrite/src/write.rs::move_delete_duplicate_set] [test: core/tests/test_dbxedit.py::cases]
[doc: bfbc2-dbx-tools/FORMAT.md:95-98]

### REF-DEL-2: A null in a list the game reads unchecked crashes

Status: **verified**

A `null` item, or a local item naming no instance, in `LevelData.Objects` crashes the game (LEVEL-OBJ-2); a null
`PhysicsData` or `MeshEntityType` on a model group, group member, forest or tree member crashes the model-group
set-up (LEVEL-MG-3). This is why removing a target removes array items rather than nulling them.

[code: rust/bc2-formats/src/dbxedit/check.rs:317-331] [test: core/tests/test_dbxedit.py::cases]
[exe: BFBC2Game.exe 0x006EFBC0 reads Objects, faults at 0x006EFF8A; 0x00AF7144 model-group set-up]

## Open questions

- How the engine resolves an external reference whose partition is in a bundle the level does not load but another
  loaded level or the startup / async bundles have (the export check counts the startup bundles; the full rule is
  not traced in the executable).
- Whether the engine compares partition paths and GUIDs without case itself, or relies on DICE's lower-case
  convention (the suite always compares without case).
- What the two partitions without a matching primary instance are for.
- Whether `primaryInstance` is used by the loader for anything but asset lookup by name.
