# AimAnimation (generated aim poses)

`AimAnimation` resources (`*_generatedaimanimation.res`) hold a third-person weapon's aim pose: the upper body and
weapon bones over a grid of aim directions. The suite reads them from BFBC2 (PC, little-endian) and Battlefield 1943
(Xbox 360, big-endian), writes the PC form and renumbers bones to BFBC2's skeleton
(`bfbc2-anim-tools/aimanim.py`, `bc2_formats::aimanim`). The layout was decoded by the suite (2026-11-08); every file
of both games reads to its exact size.

### AIM-HDR-1: Header

Status: **verified** · Variants: PC, BF1943

Words in the platform's byte order (PC little-endian, BF1943 big-endian):

| Offset | Size | Field |
|---|---|---|
| 0x00 | 4 | version, 1 |
| 0x04 | 4 | an address (the build tool's; kept as it is) |
| 0x08 | 4 | data size: the file's length - 12 |
| 0x0C | 4 | the address + 0x10 |
| 0x10 | 4 | bone count *b* |
| 0x14 | 1 | pose count *n* |
| 0x15 | 7 | zero (3 bytes, then a zero u32 at 0x18) |
| 0x1C | 2 × *b* | the bone table: `{u8 skeleton bone index, u8 kind}` in table order |

A file whose version is not 1, whose size word is not its length - 12, or whose word 0x0C is not word 0x04 + 0x10 is
refused.

[data: 1,126 of 1,126 AimAnimation entries of the game copy: version 1, size = length - 12, word 0x0C = word 0x04 + 0x10, the 7 bytes after the pose count zero, names ending _generatedaimanimation.res; pose count 25 in 1,116 (a 5 x 5 grid), 9 in 10; bone count 22 in 1,096, 17 in 20, 29 in 10]
[test: rust/bc2-formats/src/aimanim.rs::round_trip_both_orders] [test: rust/bc2-formats/src/aimanim.rs::refusals] [test: bfbc2-anim-tools/tests/test_aimanim.py::units] [code: rust/bc2-formats/src/aimanim.rs:98-110]
[doc: bfbc2-anim-tools/aimanim.py:5-15]

### AIM-BLK-1: Blocks start on a 16-byte boundary counted from 0x0C

Status: **verified** · Variants: PC, BF1943

After the bone table, zero bytes up to the next offset o with (o - 0x0C) a multiple of 16; then one block per bone in
table order:

| Kind | Block | Size |
|---|---|---|
| 0 (fixed) | position (x, y, z, 1), rotation quaternion (x, y, z, w) | 32 |
| 1 (rotation) | position (x, y, z, 0), then *n* quaternions | 16 + 16*n* |
| 2 (moving) | *n* × (position (x, y, z, 1), quaternion) | 32*n* |

All values 32-bit floats. The last block ends exactly at the end of the file. Another kind is refused (the writer
writes any kind other than 0 and 1 as a moving block).

[data: 24,742 bones over the game copy's 1,126 files: kind 1 18,374, kind 0 5,221, kind 2 1,147; every file ends exactly after its last block]
[test: rust/bc2-formats/src/aimanim.rs::round_trip_both_orders] [test: tests/test_f3_native.py::area_aim] [code: rust/bc2-formats/src/aimanim.rs:62-65] [code: rust/bc2-formats/src/aimanim.rs:111-138]
[doc: bfbc2-anim-tools/aimanim.py:15-19]

### AIM-WR-1: The PC form and bone renumbering

Status: **verified** · Variants: PC, BF1943

The suite writes the PC form little-endian with the header of AIM-HDR-1 (the address kept, the size recomputed) and
the blocks in table order. Converting BF1943's file renumbers each bone to BFBC2's skeleton index and sorts the bone
table by the new index (a stable sort), the blocks following their bones; a bone with no counterpart is refused. NaN
values are refused.

[test: rust/bc2-formats/src/aimanim.rs::renumbering_sorts_and_refuses_missing] [test: bfbc2-anim-tools/tests/test_aimanim.py::game] [code: rust/bc2-formats/src/aimanim.rs:160-209] [code: rust/bc2-formats/src/aimanim.rs:213-250]

## Open questions

- Whether the game requires the bone table sorted by skeleton index (DICE's files are; the suite keeps it so).
- The order of the 25 poses in the 5 x 5 grid (yaw-major or pitch-major) and the angles they stand for; the 9-pose
  files' grid.
- The meaning of a rotation block's position w = 0 against 1 elsewhere.
