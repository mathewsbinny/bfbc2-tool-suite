# Granny resources: GrannyModel, GrannyAnimation, DeltaAnimation

Skeletons (FBRB type `GrannyModel`) and animations (`GrannyAnimation`, `DeltaAnimation`) are RAD Game Tools' Granny 2
files inside a DICE wrapper. The suite reads them (`rust/bc2-formats/src/granny.rs`, from `bfbc2-anim-tools/granny.py`,
whose docstring describes the format), converts Xbox 360 ones to PC (`rust/bc2-convert/src/granny.rs`), and edits PC
ones (`rust/bc2-convert/src/granny_edit.rs`: tracks added, orientations set). This file covers what the suite relies
on; Granny's own object model is RAD's and is described only as far as the suite reads it.

PC files are little-endian; Xbox 360 files (BFBC2's console copy, BF1943) are the same format big-endian throughout.

### GRANNY-WRAP-1: DICE's wrapper

Status: **verified** · Variants: PC, Xbox 360, BF1943

| Field | Type |
|---|---|
| version | u32: 10 |
| DICE's per-bone table | u32 words (GRANNY-WRAP-2) |
| string table | u32 count, then count x {u16 length, that many bytes} |
| Granny size | u32 |
| the Granny 2 file | (GRANNY-FILE-1) |

The byte order is the one in which the first word reads 10. The table's length is not stored: the string table starts
where the words that follow parse as a string table; a reader that cannot tell looks for the string table's first
string, the 6-byte `<file>`, behind its u16 length.

Evidence: [code: rust/bc2-formats/src/granny.rs:316-346] [test: rust/bc2-formats/src/granny.rs::refuses_what_is_not_granny] [test: bfbc2-anim-tools/tests/test_granny_native.py::quick] [doc: bfbc2-anim-tools/granny.py:30-34]

### GRANNY-WRAP-2: The per-bone table

Status: **verified** · Variants: PC, Xbox 360, BF1943

The words between the version and the string table, by resource kind:

| Kind | Words |
|---|---|
| skeleton | none |
| animation | u32 n, then n records {u32 bone, u32 k, k x (f32 start, f32 end)} |
| delta animation | u32 n, n x u32 bone index, then a list as for an animation |

Bone indices are in the skeleton's order. A port onto another skeleton must renumber them by bone name, each list
sorted by the new index.

Evidence: [code: rust/bc2-formats/src/granny.rs:776-821] [code: rust/bc2-formats/src/granny.rs:823] [test: rust/bc2-formats/src/granny.rs::tables] [test: bfbc2-anim-tools/tests/test_granny_native.py::quick]

### GRANNY-WRAP-3: Names are string-table indices

Status: **implemented** · Variants: PC, Xbox 360, BF1943

DICE stores every name in the Granny data - member names of type definitions and string values - as an index into the
wrapper's string table, not a pointer. An index past the table reads as no string. New strings an edit needs are
appended to the table.

Evidence: [code: rust/bc2-formats/src/granny.rs:337-346] [code: rust/bc2-convert/src/granny_edit.rs:1] [doc: bfbc2-anim-tools/granny.py:35-36]

### GRANNY-FILE-1: The Granny 2 file

Status: **verified** · Variants: PC, Xbox 360, BF1943

| Offset (in the Granny file) | Field |
|---|---|
| 0 | 16-byte magic: PC `29DE6CC0 BAA4532B 25F5B7A5 F666E2EE`, big-endian `B595110E 4BB5A56A 502828EB 04B37825` |
| 32 | u32 format revision: 7 |
| 36 | u32 total size |
| 44 | u32 section table offset (from byte 32) |
| 48 | u32 section count |
| 52 | root type reference (section, offset) |
| 60 | root object reference (section, offset) |
| 32 + table | per section 44 bytes: u32 compression (must be 0), u32 data offset, u32 data size, ..., u32 relocation offset (word 7), u32 relocation count (word 8), ... |

A relocation is 12 bytes: offset in its section, target section, target offset. The CRC covers the file from 32 +
the section table offset; a writer recomputes it.

Evidence: [code: rust/bc2-formats/src/granny.rs:38-45] [code: rust/bc2-formats/src/granny.rs:347-381] [test: rust/bc2-formats/src/granny.rs::refuses_what_is_not_granny] [test: bfbc2-anim-tools/tests/test_granny.py::crc_ok] [doc: bfbc2-anim-tools/granny.py:30-34]

### GRANNY-TYPE-1: Member types

Status: **verified** · Variants: PC, Xbox 360, BF1943

A type definition is a list of 32-byte member definitions (type, name, child definition, array size, 4 x u32). Member
types and their sizes: 1 inline struct, 2 reference (4), 3 reference to array (count, pointer: 8), 4 array of
references (8), 5 variant reference (type, pointer: 8), 7 reference to variant array (12), 8 string (4), 9 transform
(68: flags, position 3, orientation 4, scale / shear 9 floats), 10 real32 (4), 11 - 14 8-bit (1), 15 - 18 16-bit (2),
19 - 20 32-bit integers (4), 21 real16 (2), 22 empty reference (4).

Evidence: [code: rust/bc2-formats/src/granny.rs:89-106] [test: rust/bc2-formats/src/granny.rs::halves] [test: bfbc2-anim-tools/tests/test_granny_native.py::game] [doc: bfbc2-anim-tools/granny.py:34-38]

### GRANNY-CURVE-1: Animation curves

Status: **implemented** · Variants: PC, Xbox 360, BF1943

A track group's transform tracks hold orientation, position and scale-shear curves, each a variant whose type name
gives its encoding: identity, constant (3 or 4 floats), float knots and controls, quantized knots and controls (value =
control x scale + offset, time = knot / one-over-knot-scale, that scale stored as its top 16 bits), one control per
knot for three components, and quaternions with 3 stored components of 15 or 7 bits (the dropped one recomputed, which
and its sign in the top bits; scales and offsets from a 16-entry table chosen by a u16 selector). The suite evaluates
curves at their knots.

Evidence: [code: bfbc2-anim-tools/granny.py:401-458] [code: bfbc2-anim-tools/granny.py:601] [test: bfbc2-anim-tools/tests/test_granny.py::tracks] [doc: bfbc2-anim-tools/granny.py:46-53]

### GRANNY-CONV-1: Xbox 360 -> PC

Status: **verified** · Variants: Xbox 360 -> PC

A console resource converts in place of its layout: the wrapper's table and string table swapped field by field (the
table optionally renumbered onto another skeleton), the Granny header, section table, relocations and marshalling
lists as u32, each section's 32-bit part as u32 and its 16-bit part as u16, then every object of the mixed-marshalling
list member by member by its type (8-bit members as they are); the CRC recomputed.

Evidence: [code: rust/bc2-convert/src/granny.rs:103-196] [test: rust/bc2-convert/src/granny.rs::minimal_resource] [test: rust/bc2-convert/src/granny.rs::swaps_like_python_slices] [test: bfbc2-anim-tools/tests/test_granny.py::big_endian] [data: all 2,654 resources of BFBC2's Xbox 360 mp_common / sp_common with a same-size PC twin: the table, strings and every object value match the PC file, Granny's unused padding members aside (doc)] [doc: bfbc2-anim-tools/granny.py:22-27]

### GRANNY-EDIT-1: Adding tracks to a PC animation

Status: **verified** · Variants: PC

New objects are appended to the sections, so no pointer or marshalling entry moves; the track group gets a new,
name-ordered transform-track array; curve objects and type definitions the file lacks are copied from a donor PC
animation (their strings into the string table); the sections are laid out again and the CRC recomputed.

Evidence: [code: rust/bc2-convert/src/granny_edit.rs:1] [test: bfbc2-anim-tools/tests/test_granny.py::add_tracks] [doc: bfbc2-anim-tools/granny.py:40-44]

## Open questions

- Compressed Granny sections (compression type non-zero) are refused; none have been seen, but the game's support is
  unknown.
- The meaning of the per-bone table's intervals (start, end) for the engine (streaming, blending?).
- The 4 x u32 at the end of a member definition and section-table words not listed above are read but not
  interpreted.
- Curves of degree above 1 are sampled at knots only, not evaluated between them.
