# Xbox 360 disc images (XDVDFS)

The Xbox / Xbox 360 game disc file system. The suite lists and extracts BFBC2's Xbox 360 disc image
(`bfbc2-console-readers/xiso/xisolib.py`, `bc2_formats::xiso`) as an input for the Xbox 360 -> PC conversion. Read
only. All numbers little-endian.

### XISO-PART-1: The game partition and its volume descriptor

Status: **verified** · Variants: Xbox 360

Sectors are 2,048 bytes. The game partition starts at a disc-type dependent offset (a redump-style image holds the
video partition first); the reader tries 0x0, 0x0FD90000, 0x02080000 and 0x18300000 in that order and takes the
first whose sector 32 starts with `MICROSOFT*XBOX*MEDIA`. That volume descriptor holds, after the 20-byte magic, the
root directory's sector (u32) and size in bytes (u32). Sector numbers count from the partition's start.

[test: rust/bc2-formats/src/xiso.rs::walks_a_disc] [test: bfbc2-console-readers/tests/test_console_readers.py::main] [code: rust/bc2-formats/src/xiso.rs:13-17] [code: rust/bc2-formats/src/xiso.rs:61-79]
[doc: bfbc2-console-readers/xiso/xisolib.py:4-8]

### XISO-DIR-1: Directory tables are binary trees

Status: **verified** · Variants: Xbox 360

A directory is a table of entries forming a binary tree, read from the table's first entry:

| Offset | Size | Field |
|---|---|---|
| 0 | 2 | left subtree: entry offset in 4-byte units (0 = none) |
| 2 | 2 | right subtree, likewise |
| 4 | 4 | the entry's sector |
| 8 | 4 | its size in bytes |
| 12 | 1 | attributes (0x10: a directory) |
| 13 | 1 | name length |
| 14 | | name |

Entries are 4-byte aligned and never cross a sector; the rest of a sector is 0xFF padding (a left offset of 0xFFFF
marks padding). The reader walks left, node, right, and descends into a directory where it meets it, giving the
files in that order.

[data: BFBC2's Xbox 360 disc image: 693 files listed in the reference's order (BENCHMARKS F0)]
[test: rust/bc2-formats/src/xiso.rs::walks_a_disc] [test: rust/bc2-formats/src/xiso.rs::a_looping_folder_is_left_to_the_reference] [test: bfbc2-console-readers/tests/test_console_readers_native.py::main] [code: rust/bc2-formats/src/xiso.rs:125-160]
[doc: bfbc2-console-readers/xiso/xisolib.py:5-8]

## Open questions

- Attribute bits other than 0x10 (read-only, hidden ...) are not used by the suite.
- Disc layouts with a partition offset not in the list (other disc types) are untested.
