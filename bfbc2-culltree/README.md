# Static cull trees (culltree)

`culltree.py` reads Battlefield: Bad Company 2's static cull trees (`World.StaticCullTreeData`), checks them
against a level's placed objects, and **adds placed objects that have no slot** - objects with `Enumeration` 0
that the game draws see-through and treats as indestructible because they are not in the level's cull tree.
It is plain Python 3 (no libraries) and writes everything it does not change byte for byte.

```
python culltree.py info <tree.dbx> [<level.dbx>]
python culltree.py verify <tree.dbx> <level.dbx> [--client <client tree.dbx>] [--ref <level.dbx>]...
                   [--ref-dir <folder>]... [--added <guid,...>] [--report <file>]
python culltree.py add <tree.dbx> <level.dbx> (--objects <guid,... | @file> | --auto)
                   --out-tree <tree.dbx> --out-level <level.dbx> [--ref ...] [--ref-dir ...] [--client ...] [--report <file>]
```

- **info**: size, leaves, EnumOffset, depth and root sphere; with a level, its objects by slot and the slot-0 objects
  whose blueprint is in a cull tree (the candidates for `add`).
- **verify**: the structure (below), every object's slot, and every object's decoded spheres against the same
  blueprint's other instances (leave-one-out). `--added` checks that every ancestor of those objects' leaves
  contains their spheres, so the traversal reaches them. Exit code 1 when a check fails.
- **add**: gives each object new leaves shaped like its blueprint's instances, inserts them, sets the objects'
  `Enumeration` in the level, writes both files to new names and checks them. `--auto` takes every slot-0 object
  whose blueprint has slotted instances. `--ref` / `--ref-dir` name other levels (their trees are found through the
  level's `ExclusiveStaticCullTree` reference, beside the level's folder) to take a shape from when the target has
  no instance of the blueprint; the target level's own instances come first.

The test (`tests\test_culltree.py`, in `run_tests.py quick`) builds trees and levels itself and needs no game data.

## Example: Nelson Bay Onslaught (ONS_MP_008)

The Onslaught level uses the Linux-server Nelson Bay tree, its objects given the slot of the same object in the
Linux layout. Fourteen objects that only the console layout has were left at slot 0:

```
python culltree.py add <unpacked>\Levels\ONS_MP_008\staticculltree_exclusive.dbx <unpacked>\Levels\ONS_MP_008.dbx --auto
    --client <unpacked>\Levels\ONS_MP_008\staticculltree_client.dbx
    --ref <OPFOR>\work\onslaught\linux_mp_008\level\Levels\MP_008.dbx --ref-dir <OPFOR>\work\levels\mp_001 ...
    --out-tree <OPFOR>\work\culltree\ons_mp_008\staticculltree_exclusive.dbx --out-level <OPFOR>\work\culltree\ons_mp_008\ONS_MP_008.dbx
```

gives them slots 8611..8637 (two leaves each), 56 new nodes (5767 -> 5823 used), and changes the level in 28 bytes
(the 14 `Enumeration` values). No existing leaf, sphere or Max changes; only the Right offsets of ancestors do.

## Format

### Where the trees are

A level's `LevelData` refers to two partitions, `ExclusiveStaticCullTree` (`levels/<x>/staticculltree_exclusive`)
and `ClientStaticCullTree` (`levels/<x>/staticculltree_client`); `ClientTreeStaticCullTree` is null in every stock
level. Each holds one `World.StaticCullTreeData` instance with two fields:

| Field | Type | Runtime offset (node) |
|---|---|---|
| `Nodes[]`, each `Right` | int32 | +0 |
| `Max` | float32 | +4 |
| `SphereKeyX`, `SphereKeyY`, `SphereKeyZ`, `SphereKeyW` | int16 (the XML shows them unsigned) | +8, +0xA, +0xC, +0xE |
| `EnumOffset` | int32 | |

The node offsets are the game's own (the type's field table in `BFBC2Game.exe`), so a node is 16 bytes at run time.

### Layout: a binary tree in preorder

- Node 0 is the root. An **internal** node `i` has its left child at `i + 1` and its right child at `i + Right`;
  `Right = 1 + the size of the left subtree`. (The root's Right of 2886 in the Linux MP_008 tree is therefore not
  "past the end" but the root's right child.)
- A **leaf** has `Right = 0x80000000 | leaf index` (negative as int32). Leaf indices run **1..L**, each once.
- `used nodes = 2 x L - 1`, then **256 all-zero spare nodes**.
- The **client** tree has `EnumOffset` 0 and one leaf per member of the world's model groups (static, rigid,
  middle-growth, tree): `SpawnDatas[].Enumeration` = its leaf index (MP_008: 5726 members, 1..5726, each once).
- The **exclusive** tree holds the placed objects (`Entity.ReferenceObjectData`); its `EnumOffset` = the client
  tree's leaf count. An object's `Enumeration` is `EnumOffset + its first leaf index` - so slots are 1-based,
  global over both trees, and slot 0 means "not in a tree" (dynamic props: chairs, oil drums, pallets).

All of this holds in every one of the 37 stock tree pairs in the game copy (multiplayer and campaign; checked with
`verify` and a corpus script).

### Leaves per object

An object owns **consecutive leaves starting at its slot**. In stock trees this is **two leaves with the same
sphere** for almost every blueprint (57,308 instances; the two are identical in 96%) and one leaf for some
(8,337 instances, e.g. distant-scenery meshes). The count belongs to the blueprint and does not follow its part
count: a barrack, a house and a lamp all take two. Of 1,023 blueprints, the 32 that look inconsistent are gaps where
the next leaves belong to objects outside the level partition. The leaf sphere is the bounding sphere of the whole
model in world space (distant-village models built away from their origin have spheres hundreds of metres from
the object's position). Napisal's Onslaught ports have shuffled slots in places, so shapes are taken by consensus.

### Spheres: quantization

- **Centre = key / 16 m**, signed 16-bit (range +-2048 m, step 6.25 cm). **Radius = W / 4 m** (step 25 cm).
  The traversal loads the four keys with `punpcklwd` / `psrad 16` / `cvtdq2ps`, divides by 16.0 (at `0x1416D28`)
  and by 4.0 (`0x14D943C`) and shuffles to (x/16, y/16, z/16, w/4).
- Fit against object positions: each leaf's centre, taken into the object's own space (inverse of its transform)
  and compared with the median of the blueprint's other instances: Linux MP_008, 2,782 leaves: median 0.047 m,
  99% within 0.105 m, largest 0.14 m. Shapes taken from the other stock levels and placed on MP_008's objects:
  1,854 leaves, median 0.062 m, largest 0.153 m, the placed sphere containing DICE's in 1,854 of 1,854.
- **Max** is the leaf's unquantized radius (float, the same for every instance of a blueprint); internal nodes hold
  the **largest Max of their subtree** (263,653 internal nodes, no exception). The quantized radius is always larger:
  `W >= 4 x Max + 0.5` in all 123,585 stock leaves (`W - ceil(4 x Max)` is 0 or 1 above 1.5 m; small objects get more).
- Internal spheres bound the children's unquantized spheres; their decoded spheres do not always contain the
  children's decoded spheres (38% of child spheres stick out, median 0.08 m, at most 1.1 m). Spheres the tool
  builds always contain what is below them.

### Traversal (`0x0056C4E0`; `0x0056BE40`, `0x0056C820` and `0x00593700` read the tree the same way)

The query keeps a stack of node pointers (128 dwords at +0x20, count at +0x220). At an internal node both
children's spheres are decoded and tested; a child that passes is pushed (right first, so the left one is taken
next). The right child is tested for overlap with the query sphere, `|c - q|^2 < (r + q_r)^2`; the left child as
`|c - q|^2 < (r x q_r)^2`. The root's own sphere is not tested here. At a leaf, `index & 0x7FFFFFFF` selects a
12-byte entry of the tree's leaf array (`[tree + 0x18]`): a pointer at +0 (0: nothing registered there, skipped)
and a 16-bit value at +8 that must lie in a range the query gives; passing entries are appended to the result.

### Adding objects: what must stay consistent

1. `EnumOffset` unchanged (it is the client tree's leaf count), the client tree untouched.
2. New leaves continue the numbering (`L + 1, L + 2, ...`); the object's `Enumeration` = `EnumOffset` + its first new
   leaf; as many leaves as its blueprint has in stock trees, with the blueprint's spheres moved and rotated with the
   object's transform, `W` = the largest of the instances' + 1 (room for the new centre's rounding), `Max` the
   blueprint's.
3. The nodes stay one preorder tree: every ancestor that gets the new nodes inside its **left** subtree gets its
   `Right` increased by their number; `Max` of every ancestor is at least the new `Max`.
4. Every ancestor's decoded sphere (the root's excepted) contains the new spheres, or the traversal never reaches
   them. `add` inserts where the spheres already contain them: down from the root into a child that contains the new
   sphere; where neither child does, a new internal node takes the place of the child that grows least. That child
   stays on its side under the new node (so it keeps the left or right test it had, and the new node, which
   contains it, gets that test too); the object's subtree (an internal node over its two leaves) takes the other side.
5. 256 zero spare nodes after the used ones; the traversal's 128-entry stack not exceeded (stock: depth about 12).

### Not known (not needed so far)

- Where the leaf array is allocated and how its size is chosen was not located. No count of leaves or slots is
  stored anywhere else in the level (the tree file holds only `Nodes` and `EnumOffset`), so it must come from the
  tree or from the objects that register; both include added leaves. This is the main thing to watch in game.
- What the two leaves of an object stand for (they have the same sphere) and why the left and right child tests
  differ.
- A console build's tree does not fit PC blueprints (a console exclusive tree installed whole crashed the traversal at
  load), so trees are built per platform: keep PC trees with PC blueprints.
