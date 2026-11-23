# Textures: ITexture (PC) and XenonTexture (Xbox 360)

A texture is a resource entry of FBRB type `ITexture` (`formats/res.md`), `<name>.res`. On the PC its payload is
the **ITexture** layout below; the Xbox 360 copy of the same asset (BFBC2's console bundles, BF1943) has the same
92-byte header in big-endian, followed by the **XenonTexture** data in the Xenos GPU's tiled order. The suite reads
PC textures (`rust/bc2-convert/src/texcheck.rs`, `rust/bc2-render/src/material.rs`, the editor's pictures), writes
them from pictures (`rust/bc2-convert/src/texwrite.rs`) and converts console textures to PC
(`rust/bc2-convert/src/texture.rs`, from `bfbc2-console-convert/texture.py`).

## Header

### TEX-HDR-1: The 92-byte header

Status: **verified** · Variants: PC, Xbox 360, BF1943

| Offset | Type | Field |
|---|---|---|
| 0 | u32 | version (3) |
| 4 | u32 | type: 0 2D, 1 cube map (six faces) |
| 8 | u32 | format (TEX-FMT-1) |
| 12 | u32 | flags (TEX-FLAG-1) |
| 16 | u32 | width in texels |
| 20 | u32 | height in texels |
| 24 | u32 | depth (0 on PC) |
| 28 | u32 | mip count *m* (1 - 15) |
| 32 | 15 x u32 | level sizes (TEX-HDR-2 PC, TEX-X-1 Xbox 360) |
| 92 | | the texel data |

PC little-endian, Xbox 360 big-endian. Format, flags, size and mip count carry over unchanged from console to PC.

Evidence: [code: rust/bc2-convert/src/texture.rs:43-45] [code: rust/bc2-convert/src/texture.rs:272-309] [code: rust/bc2-convert/src/texcheck.rs:11-20] [test: rust/bc2-convert/src/texture_tests.rs::refusals_carry_the_reference_messages] [test: rust/bc2-render/src/material.rs::texture_header_checks] [data: 3,010 distinct PC ITexture entries of the game copy (2026-11-19): version 3 in all (the 1,184 wrapped ones read after TEX-WRAP-1); of the 1,826 unwrapped, type 0 x 1,808, 1 x 18, depth 0 in all] [doc: bfbc2-console-convert/FORMAT.md:77-81]

### TEX-HDR-2: PC level sizes and file length

Status: **verified** · Variants: PC

On the PC the *i*-th level size is the byte length of level *i* of one face; sizes after the last level are 0. The
file is exactly `92 + faces x (sum of the m level sizes)` bytes, faces 6 for a cube map, else 1.

Evidence: [data: 1,826 of 1,826 unwrapped and 1,184 of 1,184 wrapped (TEX-WRAP-1) PC textures of the game copy (2026-11-19)] [code: rust/bc2-render/src/material.rs:426-454] [test: rust/bc2-formats/tests/spec_corpus.rs::textures] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: 3,826 distinct ITexture names: level sizes and file length as stated in 3,826 (889 wrapped read after TEX-WRAP-1)]

### TEX-HDR-3: Dimensions and mip chains

Status: **verified** · Variants: PC, Xbox 360, BF1943

Width and height are 1 - 16,384, the mip count 1 - 15 and at most `floor(log2(max(w, h))) + 1`; level *i* is
`max(w >> i, 1) x max(h >> i, 1)`. PC textures in the game copy are powers of two on both sides (largest 4,096) with 1
- 13 levels; a block-compressed PC texture's width and height are multiples of 4 (the renderer refuses others).

Evidence: [code: rust/bc2-convert/src/texture.rs:277-285] [code: rust/bc2-render/src/material.rs:441-448] [test: rust/bc2-convert/src/texture_tests.rs::refusals_carry_the_reference_messages] [data: 1,826 unwrapped PC textures: all power-of-two, mips 1-13 (2026-11-19)]

### TEX-WRAP-1: The 64-byte RES header on award pictures

Status: **verified** · Variants: PC

Some `ITexture` entries start with a 64-byte resource header before the 92-byte texture header: bytes 0-3 `RES` and
the byte 0x02, then the type name `ITexture` zero-padded to offset 0x24, five zero u32s (0x24-0x37), then two LE u32s
at 0x38 and 0x3C, both the length of what follows the 64 bytes. The texture after it obeys TEX-HDR-1 / TEX-HDR-2.
Every `ITexture` under `awards/` (rank, award, loading-screen and level pictures) is wrapped so; no other is. A
reader of textures must skip the 64 bytes when the entry starts with `RES` (see `formats/res.md`).

Evidence: [data: 1,184 of 1,184 distinct `awards/` ITextures of the game copy wrapped, 0 of 1,826 others (2026-11-19; the 9 bundles over 200 MB not read); both length words equal the inner length in all 1,184] [code: rust/bc2-editor/src/catalog.rs:185-190] [test: rust/bc2-editor/src/catalog.rs::a_texture_decoded] [test: rust/bc2-formats/tests/spec_corpus.rs::textures] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: 889 of 889 distinct `awards/` ITextures wrapped, 0 of 2,937 others; both length words equal the inner length in all 889]

## Formats and flags

### TEX-FMT-1: Format codes

Status: **verified** · Variants: PC, Xbox 360, BF1943

| Code | Format | Unit | Bytes per unit | PC | Xbox 360 |
|---|---|---|---|---|---|
| 0 | DXT1 / BC1 | 4 x 4 block | 8 | yes | yes |
| 18 | DXT1 (normal maps) | 4 x 4 block | 8 | yes | yes |
| 1 | DXT3 / BC2 | 4 x 4 block | 16 | yes | yes |
| 2 | DXT5 / BC3 | 4 x 4 block | 16 | yes | yes |
| 19, 20 | BC5 / DXN (two channels) | 4 x 4 block | 16 | yes | yes |
| 3 | DXT5A: DXT5's alpha half alone | 4 x 4 block | 8 | no | yes |
| 5 | RGB565 (red top 5 bits, blue low 5) | texel | 2 | no | yes |
| 9 | 32-bit: PC B, G, R, A bytes; Xbox 360 big-endian A8R8G8B8 words | texel | 4 | yes | yes |
| 10 | L8 (one channel) | texel | 1 | yes | yes |
| 13 | RGBA16F (four halves) | texel | 8 | yes | yes |

Evidence: [code: rust/bc2-convert/src/texture.rs:75-85] [test: rust/bc2-convert/src/texture_tests.rs::conversions_as_the_reference] [data: PC formats over 1,826 unwrapped textures (2026-11-19): 0 x 871, 2 x 288, 18 x 255, 10 x 226, 20 x 89, 9 x 48, 1 x 24, 19 x 24, 13 x 1; no 3 or 5] [doc: bfbc2-console-convert/FORMAT.md:93-100]

### TEX-FMT-2: PC level size by format

Status: **verified** · Variants: PC

A PC level of `lw x lh` texels takes `ceil(lw / e) x ceil(lh / e) x bytes per unit` bytes (TEX-FMT-1, `e` the unit's
edge), except L8 (10), whose rows are padded with zeros to a multiple of 4 bytes: `ceil(lw / 4) x 4 x lh` (a 2 x 2
level is 8 bytes, a 1 x 1 level 4).

Evidence: [data: every level of 1,826 unwrapped PC textures matches, per format: 0 x 871, 2 x 288, 18 x 255, 10 x 226, 20 x 89, 9 x 48, 1 x 24, 19 x 24, 13 x 1 (2026-11-19)] [code: rust/bc2-convert/src/texture.rs:243-255] [doc: bfbc2-console-convert/FORMAT.md:147-149] [test: rust/bc2-formats/tests/spec_corpus.rs::textures] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: every level of 3,826 distinct PC textures matches; unwrapped formats 0 x 1,537, 2 x 486, 18 x 375, 10 x 289, 20 x 86, 19 x 74, 9 x 59, 1 x 30, 13 x 1]

### TEX-FLAG-1: Header flags

Status: **verified** · Variants: PC, Xbox 360, BF1943

Bit 0 (1): sRGB - the PC renderer binds the texture through an sRGB view. Bit 1 (2): a texture DICE's build halves
for the console; set only on PC copies (the console build halves and clears it). Of BFBC2's 1,468 textures with an Xbox
360 and a PC copy, all 539 whose console copy is smaller have it on the PC, and 11 of the other 929. A converted
console texture is at the console's size, so it keeps the console's flags with bit 1 clear.

Evidence: [data: flags over 1,826 unwrapped PC textures of the game copy (2026-11-19): 0 x 620, 1 x 665, 3 x 335, 2 x 206; no other bit set] [code: rust/bc2-convert/src/texture.rs:52-53] [code: rust/bc2-convert/src/texture.rs:388-397] [doc: bfbc2-console-convert/FORMAT.md:83-91] [test: rust/bc2-formats/tests/spec_corpus.rs::textures] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: flags over 2,937 distinct unwrapped PC textures: 0 x 890, 1 x 1,110, 3 x 563, 2 x 374; no other bit]

## PC data

### TEX-PC-1: PC texel order

Status: **verified** · Variants: PC

After the header: each level's units (blocks or texels) in rows, top to bottom, level 0 first. A cube map is face
after face (+X first, in the file's face order), each face with all its levels.

Evidence: [code: rust/bc2-convert/src/texture.rs:487-538] [code: rust/bc2-convert/src/texcheck.rs:20] [test: rust/bc2-convert/src/texcheck.rs::levels_of_a_header] [test: bfbc2-console-convert/tests/test_texture_helpers_native.py::main] [doc: bfbc2-console-convert/FORMAT.md:104-105]

### TEX-PC-2: Block encodings the suite decodes

Status: **verified** · Variants: PC

DXT1 (0, 18): two RGB565 end colours and 16 2-bit indices; the four-colour palette when colour 0 > colour 1, else
three colours and transparent black. DXT3 / DXT5 (1, 2): an 8-byte alpha block then a DXT1 colour block. BC5 (19,
20): two DXT5-style alpha blocks, red then green. The DXT5 alpha block: two 8-bit end values and 16 3-bit indices,
eight values when end 0 > end 1, else six and 0, 255.

Evidence: [code: rust/bc2-convert/src/texcheck.rs:103] [code: rust/bc2-convert/src/texture.rs:541-560] [test: rust/bc2-convert/src/texcheck.rs::dxt1_block] [test: rust/bc2-convert/src/texcheck.rs::bc5_and_l8] [test: bfbc2-console-convert/tests/test_texture_helpers_native.py::main]

### TEX-PC-3: Textures the suite writes from pictures

Status: **verified** · Variants: PC

The suite's writer makes each level by 2 x 2 averaging down to 1 x 1, and encodes DXT1 (with or without 1-bit
alpha) and DXT5 alpha blocks along each block's principal axis; L8 atlases are halved with round-half-up.

Evidence: [code: rust/bc2-convert/src/texwrite.rs:119] [test: rust/bc2-convert/src/texwrite.rs::mips_down_to_one] [test: rust/bc2-convert/src/texwrite.rs::flat_block_is_one_colour] [test: rust/bc2-convert/src/texwrite.rs::l8_rounds_up_halves]

## Xbox 360 data (XenonTexture)

### TEX-X-1: Console level sizes

Status: **implemented** · Variants: Xbox 360, BF1943

The console header's size list holds one size, that of the whole tiled layout (TEX-X-3), not one per level. The data
must be at least that long.

Evidence: [code: rust/bc2-convert/src/texture.rs:329-338] [doc: bfbc2-console-convert/FORMAT.md:79-81]

### TEX-X-2: Byte order of console data

Status: **verified** · Variants: Xbox 360, BF1943

Console block data (formats with 2- to 16-byte units, and RGB565) is stored with every 16-bit word byte-swapped; L8
is **not** swapped; format 9 is big-endian A8R8G8B8 words, not swapped by 16 bits.

Evidence: [code: rust/bc2-convert/src/texture.rs:236-241] [code: rust/bc2-convert/src/texture.rs:487-512] [code: rust/bc2-convert/src/texture.rs:447-485] [test: rust/bc2-convert/src/texture_tests.rs::conversions_as_the_reference] [doc: bfbc2-console-convert/FORMAT.md:105-106]

### TEX-X-3: Storage per level

Status: **verified** · Variants: Xbox 360, BF1943

Each level has a storage of `pitch x align(blocks high, 32) x bytes per unit`, rounded up to 4,096 bytes, where
`pitch = align(blocks wide, 32)` (units of TEX-FMT-1). Level 0 uses the texture's own size; later levels use the
power-of-two size `(2^ceil(log2 w)) >> i` (and the same for h), at least one unit. The storages follow each other:
level 0 of every slice (face), then level 1 of every slice, and so on.

Evidence: [code: rust/bc2-convert/src/texture.rs:153-213] [test: rust/bc2-convert/src/texture_tests.rs::layouts_as_the_reference] [doc: bfbc2-console-convert/FORMAT.md:107-115]

### TEX-X-4: Xenos 2D tiling

Status: **verified** · Variants: Xbox 360, BF1943

Inside a storage `pitch` units wide, unit (x, y) is at unit index `XGAddress2DTiledOffset(x, y, pitch, log2(bytes
per unit))`, computed exactly as `rust/bc2-convert/src/texture.rs` `tiled` (32 x 32-unit macro tiles; L8 uses
log2 = 0).

Evidence: [code: rust/bc2-convert/src/texture.rs:101-114] [test: rust/bc2-convert/src/texture_tests.rs::tiling_as_the_reference] [test: bfbc2-console-convert/tests/test_texture_helpers_native.py::main]

### TEX-X-5: The packed mip tail

Status: **verified** · Variants: Xbox 360, BF1943

From the first level whose shorter side is 16 texels or less, the remaining levels share one storage (the tail),
each at a unit offset in it: for the tail's levels 0 - 2 at (0, 16 >> k) when the texture is wider than tall, else
(16 >> k, 0); from level 3 at `(2^(lw - base) >> (k - 2), 0)` when wider than tall, else `(0, 2^(lh - base) >> (k -
2))` (texels, divided by the unit edge). When the texture's shorter side is 16 texels or less, level 0 is in the tail
too. A cube map stores the tail six times.

Evidence: [code: rust/bc2-convert/src/texture.rs:117-136] [code: rust/bc2-convert/src/texture.rs:186-206] [test: rust/bc2-convert/src/texture_tests.rs::tiling_as_the_reference] [test: rust/bc2-convert/src/texture_tests.rs::layouts_as_the_reference] [doc: bfbc2-console-convert/FORMAT.md:110-114]

### TEX-X-6: Conversion moves blocks

Status: **verified** · Variants: Xbox 360 -> PC

Block formats and L8 / RGBA16F convert by moving units from their tiled place to the PC order (TEX-PC-1) with the
16-bit swap undone; no block is decoded or re-encoded. Three console-only forms are decoded: DXT5A (3) to PC L8 (10)
with bit 1 of the flags cleared, RGB565 (5) to PC format 9 (B, G, R, 255, channels widened by bit replication), and
console format 9 to PC B, G, R, A.

Evidence: [code: rust/bc2-convert/src/texture.rs:360-538] [test: rust/bc2-convert/src/texture_tests.rs::conversions_as_the_reference] [test: bfbc2-console-convert/tests/test_texture_native.py::quick] [test: bfbc2-console-convert/tests/test_texture.py::test_argb8_console_texels_become_pc_bgra] [data: every XenonTexture of BF1943's source (704) and OPFOR's Onslaught builds (451) converted, 0 refused (doc)] [doc: rust/BENCHMARKS_ARCHIVE_1.md:574-580]

### TEX-X-7: What the converter refuses

Status: **verified** · Variants: Xbox 360

Refused: a file shorter than 92 bytes; dimensions or mip counts outside TEX-HDR-3; a type other than 2D / cube or a
depth over 1; a cube map in format 3, 5 or 9 (2D only); a format outside TEX-FMT-1; data shorter than the layout;
and a layout that would read past the data (no texture of either game has one).

Evidence: [code: rust/bc2-convert/src/texture.rs:272-309] [code: rust/bc2-convert/src/texture.rs:340-350] [test: rust/bc2-convert/src/texture_tests.rs::refusals_carry_the_reference_messages] [test: rust/bc2-convert/src/texture_tests.rs::guards]

### TEX-X-8: Console twins are separately compressed

Status: **observed** · Variants: Xbox 360, PC

DICE compressed each platform's textures separately: of the console textures a stock PC bundle has at the same size
and format, 4 are byte-identical and 188 the same picture re-encoded (level-0 correlation over 0.9). Where the
console copy is half the PC's size, each console level matches the PC's next smaller one. Equality with the PC file
is therefore not a conversion test; the picture is.

Evidence: [data: 420 console textures of the four Onslaught builds against stock PC bundles (2026-10-30, doc)] [doc: bfbc2-console-convert/FORMAT.md:120-133]

## Open questions

- Flags bits other than 0 and 1 (PC values seen: 0, 1, 2, 3 only).
- What else the 64-byte RES header (TEX-WRAP-1) carries in other resource types, and why only `awards/` textures
  have it (owned by `formats/res.md`).
- The PC face order of cube maps is taken from DICE's files, not from the executable.
- Console format 9 cube maps and console formats outside TEX-FMT-1 (none seen) are refused, not specified.
- The PC Dx9 / Dx10 / Dx11 renderers' handling of non-power-of-two textures: none in the game copy, so untested.
