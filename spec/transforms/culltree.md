# Cull tree additions: `culltree add` and the export's reslot

Two transformations give placed objects new leaves in a level's exclusive static cull tree (`formats/culltree.md`):

- **`culltree add`** (`bfbc2.exe culltree`, its library `bfbc2-culltree/culltreelib.py`, its core in Rust `bc2_levelwrite::culltool`): input a cull tree
  partition, its level partition, the objects to add (named, or `--auto`: every slot-0 object) and optionally
  reference levels (`--ref`, `--ref-dir`) to take shapes from; output the tree and level written back, and a report.
  Used by OPFOR's console conversion (`console_cull`) and the WW2 install.
- **`reslot`** (`bc2_levelwrite::culltree::reslot`, step 5 of the map export): input the base level, the edited
  level and the exclusive tree; every moved base object, and every object the base does not have, gets new leaves
  of its own blueprint's shape.

Both keep the invariants of CULL-ADD-1; this file states their inputs, refusals and results.

## 1. `culltree add`

### XCULL-ADD-1: Only slot-0 objects, only with a known shape

Status: **verified**

`add` refuses an object that already has a slot (`Enumeration` not 0), and an object whose blueprint has no slotted
instance in the tree's own level or in the reference levels to take its shape (its leaves' spheres relative to the
object) from; it names the object and the fix (add a `--ref` level that has one). A shape resting on fewer than three
instances with no other level agreeing is used and reported as a warning.

[code: rust/bc2-levelwrite/src/culltool.rs:2188-2249] [test: bfbc2-culltree/tests/test_culltree.py::main]

### XCULL-ADD-2: Result: the tree keeps its invariants, the level changes only in slots

Status: **verified**

After `add`, the tree holds the objects' new leaves at the end of the numbering, each object's `Enumeration` is
`EnumOffset` + its first new leaf, its leaves are its shape moved and turned with its transform, every ancestor's
sphere contains them and its `Right` and `Max` are updated, the 256 spare nodes follow, and the level partition
differs from the input only in those `Enumeration` values (CULL-ADD-1).

[code: rust/bc2-levelwrite/src/culltool.rs:2070-2186] [test: bfbc2-culltree/tests/test_culltree.py::main] [test: bfbc2-culltree/tests/test_culltree_native.py::built_add]

### XCULL-ADD-3: The Rust core gives the retired Python tool's files

Status: **verified**

The tree, level and report `add` writes are those of the retired Python `culltree.py` (recorded before deletion):
on built trees and levels, and on OPFOR's Onslaught MP_005 with every `work\levels` folder as reference (console
conversion's run); with the reference-row cache cold, warm, and absent, and a reference level changed after the
cache was made read again.

[test: bfbc2-culltree/tests/test_culltree_native.py::run_add] [test: bfbc2-culltree/tests/test_culltree_e3m.py::three_ways]
[test: bfbc2-culltree/tests/test_culltree_e3m.py::quick]

### XCULL-RD-1: Reading trees and levels: whole or streamed, the same answer

Status: **verified**

The tree and level readers answer the same whether the partition is read whole or streamed, on every cull tree and
level partition of the game copy and OPFOR's work folders; a damaged tree is an error (a `CullTreeError`), never a
wrong tree; an unchanged tree or level is written back byte for byte.

[test: rust/bc2-levelwrite/tests/culltool_ct1.rs::streamed_readers_answer_as_the_whole_document] [test: bfbc2-culltree/tests/test_culltree_native.py::run_corpus]

## 2. Sphere keys

### XCULL-KEY-1: Encoding always contains, or refuses

Status: **verified**

A sphere written as keys (centre x 16, radius x 4, CULL-SPH-1) is rounded so that the decoded sphere contains the
sphere given and every sphere it must enclose; a sphere whose keys would leave the 16-bit range is refused, not
clipped.

[code: rust/bc2-levelwrite/src/culltree.rs:78-104] [test: rust/bc2-levelwrite/src/culltree.rs::keys_round_trip_and_contain]

## 3. The export's reslot

### XCULL-RES-1: Who gets new leaves, and whose shape

Status: **verified**

`reslot` gives new leaves to every placed object of the edited level that is either a base object in its own slot
with a changed transform (moved or turned: its own leaves in the base, at most two, carried along), or any other
object but DICE's own unslotted ones: a duplicate, a placed object (made from a template that may be another
blueprint's placement), a mode layer's object from another level (whose `Enumeration` is that level's). Such an
object takes the shape of its own blueprint (`ReferencedObject`'s partition): its first slotted placement in the
base, else in another level (the mode's donor, the levels whose own bundle holds the blueprint, then every level,
DICE's first), each centre taken into that placement's space and out into the new place, radii and `Max` kept. With
no shape anywhere the object gets `Enumeration` 0, never a slot that is not its own. Unmoved base objects are left
alone; with nothing to do the tree is not rewritten. Old leaves stay unregistered (CULL-ADD-1). Before round XF1
the shape was the slot owner's: Aurora's placed radar bunker took a map prop's one leaf (Guadalcanal's bunker has
two), Guadalcanal as Rush gave 20 grafted M-COMs the shapes of the base objects that owned Arica Harbor's slot
numbers.

[code: rust/bc2-levelwrite/src/culltree.rs:543-593] [test: rust/bc2-levelwrite/src/culltree.rs::add_keeps_one_preorder_tree]
[test: rust/bc2-levelwrite/tests/export_sp01.rs::sp_01_exports_as_a_new_conquest_level]

### XCULL-RES-3: The check holds each new shape to its blueprint

Status: **verified**

`mapexport check` reads each re-slotted object's recorded shape source again, refuses one whose source is another
blueprint, and compares the object's leaves in the staged tree with the source's shape placed where the object
stands (centres within 0.5 m, radii within 0.5 m plus 5 %, the keys' rounding: CULL-SPH-1). An object the base does
not have may stand at slot 0 only when the export recorded that no level slots its blueprint and the base does not.

[code: rust/bc2-levelwrite/src/export.rs:2998-3382]
[test: rust/bc2-levelwrite/tests/export_xf1.rs::a_cross_set_object_gets_its_entries_and_shape]

### XCULL-RES-2: The stage's cull check

Status: **verified**

A staged export passes its cull check only when the exclusive tree reads as one preorder tree and no two placed
objects share a slot and every slot names a leaf of the tree (CULL-SLOT-3). On the first Aurora export the two moved
objects got slots 8,248 and 8,250 (leaves 4,612 -> 4,615) and no slot was shared.

[code: rust/bc2-levelwrite/src/export.rs:2608-2654] [test: rust/bc2-levelwrite/tests/export_sp01.rs::sp_01_exports_as_a_new_conquest_level]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1198]

## Open questions

- An object whose blueprint has more than two leaves anywhere: `reslot` caps a shape at two leaves as the README's
  "Leaves per object" observes; a blueprint that needs more has not been met.
- Shapes for blueprints no level slots (new meshes): S3 in the editor plan; neither transformation can add them (an
  export leaves such an object at slot 0 and records it).
- `reslot` takes a blueprint's first slotted placement; `culltree add` takes a consensus of all of them (tolerance
  0.5 m). For a blueprint whose placements disagree the two can differ.
