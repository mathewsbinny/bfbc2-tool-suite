# Localization tables

The game's texts are binary tables, one set per language: `Localization/bin/language_<lang>_normal.bin` (the menu
and HUD texts), `_huge.bin` (a few long texts), `_numbers.bin`, and `language_<lang>_histogram.bin` (the character
table the texts are encoded with). They are non-resource entries (type `<non-resource>`, extension `.bin`) of
`Dist\win32\overlay-00.fbrb`. Texts are found by the hash of their key; keys themselves are not stored.

The suite reads and writes them with `bfbc2-menu-tools/bc2loc.py` and `bc2_formats::loc`.

## The text file

### LOC-HDR-1: Text file header

Status: **verified** · Variants: PC

Little-endian:

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | magic 0x00039000 |
| 4 | 4 | file size - 8 |
| 8 | 4 | entry count *n* |
| 12 | 4 | entry table offset, from byte 8 |
| 16 | 4 | string offset, from byte 8 |
| 20 | 16 | name: ASCII, zero-padded (`NORMAL`, `HUGE` or `NUMBERS`) |
| 8 + table | 8 × *n* | entries (LOC-ENT-1) |
| 8 + strings | | the texts, each ended by a zero byte |

A file whose magic differs, that is shorter than 36 bytes, whose size field is not its length - 8, or whose table
would pass its end is not read.

[data: 17 text files of the game copy (8 languages x normal / huge, 1 numbers): every size field = length - 8, table offset 28 (the table right after the name), string offset = table offset + 8n, the file ending with a zero byte]
[test: rust/bc2-formats/src/loc.rs::reads_and_writes] [test: bfbc2-menu-tools/tests/test_bc2loc_native.py::main] [code: rust/bc2-formats/src/loc.rs:98-131]
[doc: bfbc2-menu-tools/bc2loc.py:16-20]

### LOC-ENT-1: Entries: hash and text offset, sorted by hash

Status: **verified** · Variants: PC

Each entry is `{u32 hash, u32 text offset}`, the offset counted from the string offset. Entries are sorted by hash
ascending, each hash once. The texts are not in hash order. A text runs from its offset to the next zero byte (or the
end of the file).

[data: 17 of 17 text files sorted by hash with unique hashes; in 16 of them the text offsets are not ascending]
[test: rust/bc2-formats/src/loc.rs::reads_and_writes] [code: rust/bc2-formats/src/loc.rs:113-125]

### LOC-KEY-1: Key hash

Status: **verified** · Variants: PC

A key's hash is `h = 0xFFFFFFFF`, then `h = h * 33 + byte` for each byte of the key, modulo 2^32 (the key's
characters as Latin-1 bytes).

[test: rust/bc2-formats/src/loc.rs::hashes] [code: rust/bc2-formats/src/loc.rs:53-56]
[doc: bfbc2-menu-tools/bc2loc.py:20-21]

### LOC-KEY-2: PC texts are stored under WIN32_ keys

Status: **observed** · Variants: PC

A PC-specific text is stored under `WIN32_<key>`, the others under the key itself; the PC game looks up the
`WIN32_` form first.

[data: the PC files hold WIN32_ forms of the controller and server browser texts; bc2loc's get finds them first as the PC game shows them]
[code: bfbc2-menu-tools/bc2loc.py:205]
[doc: bfbc2-menu-tools/bc2loc.py:20-21]

## The histogram and the text encoding

### LOC-HIST-1: Histogram file

Status: **verified** · Variants: PC

Little-endian: u32 magic 0x00039001, u32 file size - 8, u32 count, then u16 table entries to the end of the file.

[data: 8 histograms of the game copy, each 3,072 u16 entries; the count word 256 in 7 and 1,490 in the Japanese one]
[test: rust/bc2-formats/src/loc.rs::decodes] [code: rust/bc2-formats/src/loc.rs:137-142]
[doc: bfbc2-menu-tools/bc2loc.py:22-23]

### LOC-ENC-1: Text decoding

Status: **verified** · Variants: PC

A text is a byte string. A byte below 0x80 is ASCII. A byte c >= 0x80: if `table[c]` >= 0x80 it is that character;
otherwise it starts a two-byte character, `table[table[c] * 128 + (next byte & 0x7F)]` (Japanese); an entry of 0 there
is not a character. Without a histogram, bytes >= 0x80 cannot be decoded.

[test: rust/bc2-formats/src/loc.rs::decodes] [test: bfbc2-menu-tools/tests/test_bc2loc_native.py::main] [code: rust/bc2-formats/src/loc.rs:147-180]
[doc: bfbc2-menu-tools/bc2loc.py:22-25]

## Writing

### LOC-WR-1: Writing a text file

Status: **verified** · Variants: PC

The suite writes: the header (magic, size, count, table offset 28, string offset 28 + 8n), the original name field,
the entries sorted by hash, then the original string bytes with each new or changed text appended (zero-terminated)
and its entry pointing there. Unchanged texts keep their bytes; with nothing changed the file is byte-identical.
New texts are ASCII only (those bytes mean the same in every language's encoding).

[data: every language file of the game copy written back identical (test_bc2loc_write, game tier); 59,177 texts of 25 files identical in Rust and Python]
[test: rust/bc2-formats/src/loc.rs::reads_and_writes] [test: bfbc2-menu-tools/tests/test_bc2loc_write.py::main] [code: rust/bc2-formats/src/loc.rs:188-221]

## Open questions

- What `_huge` (47 entries a language) and `_numbers` (1 entry) are used for, and when the game reads them.
- The localization `.dbx` files beside them (huge, loc, numbers partitions in `startup` and the font bundles): how
  they name the binary tables and fonts.
- Whether the game requires the entry table sorted (it is in every file; a binary search is likely).
- How the game picks the language and falls back when a key is missing.
