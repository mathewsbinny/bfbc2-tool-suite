# Unpack and pack: `.fbrb` bundle <-> folder

The suite turns a bundle into a folder of entry files and back. **Unpack** (`bc2_tools::unpack::unpack_path`, the command line
`bfbc2.exe fbrbtool unpack`, `bfbc2core.fbrb.unpack`) takes a bundle and writes every live entry to `<folder>/<name>`
plus `fbrb.manifest` (FBRB-MAN-1). **Pack** (`bc2_gamefs::bundles::pack_path`, `pack_many`, `bfbc2core.fbrb.pack`) takes such a
folder and writes a bundle (FBRB-MAN-2). Both were ported from FbrbTool (C#, retired in round R4) and are held to
the bytes FbrbTool wrote, recorded before it was deleted (`core/tests/fbrbtool_golden*.json`), but where DICE's
bytes differ from FbrbTool's (XPACK-PCK-3).

The format is layer 1's (`formats/fbrb.md`); this file states the transformation's contract: what each direction
refuses, what holds of its output, and the round trip.

## 1. Unpack

### XPACK-UNP-1: Input and refusals

Status: **verified**

Unpack refuses a bundle that is not well-formed by FBRB-HDR-1 to FBRB-ENT-8 (wrong magic, a header length past the
file, a damaged gzip member, an entry outside the payload) with an error that names the bundle, and the files it
had written by then are the ones FbrbTool left behind. It refuses an entry name that is empty, rooted (`\x`, `/x`,
`C:x`) or leads out of the folder through `..`.

[code: rust/bc2-tools/src/fbrbtool.rs:275-295] [code: rust/bc2-tools/src/fbrbtool.rs:586-633]
[test: rust/bc2-tools/src/fbrbtool.rs::round_trip_and_damage] [test: rust/bc2-tools/src/unpack.rs::unusual_bundles_as_fbrbtool_unpacks_them]
[test: core/tests/test_fbrbtool.py::damaged] [test: core/tests/test_fbrb_unpack_native.py::check_built]

### XPACK-UNP-2: Output: FbrbTool's files and manifest

Status: **verified**

For every bundle, the folder unpack writes is FbrbTool's: the same file names with their case, the same bytes, the
same `fbrb.manifest` (entry order, markers, layout tokens). Markers (`*deleted*`) get a manifest line and no file;
two entries of one name leave the later one's bytes in the file (FBRB-ENT-9).

[code: rust/bc2-tools/src/fbrbtool.rs:801-917]
[test: core/tests/test_fbrb_unpack_native.py::run_bundles] [test: core/tests/test_fbrb_unpack_native.py::check_built]
[data: every bundle of the game copy whose payload is at most 300 MB, against fbrbtool_golden_game.json by SHA-1 (test_fbrb_unpack_native.py --game)]

## 2. Pack

### XPACK-PCK-1: Input and refusals

Status: **verified**

Pack needs `fbrb.manifest` and, for every manifest line that is not a marker without a file, a regular file of
that name. It refuses (and writes nothing): a missing or unreadable file (`OSError` through Python), a folder where
a file is expected, an unsafe name (XPACK-UNP-1's rule), a manifest it cannot parse, a manifest from an older unpack
(no size column) whose size2 differs from the file's length, and a payload past 4 GB (`ValueError`). These are the
folders FbrbTool refused.

[code: rust/bc2-formats/src/fbrb/pack.rs:788-804] [code: rust/bc2-formats/src/fbrb/pack.rs:825-859]
[test: rust/bc2-formats/src/fbrb/pack.rs::safe_names] [test: rust/bc2-formats/src/fbrb/pack.rs::files_that_change_size] [test: rust/bc2-formats/src/fbrb/pack.rs::too_large]
[test: core/tests/test_fbrb_pack_native.py::check_built]

### XPACK-PCK-2: size2 of an edited entry

Status: **verified**

An entry keeps the manifest's size2 while its file has the size unpack wrote; an edited entry's size2 becomes its
new size, except in a `size2=minus64` bundle, where an edited entry of at least 64 bytes gets size - 64 (FBRB-LAY-4).
Markers keep 0.

[code: rust/bc2-formats/src/fbrb/pack.rs:575-599] [test: rust/bc2-formats/src/fbrb/pack.rs::size2_rules]

### XPACK-PCK-3: Output: FbrbTool's bytes

Status: **verified**

For any folder FbrbTool packed, pack writes FbrbTool's bundle byte for byte: zipped and stored, every padded layout,
the size2 rules, markers with and without files, old manifests, CR LF and byte-order-marked manifests. The deflate is
a byte-exact port of zlib 1.3.1's levels 4-9 (`bc2_formats::zlib`), used at level 6. One exception, DICE's bytes
winning: a zipped bundle whose payload is empty ends with DICE's 20-byte empty gzip member, where FbrbTool wrote
nothing (FBRB-DEL-2; round SPV1-fix re-recorded those records as FbrbTool's bytes + the 20, each checked so).

[code: rust/bc2-formats/src/fbrb/pack.rs:899-926] [code: rust/bc2-formats/src/zlib.rs:924-932]
[test: core/tests/test_fbrb_pack_native.py::check_built] [test: core/tests/test_fbrb_pack_native.py::check_zlib]
[test: rust/bc2-formats/src/zlib.rs::known_outputs] [test: rust/bc2-formats/src/zlib.rs::round_trips] [test: rust/bc2-formats/src/fbrb/pack.rs::padded_layouts]

### XPACK-PCK-4: The write is atomic

Status: **verified**

Pack writes `<bundle>.tmp-<process id>` beside the target and moves it over the target only when complete (retrying
a moment's lock by another process for about three seconds); on any error the temporary file is removed and the old
bundle is untouched. The payload is streamed: memory stays about 10 MB whatever the bundle's size.

[code: rust/bc2-gamefs/src/bundles.rs:1190-1210] [code: rust/bc2-gamefs/src/atomic.rs:227-229]
[test: rust/bc2-gamefs/src/atomic.rs::a_moments_lock_is_waited_out] [test: rust/bc2-gamefs/src/atomic.rs::a_real_file_is_replaced]
[test: rust/bc2-formats/src/fbrb/pack.rs::streamed_files_as_from_memory] [test: core/tests/test_fbrb_pack_native.py::check_built]

### XPACK-PCK-5: Packing many keeps order and isolates failures

Status: **verified**

`pack_many` packs each (folder, bundle) pair in parallel and returns the results in input order; a folder that
cannot be packed gives its error in its place and does not stop the others.

[code: rust/bc2-gamefs/src/bundles.rs:1212-1215] [test: core/tests/test_fbrb_pack_native.py::check_built]

## 3. The round trip

### XPACK-RT-1: pack(unpack(b)) = b for the suite's bundles

Status: **verified**

For a bundle FbrbTool or the suite wrote, and for DICE's stored (padded) bundles, unpacking and packing again gives
the same bytes. Every bundle the suite writes reads back to the entries it was given.

[test: rust/bc2-tools/src/unpack.rs::unpacks_and_packs_back] [test: rust/bc2-formats/tests/writers.rs::packed_entries_come_back]
[test: core/tests/test_fbrb_pack_native.py::run_bundles] [test: core/tests/test_fbrb.py::roundtrip]

### XPACK-RT-2: DICE's zipped bundles come back but for the gzip headers

Status: **verified**

For a zipped bundle DICE packed, pack(unpack(b)) equals b except for the two gzip members' 10-byte headers (DICE's
`xfl 0` / `os 0x0b`, the suite's `.NET` header `xfl 4` / `os 0`, FBRB-HDR-4). A zipped bundle with an empty payload
(markers only, or no entries) has one member only: the suite writes DICE's empty 20-byte payload member as DICE's
bytes (FBRB-DEL-2; since round SPV1-fix, 2026-11-22, before which it wrote none, as FbrbTool), so such a bundle
comes back but for its header member's two bytes. Every deflate stream DICE wrote is zlib level 6, so the
compressed bytes themselves agree.

[test: core/tests/test_fbrb_pack_native.py::run_bundles] [test: core/tests/test_fbrb_pack_native.py::same_but_gzip_headers]
[test: rust/bc2-formats/tests/spec_corpus.rs::fbrb_entries]
[data: all 1,086 bundles of the game copy (2026-11-16): 62 identical, 919 identical but for the gzip headers, 105 also but for the marker-only empty payload member]
[data: round SPV1-fix 2026-11-22: DICE's 105 zipped bundles with an empty payload, unpacked and packed again: DICE's bytes but for the header member's xfl / os, the 20-byte member byte for byte]
[doc: rust/BENCHMARKS_ARCHIVE_1.md:333-336]

## Open questions

- Whether the game would load a zipped marker-only bundle without DICE's empty payload member (FbrbTool's form,
  which the suite wrote before round SPV1-fix; no bundle OPFOR installs today is one; FBRB-DEL-2). The suite now
  writes DICE's form, which the game ships.
- zlib's levels 1-3 (another matcher) are not ported; nothing in the game copy needs them.
