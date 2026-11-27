# Compiled animation trees (`AnimTreeInfo`)

Every animation tree partition (`AnimTreeDataAsset` and its `AnimTreeData` / `SpecificAnimTreeData` /
`ResultAnimTreeData`, engine/animation.md) has a compiled form beside it, `<tree>.res` (bundle type
`AnimTreeInfo`; the trees DICE's pipeline generated for single animations end `_atgen`, `_atgen_nt` or
`_atgen_looping`: 620 of the 1,926): DICE's tree compiler's output, which the runtime uses instead of the partition's node graph. The
suite and OPFOR copy it, rewriting only the hand IK records (ANIM-TREE-2) and slot durations (ANIM-TREE-3). This file
is its outer layout as round SPG4 read it from DICE's files (`spec/probes/resources.py`). Little-endian.

## 1. Layout

### ATREE-HDR-1: A 20-byte header and three blocks

Status: **verified** · Variants: PC

| Offset | Type | Content |
|---|---|---|
| 0 | u32 | 6 in every DICE file (a version) |
| 4 | u32 | the base address: where the compiler's memory image lay (ATREE-PTR-1) |
| 8 | u32 | block A's size |
| 12 | u32 | block B's size |
| 16 | u32 | block C's size |
| 20 | A + B + C bytes | the blocks, in order; the file ends after C |

Each block's size is a multiple of 16. A holds the nodes, B and C one entry per animation node: its length and its
binding (sections 2 and 3).

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py on DICE's bundles, 2026-11-23: 1,926 distinct AnimTreeInfo entries (2,594 live), version 6 in all, 20 + A + B + C = size in 1,926, every block a multiple of 16]
[exe: BFBC2Game.exe typeinfo records `AnimTreeInfo` (module Animation, 36 bytes) at 0x1C0B8E4 and `AnimTreeReplacementInfo` (20 bytes) at 0x1C0B8FC]

### ATREE-PTR-1: Pointers are absolute in the compiler's image

Status: **verified** · Variants: PC

The blocks are a memory image: a word w with base <= w < base + size is a pointer, w - base its offset from the start
of block A (file offset 20). Every such word is 4-aligned and points inside the blocks or at their end. Two trees
compiled at different addresses differ in all their pointer words by the difference of their bases, which is why
ANIM-TREE-3 tells durations from pointers by that shift. A loader relocates by subtracting the base; a tool that edits
a tree keeps the base and every pointer, or rewrites both.

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, 2026-11-23: in 1,926 of 1,926 every word in [base, base + size) is 4-aligned and at most A + B + C from block A]

## 2. The blocks (round SPG2-rest4)

The loader (the Animation module's resource factory 0xF47360, its `AnimTreeInfo` branch 0xF47631 - 0xF47812) reads
the four words after the version, allocates the three blocks separately, reads each, and relocates each by its own
walker with its own delta (block address minus the base minus the blocks before it). So a pointer in a block points
into that same block, and the walkers say where the pointers are: that is the blocks' outer layout. The object kept
is 0x24 bytes (`AnimTreeInfo`, vtable 0x14AB3E0): the three blocks' addresses and sizes at +0x0C / +0x10, +0x14 /
+0x18, +0x1C / +0x20.

### ATREE-BLK-1: Block A is a node table and typed nodes

Status: **verified** · Variants: PC

Block A starts with (u32 pointer to the node table, u32 node count); the table holds one pointer per node; a node
starts with a u32 type, 1 - 26 (all 26 occur in DICE's trees, 59,725 nodes over 1,926 trees), and a pointer at +4. Nine
types hold more pointers, at fixed places: type 2 at +0x20, +0x28, +0x30, +0x38 (and the word +0x38 points at is one
more), type 3 at +0x20, +0x28, +0x30, +0x38, type 4 at +0x24, types 6 and 23 at +0x20, +0x28, types 7, 13 and 25 at
+0x20, type 16 at +0x110 (and the word it points at); the other 17 types hold none past +4. Every one of these words
is 0 or points into block A or at its end.

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, round SPG2-rest4 2026-11-23: in 1,926 of 1,926 trees the node table and every node pointer lie in block A and every typed pointer is 0 or in block A (its end included); types 1 - 26, 59,725 nodes]
[exe: BFBC2Game.exe walker 0xF598E0 (the table +0, count +4; per node +4, then the type minus 1 through the byte map 0xF599D4 and the table 0xF599AC to 0xF59730 (types 2), 0xF59780 (3), 0xF597D0 (4), 0xF597F0 (6), 0xF59820 (7), 0xF59840 (13), 0xF59860 (16), 0xF59890 (23), 0xF598C0 (25)); called at 0xF47798 with block A - base]

### ATREE-BLK-2: Block B is flat data behind one pointer

Status: **verified** · Variants: PC

Block B's walker relocates one word, its first, which is 0 or points into block B; the rest of the block is plain
data. The slot durations OPFOR rewrites (ANIM-TREE-3, 0x5100 - 0x5810 in the first-person weapon trees) lie in block
B (0x4FB4 - 0x5824 in those trees), which is why rewriting them moves no pointer.

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, round SPG2-rest4 2026-11-23: block B's first word is 0 or in block B in 1,926 of 1,926; the 10 DICE trees of the first-person weapon sizes (36,292 - 36,836 bytes) all have A 0x4FA0 and B 0x870 bytes]
[exe: BFBC2Game.exe walker 0xF599F0 (one word), called at 0xF477A9 with block B - A - base]

### ATREE-BLK-3: Block C is a table of 0x60-byte records

Status: **verified** · Variants: PC

Block C starts with (u32 pointer to a record table, u32 count); a record is 0x60 bytes with pointers at +0x00 and
+0x08 (as many as the count at +0x10, at most two), at +0x14 and +0x1C (as many as the count at +0x24, at most two),
and at +0x28, +0x30, +0x38; each is 0 or points into block C or at its end. The records are presumably the bindings
to the partition's animations (ATREE-HDR-1's reading of C).

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, round SPG2-rest4 2026-11-23: 1,926 of 1,926 trees: the table in block C, every record's pointers 0 or in block C (887 at +0x1C, 839 at +0x14 and 4 at +0x38 point at its end)]
[exe: BFBC2Game.exe walker 0xF59A30 (record stride 0x60 at 0xF59AC6; the counts at +0x10 and +0x24; +0x28, +0x30, +0x38), called at 0xF477BE with block C - A - B - base; the loader's object 0xF3F540 (vtable 0x14AB3E0, 0x24 bytes) filled at 0xF477E6 - 0xF47801]

## 3. The nodes and the animations (round SPG2-rest5)

### ATREE-NODE-1: The 26 node types are the executable's NodeInfo classes

Status: **verified** · Variants: PC

The Animation module names a node type through one function (0xF490A0: type 1 - 26 to a string, used for its debug
output). With DICE's counts over the 1,926 trees (nodes / trees that have one) and the record size most nodes of the
type take (the distance to the next node; the nodes lie in the table's order in all 1,926 trees):

| Type | Name | Nodes | Trees | Record | Pointers past +4 (ATREE-BLK-1) |
|---|---|---|---|---|---|
| 1 | AnimationNodeInfo | 28,254 | 1,926 | 0x40 | none |
| 2 | IfNodeInfo | 7,756 | 330 | 0x50 | +0x20, +0x28, +0x30, +0x38 |
| 3 | SelectorNodeInfo | 5,684 | 972 | 0x50 | +0x20, +0x28, +0x30, +0x38 |
| 4 | WeightFactorNodeInfo | 2,863 | 881 | 0x50 / 0x40 | +0x24 |
| 5 | AimNodeInfo | 1,074 | 111 | 0x40 / 0x30 | none |
| 6 | EnumeratedSwitchNodeInfo | 749 | 267 | 0x50 | +0x20, +0x28 |
| 7 | AutoEndingEnumeratedSwitchNodeInfo | 27 | 21 | 0x50 / 0x40 | +0x20 |
| 8 | TripleStateNodeInfo | 3,882 | 484 | 0x30 | none |
| 9 | TimeSynchronizerNodeInfo | 42 | 6 | 0x50 | none |
| 10 | TimeSynchronizerToParentNodeInfo | 18 | 6 | 0x30 | none |
| 11 | MovementNodeInfo | 66 | 6 | 0x30 | none |
| 12 | Movement4WayNodeInfo | 12 | 6 | 0x30 | none |
| 13 | TransitionNodeInfo | 280 | 140 | 0x50 | +0x20 |
| 14 | PingPongNodeInfo | 134 | 134 | 0x30 | none |
| 15 | AutoEndingLoopNodeInfo | 4,559 | 275 | 0x30 | none |
| 16 | WeaponHandsIkNodeInfo | 239 | 239 | 0x120 / 0x170 | +0x110 |
| 17 | GenericHandsIkNodeInfo | 37 | 37 | 0xE0 | none |
| 18 | BoneToBoneIkNodeInfo | 105 | 105 | (the last node) | none |
| 19 | TimeControllerNodeInfo | 2,974 | 997 | 0x30 | none |
| 20 | ProceduralLeaningNodeInfo | 18 | 6 | 0xA0 | none |
| 21 | LerpNodeInfo | 26 | 26 | 0x30 | none |
| 22 | TurnNodeInfo | 24 | 6 | 0x50 | none |
| 23 | RandomSelectorNodeInfo | 326 | 169 | 0x80 / 0x70 / 0x60 | +0x20, +0x28 |
| 24 | SelectorByPoseNodeInfo | 72 | 6 | 0x80 | none |
| 25 | XorSelectorNodeInfo | 487 | 248 | 0x60 | +0x20 |
| 26 | RagdollNodeInfo | 17 | 16 | 0x20 | none |

Each is the compiled form of the partition's node class of the same stem (`AnimationNodeData`, `IfNodeData`,
`SelectorNodeData` ..., engine/animation.md); `AimNodeInfo` is `AimNode2Data`'s. A record is longer than the type's
fixed part when it carries arrays (the selectors' and switches' children behind their pointers). Every tree has at
least one animation node. The fields past the pointers were not read node by node.

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, round SPG2-rest5 2026-11-27: the 26 names from the executable's function; node counts per type as listed (59,725); the nodes in table order in 1,926 of 1,926 trees]
[exe: BFBC2Game.exe 0xF490A0 (type - 1 against 0x19, jump table 0xF4915C, each case `mov eax, <name>`: "AnimationNodeInfo" 0x14AB7CC ... "RagdollNodeInfo" 0x14AB598)]

### ATREE-BLK-4: Blocks B and C hold one entry per animation node

Status: **verified** · Variants: PC

Block B is (u32 pointer to B + 16, u32 n, two zero words) and n 16-byte entries; block C is (u32 pointer to its
record table, u32 n) and n 0x60-byte records (ATREE-BLK-3); n is the tree's number of `AnimationNodeInfo` nodes, in
all 1,926 trees (B = 16 + 16 n bytes). An animation node's word +0x20 is its index among the animation nodes (in
table order), so it names its B entry and its C record. A B entry's first float is the animation's length in seconds
(1/30 - 182.3 s over DICE's 28,254, never 0: the slot durations OPFOR rewrites, ANIM-TREE-3); its other three words
are zero in 1,782 trees, and in the rest hold one more float (seen: about 1.3 at +4, or 45 / 90 at +8). A C record's
eight bytes at +0x54 are never zero and repeat within a tree when two nodes play one animation (presumably the
animation's key in the partition's list); its pointers are ATREE-BLK-3's.

So a tool that changes a tree's animation lengths writes the B entry of the animation node whose index it is (its
+0x20), and a tree with a new animation node needs a B entry and a C record more.

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, round SPG2-rest5 2026-11-27: B = 16 + 16 n with its head (B + 16, n, 0, 0) and n = C's count = the animation nodes in 1,926 of 1,926; the +0x20 index in 1,926 of 1,926; 28,254 lengths, 0.0333 - 182.3333 s; C +0x54 never zero (scratch read), unique within 822 of 1,926 trees]

## Open questions

- The node records' fields past type, +4 and the pointers (ATREE-NODE-1): parameters' indices, weights, blend times;
  the run-time evaluators of each type were not read.
- Block B's second and third floats where they are set, and block C's words beside its pointers (+0x40 - +0x50, the
  key at +0x54): read against the partitions' animation lists.
