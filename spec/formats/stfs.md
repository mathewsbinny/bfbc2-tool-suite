# STFS packages (Xbox 360)

Xbox 360 content packages (`LIVE`, `PIRS`, `CON `): title updates and downloadable content. The suite reads them
(`bfbc2-console-readers/stfs/stfs.py`, `bc2_formats::stfs`) to take BFBC2's Xbox 360 update and DLC files (the
Onslaught levels, the console menus) as inputs for the Xbox 360 -> PC conversion. Read only; the layout follows the
public STFS documentation (Free60) and Velocity's reading of the hash tables.

All offsets are from the start of the package file unless said.

### STFS-HDR-1: Header fields the reader uses

Status: **verified** · Variants: Xbox 360

| Offset | Size | Field |
|---|---|---|
| 0x000 | 4 | magic: `LIVE`, `PIRS` or `CON ` |
| 0x340 | 4 | header size (u32 BE) |
| 0x379 | | the volume descriptor |
| 0x37B | 1 | block separation (bit 0: the package is read-only, one copy of each hash table; bit 1: which copy of the top table is live) |
| 0x37C | 2 | file-table block count (u16 **LE**) |
| 0x37E | 3 | file-table first block (u24 **LE**) |
| 0x395 | 4 | allocated block count (u32 BE): decides the number of hash-table levels |
| 0x411 | 0x80 | display name (UTF-16BE) |

A header shorter than 0x399 bytes is refused.

[test: rust/bc2-formats/src/stfs.rs::reads_a_package] [test: rust/bc2-formats/src/stfs.rs::refusals] [test: bfbc2-console-readers/tests/test_console_readers.py::main] [code: rust/bc2-formats/src/stfs.rs:150-176]
[doc: bfbc2-console-readers/stfs/stfs.py:7-16]

### STFS-BLK-1: Data blocks and interleaved hash tables

Status: **verified** · Variants: Xbox 360

Data blocks are 0x1000 bytes from the first 4 KiB boundary at or after the header size. A hash table precedes every
0xAA data blocks, a level-1 table every 0x70E4 blocks, a level-2 table above that; the levels used depend on the
allocated block count (<= 0xAA: one level; <= 0x70E4: two; else three). A package that is not read-only keeps two
copies of each table (each block of tables is doubled): the live copy of the top table is named by the block
separation bit 0x02, that of a lower table by status bit 0x40 of its entry in the table above. A data block number
maps to a file offset by adding the tables before it.

[test: rust/bc2-formats/src/stfs.rs::reads_a_package] [test: bfbc2-console-readers/tests/test_console_readers_native.py::main] [code: rust/bc2-formats/src/stfs.rs:196-231] [code: rust/bc2-formats/src/stfs.rs:239-255]

### STFS-HASH-1: Hash entries chain fragmented files

Status: **verified** · Variants: Xbox 360

A hash entry is 0x18 bytes: SHA-1 of the block (20 bytes), a status byte (bit 0x40: which copy of the table below is
live), the next block (u24 BE) of the file. A file whose blocks are not consecutive is read by following the chain.

[test: rust/bc2-formats/src/stfs.rs::reads_a_package] [code: rust/bc2-formats/src/stfs.rs:233-260] [code: rust/bc2-formats/src/stfs.rs:262-273]

### STFS-FT-1: File table entries

Status: **verified** · Variants: Xbox 360

The file table (its blocks read through STFS-HASH-1) is a list of 0x40-byte entries; the first entry whose name length
is 0 ends it.

| Offset | Size | Field |
|---|---|---|
| 0x00 | 0x28 | name (ASCII, its length in the flags) |
| 0x28 | 1 | flags: bits 0 - 5 the name length; 0x40 the blocks are consecutive; 0x80 a directory |
| 0x29 | 3 | block count (u24 LE) |
| 0x2F | 3 | first block (u24 LE) |
| 0x32 | 2 | parent entry index (u16 BE; 0xFFFF = the root) |
| 0x34 | 4 | size (u32 BE) |

A path is the names from the root down, joined by `/`.

[data: the 3 BFBC2 Xbox 360 update packages (2,533 MB, every entry read) and the 4 built packages, plus 1,500 damaged ones, give the reference's entries and bytes (BENCHMARKS F0)]
[test: rust/bc2-formats/src/stfs.rs::reads_a_package] [test: rust/bc2-formats/src/stfs.rs::names] [test: bfbc2-console-readers/tests/test_console_readers_native.py::main] [code: rust/bc2-formats/src/stfs.rs:275-310]
[doc: bfbc2-console-readers/stfs/stfs.py:14-16]

## Open questions

- The package header's signature, licence and content-metadata fields (not read: the suite never writes packages).
- Packages with more than two hash-table levels are handled by formula but none was seen.
- Whether the SHA-1s should be checked (the reader does not).
