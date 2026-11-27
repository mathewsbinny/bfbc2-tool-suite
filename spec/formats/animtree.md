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
property curves (sections 2 - 4).

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
and at +0x28, +0x30, +0x38; each is 0 or points into block C or at its end. Round SPG2-rest6 read the records: an
animation node's five property curves (ATREE-BLK-6), not bindings to the partition's animations as first presumed.

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
least one animation node. Round SPG2-rest6 read the fields: the common header (ATREE-NODE-2), the animation node's
(ATREE-NODE-3) and each type's (ATREE-NODE-4).

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
are zero in 1,782 trees, and in the rest hold one more float (seen: about 1.3 at +4, or 45 / 90 at +8: the cycle's
distance and angle, ATREE-BLK-5). A C record's eight bytes at +0x54 are never zero and repeat within a tree when two
nodes play one animation; round SPG2-rest6 split them into a flag byte, three bytes of padding and the compiler's
address of the animation (ATREE-BLK-6); its pointers are ATREE-BLK-3's.

So a tool that changes a tree's animation lengths writes the B entry of the animation node whose index it is (its
+0x20), and a tree with a new animation node needs a B entry and a C record more.

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, round SPG2-rest5 2026-11-27: B = 16 + 16 n with its head (B + 16, n, 0, 0) and n = C's count = the animation nodes in 1,926 of 1,926; the +0x20 index in 1,926 of 1,926; 28,254 lengths, 0.0333 - 182.3333 s; C +0x54 never zero (scratch read), unique within 822 of 1,926 trees]

## 4. The node records' fields (round SPG2-rest6)

The run-time tree (0xF69140) makes one state object per node, by type, each with its own vtable (type 1 0x14AB8B4,
2 0x14AB8C0 ... 26 0x14AB95C; the makers 0xF5E130 - 0xF5E4B0 and 0xF39240 - 0xF52FF0); the object keeps its node's
record at +0x14, and the vtable's third slot is the type's evaluator. The fields below are read by those evaluators,
by the shared node update 0xF5A380, and by the parents that fade their children. A parameter word is a u16 index
into the tree's parameters (0xFFFF none): the run time maps it through a byte table to a slot of the bool, int or
float values (0xF59FEF, 0xF5A3D1). The names are the partition classes' fields (`BaseNodeData`, `AnimationNodeData`
... in the executable's typeinfo, engine/typeinfo) where the code or DICE's data decides them; "presumably" marks the
rest.

### ATREE-NODE-2: Every node starts with a 0x20-byte header

Status: **verified** · Variants: PC

| Offset | Type | Content |
|---|---|---|
| +0x00 | u32 | the type, 1 - 26 (ATREE-NODE-1) |
| +0x04 | pointer | an array of +0x08 pairs (u32, u32), 8 bytes each, at the end of the record (the record's end when there are none) |
| +0x08 | u32 | the pairs' count, 0 - 3 |
| +0x0C, +0x10 | f32, f32 | the two blend times (0 - 1 s; 0.3 most): a parent turns them into fade rates 1 / t; presumably `BlendInTime`, `BlendOutTime` |
| +0x14 | u16 | the node's index (`NodeIndex`): its place in the node table |
| +0x16 | u16 | the parent's index (`ParentNodeIndex`), 0xFFFF for the root |
| +0x18 | u16 | `NodeWeightParameter`: a float parameter the node writes its weight into (0xFFFF none) |
| +0x1A | u8 | 1 in every DICE node: the update calls the type's evaluator only when it is set (presumably `EnabledBySkeleton`) |
| +0x1B - +0x1F | | zero |

Node 0 is the root and the only node without a parent, and every parent comes before its children in the table. The
pairs go with the node's weight to the tree's list of the 16 heaviest (0xF50C60), presumably the node's
`InfluenceMapEffects` (values 0 - 9 and 0 - 7). So a tool that adds a node gives it the next index, its parent's
index, 0xFFFF / 1 at +0x18 / +0x1A and a pair pointer to its record's end.

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, round SPG2-rest6 2026-11-27: in 1,926 of 1,926 trees every node's +0x14 is its table index, +0x16 0xFFFF exactly for node 0 and below the node's own index otherwise, +0x1A 1, +0x1B - +0x1F zero, +0x08 at most 3 and the pairs inside the record]
[exe: BFBC2Game.exe the node update 0xF5A380 (+0x18 0xF5A3AA, written as max(old, weight) 0xF5A3D5 - 0xF5A3F8; the pairs +0x04 / +0x08 0xF5A40B - 0xF5A43E into 0xF50C60, which keeps 16; +0x1A 0xF5A443 before the vtable's slot 2 0xF5A464); the blend times +0x0C / +0x10 read by the parents 0xF38750 / 0xF38759 and 0xF3916D / 0xF39174 into 0xF48E60 (1 / t); the state objects made by type in 0xF69280 - 0xF695AE]

### ATREE-NODE-3: The animation node's fields

Status: **verified** · Variants: PC

| Offset | Type | Content |
|---|---|---|
| +0x20 | u32 | the animation's index: its block B entry and block C record (ATREE-BLK-4) |
| +0x24 | u32 | `IsCyclic`, a `TripleBool` (0 false, 1 true, 2 none: the animation's own) |
| +0x28 | u32 | `PlayFromBegining`, a `TripleBool` |
| +0x2C | i32 | `CallbackId`: -1 or 0 in DICE's trees; sent with the animation's index when the animation starts or ends (0xF50D30, up to 60 a frame) |
| +0x30 | u16 | `ScaledAnimationTimeParameter`: a float parameter that, above 0, replaces the animation's length |
| +0x32 | u16 | `NormalizedTimeParameter`: the float parameter the node writes its normalized time into |
| +0x34 - +0x3F | | zero |

DICE's generated `_atgen_looping` trees set `IsCyclic` true on all their animation nodes and leave `PlayFromBegining`
none, which decides the order of the two.

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, round SPG2-rest6 2026-11-27: +0x24 / +0x28 at most 2 and +0x2C 0 or -1 in 1,926 of 1,926 trees; +0x24 = 1 on 332 of the 332 animation nodes of the `_atgen_looping` trees]
[exe: BFBC2Game.exe evaluator 0xF59F90 (vtable 0x14AB8B4 slot 2): +0x20 into the B lookup 0xF506D0 at 0xF59FC4, +0x30 0xF59FCE - 0xF5A000, +0x28 / +0x24 0xF5A067 - 0xF5A08B (2 keeps the default, else == 1), +0x2C with +0x20 into 0xF50D30 at 0xF5A172 / 0xF5A191, +0x32 written 0xF5A217 - 0xF5A244]
[exe: BFBC2Game.exe typeinfo `AnimationNodeData` (IsCyclic and PlayFromBegining `TripleBool`, CallbackId Int32, the two `FloatParameter`s) and `TripleBool` (TB_FALSE 0, TB_TRUE 1, TB_NONE 2)]

### ATREE-NODE-4: Each type's fields; child nodes are u16 indices

Status: **verified** · Variants: PC

A node names its children by their u16 indices (ATREE-NODE-2's +0x14), in place or in arrays behind its pointers
(ATREE-BLK-1); a word marked "child" below names, in every DICE tree, a node whose parent is this node ("or none":
0xFFFF allowed). "Array" is a (pointer, u32 count) pair whose elements lie after the fixed part, padded to 16 bytes.

| Type | Fixed part | Fields past the header |
|---|---|---|
| 1 Animation | 0x40 | ATREE-NODE-3 |
| 2 If | 0x40 | three arrays +0x20 / +0x24, +0x28 / +0x2C, +0x30 / +0x34 (counts up to 3; presumably `BoolValues`, `FloatValues`, `IntValues`); +0x38 the expression (pointer to a pointer, 0 in 7,230 of 7,756); +0x3C `ThenNode` child; +0x3E `ElseNode` child or none |
| 3 Selector | 0x40 | four arrays +0x20 / +0x24 ... +0x38 / +0x3C (counts up to 6, 1, 5, 7: the always, bool, float and int child groups, order not read) |
| 4 WeightFactor | 0x40 | +0x20 `Node` child; +0x24 / +0x28 array `ParameterValues` (0 - 2); +0x2C f32 `Factor` (0, 0.5, 0.99); +0x30 u32 `Invert` (0 / 1) |
| 5 Aim | 0x30 | +0x20 u32 0 - 9 (the aim node's own index in its tree); +0x24 a parameter word (0 in all) |
| 6 EnumeratedSwitch | 0x40 | arrays +0x20 / +0x24 (`Nodes`, 2 - 3) and +0x28 / +0x2C (`Transitions`, 0 in DICE's); +0x30 `NodeSelectorParameter`; +0x32 `DisableTransitionsParameter` |
| 7 AutoEndingEnumeratedSwitch | 0x30 | array +0x20 / +0x24 (`Nodes`, 5 - 61); +0x28, +0x2A two words (presumably the selector and start parameters); +0x2C `ElseNode` child or none |
| 8 TripleState | 0x30 | +0x20, +0x22, +0x24, +0x26 four child-or-none words (the pre, loop, post and else nodes; +0x26 none in 3,620 of 3,882, so presumably `ElseNode`); +0x28 `ActivateNodeParameter`; +0x2A, +0x2B two flag bytes (`CrossFadeEnabled`, `ContinuousTime`) |
| 9 TimeSynchronizer | 0x40 | +0x20 u32 0 / 1, +0x24 u32 0 - 2, +0x28 u32 1 / 2 (presumably `IsCyclic`, `PlayFromBegining`, `SynchType`); +0x2C `Node` child; +0x2E, +0x30 two parameters |
| 10 TimeSynchronizerToParent | 0x30 | +0x20 `Node` child |
| 11 Movement | 0x30 | +0x20 a parameter; +0x22 - +0x2C six children (forward, backward, the two left and two right animations); +0x2E `FrontParameter` |
| 12 Movement4Way | 0x30 | +0x20 a parameter; +0x22 - +0x28 four children |
| 13 Transition | 0x30 | array +0x20 / +0x24 (`Transitions`, 2 or 4); +0x28 `ElseNode` child or none; +0x2A, +0x2C, +0x2E three parameters |
| 14 PingPong | 0x30 | +0x20, +0x22, +0x24 three children; +0x26 `StateParameter` |
| 15 AutoEndingLoop | 0x30 | +0x20 `Node` child; +0x22 `ElseNode` child or none; +0x24 `Parameter`; +0x26 u8 `RestartAnimation` |
| 16 WeaponHandsIk | 0x120 | the hands' baked transforms (ANIM-TREE-2; rows of three floats, each fourth word 0xCDCDCDCD); +0xC4 a child; +0x110 a pointer to a pointer (0 in 134 of 239) |
| 17 GenericHandsIk | 0xE0 | transforms as type 16; +0xC4 a child |
| 18 BoneToBoneIk | 0x70 | transforms as type 16; +0x60, +0x64 two u32 (10 and 45 in all; presumably `BoneIndex`, `TargetBoneIndex`) |
| 19 TimeController | 0x30 | +0x20 `Animation` child; +0x22 `Parameter`; +0x24 f32 `MinValue`, +0x28 f32 `MaxValue` (mostly 0 and 1) |
| 20 ProceduralLeaning | 0xA0 | its limits as floats, the same in all 18 DICE nodes |
| 21 Lerp | 0x30 | +0x20 `Node1`, +0x22 `Node2` children; +0x24 `LerpParameter`; +0x28 f32 `MinValue` (-75 - -10), +0x2C f32 `MaxValue` (7 - 45) |
| 22 Turn | 0x40 | +0x20 - +0x26 four children (the turn animations); +0x28, +0x2A, +0x2C three parameters (the turn speed, angle and time); +0x30 f32 |
| 23 RandomSelector | 0x40 | arrays +0x20 / +0x24 (`MainNodes`, 0 - 13) and +0x28 / +0x2C (`RandomNodes`, 0 - 12) of 16-byte elements; +0x30 u32 0 / 1 (presumably `ResetRandomTimes`) |
| 24 SelectorByPose | 0x80 | the leg positions as rows of three floats (fourth word 0xCDCDCDCD); +0x70 `Idle`, +0x72 `Transition` children; +0x74 u8 |
| 25 XorSelector | 0x30 | array +0x20 / +0x24 (`Nodes`, 4 or 12; 4-byte elements: a child and a bool parameter); +0x28 `ElseNode` child or none |
| 26 Ragdoll | 0x20 | the header only |

So a tool that moves or adds nodes renumbers the child words of this table, the child words inside the arrays, and
the header's +0x14 / +0x16; the arrays' element layouts (types 2, 3, 6, 7, 13, 23) were read only by their sizes.

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, round SPG2-rest6 2026-11-27: every child word of `ATREE_CHILDREN` (the table's) names a node whose +0x16 is this node, or is 0xFFFF where allowed, in 1,926 of 1,926 trees; the other ranges from a scratch read of all 59,725 nodes]
[exe: BFBC2Game.exe the evaluators (vtable slot 2) and their fields: 2 0xF5D710 (+0x38, +0x3C); 3 0xF42DC0 (+0x20 - +0x2C); 4 0xF39830 (+0x20) and 0xF3E9F0 (+0x24, +0x28); 5 0xF59F30 (+0x20, +0x24); 6 0xF41360 (+0x20 - +0x2C, +0x30, +0x32); 7 0xF41510 (+0x28, +0x2A, +0x2C); 8 0xF41860 (+0x22, +0x26, +0x28, +0x2A, +0x2B); 9 0xF3E4F0 (+0x20 - +0x30); 10 0xF39450 (+0x20); 11 0xF40AA0; 12 0xF3D010; 13 0xF415D0 (+0x20 - +0x2E); 14 0xF3D140 (+0x20 - +0x26); 15 0xF60B60 (+0x22, +0x24, +0x26); 16 0xF5D820 (+0xC0, +0xC4, +0x110); 17 0xF5A4C0; 18 0xF51530 (+0x60, +0x64); 19 0xF3E3D0 (+0x20 - +0x28); 21 0xF5A5D0 (+0x20 - +0x2C); 22 0xF3E770 (+0x20 - +0x2C); 23 0xF40F90 (+0x20, +0x28, +0x30); 24 0xF3DD20 (+0x70, +0x72, +0x74); 25 0xF41210 (+0x20 - +0x28)]

### ATREE-BLK-5: Block B's second and third floats are the cycle's distance and angle

Status: **observed** · Variants: PC

A B entry is (f32 length, f32 distance, f32 angle, u32 0). The animation node's time step (0xF492B0) runs by one of
its time modes: by time, the step is dt / length; following the movement speed, |speed| x dt / distance (metres a
cycle); following the turn speed, |speed| x dt / angle (degrees a cycle). DICE sets the distance on 700 entries (0.21 -
5.22) and the angle on 96 (45 - 90), the movement and turn animations; the fourth word is zero in all.

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, round SPG2-rest6 2026-11-27: of 28,254 entries 700 with +4 (0.2083 - 5.2213), 96 with +8 (45 - 90), none with +12]
[exe: BFBC2Game.exe 0xF59F90 passes the entry's +4 and +8 (0xF5A0EF, 0xF5A102) with the length to 0xF492B0, whose modes (jump table 0xF49348) divide dt by the length (0xF492CC - 0xF492E1), the speed's |+0x18| x dt by +4 (0xF492E3 - 0xF492F4) or by +8 (0xF492FA - 0xF4930B)]

### ATREE-BLK-6: A block C record is the animation node's five property curves

Status: **verified** · Variants: PC

An animation node's C record holds the `AnimationNodeData`'s `PropertyCurveType` fields compiled: five curves, each
a mode word and, when the mode is animated, its disable intervals.

| Curve | Mode | Curves (pointer, u32 interval count) | Run-time target |
|---|---|---|---|
| 1 | +0x4C | +0x10 curves (0 or 2): +0x00 / +0x04, +0x08 / +0x0C | the instance's +0x570 list |
| 2 | +0x40 | +0x24 curves (0 or 2): +0x14 / +0x18, +0x1C / +0x20 | a per-hand list (records of 0x90 at +0x108) |
| 3 | +0x44 | +0x28 / +0x2C | the instance's +0x1F8 |
| 4 | +0x48 | +0x30 / +0x34 | +0x1FC |
| 5 | +0x50 | +0x38 / +0x3C | +0x200 |

The mode is a `PropertyCurveType`: 0 no effect, 1 always on (weight 1), 2 always off (weight 0), 3 animated, 4
manual input (3 and 4: the node's weight times the curve). The pointers are set exactly when the mode is 3 or 4.
An interval is four ascending f32 of normalized time; outside (first, fourth) the curve is 1, inside it ramps over
the second and third (the `DisableInterval`'s four times). Mode 4 occurs only on curve 5, the one property with a
manual input in the partition class (`AimingManualInput`), so curve 5 is `Aiming`; the two-curve groups are
presumably the hands' and the legs' IK (`HandsIk`, `LegsIk`), curves 3 and 4 the weapon's (`WeaponIkAnimation`,
`WeaponIkDelta`). +0x54 is a byte copied into each curve's output (0 / 1; presumably
`FixLegsDistanceFromGround`), +0x55 - +0x57 the compiler's padding, +0x58 a u32 the compiler left (an address in its
memory, never a pointer into the blocks, equal for nodes that play one animation), +0x5C zero.

So a tool that adds an animation node writes a C record with all five modes 0 and no pointers, or copies a donor
node's record with its intervals.

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, round SPG2-rest6 2026-11-27: in 1,926 of 1,926 trees every record's modes are 0 - 4, 4 only at +0x50, the curve counts 2 exactly when animated, the pointers set exactly when animated; modes over 28,254 records: +0x40 3 in 12,272, +0x50 4 in 1,155 and 3 in 949, +0x4C 3 in 55]
[exe: BFBC2Game.exe the C lookup 0xF50720 (record stride 0x60 from block C's table, or a replacement's), called by 0xF59460: curves 1 and 2 through 0xF534C0 (0xF594AD - 0xF59500; modes by the jump table 0xF53570: 1 0xF50910 with 1.0, 2 with 0, 3 / 4 0xF50990 with the weight), curves 3 - 5 through 0xF53580 (0xF59510 - 0xF595B2); the interval test 0xF507D0; +0x54 copied at 0xF509E8]
[exe: BFBC2Game.exe typeinfo `PropertyCurveType` (PCT_NO_EFFECT 0, PCT_ALWAYS_ON 1, PCT_ALWAYS_OFF 2, PCT_ANIMATED 3, PCT_MANUAL_INPUT 4), `DisableInterval` (four Float32, 16 bytes)]

## Open questions

- The arrays' elements of types 2, 3, 6, 7, 13 and 23 (sizes read, fields not), and the order of the Selector's four
  groups (ATREE-NODE-4); the expression record behind the If node's +0x38.
- Which two-curve group is the hands' and which the legs', and which of curves 3 / 4 is `WeaponIkAnimation`
  (ATREE-BLK-6): the targets +0x570, +0x1F8 - +0x200 were not followed to their readers.
- The header's pairs (ATREE-NODE-2): what the 16 heaviest drive (presumably the influence maps).
