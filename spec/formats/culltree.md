# Static cull trees (`World.StaticCullTreeData`)

A level's static cull trees are bounding-sphere hierarchies over its static objects, which the game walks to find
what to draw and what a query touches. They are not a file format of their own: each tree is a DBX partition
(`dbx.md`) holding one `World.StaticCullTreeData` instance, and the objects point into it through their
`Enumeration` field (their **cull slot**). This file specifies the tree as data: the fields, the preorder layout, the
sphere quantization, the slot numbering and what must stay consistent when objects are added.

Readers and writers: `bfbc2-culltree/culltree.py` and its Rust model `bc2_levelwrite::culltool` (`bfbc2.exe
culltree`: `info`, `verify`, `add`), the map editor's export `bc2_levelwrite::culltree` (new leaves for moved and
copied objects), and the element reader `bc2_formats::culltree::elements`. `DbxEdit check` warns about shared slots and
`fixslots` copies slots from another layout.

**Corpus.** "Stock trees" are the 37 tree pairs (client and exclusive) of DICE's levels in the game copy, multiplayer
and campaign, counted by `culltree.py verify` and a corpus script (`bfbc2-culltree/README.md`); the test corpus is
1,269 trees and level partitions of the game copy's level bundles and OPFOR's work folders
(`bfbc2-culltree/tests/culltree_golden.json`).

**Variants.** The DBX encoding is the same on every platform, but a tree is built against its platform's blueprints:
a console exclusive tree installed whole on the PC crashed the traversal at load. Trees are per platform (CULL-PLAT-1).

## 1. Where the trees are

### CULL-LVL-1: A level's two trees

Status: **observed**

`LevelData` refers to two cull-tree partitions: `ExclusiveStaticCullTree` (`levels/<level>/staticculltree_exclusive`)
and `ClientStaticCullTree` (`levels/<level>/staticculltree_client`). A third field, `ClientTreeStaticCullTree`, is null
in every stock level. Each partition holds one `World.StaticCullTreeData` instance.

[data: 37 of 37 stock levels have both partitions and a null ClientTreeStaticCullTree]
[doc: bfbc2-culltree/README.md:53-58]

### CULL-FLD-1: Fields

Status: **verified**

`World.StaticCullTreeData` has two fields:

| Field | DBX value | Runtime node offset |
|---|---|---|
| `Nodes[]`, each with `Right` | 4-byte number, int32 | +0 |
| `Max` | 4-byte number, float32 | +4 |
| `SphereKeyX`, `SphereKeyY`, `SphereKeyZ`, `SphereKeyW` | 2-byte numbers, int16 (DbxEdit's XML prints them unsigned) | +0x8, +0xA, +0xC, +0xE |
| `EnumOffset` | 4-byte number, uint32 | (the tree's) |

A node is 16 bytes at run time (the offsets are the type's field table in `BFBC2Game.exe`). DBX numbers are
big-endian (`dbx.md`).

[code: rust/bc2-levelwrite/src/culltree.rs:164-201] [code: rust/bc2-levelwrite/src/culltree.rs:454-490]
[test: bfbc2-culltree/tests/test_culltree.py::main] [test: rust/bc2-levelwrite/src/culltree.rs::add_keeps_one_preorder_tree]
[exe: the type's field table: Right +0, Max +4, SphereKeyX..W +8..+0xE]
[doc: bfbc2-culltree/README.md:60-66]

## 2. The node array

### CULL-NODE-1: Preorder binary tree

Status: **verified**

`Nodes` is a binary tree in preorder. Node 0 is the root. An **internal** node *i* has its left child at *i* + 1 and its
right child at *i* + `Right`, with `Right` = 1 + the size of the left subtree (so `Right` >= 2). A **leaf** has `Right`
= `0x80000000 | leaf index` (negative as int32). The `Right` offsets must describe exactly one tree over the used
nodes: no loop, no node reached twice, none left over.

[code: rust/bc2-levelwrite/src/culltree.rs:205-260] [code: rust/bc2-levelwrite/src/culltool.rs:2338-2347]
[test: rust/bc2-levelwrite/src/culltree.rs::add_keeps_one_preorder_tree] [test: bfbc2-culltree/tests/test_culltree.py::main]
[data: 37 of 37 stock tree pairs]

### CULL-NODE-2: Leaf numbering

Status: **verified**

The leaf indices run 1 .. L, each exactly once, and the used nodes number 2 x L - 1 (every internal node has two
children).

[code: rust/bc2-levelwrite/src/culltool.rs:2355-2366] [code: rust/bc2-levelwrite/src/culltool.rs:896]
[test: bfbc2-culltree/tests/test_culltree.py::main] [data: 37 of 37 stock tree pairs]

### CULL-NODE-3: Spare nodes

Status: **verified**

After the used nodes come **256 all-zero nodes** (`Right` 0, `Max` 0, keys 0). A reader finds the used nodes by
dropping trailing all-zero nodes; a writer appends 256 again.

[code: rust/bc2-levelwrite/src/culltree.rs:30] [code: rust/bc2-levelwrite/src/culltree.rs:196-201] [code: rust/bc2-levelwrite/src/culltree.rs:462]
[test: bfbc2-culltree/tests/test_culltree.py::main] [data: 256 spare nodes in every stock tree]

### CULL-NODE-4: Max

Status: **verified**

A leaf's `Max` is its object's unquantized bounding radius (a float, the same for every instance of a blueprint); an
internal node's `Max` is the largest `Max` of its subtree, which equals the larger of its two children's.

[code: rust/bc2-levelwrite/src/culltool.rs:2367-2385] [test: bfbc2-culltree/tests/test_culltree.py::main]
[data: 263,653 internal nodes of the stock trees, no exception]

### CULL-NODE-5: Depth

Status: **implemented**

The traversal keeps an explicit stack of 128 node pointers, so no path may need more entries (stock depth about 12).

[code: rust/bc2-levelwrite/src/culltool.rs:56] [code: rust/bc2-levelwrite/src/culltool.rs:2386-2390]
[exe: 0x0056C4E0 keeps 128 dwords at +0x20 of the query object, the count at +0x220]

## 3. Spheres

### CULL-SPH-1: Quantization

Status: **verified**

A node's sphere is four signed 16-bit keys: centre = (`SphereKeyX`, `SphereKeyY`, `SphereKeyZ`) / 16 m (range +-2048 m,
step 6.25 cm), radius = `SphereKeyW` / 4 m (step 25 cm). The game decodes them with `punpcklwd` / `psrad 16` /
`cvtdq2ps` and divides by 16.0 and 4.0. A writer rounds the centre to the nearest key and rounds the radius **up**,
so the decoded sphere contains the sphere it encodes (and whatever it must contain); a centre outside +-2048 m or a
radius over 8191.75 m cannot be stored.

[code: rust/bc2-levelwrite/src/culltree.rs:63-103] [code: bfbc2-culltree/culltree.py:281-299]
[test: rust/bc2-levelwrite/src/culltree.rs::keys_round_trip_and_contain] [test: bfbc2-culltree/tests/test_culltree.py::main]
[exe: divisors 16.0 at 0x1416D28 and 4.0 at 0x14D943C in the traversal]

### CULL-SPH-2: Leaf spheres

Status: **observed**

A leaf's sphere is the bounding sphere of its object's whole model in world space (models built away from their origin
have spheres far from the object's position). Its quantized radius is always larger than `Max`: `W >= 4 x Max + 0.5`.

[data: 123,585 of 123,585 stock leaves satisfy W >= 4 x Max + 0.5; W - ceil(4 x Max) is 0 or 1 above 1.5 m]
[data: Linux MP_008, 2,782 leaves: centre in the object's space within 0.14 m of the blueprint's median, 99 % within 0.105 m]
[doc: bfbc2-culltree/README.md:94-105]

### CULL-SPH-3: Internal spheres

Status: **observed**

An internal node's sphere bounds its children's **unquantized** spheres; decoded, it does not always contain the
children's decoded spheres in DICE's trees (38 % of child spheres stick out, median 0.08 m, at most 1.1 m). The suite
always writes internal spheres that contain the decoded spheres below them.

[data: stock trees: 38 % of child spheres stick out of their parent's decoded sphere, at most 1.1 m]
[doc: bfbc2-culltree/README.md:106-108]

### CULL-TRV-1: Traversal

Status: **observed**

The query starts at the root without testing the root's own sphere. At an internal node it decodes both children's
spheres and pushes each child that passes, the right one first so the left is taken next. The right child passes when
`|c - q|^2 < (r + q_r)^2` (sphere overlap); the left child when `|c - q|^2 < (r x q_r)^2`. At a leaf, `index &
0x7FFFFFFF` selects a 12-byte entry of the tree's runtime leaf array (`[tree + 0x18]`): a pointer at +0 (0: nothing
registered, skipped) and a 16-bit value at +8 that must lie in a range the query gives. So a leaf is reached only if
every ancestor below the root passes its test, and a leaf with no object registered on it is harmless.

[exe: 0x0056C4E0, with 0x0056BE40, 0x0056C820 and 0x00593700 reading the tree the same way]
[doc: bfbc2-culltree/README.md:110-117]

## 4. Slots

### CULL-SLOT-1: Client tree

Status: **observed**

The client tree has `EnumOffset` 0 and one leaf per member of the world's model groups (static, rigid, middle-growth,
tree): each member's `SpawnDatas[].Enumeration` is its leaf index.

[data: MP_008: 5,726 members, leaves 1..5,726 each once; all 37 stock pairs]
[doc: bfbc2-culltree/README.md:75-76]

### CULL-SLOT-2: Exclusive tree and an object's slot

Status: **verified**

The exclusive tree holds the placed objects (`Entity.ReferenceObjectData`). Its `EnumOffset` is the client tree's leaf
count. A placed object's `Enumeration` (a single 4-byte number) is `EnumOffset` + the index of its first leaf, so slots
are 1-based and global over both trees, and **slot 0 means "not in a tree"** (dynamic props: chairs, oil drums,
pallets). An object owns the consecutive leaves from its slot: two leaves with the same sphere for most blueprints, one
for some (distant scenery); the count belongs to the blueprint.

[code: rust/bc2-levelwrite/src/culltool.rs:2391-2398] [code: rust/bc2-levelwrite/src/culltree.rs:442-447]
[code: rust/bc2-formats/src/dbxedit/check.rs:66]
[test: bfbc2-culltree/tests/test_culltree.py::main]
[data: stock trees: 57,308 instances with two leaves (the same sphere in 96 %), 8,337 with one; EnumOffset = client leaf count in 37 of 37 pairs]

### CULL-SLOT-3: Slots are unique; slot 0 is a defect for static objects

Status: **observed**

Objects that share a slot are shown and hidden together. A static placed object left at slot 0 is drawn see-through
and treated as indestructible (it is in no cull tree), which is why objects added to a level need leaves.

[data: Napisal's ONS_MP_008 port left 14 console-only objects at slot 0, fixed by culltree add (slots 8,611..8,637)]
[doc: bfbc2-culltree/README.md:1-5] [doc: bfbc2-dbx-tools/FORMAT.md:173-177]

## 5. Adding objects

### CULL-ADD-1: What an addition must keep

Status: **verified**

Adding objects to an exclusive tree keeps the tree valid when:

1. `EnumOffset` is unchanged and the client tree untouched;
2. the new leaves continue the numbering (L + 1, L + 2 ...), the object's `Enumeration` is `EnumOffset` + its first
   new leaf, and it gets as many leaves as its blueprint has, with the blueprint's spheres moved and turned with the
   object's transform and its `Max`;
3. the nodes stay one preorder tree: every ancestor that gets the new nodes in its **left** subtree has its `Right`
   increased by their number, and every ancestor's `Max` is at least the new `Max`;
4. every ancestor's decoded sphere (the root's excepted) contains the new spheres (CULL-TRV-1);
5. the 256 spare nodes follow, and the traversal stack is not exceeded.

The level partition changes only in the objects' `Enumeration` values. A deleted or moved object's old leaves stay
in the tree with nothing registered on them.

[code: rust/bc2-levelwrite/src/culltree.rs:370-447] [code: rust/bc2-levelwrite/src/culltree.rs:543-597]
[test: rust/bc2-levelwrite/src/culltree.rs::add_keeps_one_preorder_tree] [test: bfbc2-culltree/tests/test_culltree.py::main]
[test: bfbc2-culltree/tests/test_culltree_native.py::main]
[doc: bfbc2-culltree/README.md:119-133]

### CULL-PLAT-1: Trees are per platform

Status: **observed**

A cull tree is built for its platform's blueprints: an Xbox 360 exclusive tree installed whole on the PC crashed the
traversal at load. PC levels keep PC trees; console objects are added to them (ONS_MP_008 uses the Linux-server
MP_008 tree with its objects' slots).

[data: one console exclusive tree installed on the PC crashed at load; the Onslaught ports run on PC / Linux-server trees]
[doc: bfbc2-culltree/README.md:142-143]

## 6. The DBX element stream as the cull tools read it

### CULL-DBX-1: Element stream

Status: **verified**

`culltree.py` and `bc2_formats::culltree::elements` read a partition's binary DBX element stream directly: each element a
varint tag (0 closes the open container; at the top level it ends the stream), a head byte (kind in the high nibble,
attribute count in the low), varint attribute pairs, then by kind a container's extra byte (children follow), a text
varint, a numbers value (varint count, varint size, count x size bytes) or two bytes. The bytes after the stream are
kept as the trailer. This is the general DBX element tree (`dbx.md`); the cull tools keep every byte, so an unchanged
partition is written back exactly.

[code: rust/bc2-formats/src/culltree.rs:100-190] [test: rust/bc2-formats/src/culltree.rs::reads_a_stream]
[test: rust/bc2-formats/src/culltree.rs::edges] [test: bfbc2-culltree/tests/test_culltree_native.py::main]
[data: 1,269 trees and level partitions of the game copy and OPFOR's work folders read the same by the Rust and the Python]

## Open questions

- Where the runtime leaf array (`[tree + 0x18]`) is allocated and how its size is chosen: no leaf or slot count is
  stored besides the nodes and `EnumOffset`, so added leaves must be covered by whatever sizes it. The main thing to
  watch in game.
- What an object's two leaves with the same sphere stand for, and why the left child's test multiplies the radii
  (CULL-TRV-1).
- The 16-bit value at +8 of a leaf entry and the range the query compares it with.
- Why DICE's internal spheres do not always contain their children's decoded spheres (CULL-SPH-3), and whether the
  game misses anything because of it.
- Whether a level may have a non-null `ClientTreeStaticCullTree` and what it would mean.
