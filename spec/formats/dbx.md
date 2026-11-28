# Binary DBX (`.dbx`)

A DBX file is one **partition**: Frostbite 1's serialised object data (blueprints, level data, weapons, kits, menus,
sounds' graphs ...). In the game every `.dbx` bundle entry is binary: a header, a string table and a tree of tagged
elements whose values are string indexes, big-endian numbers or two-byte scalars. This file specifies the bytes and
the suite's two text forms of them (the XML DbxEdit writes and imports); what the elements mean - the tag vocabulary,
instances, references, value types - is the data model (`model/types.md`, `model/references.md`).

The suite's readers: `rust/bc2-formats/src/dbx` (the reader, the writer, the XML; ported from `core/bfbc2core/dbx.py`,
its answers kept in `core/tests/dbx_golden*.json`), `rust/bc2-formats/src/dbx/edit.rs` (an editable tree written
back byte for byte) and `rust/bc2-formats/src/dbxedit` (DbxEdit's model, commands, export and import, ported from
DbxEdit.cs). The older description of the same format and DbxEdit's behaviour is `bfbc2-dbx-tools/FORMAT.md`; this
file supersedes it as the normative text and adds the corpus facts.

Corpus counts: every `.dbx` entry of the game copy's 1,086 bundles, **156,533 files, 1.70 GB**, surveyed in round
SP1 (byte structure only: counts of kinds, sizes and header values, nothing kept).

A *varint* is an unsigned LEB128 number: 7-bit groups, least significant first, the high bit of each byte set when
another byte follows.

## 1. Header

| Offset | Size | Content |
|---|---|---|
| 0 | 8 | ASCII `{binary}` |
| 8 | 4 | BE u32 the body's offset: 24 + *A* |
| 12 | 4 | BE u32 a value DbxEdit keeps (always 0) |
| 16 | 4 | BE u32 *A*: the string area's size, 4 × *N* + the string bytes |
| 20 | 4 | BE u32 *N*: the string count |
| 24 | 4 × *N* | BE u32 each string's offset, from the end of this table (24 + 4*N*) |
| 24 + 4*N* | *A* - 4*N* | the strings: UTF-8, each ended by a zero byte |
| 24 + *A* | to the end | the body: the element tree (section 3) |

### DBX-HDR-1: Magic and minimum size

Status: **verified** · Variants: PC, Xbox 360, BF1943

A binary DBX starts with the eight ASCII bytes `{binary}` and is longer than 24 bytes. A `.dbx` without them is
not binary: the suite reads it as plain XML (DbxEdit's own text) or refuses it. In the game every `.dbx` entry is
binary.

Evidence: [code: rust/bc2-formats/src/dbx/strings.rs:9-15] [code: rust/bc2-formats/src/dbx/tree.rs:111-113] [test: rust/bc2-formats/src/dbx/strings.rs::is_binary_needs_magic_and_header] [test: rust/bc2-formats/src/dbx/reader.rs::errors] [data: 156,533 of 156,533 `.dbx` entries binary]

### DBX-HDR-2: An empty file is a marker, not a partition

Status: **verified**

A `.dbx` of zero bytes is not a damaged partition: it is a `*deleted*` marker's data or an emptied file
(`model/bundles.md`, PART-EMPTY-1). The reader answers it with its own error (`Deleted`), distinct from damage.

Evidence: [code: rust/bc2-formats/src/dbx/tree.rs:108-110] [test: rust/bc2-formats/src/dbx/reader.rs::read_from_disk] [test: rust/bc2-formats/src/dbx/reader.rs::errors]

### DBX-HDR-3: The body offset

Status: **verified**

The word at 8 is 24 + *A*, the body's offset. The suite's reader does not read it - it starts the body at 24 + *A*
from the word at 16 - and both writers write it as 24 + *A*.

Evidence: [data: word 8 = 24 + word 16 in 156,533 of 156,533 files] [code: rust/bc2-formats/src/dbx/strings.rs:32-54] [code: rust/bc2-formats/src/dbx/writer.rs:207-214] [code: rust/bc2-formats/src/dbxedit/file.rs:255-255] [test: rust/bc2-formats/tests/spec_corpus.rs::dbx_headers_and_strings] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: word 8 = 24 + word 16 in 124,458 of 124,458 `.dbx` entries]

### DBX-HDR-4: The kept word

Status: **verified**

The word at 12 is 0 in every file. Its meaning is unknown; DbxEdit keeps it as it is and the suite's own writer
writes 0.

Evidence: [data: 0 in 156,533 of 156,533 files] [code: rust/bc2-formats/src/dbxedit/file.rs:246-246] [code: rust/bc2-formats/src/dbx/writer.rs:209] [test: rust/bc2-formats/tests/spec_corpus.rs::dbx_headers_and_strings] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: 0 in 124,458 of 124,458]

## 2. String table

### DBX-STR-1: Offsets and strings

Status: **verified**

String *i* starts at 24 + 4*N* + offset *i* and runs to its zero byte. Strings are UTF-8; the reader replaces an
invalid byte with U+FFFD. The reader refuses a string without a zero before the end of the file; the XML export
tolerates one and reads it to the end of the file.

Evidence: [code: rust/bc2-formats/src/dbx/strings.rs:31-55] [test: rust/bc2-formats/src/dbx/strings.rs::reads_strings_and_body_offset] [test: rust/bc2-formats/src/dbx/strings.rs::missing_terminator] [test: rust/bc2-formats/src/dbx/strings.rs::invalid_utf8_replaced] [data: every string of the 156,533 files valid UTF-8]

### DBX-STR-2: String 0 is the empty string

Status: **verified**

String 0 is the empty string in every file. It must be: a tag index of 0 closes the open container (DBX-ELEM-2), so
string 0 cannot name an element, and a writer starts its table with `""`.

Evidence: [data: 156,533 of 156,533 files] [code: rust/bc2-formats/src/dbx/writer.rs:86-90] [code: rust/bc2-formats/src/dbxedit/file.rs:246-254] [test: rust/bc2-formats/tests/spec_corpus.rs::dbx_headers_and_strings] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: 124,458 of 124,458]

### DBX-STR-3: DICE's tables are packed but not deduplicated

Status: **verified**

In DICE's files the offsets ascend and the strings are packed: each starts right after the previous one's zero, and
*A* = 4*N* + the strings' bytes exactly. The table may hold one string twice and strings no element uses, and its
order is not the order of first use in the tree. A writer may therefore not assume unique strings; an index, not the
string's text, is what an element stores.

Evidence: [data: offsets ascending and packed in 156,533 of 156,533 files; 44,510 files with a string twice; 4,756 with an unused string; 305 in first-use order] [test: rust/bc2-formats/tests/spec_corpus.rs::dbx_headers_and_strings] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: packed in 124,458 of 124,458; a string twice in 34,819; an unused string in none and first-use order in none (the game copy's other counts came from OPFOR-written files)]

### DBX-STR-4: Indexes must be in range

Status: **verified**

Every string index the tree uses (tag, attribute name and value, text value) is less than *N*; an index past the
table is damage and the reader refuses the file.

Evidence: [code: rust/bc2-formats/src/dbx/reader.rs:10-16] [test: rust/bc2-formats/src/dbx/reader.rs::errors]

## 3. Element tree

Each element is:

1. **tag**: a varint string index (`partition`, `instance`, `field`, `complex`, `array`, `item`); 0 is not a tag but
   closes the open container;
2. **head**: one byte, the kind in the high 4 bits, the attribute count in the low 4 bits;
3. **attributes**: that many (name, value) pairs of varint string indexes, in order;
4. **value**, by kind:

| Kind | Name | Value bytes |
|---|---|---|
| 0xA | container | one byte (always 0), then child elements up to a closing 0 |
| 0x2 | text | a varint string index |
| 0x7 | numbers | varint count *n*, varint size *s*, then *n* × *s* bytes, each value big-endian |
| any other | scalar | two bytes, the value in the second (in the game: kind 6 only) |

### DBX-VAR-1: Varints

Status: **verified**

Varints are unsigned LEB128. DICE's files always use the shortest form; the suite's writers write the shortest form.
The reader takes any length; a value past 64 bits saturates and then fails as an index or count. (DbxEdit's own model
reads a varint as a C# 32-bit `int`, shifts wrapping, as DbxEdit.cs did.)

Evidence: [code: rust/bc2-formats/src/dbx/cursor.rs:49-72] [code: rust/bc2-formats/src/dbx/writer.rs:23-25] [code: rust/bc2-formats/src/dbxedit/file.rs:27-40] [test: rust/bc2-formats/src/dbx/writer.rs::varints] [test: rust/bc2-formats/src/dbxedit/file.rs::varints_as_dbxedit] [data: no varint of the 156,533 files has a redundant trailing group]

### DBX-ELEM-1: The head byte

Status: **verified**

The head byte's high nibble is the kind, its low nibble the attribute count (so at most 15 attributes). A writer
refuses an element with more than 15 attributes, a kind above 15, or the empty tag. In the game an element has 0 to
3 attributes.

Evidence: [code: rust/bc2-formats/src/dbx/tree.rs:136-143] [code: rust/bc2-formats/src/dbx/writer.rs:125-134] [test: rust/bc2-formats/src/dbx/writer.rs::refuses_unwritable_elements] [data: attribute counts over 81.3 million elements: 0 3,349,462; 1 60,398,557; 2 17,231,281; 3 313,065]

### DBX-ELEM-2: Containers and the closing zero

Status: **verified**

A container's head is followed by one byte (DbxEdit keeps it; 0 in every file and written as 0), then its children,
then a varint 0 that closes it. Children keep their file order. A container left open at the end of the file is
damage.

Evidence: [code: rust/bc2-formats/src/dbx/tree.rs:126-149] [code: rust/bc2-formats/src/dbx/tree.rs:186-188] [test: rust/bc2-formats/src/dbx/reader.rs::nested_containers_keep_order] [test: rust/bc2-formats/src/dbx/reader.rs::errors] [data: container byte 0 in all 10,056,595 containers]

### DBX-ELEM-3: One root, then the end

Status: **verified**

The body holds one root element, the `partition` container; a 0 where a tag would start at the top level ends the
tree, and bytes after it are kept by DbxEdit as a trailer. DICE's files end exactly at the partition's closing 0.

Evidence: [code: rust/bc2-formats/src/dbx/tree.rs:127-129] [code: rust/bc2-formats/src/dbxedit/file.rs:255] [test: rust/bc2-formats/src/dbx/reader.rs::stops_at_a_zero_tag_with_nothing_open] [data: one root, `partition`, in 156,533 of 156,533 files; no bytes after the tree in any]

### DBX-ELEM-4: Text values

Status: **verified**

A text element (kind 2) stores a varint string index; the empty string is a valid value. References, enum names,
names and paths are text values (`model/types.md`).

Evidence: [code: rust/bc2-formats/src/dbx/tree.rs:151] [test: rust/bc2-formats/src/dbx/reader.rs::tree] [data: 26,812,339 text elements]

### DBX-ELEM-5: Scalars

Status: **verified**

Every kind other than 0xA, 2 and 7 is a two-byte scalar; the value is the second byte. In the game the only such kind
is 6 (booleans and bytes), its first byte is always 1, and its second is 0 or 1 except for 2,992 byte values of 2
or more. A file whose last element is a scalar with fewer than two bytes left is damage.

Evidence: [code: rust/bc2-formats/src/dbx/tree.rs:168-175] [test: rust/bc2-formats/src/dbx/reader.rs::tree] [test: rust/bc2-formats/src/dbx/writer.rs::bodies_follow_the_kind] [data: 11,796,795 kind-6 scalars: first byte 1 in all; second byte 0 7,242,287, 1 4,551,516, 2 or more 2,992; no other scalar kind]

### DBX-ELEM-6: Only four kinds occur

Status: **verified**

The game's files use kinds 0xA, 2, 7 and 6 only.

Evidence: [data: kinds over 81.3 million elements: 7 32,626,636; 2 26,812,339; 6 11,796,795; 0xA 10,056,595; no other] [test: rust/bc2-formats/tests/spec_corpus.rs::dbx_elements] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: kinds over 66.8 million elements: 7 27,177,620; 2 21,857,917; 6 9,575,434; 0xA 8,222,821; no other]

### DBX-ELEM-7: Nesting depth

Status: **verified**

The game's partitions nest at most 9 containers deep (3 in most). DbxEdit's model refuses nesting past 256.

Evidence: [data: deepest container per file: 2 29,767 files, 3 99,141, 4 14,651, 5 9,655, 6 3,311, 7-9 8] [code: rust/bc2-formats/src/dbxedit/file.rs:21] [test: rust/bc2-formats/tests/spec_corpus.rs::dbx_elements] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: deepest container per file: 2 22,648; 3 80,799; 4 10,852; 5 7,478; 6 2,678; 7, 8 and 9 one each]

## 4. Numbers

### DBX-NUM-1: Number lists

Status: **verified**

A numbers element (kind 7) stores a varint count *n*, a varint size *s* and *n* × *s* bytes; every value is
big-endian, on the PC as on the consoles. *n* may be 0. A list that passes the end of the file is damage.

Evidence: [code: rust/bc2-formats/src/dbx/tree.rs:152-167] [code: rust/bc2-formats/src/dbx/number.rs:244-255] [test: rust/bc2-formats/src/dbx/reader.rs::errors] [test: rust/bc2-formats/src/dbx/reader.rs::huge_counts_are_errors] [data: 32,626,636 number lists; 186,480 with count 0]

### DBX-NUM-2: Sizes 4, 2 and 8

Status: **verified**

Values are 4 bytes (a 32-bit float or integer: the bytes do not say which, `model/types.md` TYPE-NUM-1), 2 bytes
(a 16-bit integer) or 8 bytes (a 64-bit value, printed as a double). Size 4 is on `field`, `complex` and `array`
elements, sizes 2 and 8 only on `field`s.

Evidence: [data: values of size 4 in 24,578,687 lists, size 2 in 8,047,646, size 8 in 303; size 4 on field 20,298,193, complex 3,963,739, array 316,755] [code: rust/bc2-formats/src/dbx/number.rs:189-213] [test: rust/bc2-formats/tests/spec_corpus.rs::dbx_elements] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: size 4 on field 16,882,127, complex 3,273,333, array 104,332; size 2 on field 6,766,214; size 8 on field 240; no other]

### DBX-NUM-3: Floats are IEEE 754, big-endian

Status: **verified**

A 4-byte float is an IEEE 754 single, an 8-byte value a double, both big-endian. Vectors and transforms are lists of
4-byte floats (`model/types.md` TYPE-CPLX-1, TYPE-CPLX-2).

Evidence: [code: rust/bc2-formats/src/dbx/number.rs:286-296] [test: rust/bc2-formats/src/dbx/number.rs::values_and_floats] [test: rust/bc2-formats/src/dbx/number.rs::singles_and_doubles]

## 5. The XML form

The suite's readable text of a binary DBX, as DbxEdit (`DbxEdit xml`) and `dbx.to_xml` write it. It is for reading
and for DbxEdit's import (section 7); the binary stays the reference.

### DBX-XML-1: Lines and indentation

Status: **verified**

UTF-8 without a byte-order mark, lines ended CR LF. The first line is `<?xml version="1.0"?>`. Each element starts a
line indented by one tab per open container: `<tag` and ` name="value"` per attribute in file order; a container
`<tag ...>` with its children on the following lines and `</tag>` at its own indentation; any other element on one
line, `<tag ...>value</tag>`, or `<tag ... />` for an empty text. `&`, `<`, `>` and `"` are escaped as `&amp;`,
`&lt;`, `&gt;`, `&quot;` in attribute values and text.

Evidence: [code: rust/bc2-formats/src/dbx/xml.rs:12-27] [code: rust/bc2-formats/src/dbx/xml.rs:54-140] [test: rust/bc2-formats/src/dbx/xml.rs::escaping] [test: rust/bc2-formats/tests/corpus.rs::corpus_xml_matches_dbxedit] [test: core/tests/test_dbx_native.py::run_corpus]

### DBX-XML-2: Values

Status: **verified**

A text element shows its string; a scalar `false` (0), `true` (1) or its second byte as a number; a number list its
values joined by `/` (an empty list shows nothing between the tags).

Evidence: [code: rust/bc2-formats/src/dbx/xml.rs:98-133] [code: rust/bc2-formats/src/dbx/number.rs:230-236] [test: rust/bc2-formats/tests/robustness.rs::xml_shows_the_values] [test: rust/bc2-formats/src/dbx/number.rs::number_texts]

### DBX-XML-3: A 4-byte value prints as an integer or a float

Status: **verified**

A 4-byte value prints as a whole number (the bytes read as a signed 32-bit integer) when the element's first
attribute value is one of `OriginalHashedWaveName`, `HashedName`, `HashedWaveName`, `OnRoadMaterialNameHashes`,
`Hash`, `Id`, `CompositeMeshPartNames`; or when its top byte is 0; or when its eight exponent bits are all ones.
Otherwise it prints as a single in .NET's round-trip form: `G7`, or `G9` when 7 digits do not read back as the same
single. An 8-byte value prints as a double in round-trip form (`G15`, else `G17`; `NaN`, `Infinity`, `-Infinity`).
Any other size prints its first two bytes as an unsigned 16-bit number.

Evidence: [code: rust/bc2-formats/src/dbx/number.rs:9-18] [code: rust/bc2-formats/src/dbx/number.rs:188-213] [code: rust/bc2-formats/src/dbx/number.rs:91-172] [test: rust/bc2-formats/src/dbx/number.rs::number_texts] [test: rust/bc2-formats/src/dbx/number.rs::g_formatting] [test: rust/bc2-formats/src/dbx/number.rs::ties_and_near_ties]

### DBX-XML-4: The XML loses information

Status: **verified**

The XML does not show a value's kind (text, numbers or scalar), a number's size, whether a 4-byte value is an integer
or a float, the container and scalar first bytes, the kept header word, a trailer, or the string table's order and
duplicates. A tiny float (top byte 0: magnitudes below about 2.4e-38, denormals included) prints as an integer, so
`7.3e-40` reads back as `524288`; a 4-byte integer with an exponent pattern prints as a float. A tool that needs
exact values reads the binary; DbxEdit's import recovers types from a reference file (DBX-IMP-1).

Evidence: [code: rust/bc2-formats/src/dbx/number.rs:195-200] [test: rust/bc2-formats/src/dbx/number.rs::number_texts] [test: rust/bc2-formats/tests/robustness.rs::one_byte_numbers_show_two_bytes_in_the_xml] [doc: bfbc2-dbx-tools/FORMAT.md:126-137]

## 6. Writing

### DBX-WR-1: The suite's writer

Status: **verified**

`dbx::write` writes a tree with a fresh string table: the empty string first, then each string in the order the tree
first uses it (tag, attribute names and values, text), each once; header word 12 and every container byte 0. It is
not DICE's string order (DBX-STR-3), so its bytes differ from a game file's, but `parse(write(f))` gives `f`'s tree
back for any tree it accepts.

Evidence: [code: rust/bc2-formats/src/dbx/writer.rs:1-7] [code: rust/bc2-formats/src/dbx/writer.rs:96-107] [code: rust/bc2-formats/src/dbx/writer.rs:203-219] [test: rust/bc2-formats/src/dbx/writer.rs::round_trip_keeps_the_tree] [test: rust/bc2-formats/tests/robustness.rs::write_then_read_keeps_the_tree] [test: rust/bc2-formats/tests/writers.rs::writer_calls_agree] [test: core/tests/test_dbx_write_native.py::edge_trees]

### DBX-WR-2: The byte-keeping writers

Status: **verified**

DbxEdit's model (`dbxedit::file`) and the editable tree (`dbx::edit`) write a file back byte for byte when nothing
changed: the original string area (offsets and order included) unless a string was added or changed, the kept header
word, each container's byte, every value's bytes, and the trailer. When the table changed, it is rebuilt: every
string in table order (unused ones included) with its zero, offsets from the start of the string bytes. A string an
edit needs reuses the first identical entry or is appended.

Evidence: [code: rust/bc2-formats/src/dbxedit/file.rs:255-255] [code: rust/bc2-formats/src/dbxedit/file.rs:208-211] [test: rust/bc2-formats/src/dbxedit/file.rs::empty_file_round_trips] [test: rust/bc2-formats/src/dbxedit/file.rs::interning_reuses_the_first_entry] [test: rust/bc2-formats/src/dbx/edit.rs::unchanged_files_come_back] [test: core/tests/test_dbx_write_native.py::check_dbxedit] [doc: bfbc2-dbx-tools/FORMAT.md:73-84]

### DBX-WR-3: What the game requires of a written file

Status: **inferred**

Files written by the suite's writers load in the game (OPFOR's installed levels, the Pacific War and Vietnam ports,
the map exporter's levels all carry suite-written partitions). So the game needs neither DICE's string order nor a
deduplicated table; it reads strings through the offsets. What it does require beyond a well-formed tree (DBX-HDR-1
to DBX-NUM-1) is the data model's business: resolvable references, the expected field kinds and sizes per class.

Evidence: [inferred: suite-written partitions with first-use string tables are in every level OPFOR installs and plays; DICE's own tables are in neither first-use nor sorted order (DBX-STR-3), so no order can be required]

## 7. DbxEdit's XML import

### DBX-IMP-1: Types come from a reference file

Status: **verified**

DbxEdit's import turns its XML back into binary using a reference file (normally the `.dbx` the XML came from) for
everything the XML lost (DBX-XML-4). Each XML element is matched to a reference sibling by `guid`, else `name`, else
`ref`, else all attributes, in order, each reference element used once. Unchanged values keep the reference's bytes
and string indexes, the reference's table is kept and new strings appended, so an unedited XML gives the original
file byte for byte. A changed 4-byte token is read as the integer or the float that would print as exactly that
token; when both would, the reference value's reading decides. New elements take kind and size from the same field
in the reference or the `--types` files, else the XML's shape decides and the guess is reported (exit code 3).

Evidence: [code: rust/bc2-formats/src/dbxedit/import.rs:1-8] [test: rust/bc2-formats/src/dbxedit/import.rs::reader_keeps_what_dbxedit_keeps] [test: core/tests/test_dbxedit.py::quick] [test: core/tests/test_dbxedit.py::golden_edits] [doc: bfbc2-dbx-tools/FORMAT.md:124-159]

### DBX-IMP-2: New files' defaults

Status: **verified**

A file imported without a reference gets header word 12 = 0, container bytes 0, scalar first bytes 1 and no trailer:
the values every game file has (DBX-HDR-4, DBX-ELEM-2, DBX-ELEM-5, DBX-ELEM-3).

Evidence: [code: rust/bc2-formats/src/dbxedit/file.rs:246-254] [test: rust/bc2-formats/src/dbxedit/file.rs::empty_file_round_trips] [doc: bfbc2-dbx-tools/FORMAT.md:157]

## Worked example

The reader's own test file (`rust/bc2-formats/src/dbx/reader.rs`, test `tree`), strings
`["", "partition", "guid", "g", "field", "name", "x"]`, body:

```
01 A1 02 03 00          partition (kind A, 1 attribute: guid="g"), container byte 0
04 21 05 06 03          field name="x", text "g"
04 61 05 06 00 01       field name="x", kind 6 scalar: true
04 71 05 06 01 04 3F 80 00 00
                        field name="x", numbers: count 1, size 4, 1.0f big-endian
00                      closes the partition
```

Its XML (tabs shown as four spaces):

```
<?xml version="1.0"?>
<partition guid="g">
    <field name="x">g</field>
    <field name="x">true</field>
    <field name="x">1</field>
</partition>
```

The last line shows DBX-XML-4: 1.0f prints as `1`, the same text as the integer 1.

## Open questions

- What header word 12 is (0 everywhere) and whether the game reads the body-offset word at 8.
- What the container byte and the scalar's first byte encode (always 0 and 1); whether another scalar kind than 6
  exists in any Frostbite 1 game (BF1943's files have not been surveyed for this).
- How DICE ordered the string table (neither first use nor sorted) and why it keeps duplicates and unused strings
  (the likely answer: a per-build string pool, cut per partition).
- The game's limits: the largest partition, string count and depth it loads.
- Whether the game checks a field's kind and size against its class (a float written where an int is expected),
  or reads the bytes as its class says: the class schema (`model/types.md` TYPE-SCH-1) is in the executable's type
  registry (read by round SPG4-rest, `engine/typeinfo/`), but no check of the loader against it has been made.
