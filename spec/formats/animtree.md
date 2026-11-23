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

## Open questions

- The node, parameter and binding records inside the blocks (the typeinfo's `AnimTreeInfo` object is 36 bytes: the
  three blocks' runtime pointers and sizes are the likely fields).
