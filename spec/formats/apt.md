# APT movies (SwfMovie resources)

The menus, HUDs and loading screens of BFBC2 are EA "APT" movies: Flash content compiled for EA's own runtime (not
Scaleform). A movie ships as a `SwfMovie` resource: a BIGF archive ([bigf.md](bigf.md)) of three members,
`<name>.apt` (the movie: characters, timelines, byte code), `<name>.const` (the constant table) and `<name>.geo`
(vector geometry). Image pixels live outside the movie, in the menu's texture atlas.

The suite reads movies with `apt.py` / `bc2_formats::apt` (the dump and the instruction records), writes them with
`aptgraft.py` / `bc2_formats::aptmodel` (the node model, the `keep` and `pack` layouts, either byte order), and checks
them with `aptcheck.py` / `bc2_formats::aptscan` (the loader's rules). The knowledge comes from OpenSAGE's APT reader,
inspection of BC2's files and OPFOR's crash investigations.

Conventions in this file: every field is a 32-bit word in the movie's byte order unless the rule says otherwise;
**P** marks a word that is a file offset into the same member (0 = none); offsets in a structure are from its first
byte.

## The resource

### APT-RES-1: A SwfMovie resource is a BIGF archive of .apt, .const and .geo

Status: **verified** · Variants: PC, Xbox 360

A `SwfMovie` entry (type `SwfMovie`, extension `.res`) is a BIGF archive whose members are `<name>.apt`,
`<name>.const` and `<name>.geo`, in that order, with the same `<name>`. The `.apt` and `.const` members are required
to read a movie; `.geo` is needed only for shapes. The members start on 4-byte boundaries (the gap after a member is
0 - 3 bytes) and the last member ends at the end of the resource.

[data: all 1,027 SwfMovie entries of the game copy (105 bundles: 99 in Package, 6 in Dist) have exactly these three members in this order, every member offset a multiple of 4, gaps of 0 - 3 bytes, the last member ending at the archive's end; 92 distinct movies (aptgraft.py's 2026-10-29 count, 982 entries then)]
[test: bfbc2-console-readers/tests/test_console_readers.py::apt_checks] [code: bfbc2-console-readers/apt/apt.py:174-191]
[doc: bfbc2-menu-tools/aptgraft.py:100-101]

### APT-RES-2: The byte order of a movie is the platform's

Status: **verified** · Variants: PC, Xbox 360

PC movies are little-endian, Xbox 360 movies big-endian: every 32-bit field, float and multi-byte immediate operand
is in the platform's order; C strings and single bytes are the same on both. The layout is otherwise identical, so a
movie converts between platforms by swapping words (`aptgraft.py convert`). The BIGF container around it is the same
on both platforms ([bigf.md](bigf.md)).

[data: all 1,027 SwfMovies of the PC game copy little-endian; the 17 Xbox 360 HUD movies OPFOR imported big-endian, written back byte for byte in both orders (aptgraft.py 2026-10-29)]
[test: rust/bc2-formats/src/aptmodel.rs::parse_and_write_back] [test: bfbc2-menu-tools/tests/test_aptgraft.py::quick]
[doc: bfbc2-console-readers/apt/apt.py:25-28]

### APT-RES-3: The byte order is detected from the .const header's movie offset

Status: **implemented** · Variants: PC, Xbox 360

The `.const` member's word at 20 (the movie offset, APT-CONST-2) is read both ways. The movie is big-endian when the
little-endian reading does not lie inside `.apt` (`0 < le < len(.apt)` false) and the big-endian one does. Both can
lie inside the file (one Xbox 360 HUD movie): then the movie is big-endian when the word 4 bytes after the
big-endian offset is the character signature `09 87 65 43` as stored big-endian and the little-endian offset does not
point at the signature stored little-endian (`43 65 87 09`). The grafting model uses the signature test alone: big
when the little-endian offset does not point at a movie character (the signature after it, in range) and the
big-endian one does.

[code: bfbc2-console-readers/apt/apt.py:229-235] [code: rust/bc2-formats/src/aptmodel.rs:1776-1784] [code: rust/bc2-formats/src/aptmodel.rs:305-316]
[doc: bfbc2-menu-tools/aptgraft.py:107-108]

## The .const member

### APT-CONST-1: .const signature

Status: **verified** · Variants: PC, Xbox 360

`.const` starts with the 20 bytes `Apt constant file`, `1A 00 00` (17 ASCII characters, the byte 0x1A and two zero
bytes), the same on both platforms. A file that does not start with `Apt constant file` is not a movie's constant
file.

[data: 1,027 of 1,027 SwfMovies of the game copy start .const with these 20 bytes]
[test: rust/bc2-formats/src/aptmodel.rs::refuses_what_python_raises] [code: rust/bc2-formats/src/aptmodel.rs:1773-1775]

### APT-CONST-2: .const header

Status: **verified** · Variants: PC, Xbox 360

| Offset | Size | Field |
|---|---|---|
| 0 | 20 | signature (APT-CONST-1) |
| 20 | 4 | P movie: the offset in `.apt` of the movie character (APT-MOV-1) |
| 24 | 4 | constant count *n* |
| 28 | 4 | P entries: the offset in `.const` of the entry table |

The movie offset points into the other member (`.apt`); the entry and string offsets point into `.const` itself.

[test: rust/bc2-formats/src/apt.rs::dump_and_records] [code: rust/bc2-formats/src/apt.rs:409-413] [code: rust/bc2-formats/src/aptmodel.rs:941-952]
[doc: bfbc2-menu-tools/aptgraft.py:93-94]

### APT-CONST-3: Constant entries

Status: **verified** · Variants: PC, Xbox 360

The entry table holds *n* entries of 8 bytes, `{u32 type, u32 value}`:

| Type | Meaning of value |
|---|---|
| 1 | a string: P to a zero-terminated byte string in `.const` (read as Latin-1) |
| 4 | a register number |

Any other type is not known; the reader shows it as `type<T>:<V>`. A string offset of 0 or past the end of `.const`
reads as `null`. Constants are numbered from 0 in table order; this number is the *global constant index* that
PushData lists and ConstantPool lists hold (APT-CODE-6).

[data: the game copy's 1,027 movies hold 4,554,390 constants: 2,996,536 of type 4, 1,557,854 of type 1, no other type]
[test: rust/bc2-formats/src/apt.rs::dump_and_records] [code: rust/bc2-formats/src/apt.rs:409-440] [code: rust/bc2-formats/src/apt.rs:383-391]

### APT-CONST-4: The game's constant files keep their strings behind the entry table

Status: **inferred** · Variants: PC, Xbox 360

When a writer grows the entry table (constants added), the table stays where it is and the strings move behind it;
the suite writes grafted movies' `.const` in the `pack` layout for this reason, matching the shape of the game's own
constant files (header, entry table, then strings).

[inferred: aptgraft.py writes .const packed after grafting "the shape of the game's own constant files"; the game copy's .const files place the entry table before the strings, and no game file is known whose strings precede the table]
[doc: bfbc2-menu-tools/aptgraft.py:120-124]

## The .apt member

### APT-HDR-1: .apt signature

Status: **verified** · Variants: PC, Xbox 360

`.apt` starts with the ASCII text `Apt Data:` followed by a version text and the byte 0x1A (`Apt Data:74`, 0x1A in
the suite's built movies). The header is everything up to and including the first 0x1A byte; the writer keeps it
byte for byte.

[data: 1,027 of 1,027 SwfMovies of the game copy start .apt with "Apt Data:"]
[test: rust/bc2-formats/src/aptmodel.rs::refuses_what_python_raises] [code: rust/bc2-formats/src/aptmodel.rs:920-933]

### APT-HDR-2: Every pointer is a file offset into its own member

Status: **verified** · Variants: PC, Xbox 360

Every pointer field in `.apt` is an offset from the start of `.apt`; in `.const` from the start of `.const`; in
`.geo` from the start of `.geo`. The only cross-member pointer is `.const`'s movie offset (APT-CONST-2). Structures
may lie anywhere in their member; the order in the game's files is not required by the format (the `pack` layout
moves every structure and the reader reads the same movie, APT-WR-3).

[test: bfbc2-menu-tools/tests/test_aptgraft.py::quick] [code: rust/bc2-formats/src/aptmodel.rs:1095-1100]
[doc: bfbc2-menu-tools/aptgraft.py:69]

### APT-CHR-1: Character records start with a type and the signature 0x09876543

Status: **verified** · Variants: PC, Xbox 360

Every character starts with `{u32 type, u32 0x09876543}`. A character table slot whose pointer is 0, points past
the end, or whose second word is not the signature is not a character of this movie (an imported or empty slot).

| Type | Character | Size (bytes) |
|---|---|---|
| 1 | shape | 28 |
| 2 | text field | 60 |
| 3 | font | 20 |
| 4 | button | - |
| 5 | sprite | 20 |
| 6 | sound | 12 |
| 7 | image | 12 |
| 8 | morph | - |
| 9 | movie | 60 |
| 10 | static text | 56 |
| 11 | none | - |
| 12 | video | - |

Buttons, morphs and videos (and types 11, 12) are not in BC2's movies; the model refuses them.

[data: the game copy's 1,027 movies: 72,638 shapes, 59,399 sprites, 51,285 images, 2,246 static texts, 1,114 sounds, 1,027 movies (each movie's root, one per file), 801 text fields, 580 fonts, 6,105 null slots; no other type; every non-null slot carries the signature]
[test: rust/bc2-formats/src/apt.rs::dump_and_records] [code: rust/bc2-formats/src/apt.rs:821-833] [code: rust/bc2-formats/src/aptmodel.rs:572-588]
[doc: bfbc2-console-readers/apt/apt.py:14-20]

### APT-MOV-1: The movie character

Status: **verified** · Variants: PC, Xbox 360

The movie (character 0 of its own table, type 9) is 60 bytes:

| Offset | Field |
|---|---|
| 0 | 9 (type) |
| 4 | 0x09876543 |
| 8 | frame count |
| 12 | P frames (APT-FRM-1) |
| 16 | 0 |
| 20 | character count |
| 24 | P character table: one P per character (0 = none or imported) |
| 28 | width (i32, pixels) |
| 32 | height (i32, pixels) |
| 36 | milliseconds per frame |
| 40 | import count |
| 44 | P imports: 16 bytes each, {P movie name, P name, character id, 0} |
| 48 | export count |
| 52 | P exports: 8 bytes each, {P name, character id} |
| 56 | 0 |

The `.const` header's movie offset must point at this character: a movie whose type there is not 9 is refused.
Character ids are indices into the character table; an imported character's slot is 0 and the import names it.

[data: 1,027 of 1,027 game-copy movies have type 9 and the signature at the .const movie offset, which is a multiple of 4]
[test: rust/bc2-formats/src/apt.rs::dump_and_records] [code: rust/bc2-formats/src/apt.rs:797-834] [code: rust/bc2-formats/src/aptmodel.rs:946-949]
[doc: bfbc2-menu-tools/aptgraft.py:70-72]

### APT-CHR-2: Character layouts

Status: **implemented** · Variants: PC, Xbox 360

After the 8-byte type and signature:

| Character | Fields (offset from the character's start) |
|---|---|
| shape (28) | 8: bounds, 4 floats (left, top, right, bottom); 24: geometry id (= the shape's own character id, APT-GEO-2) |
| text (60) | 8: bounds (4 floats); then font, alignment, colour, height, read-only, multiline, word wrap; 52: P initial text; 56: P variable name |
| font (20) | 8: P name; 12: glyph count; 16: P glyphs (32 bits each) |
| sprite (20) | 8: frame count; 12: P frames (APT-FRM-1); 16: 0 |
| sound (12) | 8: id |
| image (12) | 8: texture id (= the image's own character id: its entry in the movie's texture atlas, APT-ATL-1) |
| static text (56) | 8: bounds (4 floats); 24: 2 x 2 matrix; 40: x, y; 48: record count; 52: P records (56 bytes each: font, colour as 4 floats, 5 fields, height, one unknown field, glyph count, P glyphs) |

[code: rust/bc2-formats/src/aptmodel.rs:578-626] [code: rust/bc2-formats/src/apt.rs:835-858]
[doc: bfbc2-menu-tools/aptgraft.py:76-81]

### APT-FRM-1: Frames and frame items

Status: **verified** · Variants: PC, Xbox 360

A timeline (the movie's or a sprite's) is an array of `frame count` frames of 8 bytes, `{item count, P item list}`;
the item list holds one P per item. Each item starts with its type:

| Type | Item | Size | Fields |
|---|---|---|---|
| 1 | action | 8 | 4: P code (APT-CODE-1) |
| 2 | frame label | 16 | 4: P name; 8: flags; 12: frame |
| 3 | place object | 64 | 4: flags; 8: depth; 12: character; 16: 2 x 2 matrix (4 floats); 32: x; 36: y (floats); 40: colour; 44: unknown; 48: ratio; 52: P instance name or 0; 56: clip depth; 60: P clip actions or 0 |
| 4 | remove object | 8 | 4: depth |
| 5 | background colour | 8 | 4: colour |
| 8 | init action | 12 | 4: sprite; 8: P code |

Clip actions: `{count, P events}`, each event 12 bytes `{flags, key, P code}`. Other item types are not known (the
reader prints them; the model refuses them).

[data: every one of the 128 movie-set movies (menus, HUDs, loaders) and the Xbox 360 menus decode with known item types only (aptcheck, 2026-10-26)]
[test: rust/bc2-formats/src/apt.rs::dump_and_records] [test: bfbc2-menu-tools/tests/test_apt_e3n.py::game] [code: rust/bc2-formats/src/apt.rs:718-795] [code: rust/bc2-formats/src/aptmodel.rs:630-668]
[doc: bfbc2-menu-tools/aptgraft.py:73-76]

### APT-FRM-2: A control is hidden by moving it far off screen

Status: **observed** · Variants: PC

OPFOR hides a placed control by setting its place-object `y` (offset 36, APT-FRM-1) to -5000.0; the game draws
nothing and the control's logic still runs.

[data: OPFOR's menu patches hide controls this way in its installed movies (DEVELOPMENT_NOTES.md (private research notes) section 3), confirmed in game]
[doc: private:opfor-research/DEVELOPMENT_NOTES.md:740-743]

## Byte code

### APT-CODE-1: Action streams are AVM1 plus EA opcodes

Status: **verified** · Variants: PC, Xbox 360

Byte code is Flash's AVM1 (ActionScript 1 / 2) with EA's own opcodes added. An opcode below 0x80 has no operand; an
opcode at or above 0x80 has operands whose layout is fixed per opcode (APT-CODE-2), never a Flash-style length
field. A top-level stream (a frame action, an init action, a clip event) ends at its End opcode (0x00); a function
body ends at its stated size (APT-CODE-4). The opcode names the suite uses are `apt.NAMES` (Flash's names, `EA_`
for EA's): 0x56 - 0x5D and 0x70 - 0x76 are EA's no-operand pushes (this, global, zero, one, null, undefined, true,
false ...), 0xA1 - 0xB9 EA's operand forms.

[test: rust/bc2-formats/src/apt.rs::dump_and_records] [code: rust/bc2-formats/src/apt.rs:657-708] [code: rust/bc2-formats/src/apt.rs:74-207]
[doc: bfbc2-console-readers/apt/apt.py:21-24]

### APT-CODE-2: Operand layouts

Status: **verified** · Variants: PC, Xbox 360

**Aligned** means the operand starts at the next multiple of 4 counted from the start of `.apt` (the padding bytes
between the opcode and the operand are zero in written movies). Multi-byte operands are in the movie's byte order.

| Opcode(s) | Operands |
|---|---|
| 0x81 GotoFrame, 0x87 SetRegister, 0x94 With, 0x9F GotoFrame2 | aligned i32 |
| 0x99 BranchAlways, 0x9D BranchIfTrue, 0xB8 EA_BranchIfFalse | aligned i32 offset, relative to the end of the instruction |
| 0x83 GetURL | aligned P url, P target (two strings) |
| 0x88 ConstantPool | aligned count, P list of global constant indices (u32 each) |
| 0x96 PushData | aligned count, P list of global constant indices (u32 each) |
| 0x8A WaitForFrame | aligned i32 frame, i32 skip |
| 0x8B SetTarget, 0x8C GotoLabel, 0xA1 EA_PushString, 0xA4 - 0xA7 EA string get / set | aligned P string |
| 0x8D WaitForFrame2 | u8 |
| 0xA2, 0xAE, 0xAF, 0xB0 - 0xB3 (pool forms) | u8 constant-pool index |
| 0xA3 EA_PushWordConstant | u16 constant-pool index (not aligned) |
| 0xB4 EA_PushFloat | f32 (not aligned) |
| 0xB5 EA_PushByte | i8 |
| 0xB6 EA_PushShort | i16 (not aligned) |
| 0xB7 EA_PushLong | i32 (not aligned) |
| 0xB9 EA_PushRegister | u8 register |
| 0x9B DefineFunction, 0x8E DefineFunction2 | APT-CODE-4 |
| 0x9A GetURL2 | none (unlike Flash) |

Every other opcode at or above 0x80 is unknown: a reader cannot step over it and stops (`unknown-op`).

[data: with these layouts every function body of the 56 PC and Xbox 360 menu and HUD movies checked on 2026-10-26 decodes exactly to its end; 1,924,750 instructions of the 128 movie-set movies decode (BENCHMARKS E3n)]
[test: rust/bc2-formats/src/apt.rs::dump_and_records] [test: bfbc2-menu-tools/tests/test_apt_e3n.py::quick] [code: rust/bc2-formats/src/apt.rs:513-646] [code: rust/bc2-formats/src/aptmodel.rs:216-231]
[doc: bfbc2-menu-tools/aptgraft.py:82-87]

### APT-CODE-3: Differences from Flash

Status: **verified** · Variants: PC, Xbox 360

GetURL2 (0x9A) has no operand and GotoFrame2 (0x9F) one aligned 32-bit operand, unlike Flash. Read with Flash's
layouts, function bodies do not decode to their ends.

[test: bfbc2-menu-tools/tests/test_apt_e3n.py::game] [code: rust/bc2-formats/src/apt.rs:524-526]
[doc: bfbc2-console-readers/apt/apt.py:21-24]

### APT-CODE-4: Function definitions

Status: **verified** · Variants: PC, Xbox 360

After the opcode and the alignment:

| Opcode | Operand words | Body starts |
|---|---|---|
| 0x9B DefineFunction | P name, parameter count, P parameters (one P name each), body size, 0x98765432, 0x12345678 | 24 bytes after the aligned start |
| 0x8E DefineFunction2 | P name, parameter count, u16 register count + u16 flags, P parameters ({register, P name} each), body size, 0x98765432, 0x12345678 | 28 bytes after the aligned start |

DefineFunction2's register count and flags are two 16-bit fields in the movie's byte order (stored `02 00 29 00` on
the PC, `00 02 00 29` on the Xbox 360 for the same values). The body (body size bytes) follows the definition
inline; the enclosing stream resumes after the body. The two magic words are written as they are.

[test: rust/bc2-formats/src/apt.rs::dump_and_records] [code: rust/bc2-formats/src/apt.rs:624-645]
[doc: bfbc2-menu-tools/aptgraft.py:88-91]

### APT-CODE-5: A function body decodes exactly to its end

Status: **verified** · Variants: PC, Xbox 360

Decoding a function body instruction by instruction from its start must land exactly on start + body size: no
instruction straddles the end, no operand is misaligned. A body that does not (`body-end`) breaks the loader.
A top-level End (0x00) ends only top-level streams: an End inside a function body is an error (`end-in-function`),
and so is a written byte that becomes opcode 0x00 (`zero-opcode`; OPFOR fills with `59 17`, push 0 then pop).

[data: every movie of the game copy's movie sets (128) and the Xbox 360 menus pass; patched menus that broke these crashed while loading (OPFOR DEVELOPMENT_NOTES (private research notes) section 3)]
[test: bfbc2-menu-tools/tests/test_menu_tools.py::quick] [test: rust/bc2-formats/src/aptscan.rs::findings_sorted] [code: rust/bc2-formats/src/aptscan.rs:120-130] [code: rust/bc2-formats/src/aptscan.rs:199-209]
[doc: bfbc2-menu-tools/README.md:44-53]

### APT-CODE-6: Constant indices: global lists and the constant pool

Status: **verified** · Variants: PC, Xbox 360

PushData (0x96) and ConstantPool (0x88) lists hold **global** indices into the `.const` table (APT-CONST-3). The
pool forms (0xA2, 0xAE, 0xAF, 0xB0 - 0xB3 with a u8 index; 0xA3 with a u16 index) index the **current constant
pool**: the list of the last ConstantPool executed in the stream; a function body inherits the pool current at its
definition. A global index past the table, or a pool index past the current pool, is an error (`const-index`).

[test: bfbc2-menu-tools/tests/test_menu_tools.py::quick] [code: rust/bc2-formats/src/apt.rs:540-555] [code: rust/bc2-formats/src/apt.rs:590-603] [code: rust/bc2-formats/src/aptscan.rs:140-165]
[doc: private:opfor-research/DEVELOPMENT_NOTES.md:730-734]

### APT-CODE-7: Branches land on an instruction of their own stream

Status: **verified** · Variants: PC, Xbox 360

A branch target (end of the branch instruction + offset) must be the start of an instruction of the same stream, or
the end of the function body that holds it (`branch-target`).

[test: bfbc2-menu-tools/tests/test_menu_tools.py::quick] [code: rust/bc2-formats/src/aptscan.rs:131-139]

## The loader

### APT-LOAD-1: The loader converts offsets to pointers in place, once

Status: **observed** · Variants: PC

When the game loads a movie it walks its structures once and turns every offset into a pointer in place (byte code
at 0x1309880; the character walk and its inverse at 0x1319730 in build 795745). Every structure is therefore
converted exactly once per load: a structure reachable twice is converted twice and corrupted.

[exe: BFBC2Game.exe 0x1309880 (byte code offsets to pointers), 0x1319730 (character walk and its inverse)]
[doc: bfbc2-console-readers/apt/apt.py:37-39]

### APT-LOAD-2: Every PushData list has exactly one user

Status: **verified** · Variants: PC, Xbox 360

Each PushData item list must be referenced by exactly one PushData instruction. The loader converts each list's
global constant indices to pointers in place (APT-LOAD-1); a second PushData on the same list makes the game exit
while the menus load, with no log output (`shared-list`). To add a PushData, a writer reuses the list of one being
removed or appends a new list; the suite's model copies, never shares, new data.

[data: OPFOR's menu patches crashed the menus when two PushData shared a list (DEVELOPMENT_NOTES.md (private research notes) section 3); no movie of the game copy shares one]
[test: bfbc2-menu-tools/tests/test_menu_tools.py::quick] [code: rust/bc2-formats/src/aptscan.rs:170-184]
[doc: private:opfor-research/DEVELOPMENT_NOTES.md:735-738]

### APT-LOAD-3: Externs, FSCommands and events are names the engine resolves

Status: **observed** · Variants: PC

A movie reads engine values as `extern.<name>` (the name resolved through the executable's name -> id table, 574
names at 0x0143C898 in build 795745), sends FSCommands (`"FSCommand:<name>"` strings and `fscommand` /
`fscommandProxy` calls), and handles engine events (`_root.catchEvent(name, handler)` in the HUD,
`gameEventListener.<name> = handler` in the menus). An extern the executable does not know can crash the game (the
first Onslaught timer graft read one the PC executable lacks).

[exe: BFBC2Game.exe build 795745: name table 0x0143C898; main menu getter 0x76EC30; UI manager getter 0x95A1B0; HUD getter 0x777B30]
[test: bfbc2-menu-tools/tests/test_menu_tools.py::game] [code: rust/bc2-formats/src/aptscan.rs:296]
[doc: bfbc2-menu-tools/README.md:59-82]

## Geometry and images

### APT-GEO-1: .geo header

Status: **verified** · Variants: PC, Xbox 360

`.geo` starts with `Apt PCGL geometry file`, 0x1A, 0x00 (24 bytes, the same text on both platforms), then a count at
0x18 and P table at 0x1C. The table holds `count` entries of 12 bytes, `{geometry id, part count, P parts}`, the
part list one P per part. The table is not sorted: geometry is looked up by id.

[data: 1,027 of 1,027 game-copy movies start .geo with this text]
[test: rust/bc2-formats/src/aptmodel.rs::parse_and_write_back] [code: rust/bc2-formats/src/aptmodel.rs:959-975]
[doc: bfbc2-menu-tools/aptgraft.py:95-99]

### APT-GEO-2: Geometry parts

Status: **implemented** · Variants: PC, Xbox 360

A shape's geometry id equals the shape's character id. Parts, by their first word:

| Type | Part | Fields |
|---|---|---|
| 1 | solid triangles | count; P vertices (6 floats per triangle: x, y three times); colour ARGB |
| 6 | lines | count; P vertices (4 floats per line); colour |
| 4, 5 | textured triangles (88 bytes) | triangle count; P vertices; colour; 16 texture-matrix fields; 80: image character id; 84: 0 |

[code: rust/bc2-formats/src/aptmodel.rs:959-980]
[doc: bfbc2-menu-tools/aptgraft.py:95-99] [doc: bfbc2-console-readers/apt/apt.py:29-33]

### APT-ATL-1: Image pixels are in the menu's texture atlas

Status: **implemented** · Variants: PC, Xbox 360

An image character's texture id is its own character id; its pixels are a rectangle of the menu's texture atlas
(`<menu>_textures_Win32` / `_Xenon`): `atlas_meta.dbx` (`UI.UITextureAtlasInfo`: `TextureFile` = the image's
character id, `MinU` / `MaxU` / `MinV` / `MaxV`) and `atlas0.res` (a texture: [texture.md](texture.md); PC a linear
DXT5 4096 x 4096, Xbox 360 tiled 16-bit-swapped DXT5). A grafted image needs an atlas entry of its own; the suite
reports it rather than copying atlas entries.

[code: bfbc2-menu-tools/aptgraft.py:1983]
[doc: bfbc2-console-readers/apt/apt.py:33-36] [doc: bfbc2-menu-tools/aptgraft.py:109-115]

## Writing

### APT-WR-1: An unchanged movie writes back byte for byte

Status: **verified** · Variants: PC, Xbox 360

The suite's writer (the `keep` layout, the movie's own byte order) gives an unchanged movie back byte for byte: the
BIGF container and all three members.

[data: every distinct SwfMovie of the game copy and OPFOR's Xbox 360 HUD copies: 1,463 writes byte-identical to the Python reference (BENCHMARKS E3n); 92 + 17 movies byte for byte, packed and through the other byte order (2026-10-29)]
[test: rust/bc2-formats/src/aptmodel.rs::parse_and_write_back] [test: bfbc2-menu-tools/tests/test_apt_e4c.py::game]

### APT-WR-2: The keep layout

Status: **verified** · Variants: PC, Xbox 360

In the `keep` layout every structure that kept its size stays at its offset; new and grown structures go after the
end of their member, 4-byte aligned, and byte code keeps its offset modulo 4 (its operand alignment depends on it,
APT-CODE-2); the space a moved structure leaves is zeroed. An edited movie therefore keeps all its old data where it
was, which in-place byte patches (OPFOR's `AptTool patch`) rely on.

[test: rust/bc2-formats/src/aptmodel.rs::grown_node_moves_in_keep] [code: rust/bc2-formats/src/aptmodel.rs:1131-1146]
[doc: bfbc2-menu-tools/aptgraft.py:61-64]

### APT-WR-3: The pack layout

Status: **verified** · Variants: PC, Xbox 360

In the `pack` layout every structure keeps its old order, shifted by a multiple of 4 where something before it grew
(the suite's round-trip test adds a 16-byte gap after each header so that every structure moves). A movie packed
this way reads as the same movie: the test that every pointer is known to the model.

[test: rust/bc2-formats/src/aptmodel.rs::parse_and_write_back] [test: bfbc2-menu-tools/tests/test_aptgraft.py::quick] [code: rust/bc2-formats/src/aptmodel.rs:1147-1160]
[doc: bfbc2-menu-tools/aptgraft.py:65]

### APT-WR-4: Dead data in patched movies

Status: **observed** · Variants: PC

A movie patched in place may hold bytes no structure points at (OPFOR's installed main_nav, main_root, main_popups
and server_browser); the game ignores them, and the `keep` layout writes them back unchanged. The pristine copies
have none.

[data: 4 of OPFOR's installed movies with dead data, their 4 pristine copies with none (aptgraft.py roundtrip, 2026-10-29)]
[doc: bfbc2-menu-tools/aptgraft.py:103-106]

## Open questions

- The `Apt Data:` version text: whether the runtime checks it, and what other versions exist (BF1943's movies).
- The meaning of the place-object word at 44, the static-text record's unknown field, and the text field's middle
  words (alignment, colour and flags are named after OpenSAGE, not proven against the BC2 runtime).
- Frame-label flags; clip-event flag bits and the key word.
- Constant types other than 1 and 4 (none seen in BC2; OpenSAGE knows more).
- Sound characters (1,114 in the game copy): what the id names and how it reaches the sound system.
- Whether the loader also converts ConstantPool lists, text and font structures in place (only PushData sharing is
  proven to crash); which other structures may never be shared.
- `.geo` part types beyond 1, 4, 5 and 6; the 16 texture-matrix fields' exact order.
- The 128 "movie-set" movies vs the 92 distinct SwfMovies counted by aptgraft: the corpora differ (Package and Dist
  copies, OPFOR's pristine bundle, Xbox 360 copies) and no single list is kept.
