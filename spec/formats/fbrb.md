# FBRB bundles (`.fbrb`)

A bundle is Frostbite 1's archive: a gzip-compressed header that lists named, typed entries, then a payload that
holds their data back to back. Every level, the shared `*_common` data, the menus, the async picture and sound
banks and the streamed sound are bundles. A bundle is named `<name>-00.fbrb`; the game's two roots are
`game\Dist\win32` (the install) and `game\Package` (patches and DLC). Which bundles a level loads and in which order
is the data model's business (`model/bundles.md`, `model/registration.md`); this file is the bytes.

The suite reads bundles in `rust/bc2-formats/src/fbrb` (header, read, the bundle cache) and writes them byte for
byte as FbrbTool wrote them (`pack.rs`; FbrbTool.cs was the reference until round R4, its answers kept in
`core/tests/fbrbtool_golden*.json`).

The corpus counts below are of the game copy (2026-11-19): **1,086 bundles, 740,662 entries, 296,376 of them
`*deleted*` markers**, read by the structure surveys of round SP1 (headers only, nothing kept). The bundles are DICE's
plus those Project OPFOR BFBC2 installs; where only DICE's files are meant, the rule says "stock".

## 1. File layout

| Offset | Size | Content |
|---|---|---|
| 0 | 4 | ASCII `FbRB` |
| 4 | 4 | BE u32 *H*: the length of the header part that follows |
| 8 | *H* | the header: one gzip member, then zero padding (section 2) |
| 8 + *H* | to the end | the payload: one gzip member when the header's zipped byte is set, else the entries' bytes stored as they are |

All header numbers are big-endian on every platform (the payload's resources have their own byte order).

### FBRB-HDR-1: Magic

Status: **verified** · Variants: PC, Xbox 360, BF1943

A bundle starts with the four ASCII bytes `FbRB`. A file shorter than 8 bytes, or with other first bytes, is not a
bundle and the suite refuses it.

Evidence: [code: rust/bc2-formats/src/fbrb/header.rs:10] [code: rust/bc2-formats/src/fbrb/header.rs:170-172] [test: rust/bc2-formats/src/fbrb/header.rs::read_header_refuses] [data: 1,086 of 1,086 bundles of the game copy]

### FBRB-HDR-2: Header length

Status: **verified** · Variants: PC, Xbox 360, BF1943

Bytes 4-7 are a BE u32 *H*: the number of bytes of the header part (the gzip member and any zero padding after it).
The payload starts at byte 8 + *H*.

Evidence: [code: rust/bc2-formats/src/fbrb/header.rs:173-178] [code: rust/bc2-formats/src/fbrb/header.rs:186] [code: rust/bc2-formats/src/fbrb/pack.rs:264-272] [test: rust/bc2-formats/src/fbrb/header.rs::read_header_refuses] [test: rust/bc2-formats/tests/fbrb_robustness.rs::built_bundles_read_back]

### FBRB-HDR-3: The header part is one gzip member, then zeros

Status: **verified**

The header part is a single gzip member (RFC 1952). In a padded layout (FBRB-LAY-3) zero bytes follow the member up
to *H*; a reader decompresses the member and ignores what follows it. In 1,081 bundles nothing follows the member;
in 5 (the stored async / streaming bundles with `hdr=4` or `hdr=4096`) 3 to 3,686 zero bytes do, always zeros.

Evidence: [data: 1,086 bundles: 1,081 with no bytes after the header member, 5 with 3, 151, 384, 1,392 and 3,686 zero bytes] [code: rust/bc2-formats/src/fbrb/pack.rs:265-268] [test: rust/bc2-formats/tests/spec_corpus.rs::fbrb_headers] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: 1,017 with nothing after the header member, 4 with zero bytes only]

### FBRB-HDR-4: The gzip members' header bytes

Status: **verified**

Both gzip members begin `1F 8B 08 00` (deflate, no flags: no name, comment, extra field or header CRC) and a zero
modification time. DICE's packer writes extra flags 0 and OS 0x0B (NTFS); FbrbTool (and the suite's packer, which
reproduces it) writes extra flags 4 and OS 0, but for the empty payload member, which the suite writes as DICE's
(FBRB-DEL-2). The game accepts both. The trailer is the CRC-32 and the uncompressed
length modulo 2^32 (LE), as RFC 1952 has it.

Evidence: [data: header members: 1,024 with `1F 8B 08 00 00000000 00 0B` (DICE), 62 with `... 04 00` (FbrbTool), 1 other tool's (a set modification time, OS 0xFF); payload members 1,015 / 60 / 1, 11 payloads stored or empty] [code: rust/bc2-formats/src/zlib.rs:985-990] [doc: rust/BENCHMARKS_ARCHIVE_1.md:2552] [test: rust/bc2-formats/tests/spec_corpus.rs::fbrb_headers] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: header members 1,021 of 1,021 and payload members 907 of 907 `1F 8B 08 00 00000000 00 0B` (DICE's form)]

### FBRB-HDR-5: The suite's gzip reading rules

Status: **verified**

The suite reads the header member as Python's `gzip.decompress` did (every member, zeros between members skipped,
reserved flag bits and the header CRC not checked) and the payload member as `zlib.decompressobj(31)` fed 1 MiB at
a time did (the first member only, reserved flags refused, the header CRC checked, the trailer checked once its 8
bytes are fed). FbrbTool's own commands read gzip as .NET Framework's `GZipStream` did (`netgzip.rs`), so damaged
bundles are refused exactly where the retired tools refused them. These are the suite's rules, not the game's: a
well-formed bundle reads the same under all of them.

Evidence: [code: rust/bc2-formats/src/fbrb/gzip.rs:1-10] [code: rust/bc2-formats/src/fbrb/read.rs:13] [test: rust/bc2-formats/src/fbrb/gzip.rs::python_reader_rules] [test: rust/bc2-formats/src/fbrb/gzip.rs::member_rules] [test: rust/bc2-formats/src/fbrb/netgzip.rs::damage] [doc: rust/BENCHMARKS_ARCHIVE_1.md:2551]

## 2. The decompressed header

| Offset | Size | Content |
|---|---|---|
| 0 | 4 | BE u32 version: 2 |
| 4 | 4 | BE u32 *S*: the string table's length in bytes |
| 8 | *S* | the string table: zero-terminated ASCII names and types |
| 8 + *S* | 4 | BE u32 *N*: the entry count |
| 12 + *S* | 24 × *N* | the entry table (section 3) |
| 12 + *S* + 24*N* | 1 | the zipped byte: non-zero when the payload is one gzip member |
| 13 + *S* + 24*N* | 4 | BE u32 the payload length field (FBRB-HDR-9) |

Nothing follows the payload length field.

### FBRB-HDR-6: Version

Status: **verified** · Variants: PC

The header's first word is 2. The suite's reader keeps the value without checking it; its writer writes 2.

Evidence: [data: 2 in 1,086 of 1,086 bundles] [code: rust/bc2-formats/src/fbrb/header.rs:133] [code: rust/bc2-formats/src/fbrb/pack.rs:253] [test: rust/bc2-formats/tests/spec_corpus.rs::fbrb_headers] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: 2 in 1,021 of 1,021]

### FBRB-HDR-7: String table

Status: **verified**

The string table is *S* bytes of zero-terminated strings. String offsets in the entry table count from the start of
the table (header byte 8). Bytes outside ASCII do not occur; the suite reads one as U+FFFD and writes a non-ASCII
character as `?`. A reader looks for a string's zero from its offset onwards, so a malformed table whose last
string runs into the entry table still reads; a string with no zero before the end of the header is an error.

Evidence: [code: rust/bc2-formats/src/fbrb/header.rs:87-107] [code: rust/bc2-formats/src/fbrb/pack.rs:154] [test: rust/bc2-formats/src/fbrb/header.rs::fields] [test: rust/bc2-formats/src/fbrb/header.rs::damaged_headers]

### FBRB-HDR-8: String table order

Status: **verified**

The table holds, entry by entry in entry-table order, the entry's name, then its type when this type has not been
written before; each entry's type offset points at the first copy of its type. DICE's packer writes the table this
way: every bundle of the game copy, stock and OPFOR's, has exactly the table this rule rebuilds from its entries, so
the suite's packer reproduces it byte for byte.

Evidence: [code: rust/bc2-formats/src/fbrb/pack.rs:230-249] [test: rust/bc2-formats/src/fbrb/pack.rs::strings_types_once_and_ascii] [data: 1,086 of 1,086 string tables equal the rebuilt one]

### FBRB-HDR-9: The payload length field

Status: **verified**

The last header word is the payload's length: the decompressed length of a zipped payload (equal to the last live
entry's offset + size), the stored payload's length otherwise. In DICE's padded async bundles (`plen=size2`) it is
instead the last live entry's offset + size2 (FBRB-LAY-4). In a bundle of markers only it is 0. The suite's reader
keeps the field but locates entries by their offsets, never by it; whether the game reads it is not known.

Evidence: [code: rust/bc2-formats/src/fbrb/header.rs:159] [code: rust/bc2-formats/src/fbrb/pack.rs:259-263] [test: rust/bc2-tools/src/fbrbtool.rs::layout_tokens_of_padded_payloads] [test: rust/bc2-formats/src/fbrb/pack.rs::padded_layouts] [data: 1,075 zipped bundles: the decompressed length; 8 stored: the payload's length; 3 stored async bundles: the last live entry's offset + size2]

### FBRB-HDR-10: The zipped byte

Status: **verified**

A non-zero zipped byte means the payload is one gzip member; 0 means the entries' bytes are stored. Stock zips every
bundle but the uncompressed async `ondemand_*`, the `streaming_*` sound bundles and the two `build_overlay` bundles
(corrected 2026-11-22, round SPV1: the earlier text left out `build_overlay`, stored in Dist and Package alike).

Evidence: [code: rust/bc2-formats/src/fbrb/header.rs:158] [data: 1,075 zipped; 11 stored: Dist 6, Package 5 (the async ondemand and streaming bundles)] [test: rust/bc2-formats/tests/spec_corpus.rs::fbrb_headers] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: 1,012 zipped; 9 stored: async/ondemand_awards (Dist), async/ondemand_sounds, streaming_sounds, streaming_vo_en and build_overlay (Dist and Package each)]

## 3. Entries

Each entry is six BE u32s:

| Offset | Field | Meaning |
|---|---|---|
| 0 | name offset | the entry's name in the string table |
| 4 | flags | `0x00010000` for a live entry, 0 for a marker |
| 8 | offset | the data's offset in the (decompressed) payload |
| 12 | size | the data's length |
| 16 | size2 | a second length (FBRB-ENT-5) |
| 20 | type offset | the entry's type in the string table |

### FBRB-ENT-1: Entry record

Status: **verified** · Variants: PC, Xbox 360, BF1943

Each entry is 24 bytes, the six BE u32s above, in archive order. A header too short for *N* records and the zipped
byte is damaged; the suite checks this before allocating anything for the entries.

Evidence: [code: rust/bc2-formats/src/fbrb/header.rs:136-157] [test: rust/bc2-formats/src/fbrb/header.rs::fields] [test: rust/bc2-formats/src/fbrb/header.rs::damaged_headers]

### FBRB-ENT-2: Names

Status: **verified**

A name is the entry's archive path: relative, `/`-separated, with an extension (`Levels/MP_005/MP_005.dbx`). Names
keep their case (364,121 live entries mixed case, 80,165 lower case) but compare without it: no bundle holds two
names that differ only in case. A name must not be empty, rooted or lead out of its folder; the suite refuses such a
name when unpacking.

Evidence: [data: 444,286 live entries: all `/`-separated, none with a backslash; 1,086 of 1,086 bundles without a case-insensitive duplicate] [code: rust/bc2-formats/src/fbrb/header.rs:65-71] [code: rust/bc2-tools/src/fbrbtool.rs:275-294] [test: rust/bc2-formats/src/fbrb/pack.rs::safe_names] [test: rust/bc2-formats/tests/spec_corpus.rs::fbrb_entries] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: 392,246 live entries: no backslash, none empty or rooted; no bundle with a case-insensitive duplicate]

### FBRB-ENT-3: Types

Status: **verified**

The type names the resource kind: `<non-resource>` for data with no resource type (every `.dbx` partition and some
`.res` / `.bin` files), a resource type such as `ITexture` or `MeshData` for a `.res` resource, or `*deleted*` for a
marker. The catalogue of types and their extensions is `formats/res.md` (RES-TYPE-1).

Evidence: [data: 37 types besides `*deleted*` over 444,286 live entries; every resource type on `.res` names] [code: rust/bc2-formats/src/fbrb/header.rs:20-21] [test: rust/bc2-formats/tests/spec_corpus.rs::fbrb_entries] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: 37 types besides `*deleted*` over 392,246 live entries; every resource type on `.res` names]

### FBRB-ENT-4: Flags

Status: **verified**

A live entry's flags are `0x00010000`, a marker's 0. No other value occurs. The suite keeps the flags as they are
(FbrbTool's manifest records them in hex) and does not interpret them.

Evidence: [data: 0x00010000 on 444,286 live entries, 0 on 296,376 markers, no other value] [code: rust/bc2-tools/src/fbrbtool.rs:901-906] [test: rust/bc2-formats/tests/spec_corpus.rs::fbrb_entries] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: 0x00010000 on 392,246 live entries, 0 on 286,000 markers, no other value]

### FBRB-ENT-5: size2

Status: **verified**

size2 equals size, except in a RES-wrapped entry (`formats/res.md`, RES-WRAP-1), where DICE writes size - 64, the
resource's length without its 64-byte header. The suite's packer keeps the size2 a bundle had while the entry's data
has its unpacked size; an edited entry gets size2 = size, or size - 64 in a `size2=minus64` bundle when the entry is
at least 64 bytes long.

Evidence: [code: rust/bc2-formats/src/fbrb/pack.rs:580-596] [test: rust/bc2-formats/src/fbrb/pack.rs::size2_rules] [data: live entries: 442,980 size2 = size, 1,293 size2 = size - 64 (all RES-wrapped), 13 other (FBRB-ENT-6)]

### FBRB-ENT-6: The game tolerates a size2 that disagrees

Status: **observed**

The game reads entries whose size2 is neither size nor size - 64: 13 `.dbx` entries of the installed Onslaught
levels (edited by another tool, size2 41 to 45 bytes less than size), and 110 RES-wrapped pictures OPFOR added to
`Package\async\ondemand_awards` with size2 = size. Both sets load and play. So the game takes an entry's length from
size (and the RES header's own length words), not from size2; whether it reads size2 at all is not known.

Evidence: [data: 13 `.dbx` entries in 3 Onslaught level bundles; 110 of 295 RES-wrapped entries of Package's ondemand_awards; both in use by OPFOR's playtested installs] [inferred: the levels and pictures with the disagreeing size2 load, so the loader cannot rely on size2 for the entry's length]

### FBRB-ENT-7: Offsets ascend in archive order

Status: **verified**

Live entries' offsets ascend in archive order. In a zipped bundle the live entries are contiguous from offset 0:
each starts where the previous one ends, and the last ends at the payload's length. A reader must not assume this
(the suite's reader sorts the entries it collects by offset and allows overlaps), but every writer keeps it.

Evidence: [data: 1,086 of 1,086 bundles ascending; 970 of 970 zipped bundles with live entries contiguous from 0, the last ending at the payload length] [code: rust/bc2-formats/src/fbrb/read.rs:44-55] [test: rust/bc2-formats/src/fbrb/read.rs::overlapping_entries] [test: rust/bc2-formats/tests/spec_corpus.rs::fbrb_entries] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: 1,021 of 1,021 bundles ascending (equal offsets only at zero-size entries); 907 of 907 zipped bundles with live entries contiguous from 0, the last ending at the payload length]

### FBRB-ENT-8: An entry lies inside the payload

Status: **verified**

offset + size must not pass the payload's end. The suite refuses to read such an entry (its reader stops at the
first missing byte; FbrbTool's unpack leaves the empty file it had created, then fails). Since offsets and sizes are
u32, the decompressed payload is less than 4 GiB; the packer refuses a larger one.

Evidence: [code: rust/bc2-formats/src/fbrb/header.rs:41-43] [test: rust/bc2-formats/src/fbrb/read.rs::short_payload_is_refused] [test: rust/bc2-formats/src/fbrb/pack.rs::too_large] [test: rust/bc2-formats/tests/fbrb_robustness.rs::every_truncation_is_an_error_or_a_prefix]

### FBRB-ENT-9: Names are looked up without case, the later entry winning

Status: **verified**

A lookup by name lowers the case. Within one bundle a name listed twice resolves to the later entry, and a marker is
not a match (`model/bundles.md` PREC-ORD-4 gives the precedence across bundles).

Evidence: [code: rust/bc2-formats/src/fbrb/header.rs:65-71] [test: rust/bc2-formats/src/fbrb/header.rs::names_skip_markers_and_later_wins] [test: rust/bc2-formats/src/fbrb/read.rs::reads_entries_both_layouts]

## 4. `*deleted*` markers

### FBRB-DEL-1: Marker record

Status: **verified**

A marker is an entry of type `*deleted*` and size 0. DICE writes it with flags 0, size2 0 and offset = the end of
the previous live entry (0 when none precedes it): it takes the payload position as it is and holds no bytes. An
entry of type `*deleted*` with a size other than 0 is not a marker (none occurs). What a marker means is the data
model's (`model/bundles.md`, BNDL-DEL-1 and BNDL-DEL-2).

Evidence: [code: rust/bc2-formats/src/fbrb/header.rs:35-37] [code: rust/bc2-formats/src/fbrb/pack.rs:145-147] [test: rust/bc2-formats/src/fbrb/header.rs::names_skip_markers_and_later_wins] [data: 296,376 markers: all size 0, size2 0, flags 0, offset = the previous live entry's end]

### FBRB-DEL-2: Bundles of markers only

Status: **verified**

A bundle may hold only markers (each level's own terrain bundle hides the other levels' terrains this way). Its
payload is empty: the zipped byte is set and the payload length field is 0. DICE's packer then writes the gzip
member of no data (20 bytes: `1F 8B 08 00 00000000 00 0B`, `03 00`, a zero CRC and length) right after the header
region, which ends the file: every zipped bundle of DICE's has a payload member, an empty payload too. One bundle
holds no entries at all and ends the same way. The suite's packer writes these 20 bytes for any zipped bundle whose
payload is empty (since round SPV1-fix, 2026-11-22: DICE's bytes win over FbrbTool's); FbrbTool, whose .NET
`GZipStream` writes nothing for no data, ended such a bundle at 8 + *H*. The game reads DICE's form; FbrbTool's has
not been tried in the game (no bundle OPFOR installs today holds markers only). (Corrected 2026-11-22, round SPV1:
the earlier text said no member follows; all 105 of DICE's such bundles carry the empty one.)

Evidence: [data: 105 bundles without a live entry, all zipped with payload length 0; 1 bundle with no entries] [code: rust/bc2-formats/src/fbrb/pack.rs:437-455] [test: rust/bc2-formats/src/fbrb/pack.rs::empty_zipped_payload_is_dices_member] [test: rust/bc2-formats/tests/spec_corpus.rs::fbrb_entries] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: 104 bundles of markers only and 1 with no entries, all 105 zipped with payload length 0 and followed by the 20-byte empty gzip member; no stored bundle has an empty payload, and no zipped bundle with live entries has bytes after its payload member] [data: round SPV1-fix 2026-11-22: the suite's packer, given each of the 105 bundles' entries, writes DICE's file byte for byte but for the header member's extra flags and OS bytes (FBRB-HDR-4)]

## 5. DICE's padded layouts

DICE's packer lays out the stored async and streaming bundles with padding. The suite detects and records it
(FbrbTool's manifest tokens) so that `pack` rebuilds such a bundle byte for byte.

### FBRB-LAY-1: Contiguous stored payloads

Status: **observed**

A stored bundle without padding has its live entries back to back from offset 0, as a zipped payload does.

Evidence: [data: 3 of 11 stored bundles contiguous (Dist), 2 with one live entry or none]

### FBRB-LAY-2: align4096 and gap64

Status: **verified**

In a padded stored payload every live entry after the first starts either on the next 4096-byte boundary
(`align4096`: the gap shorter than 4096 bytes) or exactly 64 bytes after the previous one ends (`gap64`), and every
gap byte is `0xBF`. Markers take no padding. The suite recognises a layout only when every pair of consecutive live
entries follows it.

Evidence: [code: rust/bc2-tools/src/fbrbtool.rs:739-756] [code: rust/bc2-tools/src/fbrbtool.rs:784-794] [code: rust/bc2-formats/src/fbrb/pack.rs:193-207] [test: rust/bc2-tools/src/fbrbtool.rs::layout_tokens_of_padded_payloads] [test: rust/bc2-formats/src/fbrb/pack.rs::padded_layouts] [data: stored bundles: 4 align4096 (2 Dist, 2 Package), 2 gap64 (Package)]

### FBRB-LAY-3: Header region alignment

Status: **verified**

In a padded bundle the header region (8 + *H*) is padded with zeros after the gzip member to a multiple of 4096 or
of 4 (the manifest's `hdr=`); zipped bundles are not padded, though 312 of them happen to end their header on a
multiple of 4.

Evidence: [code: rust/bc2-formats/src/fbrb/pack.rs:265-268] [code: rust/bc2-tools/src/fbrbtool.rs:761-771] [test: rust/bc2-formats/src/fbrb/pack.rs::padded_layouts] [data: stored bundles: 4 with hdr 4096, 3 with hdr 4, 4 unpadded]

### FBRB-LAY-4: plen=size2 and size2=minus64

Status: **verified**

The async `ondemand_*` bundles also set the payload length field to the last live entry's offset + size2
(`plen=size2`), and give every live entry size2 = size - 64 (`size2=minus64`: their entries are RES-wrapped,
RES-WRAP-1).

Evidence: [code: rust/bc2-tools/src/fbrbtool.rs:774-779] [test: rust/bc2-tools/src/fbrbtool.rs::layout_tokens_of_padded_payloads] [test: rust/bc2-formats/src/fbrb/pack.rs::size2_rules] [data: 3 stored bundles with plen=size2; 1,293 entries with size2 = size - 64, all in Dist and Package async ondemand bundles]

## 6. The suite's unpacked form

### FBRB-MAN-1: fbrb.manifest

Status: **verified**

The suite's unpack writes every live entry to `<folder>/<name>` and a `fbrb.manifest`: a first line
`# fbrb.manifest - entry order matters. name|type|flags(hex)|size2|size  zipped=<0|1>`, followed for a stored bundle
by the layout tokens (`layout=align4096|gap64 hdr=<1|4|4096> [plen=size2] [size2=minus64]`), then one line per entry
in archive order, markers included, `name|type|flags as 8 hex digits|size2|size`, lines ended CR LF. `pack` reads a
manifest back (UTF-8 or a byte-order mark's encoding; any line end; lines starting `#` other than the first are
comments; older manifests without the size column still pack while size2 holds) and rebuilds the bundle: unpack
then pack gives the original bytes for every bundle FbrbTool wrote and for DICE's padded layouts, and the same header
and payload for DICE's zipped bundles (only the gzip members' bytes differ, FBRB-HDR-4).

Evidence: [code: rust/bc2-tools/src/fbrbtool.rs:893-910] [code: rust/bc2-formats/src/fbrb/pack.rs:726] [test: rust/bc2-formats/src/fbrb/pack.rs::manifest_tokens] [test: rust/bc2-formats/src/fbrb/pack.rs::numbers_as_dotnet_parses_them] [test: rust/bc2-tools/src/unpack.rs::unpacks_and_packs_back] [test: core/tests/test_fbrb.py::roundtrip] [test: core/tests/test_fbrb_pack_native.py::run_bundles] [doc: bfbc2-fbrb-tool/README.md:25-41]

### FBRB-MAN-2: Writing a bundle

Status: **verified**

The suite's packer writes, in this order: `FbRB`; *H*; the header (FBRB-HDR-6 to FBRB-HDR-9, the string table by
FBRB-HDR-8) as one gzip member with zlib level-6 deflate and .NET's member header (FBRB-HDR-4), zero padded to `hdr`;
the payload, the entries in manifest order with the layout's padding, as one gzip member when zipped. Any bundle it
writes reads back to the entries it was given.

Evidence: [code: rust/bc2-formats/src/fbrb/pack.rs:222-272] [test: rust/bc2-formats/tests/writers.rs::packed_entries_come_back] [test: rust/bc2-gamefs/src/bundles_tests.rs::packs_a_folder] [test: rust/bc2-formats/tests/fbrb_robustness.rs::built_bundles_read_back]

## Worked example

`fbrb::build(&[("Levels/X.dbx", "<non-resource>", b"{binary}..."), ("Gone", "*deleted*", b"")], true)` (the
module's own doc test, `rust/bc2-formats/src/fbrb/mod.rs`) gives a header of: version 2; *S* = 43 (the strings
`Levels/X.dbx` at 0, `<non-resource>` at 13, `Gone` at 28, `*deleted*` at 33, each with its zero); *N* = 2;
entry 0 `(0, 0x00010000, 0, 11, 11, 13)`; entry 1 `(28, 0x00010000, 11, 0, 0, 33)` (`build` is a test writer and
gives markers the live flags); zipped 1; payload length 11. The payload is one gzip member of the 11 data bytes.

## Open questions

- Whether the game reads the header's version, the flags word, size2 or the payload length field, and what it does
  with a value other than the observed ones (FBRB-HDR-6, FBRB-ENT-4, FBRB-ENT-6, FBRB-HDR-9). What bit 16 of the
  flags word stands for.
- Whether the game needs the padded layouts of the async / streaming bundles (alignment for streaming reads) or
  merely tolerates them; whether a zipped async bundle would load.
- Whether the game accepts a name with a backslash, a non-ASCII byte, or two entries of one name in one bundle.
- The engine's limits: the largest bundle, entry count and header it loads (the largest in the game copy is a
  Pacific War level of about 1.5 GB of payload).
- The `.fbrb` loader's address in `BFBC2Game.exe` is not yet recorded here (layer 3).
