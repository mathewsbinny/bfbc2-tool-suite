# Xbox 360 and PC resource formats

What the converter knows about Bad Company 2's resources on the two platforms, and how each conversion was
checked. Every rule here was learnt from, and is tested against, resources that exist on both platforms (the
console's `mp2_common` / level bundles and the PC's `mp_common`).

## 1. HavokPhysicsData (physics)

A resource is DICE's wrapper followed by a Havok 5.5.0 binary packfile. Console and PC files of the same asset
have the same size: only the byte order differs.

### 1.1 The wrapper

Console values are big-endian, PC values little-endian; "bytes" are copied as they are.

| Field | Size |
|---|---|
| part count *n* | u32 |
| part count again | u32 |
| part hashes | *n* × u32 |
| vector count *a* | u16 |
| vectors | *a* × 3 floats |
| box count *c* | u16 |
| boxes (min, max) | *c* × 6 floats |
| a byte run (per-part bytes; the same on both platforms) | bytes |
| 1 | u32 |
| hash count *k* | u32 |
| hashes | *k* × u32 |

The packfile follows (its magic `57 E0 E0 57 10 C0 C0 10` reads the same on both platforms). The byte run's length
is not stored: it ends where "1, *k*, *k* hashes" ends exactly at the packfile, which is unique in every file seen.

### 1.2 The packfile

Header (64 bytes): magic, user tag, file version, layout rules (pointer size, little-endian, reuse padding,
empty-base optimisation: `04 00 00 01` on the Xbox 360, `04 01 00 01` on the PC), section count, contents section
and offset, class-name section and offset, the version string `Havok-5.5.0-r1` (16 bytes), flags, padding. Then a
48-byte header per section: a 19-byte name, a 0xFF byte and seven offsets (data start, local, global and virtual
fixups, exports, imports, end). The sections are `__classnames__`, `__types__` (empty) and `__data__`.

- `__classnames__`: entries of a u32 class signature, the byte 0x09 and the name ending in 0; 0xFF padding.
- Fixup tables: local (from, to), global (from, section, to), virtual (object, class-name section, name offset);
  u32 values; 0xFF padding at the end of each.
- `__data__`: the objects. Each virtual fixup names an object and its class. The converter reverses each member's
  bytes by the member's type, from the class layouts in the game executable: the hkClass initializers (push 12
  arguments: name, parent, size, interfaces and count, enums and count, members and count, defaults, attributes,
  flags; load the hkClass object into ecx; call the constructor) and their 24-byte member records (name, class,
  enum, type, subtype, C array size, flags, offset, attributes). Parents' members come first. Arrays, struct
  pointers and strings are followed through the local and global fixups; bytes are claimed by one value width
  only, and a byte claimed with two widths stops the conversion.

Havok 5.5 member types: 0 void, 1 bool, 2 char, 3 int8, 4 uint8, 5 int16, 6 uint16, 7 int32, 8 uint32, 9 int64,
10 uint64, 11 real, 12 vector4, 13 quaternion, 14 matrix3, 15 rotation, 16 qstransform, 17 matrix4, 18 transform,
19 zero, 20 pointer, 21 function pointer, 22 array, 23 in-place array, 24 enum, 25 struct, 26 simple array,
27 homogeneous array, 28 variant, 29 C string, 30 ulong, 31 flags, 32 half, 33 string pointer. 289 classes are
read from the PC executable (285 hk*, plus DICE's own such as `HavokMaterials`).

### 1.3 Checked

`convert.py verify-havok` on the 358 HavokPhysicsData resources the console and PC share (2026-10-30): 355
converted; 3 are different assets (their sizes differ). Every wrapper is byte-identical. In the packfiles every byte
is identical except values the game does not read or rebuilds at load, all of which also differ between the PC's
own files:

| Bytes | Value | Why it does not matter |
|---|---|---|
| 51,344 | the 4th 16-bit index of each triangle in `hkpStorageExtendedMeshShapeMeshSubpartStorage.indices16` (index striding 8, three indices used) | padding |
| 10,253 | `hkpCollidableBoundingVolumeData` after `min[0]` / `max[0]` | a cached broadphase volume marked invalid (`min[0] > max[0]`) on both platforms: rebuilt when the body enters the world |
| 297 | `hkpExtendedMeshShapeTrianglesSubpart.triangleOffset` | computed at load (the PC files hold junk there) |
| 69 | `hkpMotion.savedQualityTypeIndex` | read only with a saved motion; these have none |

With the console bundles of all four Onslaught builds, 400 shared resources are compared, all equivalent. All 594
HavokPhysicsData resources of the four console Onslaught levels convert without an error.

## 2. Textures (XenonTexture -> ITexture)

### 2.1 Header

92 bytes on both platforms (console big-endian): version (3), type (0 2D, 1 cube), format, flags, width, height,
depth, mip count, then 15 mip sizes. The PC lists each level's size (one face's, for a cube); the console one size,
the whole layout. Format, flags, size and mip count carry over unchanged.

| Code | Format | Bytes per 4x4 block |
|---|---|---|
| 0 | DXT1 | 8 |
| 18 | DXT1 (normal maps) | 8 |
| 1 | DXT3 | 16 |
| 2 | DXT5 | 16 |
| 19, 20 | BC5 / DXN (two channels) | 16 |

### 2.2 Data

PC: each level's blocks in rows, level after level; a cube map face after face (+X ... in the file's order), each
face with all its levels. Console: 16-bit words byte-swapped, blocks tiled by the Xenos GPU:

- a level's storage is its block size rounded up to 32 x 32 blocks (levels after the first: the power-of-two size),
  4 KB aligned; level 0 first, then levels 1, 2, ...; block (x, y) of a storage `pitch` blocks wide is at
  XGAddress2DTiledOffset(x, y, pitch, log2(bytes per block));
- packed mip tail: from the first level whose shorter side is 16 texels or less, the remaining levels share one
  storage, each at a block offset in it (`packed()` in texture.py); when the texture's shorter side is
  16 texels or less, level 0 is in it too;
- cube maps: level 0 of the six faces, then level 1 of the six faces, ... (each face a full storage of the level),
  the packed tail six times.

Converting moves blocks; it never decodes or re-encodes one.

### 2.3 Checked

DICE compressed each platform's textures separately, so byte equality with the PC file is rare; the check is the
picture (`texture.check`, `convert.py verify-texture`, `tests/test_texture.py`, 2026-10-30):

- 420 console textures of the four Onslaught builds (DXT / BC5, 2D and cube) convert; every one is exactly as long
  as the layout says;
- of the textures a stock PC bundle (unpacked: the PC's mp_common, sp_common, mp_005, sp_005 and level bundles - no
  folder holding console-derived files) has at the same size and format, 4 are byte-identical, 188 the same picture
  re-encoded (level 0 correlation over 0.9; 5 of them cube maps, every face and level correlating at 0.97 or more),
  1 different art (DICE changed it). Every converted file's mip chain (each level against its larger neighbour
  scaled down) is no worse than the PC file's own (within +3, or +25 % for high-contrast masks), which a misplaced
  block or level would break;
- where the console texture is half the PC's size, each level matches the PC's next smaller one;
- the 2 x 2 and 1 x 1 levels of BC5 textures differ from the PC's (DICE's PC compressor encodes those blocks
  differently; the console's blocks are not in the PC files at all).

### 2.4 Not converted

Console 3 and 5 (the PC's 8-bit format 10 and 32-bit format 9 textures: masks, decals; 31 files in the four
builds) are smaller on the console than the PC's uncompressed data (about 4 and 16 bits per texel), so they are
another, compressed encoding; they are refused until it is identified.
