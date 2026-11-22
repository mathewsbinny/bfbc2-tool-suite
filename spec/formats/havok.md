# HavokPhysicsData: DICE's wrapper, the Havok packfile, MOPP

A `HavokPhysicsData` resource is a blueprint's collision and rigid-body description: **DICE's wrapper** (part
hashes, boxes, the per-part material run, the material list) followed by a **Havok binary packfile** (Havok
5.5.0 in BFBC2, 4.6.1 in BF1943). The packfile's MOPP code (the bounding-volume tree of a mesh shape) is its own
byte code (section 5).

| Variant | Wrapper | Packfile |
|---|---|---|
| PC (BFBC2) | little-endian | Havok 5.5.0, little-endian (layout rules `04 01 00 01`) |
| Xbox 360 (BFBC2) | big-endian | Havok 5.5.0, big-endian (layout rules `04 00 00 01`); the same size as the PC file |
| BF1943 (Xbox 360) | big-endian, its material table possibly big-endian | Havok 4.6.1, big-endian (section 4) |

The suite reads packfiles (`rust/bc2-formats/src/havok.rs`), converts Xbox 360 -> PC
(`rust/bc2-convert/src/havok.rs`, layouts from the game executable: `rust/bc2-convert/src/hkclasses.rs`), ports
BF1943 -> BFBC2 (`rust/bc2-convert/src/havokport.rs`) and decodes MOPP (`rust/bc2-convert/src/mopp.rs`). The
earlier description is `bfbc2-console-convert/FORMAT.md` section 1.

Counts are of the game copy (2026-11-19): 4,128 HavokPhysicsData resources, all in level bundles.

## 1. The resource

### HAVOK-RES-1: Names

Status: **observed**

A blueprint's physics resource is named `<blueprint>_Havok.res`; a level's model groups (static, rigid, forest,
middle growth: the placed objects merged into one shape) use `<group>_HavokAsset.res`. A few names repeat the
suffix (`<name>_Havok_Havok.res`); the blueprint is then `<name>` (COLL-IDX-2).

[data: game copy: 3,595 `_Havok.res`, 529 `_HavokAsset.res`, 4 `_Havok_Havok.res`; all 4,128 of type HavokPhysicsData in level bundles]

### HAVOK-RES-2: Wrapper, then packfile

Status: **verified** · Variants: PC, Xbox 360, BF1943

The resource is the wrapper (section 2) immediately followed by the packfile. The packfile is found by its 8-byte
magic `57 E0 E0 57 10 C0 C0 10` (the same bytes in both byte orders); its offset is not stored and is not aligned.

[code: rust/bc2-formats/src/havok.rs:38]
[code: rust/bc2-formats/src/havok.rs:213-217]
[test: rust/bc2-formats/src/havok.rs::reads_meshes_and_boxes]
[data: game copy: magic found in 4,128 of 4,128; packfile offsets 56 - 74,206 bytes, offset mod 4 = 0 x 3,051, 1 x 266, 2 x 535, 3 x 276]

## 2. DICE's wrapper

### HAVOK-WRAP-1: Layout

Status: **verified** · Variants: PC, Xbox 360, BF1943

PC little-endian, Xbox 360 big-endian:

| Field | Type |
|---|---|
| part count n | u32 |
| part count again | u32 (equal to n) |
| part words | n x u32 (what they hold depends on the resource: below) |
| vector count a | u16 |
| vectors | a x 3 x f32 |
| box count c | u16 |
| boxes | c x 6 x f32 (min x, y, z, max x, y, z) |
| the part run | bytes (HAVOK-RUN-1, HAVOK-RUN-2) |
| 1 | u32 |
| material count k | u32 |
| material hashes | k x u32 |
| the packfile | (HAVOK-PF-1) |

The part run's length is not stored: it ends where "1, k, k hashes" ends exactly at the packfile. Readers search
for that position; it is unique in every file seen.

The n part words are not always hashes. In a blueprint's physics resource they are the part names' hashes (`h = h *
33 ^ byte` from 5381 over the lower-case name: `bc2_formats::havok::part_hash`, read as `havok::Parts.hashes`); in a
model group's `_HavokAsset.res` they are each physics part's owning instance's leaf (HAVOK-RUN-3), not hashes.

[code: rust/bc2-convert/src/havokport.rs:934-963]
[code: rust/bc2-convert/src/havok.rs:745-794]
[test: tests/test_converter_units.py::part_materials]
[test: bfbc2-console-convert/tests/test_havok.py::main]
[data: game copy: n equals the second count in 4,128 of 4,128; the 1, k, hashes tail found in 4,128; n 1 - 14,752, a 0 - 60, c 1 - 115, k 1 - 22]

### HAVOK-WRAP-2: Xbox 360 -> PC wrapper

Status: **verified** · Variants: Xbox 360

The Xbox 360 wrapper converts field by field: counts and hashes as u32, a and c as u16, vectors and boxes as f32,
the part run copied as it is (the same bytes on both platforms), then 1, k and the hashes as u32. The converted
wrapper is byte-identical to the PC file's on every shared resource.

[code: rust/bc2-convert/src/havok.rs:745-794]
[test: bfbc2-console-convert/tests/test_havok.py::main]
[test: bfbc2-console-convert/tests/test_console_f2a_native.py::wrapper_x]
[data: 358 HavokPhysicsData the console and PC share: 355 converted with identical wrappers (3 are different assets)]

### HAVOK-RUN-1: Part run, index form

Status: **verified** · Variants: PC, BF1943

On every physics resource but the model groups the part run is `u16 0`, `u8 d` (the number of distinct
materials the parts use), `u8 t` (the highest index), then, when d is not 1, one u8 per part: the part's index into
the material list (HAVOK-MAT-1). With d = 1 every part uses index t and the run is 4 bytes; otherwise 4 + n bytes.
The indices used must be exactly d distinct values, the highest t, and t must be below k.

[code: rust/bc2-convert/src/havokport.rs:976-1012]
[code: rust/bc2-convert/src/havokport.rs:1015-1034]
[test: tests/test_converter_units.py::part_materials]
[test: bfbc2-bf1943-import/tests/test_havokport.py::material_slots]
[data: game copy: 3,599 runs in this form (every `_Havok.res`), 2,652 with d = 1, 947 one byte per part, every one of the stated length]

### HAVOK-RUN-2: Part run, material-table form (model groups)

Status: **observed** · Variants: PC, BF1943

The model groups' `_HavokAsset.res` wrappers open the run with a table instead: `u16 m`, `u16 k2`, k2 x u32, m x u16
ascending end positions whose last equals k2 (all little-endian on the PC; BF1943's may be big-endian and are
swapped by the port). Bytes the suite does not decode follow up to the material tail. The suite reads the table's
shape only; it refuses to read or write part indices in this form.

[code: rust/bc2-convert/src/havokport.rs:561-601]
[code: rust/bc2-convert/src/havokport.rs:605-632]
[data: game copy: 529 runs whose first u16 is not 0; all 529 are `_HavokAsset.res` files and all 529 have a valid little-endian table (m not equal to n); the undecoded bytes after the table are 8 - 8,506 bytes]

### HAVOK-RUN-3: The model groups' wrapper decoded

Status: **verified** · Variants: PC

Round SG1. In a model group's `_HavokAsset.res` the wrapper's fields mean, against its group's member types
(LEVEL-MG-5): the n part words are **not hashes** but each physics part's owning instance's leaf (`Enums`),
repeated per part, n the parts of all instances; there are no vectors; the c boxes and the table's k2 words are one
per **type part** (each member type's parts of one instance, in type order; k2 = c); the m end positions are the
cumulative parts per instance of the m member types. The undecoded bytes are the index-form run of HAVOK-RUN-1 over
the n instance parts: `d`, `t`, then one material byte per part when d is not 1, and every instance of a type has the
same bytes. A group laid out again (instances deleted or added, of kinds it holds) needs only these lists made again;
the boxes, the table, the material list and the packfile stay.

[code: rust/bc2-levelwrite/src/members.rs:575-790]
[test: rust/bc2-levelwrite/src/members.rs::the_physics_reads_and_relays]
[test: rust/bc2-levelwrite/tests/model_group_members.rs::every_stock_group_keeps_the_rules]
[data: 131 of 131 groups of 31 levels: part words = the owners' leaves per part, m = the member types, ends = the cumulative per-instance parts, c = k2 = the last end, run d / t then n bytes or none; 78 of 78 long runs the same bytes per instance of a type]

### HAVOK-MAT-1: The material list

Status: **observed**

The wrapper's tail hashes are the resource's physics material list, one u32 hash per material name. DICE keeps it
in step with two other lists: the HavokAsset partition's `MaterialMappings` and the packfile's `HavokMaterials`
object. The part run indexes this list; a list may only grow at its end (existing indices keep their material).

[code: rust/bc2-convert/src/havokport.rs:1037-1060]
[test: tests/test_converter_units.py::part_materials]
[test: bfbc2-bf1943-import/tests/test_havokport.py::material_slots]
[data: on every object HavokAsset BFBC2 ships, HavokMaterials equals MaterialMappings and the part run names PartInfoDatas' materials]
[doc: opfor:research/PHYSICS_MATERIALS.md:46-56]

### HAVOK-MAT-2: Which material slot the game reads

Status: **observed**

The game has three material slots. The **part material** (the part run's index; it mirrors the partition's
`PartInfoData.MaterialName`) decides damage to destructible parts, bullet and explosion alike. The **face
material** (the first `hkpMeshMaterial` of an `hkpExtendedMeshShape` subpart: per subpart, not per triangle) is the
hit material on static model-group members. The **runtime material id** is built from the material grid (grid index
plus flag bits: Penetrable 0x80000000, SeeThrough 0x20000000). Penetration, impact effects, sounds and decals follow
the hit material's flags and relations; vehicle contacts use the face for mesh / MOPP shapes, the part for convex
shapes.

[exe: part material read at vtable+0xE0 (0xAAED90) and per health state at 0x6D84D0; face material at 0xAAEEE0 / 0xAADE10; runtime id built at 0x10029E0; bullet part damage 0x722990 (server), 0x8D6730 (client); explosion 0x6A19A0; penetration 0x6F6570; impact effect 0x822D60; contacts 0xAB0710 / 0x70F720]
[data: DICE's 991 object HavokAssets: in 922 the per-part bytes name exactly PartInfoData.MaterialName's materials, 69 share one byte]
[doc: opfor:research/MATERIAL_SLOTS_RE.md:13-48]

## 3. The Havok 5.5.0 packfile

### HAVOK-PF-1: Header

Status: **verified** · Variants: PC, Xbox 360

64 bytes from the magic, in the platform's byte order:

| Offset | Size | Field | BFBC2 |
|---|---|---|---|
| 0 | 8 | magic | `57 E0 E0 57 10 C0 C0 10` |
| 8 | 4 | user tag | 0 |
| 12 | 4 | file version | 5 |
| 16 | 4 | layout rules: pointer size, little-endian, reuse padding, empty-base optimisation | `04 01 00 01` PC, `04 00 00 01` Xbox 360 |
| 20 | 4 | section count | 3 |
| 24 | 4 | contents section index | 2 (`__data__`) |
| 28 | 4 | contents section offset | 0 |
| 32 | 4 | contents class-name section index | 0 |
| 36 | 4 | contents class-name offset | the root class's name in `__classnames__` |
| 40 | 16 | contents version | `Havok-5.5.0-r1`, NUL, 0xFF |
| 56 | 8 | flags, padding | |

[code: rust/bc2-formats/src/havok.rs:218-230]
[code: rust/bc2-convert/src/havokport.rs:513-522]
[test: rust/bc2-formats/src/havok.rs::classes_are_read]
[data: game copy: 4,128 of 4,128 PC packfiles: user tag 0, version 5, layout `04 01 00 01`, 3 sections, `Havok-5.5.0-r1`]

### HAVOK-PF-2: Section headers

Status: **verified** · Variants: PC, Xbox 360, BF1943

After the header, one 48-byte header per section: a 19-byte NUL-padded name, the byte 0xFF, then seven u32: the
section's data start (from the packfile's magic) and, relative to that start, the local fixups' start, the global
fixups' start, the virtual fixups' start, the exports' start, the imports' start, and the section's end. Each table
ends where the next begins. BFBC2's sections are `__classnames__`, `__types__` (empty) and `__data__`, in that
order.

[code: rust/bc2-formats/src/havok.rs:231-256]
[test: rust/bc2-formats/src/havok.rs::refusals_and_damage]
[data: game copy: 4,128 of 4,128 have exactly (`__classnames__`, `__types__`, `__data__`), byte 19 = 0xFF on all 12,384 headers]

### HAVOK-PF-3: Fixup tables

Status: **verified** · Variants: PC, Xbox 360, BF1943

| Table | Entry | Meaning |
|---|---|---|
| local | 2 x u32 (from, to) | a pointer slot at `from` points at `to`, both in this section |
| global | 3 x u32 (from, section, to) | a pointer slot at `from` points at `to` in another section |
| virtual | 3 x u32 (object, section, name) | an object starts at `object`; its class name is at `name` in `section` (`__classnames__`) |

Offsets are relative to their section's start. An entry whose first word is 0xFFFFFFFF is padding (each table is
padded to its end with 0xFF bytes).

[code: rust/bc2-formats/src/havok.rs:261-284]
[test: rust/bc2-formats/src/havok.rs::reads_meshes_and_boxes]
[test: rust/bc2-convert/src/havok_tests.rs::objects_members_pointers_and_arrays]

### HAVOK-PF-4: Class names section

Status: **verified** · Variants: PC, Xbox 360, BF1943

`__classnames__` is a list of entries `u32 class signature`, the byte 0x09, the class name, NUL; padded to a
multiple of 16 with 0xFF. A virtual fixup's name offset points at the name (5 bytes after the entry's start).

[code: rust/bc2-convert/src/havokport.rs:131-171]
[code: rust/bc2-convert/src/havokport.rs:276-286]
[test: bfbc2-bf1943-import/tests/test_havokport.py::compare]
[data: game copy: 260,287 class-name entries, the separator byte 0x09 on all]

### HAVOK-PF-5: Classes present

Status: **observed** · Variants: PC

Every BFBC2 packfile's class-name list has the core set (`hkRootLevelContainer`, `hkpPhysicsData`,
`hkpPhysicsSystem`, `hkpRigidBody`, `hkpCollidable`, `hkpMaxSizeMotion`, `hkpKeyframedRigidMotion`, DICE's
`HavokMaterials` / `HavokMaterial`, ...: 47 names in all 4,128). Mesh shapes add the MOPP set (`hkpMoppBvTreeShape`,
`hkpMoppCode`, ...: 3,890 files) and the extended mesh set (`hkpExtendedMeshShape`, `hkpStorageExtendedMeshShape`,
its subpart classes: 3,878). 70 distinct names occur. The data section holds no `hkClass` objects: class layouts
are not in the files (HAVOK-CLS-1).

[data: game copy: class-name census over 4,128 packfiles]
[doc: rust/BENCHMARKS.md:4058]

### HAVOK-CLS-1: Class layouts come from the executable

Status: **verified** · Variants: PC

Member layouts are read from `BFBC2Game.exe`'s hkClass initializers: a function that pushes the constructor's 12
arguments (name, parent, size, interfaces, interface count, enums, enum count, members, member count, defaults,
attributes, flags), loads the static hkClass object into `ecx` (`B9 imm32`) and calls the constructor (`E8 rel32`).
Each member is a 24-byte record:

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | name pointer |
| 4 | 4 | class pointer (struct / pointer target) |
| 8 | 4 | enum pointer |
| 12 | 1 | type |
| 13 | 1 | subtype (array / pointer element type) |
| 14 | 2 | C array size (i16) |
| 16 | 2 | flags |
| 18 | 2 | offset in the object |
| 20 | 4 | attributes |

A class's members follow its parent's. 289 classes are read (285 `hk*` and DICE's own such as `HavokMaterials`).

[exe: BFBC2Game.exe hkClass initializers, the constructor the most-called target of the `B9 .. E8` sites]
[code: rust/bc2-convert/src/hkclasses.rs:120-190]
[test: bfbc2-console-convert/tests/test_havok_classes_native.py::build_pe]
[test: bfbc2-console-convert/tests/test_havok.py::main]
[doc: bfbc2-console-convert/FORMAT.md:44-56]

### HAVOK-CLS-2: Member types

Status: **implemented**

Havok 5.5 member types and the width each value's bytes are reversed by:

| Type | Name | Bytes, value width |
|---|---|---|
| 0 | void | - |
| 1-4 | bool, char, int8, uint8 | 1, 1 |
| 5, 6, 32 | int16, uint16, half | 2, 2 |
| 7, 8, 11, 30 | int32, uint32, real, ulong | 4, 4 |
| 20, 21, 29, 33 | pointer, function pointer, C string, string pointer | 4, 4 |
| 9, 10 | int64, uint64 | 8, 8 |
| 12, 13 | vector4, quaternion | 16, 4 |
| 14, 15, 16 | matrix3, rotation, qstransform | 48, 4 |
| 17, 18 | matrix4, transform | 64, 4 |
| 19 | zero | - |
| 22, 23 | array, in-place array | followed through the fixups |
| 24, 31 | enum, flags | by subtype |
| 25 | struct | by its class |
| 26, 27 | simple array, homogeneous array | followed |
| 28 | variant | followed |

[code: rust/bc2-convert/src/havok.rs:37-63]
[test: rust/bc2-convert/src/havok_tests.rs::objects_members_pointers_and_arrays]

### HAVOK-X360-1: Xbox 360 and PC packfiles differ only in byte order

Status: **verified** · Variants: PC, Xbox 360

Both platforms use the same layout rules apart from the byte order (pointer size 4, no padding reuse, empty-base
optimisation on), so every field has the same offset and only its bytes are reversed by its width. A conversion
walks each object of the data section member by member from its class layout (parents first), follows arrays,
pointers and strings through the fixups, and reverses each value; a byte claimed by two different widths stops it.
Bytes that differ from DICE's own PC file after a correct conversion are values the game does not read or rebuilds:

| Bytes | Value |
|---|---|
| the 4th u16 of each triangle in `hkpStorageExtendedMeshShapeMeshSubpartStorage.indices16` | padding |
| `hkpCollidableBoundingVolumeData` after `min[0]` / `max[0]` | a cached volume marked invalid (min[0] > max[0]), rebuilt on entering the world |
| `hkpExtendedMeshShapeTrianglesSubpart.triangleOffset` | computed at load |
| `hkpMotion.savedQualityTypeIndex` without a saved motion | not read |

[code: rust/bc2-convert/src/havok.rs:577-581]
[code: rust/bc2-convert/src/havok.rs:646-720]
[test: rust/bc2-convert/src/havok_tests.rs::a_byte_converted_as_two_widths_is_refused]
[test: bfbc2-console-convert/tests/test_havok.py::main]
[data: 358 shared resources: 355 convert, every other byte identical (51,344 padding, 10,253 bounding-volume, 297 triangle-offset, 69 saved-quality bytes differ); all 594 console HavokPhysicsData of the four Onslaught levels convert]

### HAVOK-TRI-1: Collision triangles as the suite extracts them

Status: **verified**

The suite's collision triangles of a packfile (local space): every `hkpStorageExtendedMeshShapeMeshSubpartStorage`'s
vertices (hkVector4 rows, pointer at +8, count at +12) and `indices16` (pointer at +0x14, count at +0x18) in
groups of four (three corners and a padding index; a count not a multiple of 4 is damage, a subpart with an index
past its vertex count is skipped); then, for each `hkpConvexVerticesShape`, the 12 triangles of the bounding box of
its first array member between offsets 0x10 and 0x5C holding 1 - 9,999 FourVectors (x0..x3, y0..y3, z0..z3 rows).
The convex box is the suite's approximation, not the game's shape.

[code: rust/bc2-formats/src/havok.rs:472-535]
[test: rust/bc2-formats/src/havok.rs::reads_meshes_and_boxes]
[test: rust/bc2-formats/src/havok.rs::subparts_with_indices_out_of_range_are_skipped]
[test: rust/bc2-formats/src/havok.rs::index_groups_must_be_whole]
[test: core/tests/test_geometry_native.py::main]

### HAVOK-AABBW-1: Model-group w lanes

Status: **verified** · Variants: PC

On BFBC2's model-group shapes (an `hkpExtendedMeshShape` of shape subparts) DICE's exporter leaves non-zero w
lanes: each shape subpart's `translation.w` is 1.0 when it has children (0 on Havok's empty last subpart), the
rotation columns' w lanes are `2(yz - xw)`, `2(yw + xz)`, 0 from the subpart's quaternion, and the shape's
`aabbCenter.w` / `aabbHalfExtents.w` are those lanes run through the AABB arithmetic in f32: per child (box or
convex vertices), centre.w = 1 + lanes . child centre, half.w = |lanes| . child half extents + convex radius; the
subpart's lo / hi over its children, widened by the box stored so far; centre = (hi + lo) / 2, half = (hi - lo) / 2,
subpart by subpart in file order. A shape with triangle subparts gets no w from this rule.

[code: rust/bc2-convert/src/havokport.rs:663-719]
[test: tests/test_converter_units.py::havok_model_group]
[test: bfbc2-bf1943-import/tests/test_havokport.py::model_group_w]
[data: bit-identical to DICE's on all 299 model-group shapes of BFBC2's levels; translation.w 1.0 on all 277,950 subparts with children]
[doc: opfor:research/HAVOK_AABB_W.md:1-60]

## 4. BF1943: Havok 4.6.1

### HAVOK-V461-1: The 4.6.1 packfile

Status: **verified** · Variants: BF1943

BF1943's physics packfiles (Xbox 360, big-endian) carry the contents version `Havok-4.6.1` at header offset 40.
Their class names lack 5.5.0's `p` (`hkRigidBody` for `hkpRigidBody`); every object has the same size as in 5.5.0
but two classes changed layout:

| Class | 4.6.1 -> 5.5.0 |
|---|---|
| hkRigidBody | collidable gains a 40-byte cached bounding volume (written marked invalid), loses the deactivator pointer, gains the SPU collision callback (BFBC2's filters 3 / 1) and extended listeners; the motion packs its two velocity limits into bytes after the damping (hkUFloat8). Copied ranges (4.6.1 -> 5.5.0, bytes): 0 -> 0 (48), 48 -> 88 (52), 100 -> 140 (44), 148 -> 184 (12), 160 -> 208 (16), 176 -> 224 (160), 336 -> 384 (4), 348 -> 388 (8), 368 -> 400 (96); constants at 48 (`00 00 00 01`) and 202 (`03 01`) |
| hkMoppBvTreeShape | the code pointer moves from 24 to 16, the child shape pointer from 20 to 52 |

`hkMoppCode` gains a build type at +44 (1: built without chunk subdivision, as BFBC2's). The 4.6.1-only
`hkSpatialRigidBodyDeactivator` / `hkRigidBodyDeactivator` objects have no 5.5.0 counterpart and are dropped. The
5.5.0 file is written with BFBC2's class-name section, sections in the order class names, types, data, and file
version 5.

[code: rust/bc2-convert/src/havokport.rs:174-190]
[code: rust/bc2-convert/src/havokport.rs:250-261]
[code: rust/bc2-convert/src/havokport.rs:394-395]
[code: rust/bc2-convert/src/havokport.rs:513-536]
[test: bfbc2-bf1943-import/tests/test_havokport.py::compare]
[data: BF1943's Kurogane converted pairs BFBC2's own Kurogane object for object (62 objects) and field for field (floats within 0.1 %) but for values rebuilt at load and MOPP lengths DICE rebuilt]

### HAVOK-V461-2: hkUFloat8 velocity limits

Status: **implemented** · Variants: BF1943, PC

5.5.0's motion stores its two velocity limits as hkUFloat8 bytes: indices into a 256-entry table of increasing
f32 values from 0 to about 10^6, its second entry 0.01. The port takes the table from `BFBC2Game.exe` (found by
those properties) and writes the nearest entry (the first of equals).

[code: rust/bc2-convert/src/havokport.rs:196-246]
[exe: the hkUFloat8 decoding table in BFBC2Game.exe: 256 increasing f32, 0.0 first, 0.01 second, the last between 9e5 and 1.1e6]

## 5. MOPP byte code

### MOPP-SPACE-1: Quantized space

Status: **verified** · Variants: PC, Xbox 360, BF1943

A point p is quantized to `q = (p - info.xyz) * info.w` (the `hkpMoppCode` code info: offset and scale), 24-bit
integers. A command compares bytes of `(q - o) >> s`, with o = (0, 0, 0) and s = 16 at the start.

[code: rust/bc2-convert/src/mopp.rs:202-218]
[test: tests/test_converter_units.py::mopp_vm]

### MOPP-OP-1: Opcodes

Status: **verified** · Variants: PC, Xbox 360, BF1943

The same byte code in Havok 4.6.1 and 5.5.0 (multi-byte operands big-endian on every platform):

| Opcode | Operands | Meaning |
|---|---|---|
| 0x01-0x04 | bx, by, bz | rescale: o += b << s, s -= opcode (zoom into a cell); s below 0 is an error |
| 0x05-0x08 | 1-4 bytes | jump forward by the operand |
| 0x09 / 0x0A | 1 / 2 bytes | terminal offset += operand |
| 0x0B | 4 bytes | terminal offset = operand |
| 0x10-0x1C | hi, lo, j | split on x, y, z, y+z, y-z, x+z, x-z, x+y, x-y, x+y+z, x+y-z, x-y+z, x-y-z (sums averaged, differences offset by 255 first): v <= hi -> next; v >= lo -> next + j |
| 0x20-0x22 | sp, j | split on x / y / z at one value: v <= sp -> next; v >= sp -> next + j |
| 0x23-0x25 | hi, lo, j1 (u16), j2 (u16) | split with 16-bit jumps: v <= hi -> next + j1; v >= lo -> next + j2 |
| 0x26-0x28 | min, max | bound on x / y / z: outside -> no terminal on this path |
| 0x30-0x4F | | terminal (opcode - 0x30) + offset |
| 0x50-0x53 | 1-4 bytes | terminal operand + offset |

Both branches of a split may be taken; comparisons are inclusive. Any other opcode is an error.

[code: rust/bc2-convert/src/mopp.rs:94-193]
[test: rust/bc2-convert/src/mopp.rs::terminals_and_splits]
[test: rust/bc2-convert/src/mopp.rs::errors_and_refusals]
[test: tests/test_converter_units.py::mopp_vm]
[data: every opcode of the 88 physics files both games ship, BF1943's converted and DICE's own, is in this set]

### MOPP-KEEP-1: BF1943's MOPP code is valid in BFBC2

Status: **verified** · Variants: BF1943

A converted BF1943 packfile keeps its 4.6.1-built MOPP code: it differs from DICE's rebuilt code only in split
choices and jump offsets. Every triangle's own box reaches its terminal, rays through the tree hit what brute force
hits, and the trees are as tight as DICE's.

[code: rust/bc2-convert/src/mopp.rs:231-262]
[test: bfbc2-bf1943-import/tests/test_havokport.py::mopp_check]
[test: rust/bc2-convert/src/mopp.rs::ray_hits]
[data: 88 shared physics files: 54,245 triangles of BF1943's 236 converted codes all reach their terminals; 11,800 rays equal brute force; a triangle's own box returns 28.3 % of the keys against DICE's 27.7 %]

## Open questions

- The hash function of the wrapper's material hashes (the suite copies and compares them, never computes one):
  needed to add a material that has no hash yet. (Closed for the part words: blueprints hash the part names with
  `bc2_formats::havok::part_hash`, model groups hold the owners' leaves: HAVOK-WRAP-1, HAVOK-RUN-3.)
- The wrapper's vectors (a x 3 f32) and boxes (c x 6 f32): presumably per-part pivots and bounds; not decoded.
- MATERIAL_SLOTS_RE's one LIKELY row: the hit material on plain placed objects (part or face), to be settled by the
  playtest probe.
- The header's flags word at 56 and the exports / imports tables (empty in every file the suite has walked).
- What the game does with a model-group shape whose w lanes are 0 (BF1943's): the suite writes DICE's values, but
  whether the game reads them is not known.
- PS3 packfiles (big-endian, another layout-rules word) are out of scope.
