# BIGF / BIG4 archives

EA's BIG archive, used in BFBC2 only as the container of a menu movie (`SwfMovie` resources: [apt.md](apt.md)). A
flat list of named members; no compression.

The suite reads it with `bc2_formats::bigf::parse` (the movie readers) and extracts it with `fbrbtool big`; it writes
it with `aptgraft.py` (a movie written back).

### BIGF-HDR-1: Header

Status: **verified** · Variants: PC, Xbox 360

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | magic `BIGF` (FbrbTool's `big` also accepts `BIG4`) |
| 4 | 4 | the archive's total length, **little-endian** |
| 8 | 4 | member count *n*, big-endian |
| 12 | 4 | header size: the offset just past the index, big-endian |
| 16 | | the index (BIGF-IDX-1) |

The index words are big-endian on every platform; only the length word at 4 is little-endian (on the PC and the
Xbox 360 alike).

[data: 1,027 of 1,027 SwfMovies of the game copy: magic BIGF, the word at 4 = the length read little-endian, the word at 12 = the end of the index read big-endian]
[test: rust/bc2-formats/src/bigf.rs::members] [test: bfbc2-menu-tools/tests/test_bc2loc_native.py::bigf_checks] [code: rust/bc2-formats/src/bigf.rs:13-18] [code: rust/bc2-tools/src/fbrbtool.rs:966-980]
[doc: bfbc2-menu-tools/aptgraft.py:100-101]

### BIGF-IDX-1: Index entries

Status: **verified** · Variants: PC, Xbox 360

*n* entries from offset 16, each `{u32 offset, u32 size, name}` (big-endian words; the name UTF-8, zero-terminated,
no padding: the next entry starts right after the zero). Offsets count from the start of the archive. Readers keep
the members in index order; a name listed twice keeps its first place and its last data (the reference's dict).

[test: rust/bc2-formats/src/bigf.rs::members] [code: rust/bc2-formats/src/bigf.rs:19-30]

### BIGF-DAT-1: Member data follows the index, 4-byte aligned, in index order

Status: **observed** · Variants: PC

In the game's movies the members follow the index in index order, each starting on a multiple of 4 (0 - 3 bytes of
padding after the index and between members), the last ending at the archive's end. The suite's movie writer lays
them out the same way.

[data: 1,027 of 1,027 SwfMovies: member offsets ascending and multiples of 4, the last member ending at the archive's end]
[doc: bfbc2-menu-tools/aptgraft.py:100-101]

## Open questions

- `BIG4`: no BIG4 archive is in the game copy; which files (if any, on any platform) use it, and whether its length
  word has the same byte order.
- Whether the game reads the length and header-size words at all, or only the count and the index.
- Whether member order or alignment matters to the game (only the movie's three names are looked up).
