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

Each block's size is a multiple of 16. The blocks' contents are not decoded beyond ANIM-TREE-2's hand records and
ANIM-TREE-3's durations (inferred: A the tree's nodes, B its parameters, C the bindings to the partition's
animations, by place, ANIM-TREE-1).

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

## Open questions

- What each of the 26 node types is (ATREE-BLK-1 gives their pointer places, not their names or fields): the
  evaluator that switches on the node type, and an enum of 26 in the Animation module's typeinfo, would name them.
- Block B's contents beyond its first pointer, and block C's records beyond their pointers (ATREE-BLK-2, -3): which
  words are the durations, weights and animation indices.
