# The DBX type system and value encoding

The binary layer (`formats/dbx.md`: the header, string table, varints, the element head byte, the DBX-ELEM, DBX-NUM
and DBX-XML rules) says how an element is stored. This file says what the elements **mean**: the vocabulary of
tags and attributes, how a field's value kind follows from its type, and how the engine's value types (integers,
floats, hashes, booleans, enumerations, strings, vectors, transforms, arrays, structures, references) are laid
out in those elements.

The file carries no class schema: DBX files do not declare their classes, so a field's type is learnt from the
data (TYPE-SCH). Class and field names quoted here are the ones the suite's code names.

**Corpus.** "The level corpus" is the 41 `level-00.fbrb` bundles of OPFOR's `game\Dist\win32\levels` (the 33
stock levels plus `mp_common`, `sp_common` and OPFOR's Onslaught bundles), 79,526 binary partitions, 2,313,497
instances, 34.86 million elements, read 2026-11-19 (counts of tags, attributes and shapes only).

## 1. The element vocabulary

### TYPE-ELEM-1: Six tags make every partition

Status: **observed**

A partition's element tree uses exactly six tags: `partition` (the root), `instance`, `field`, `complex`, `array`
and `item`. No other tag occurs.

[data: level corpus: 26,113,943 field, 3,235,928 complex, 2,313,497 instance, 2,027,833 item, 1,102,409 array, 79,526 partition; no other tag]

### TYPE-ELEM-2: Nesting

Status: **observed**

The tree is `partition` > `instance` > members, where a member is a `field`, `complex` or `array`; a `complex`
holds members; an `array` holds `item`s or, for arrays of structures, `complex`es. Seen parent > child pairs:
partition>instance, instance>field / complex / array, complex>field / complex / array, array>item / complex.
Nothing else nests: an `item` and a `field` never have children.

[data: level corpus parent>child counts: instance>field 19,266,821; complex>field 6,847,122; partition>instance 2,313,497; array>item 2,027,833; instance>complex 1,887,137; array>complex 1,134,815; instance>array 1,078,337; complex>complex 213,976; complex>array 24,072; no other pair]

### TYPE-ELEM-3: One root, the partition, holding only instances

Status: **implemented**

A partition file has exactly one root element, tagged `partition`, and every child of the partition is an
`instance`. Readers take the first root as the partition and its `instance` children as the instances.

[code: rust/bc2-formats/src/dbx/model.rs:171-183] [test: rust/bc2-formats/src/dbx/model.rs::lookups]
[data: level corpus: 79,526 of 79,526 files have one root; in every one the partition's children are all instance]

### TYPE-ELEM-4: Element kinds by tag

Status: **observed**

The element kind (the head byte's high nibble, `formats/dbx.md`) follows the tag and the member's type:

| Tag | Kinds seen | Meaning |
|---|---|---|
| `partition` | 0xA container | always |
| `instance` | 0xA container | always, except 4 empty text instances (TYPE-ELEM-6) |
| `field` | 7 numbers, 2 text, 6 scalar | number fields; strings, enums and references; booleans and bytes |
| `complex` | 0xA container, 7 numbers | a structure; or a vector / matrix stored as one number list (TYPE-CPLX) |
| `array` | 0xA container, 2 text, 7 numbers | items; an empty array (TYPE-ARR-2); a number array (TYPE-ARR-3) |
| `item` | 2 text | always: a reference or a string (TYPE-ARR-1) |

[data: level corpus tag:kind counts: field:7 10,534,080; field:2 9,710,414; field:6 5,869,449; instance:0xA 2,313,493; item:2 2,027,833; complex:7 1,858,095; complex:0xA 1,377,833; array:2 664,969; array:0xA 285,441; array:7 151,999; partition:0xA 79,526; instance:2 4]

### TYPE-ELEM-5: The container byte is always 0

Status: **observed**

A container element's value byte (`formats/dbx.md`) carries no meaning in game data: it is 0 on every container.
Writers write 0.

[data: dbx-tools FORMAT.md section 6: 0 on all 43,289 game files of that count] [doc: bfbc2-dbx-tools/FORMAT.md:134]

### TYPE-ELEM-6: Empty instances

Status: **observed**

Four instances of the level corpus are text elements with empty text instead of containers (an instance with no
members). Readers must accept an instance that is not a container; `bc2-scene` and the catalogue treat it as an instance
without fields.

[data: level corpus: 4 instances of kind 2, all with empty text]

## 2. Attributes

### TYPE-ATTR-1: Attribute sets per tag

Status: **observed**

Each tag has a fixed attribute set, in this order:

| Tag | Attributes (in file order) |
|---|---|
| `partition` | `guid`, `primaryInstance`, `exportMode` |
| `instance` | `guid`, `type`; the primary instance also `id` (INST-PRIM-3) |
| `field` | `name`; a reference field `ref`, `name` |
| `complex` | `name`; none for a structure that is an array element |
| `array` | `name` |
| `item` | `ref`; none for a string item |

The order matters for the XML form (DBX-XML) and for DbxEdit's import matching, and writers keep it.

[data: level corpus tag:attribute-list counts: field:name 20,206,769; field:ref,name 5,907,174; instance:guid,type 2,233,973; instance:guid,type,id 79,524; complex:name 2,101,113; complex:(none) 1,134,815; array:name 1,102,409; item:ref 2,015,781; item:(none) 12,052; partition:guid,primaryInstance,exportMode 79,526]

### TYPE-ATTR-2: The member name

Status: **implemented**

A member's field name is its `name` attribute. Members are looked up by the first direct child whose `name`
equals the field name (exact case). A structure that is an array element has no name and is addressed by its
position.

[code: rust/bc2-formats/src/dbx/model.rs:165-169] [code: rust/bc2-formats/src/dbxedit/check.rs:261-288]
[test: rust/bc2-formats/src/dbx/model.rs::lookups]

### TYPE-ATTR-3: The instance type is `<Namespace>.<Class>`

Status: **observed**

An instance's `type` attribute is the class name qualified by a namespace, `<Namespace>.<Class>` with one dot.
The suite compares types by their suffix after the last dot (`.LevelData`, `.ReferenceObjectData`), never by the
namespace. Twelve namespaces occur in the level corpus (`Emitter`, `GameSharedResources`, `Entity`, `Audio`,
`Terrain`, `Animation`, `Render`, `Decal`, `Physics`, `Weather`, `WorldRender`, `Overgrowth`, ...).

[data: level corpus: 14 distinct namespace prefixes; the largest Emitter 1,378,673 instances, GameSharedResources 357,107, Entity 266,203]
[code: rust/bc2-scene/src/scene.rs:81-93] [code: rust/bc2-formats/src/dbxedit/check.rs:261-263]

### TYPE-ATTR-4: exportMode

Status: **observed**

The partition's `exportMode` attribute is `All` in every partition of the level corpus. No other value is known
and no reader uses it; writers copy it.

[data: level corpus: exportMode=All on 79,526 of 79,526 partitions]

## 3. Value kinds

### TYPE-VAL-1: A field's kind follows its type, not its value

Status: **implemented**

The engine's value types map onto the three stored kinds:

| Engine type | Stored as | Notes |
|---|---|---|
| string, enumeration | text (kind 2) | an enum value is its name: `Team1`, `TeamNeutral`, `SSHumanPlayer` |
| reference | text (kind 2) with a `ref` attribute | the text is empty; the target is the attribute (REF-FORM) |
| bool | scalar (kind 6) | second byte 0 or 1 |
| u8 / enum-like byte | scalar (kind 6) | second byte the value |
| int32, uint32, float32, hash | numbers (kind 7), one value of size 4 | the bytes do not say which (TYPE-NUM-1) |
| uint16 | numbers, one value of size 2 | |
| double | numbers, one value of size 8 | rare |
| Vec2 / Vec3 / Vec4, LinearTransform | `complex` of kind 7 | TYPE-CPLX |
| structure | `complex` container | |
| array | `array` | TYPE-ARR |

A field of a given class keeps its kind in every instance; the schema reader records the shapes seen per (type,
field path) and writes a field only in the shape the data shows.

[code: rust/bc2-levelwrite/src/schema.rs:36-75] [code: rust/bc2-levelwrite/src/schema.rs:114-140]
[test: rust/bc2-levelwrite/src/schema.rs::types_from_the_data]

### TYPE-VAL-2: Booleans and bytes are two-byte scalars

Status: **observed**

A scalar element holds two bytes. The first is always 1; the second is the value. Booleans use 0 and 1; a few
byte fields use other values (2, 4 and 6 seen, on channel counts). The engine's scalars are all kind 6 in the
level corpus.

[data: level corpus scalars: kind 6 with bytes 01 00 3,542,382; 01 01 2,325,640; 01 02 1,169; 01 06 256; 01 04 2; no other kind or first byte]
[doc: bfbc2-dbx-tools/FORMAT.md:134]

### TYPE-VAL-3: Text values

Status: **observed**

A non-reference text field holds a string: a name, an asset path or an enumeration value. 228,431 text fields of
the level corpus are empty. Strings are shared through the partition's string table (`formats/dbx.md`), so the same
text is stored once.

[data: level corpus non-reference text fields: 3,574,809 non-empty, 228,431 empty]

### TYPE-VAL-4: Enumerations are stored by name

Status: **implemented**

An enumeration field is a text field holding the enumerator's name (`Team1`, `Team2`, `TeamNeutral`,
`SSHumanPlayer`, ...), not its number. The suite's readers compare the name.

[code: rust/bc2-scene/src/scene.rs:448-473] [test: rust/bc2-scene/tests/scene.rs::loads_a_level]

## 4. Numbers

### TYPE-NUM-1: A 4-byte value does not say integer or float

Status: **implemented**

A 4-byte number is a big-endian 32-bit pattern. Whether it is an int32, a uint32, a hash or a float32 is a
property of the field, not of the bytes. The suite decides from the data: a nonzero value whose top byte is 0x00
or 0xFF reads as a whole number (`looks_whole`), any other as a float; per (type, field path) the schema counts
both and takes the majority. A field whose values never decide (only zeros) is "unknown": only 0 may be written to
it, because 0 is the same bits either way.

[code: rust/bc2-levelwrite/src/schema.rs:102-111] [code: rust/bc2-levelwrite/src/schema.rs:178-195]
[test: rust/bc2-levelwrite/src/schema.rs::types_from_the_data]

### TYPE-NUM-2: Hash and id fields are whole numbers

Status: **verified**

The fields `OriginalHashedWaveName`, `HashedName`, `HashedWaveName`, `OnRoadMaterialNameHashes`, `Hash`, `Id` and
`CompositeMeshPartNames` hold 32-bit hashes or ids; their 4-byte values are printed as signed integers whatever
their bits look like.

[code: rust/bc2-formats/src/dbx/number.rs:9-18] [test: rust/bc2-formats/src/dbx/number.rs::number_texts]

### TYPE-NUM-3: The XML form loses the integer / float distinction

Status: **verified**

The XML form (DBX-XML) prints a 4-byte value as an integer when its top byte is 0 or its exponent bits are all
ones, so tiny floats (denormals such as 7.3e-40) print as integers (524288) and whole decimals print without a
point (1.0 as `1`). The XML cannot be turned back into bytes without the type: DbxEdit's import takes each value's
kind and size from a reference file (DBX-XML import rules). A tool that needs exact values reads the binary.

[code: rust/bc2-formats/src/dbx/number.rs:189-200] [test: rust/bc2-formats/src/dbx/number.rs::number_texts]
[doc: bfbc2-dbx-tools/FORMAT.md:126-137]

### TYPE-NUM-4: Sizes 2 and 8

Status: **observed**

Besides size 4, number elements of size 2 (unsigned 16-bit: the cull sphere keys `SphereKeyX` .. `SphereKeyW`,
`SourceId` / `TargetId`) and size 8 (rare) occur. No other size occurs.

[data: level corpus number elements by count x size: 1x4 7,145,451; 1x2 3,003,402; 4x4 1,566,863; 16x4 240,726; 2x4 52,087; 1x8 68; empty 0x0 95,341]
[data: MP_005 level bundle: 1x2 only on SphereKeyX/Y/Z/W (24,798 each) and SourceId / TargetId (88 each)]

## 5. Vectors and transforms

### TYPE-CPLX-1: Vectors are `complex` number lists of four floats

Status: **observed**

A Vec3 or Vec4 member is a `complex` element of kind 7 holding four 4-byte floats (x, y, z, w). For a Vec3 the
fourth word is padding: 0x3F800000 (1.0), 0x00000000 or 0xCDCDCDCD (the Microsoft debug heap's fill pattern) in the
data; writers keep it as read. A Vec2 is a `complex` of two floats.

[data: MP_005 level bundle: 4x4 complexes 103 distinct names; fourth words 3F800000 31,328, CDCDCDCD 9,555, 00000000 8,086, other values on Vec4 colours]
[data: level corpus: 1,566,863 4x4 and 52,087 2x4 number elements]

### TYPE-CPLX-2: LinearTransform is sixteen floats, four rows

Status: **implemented**

A `LinearTransform` member (`Transform`, `InitialRootBoneTransform`, ...) is a `complex` of kind 7 with sixteen
4-byte floats: four rows of four, the rows **right**, **up**, **forward** and **translation**, each row's first
three words used. The position is words 12, 13 and 14. Each row's fourth word is padding (all four 0, or all four
0xCDCDCDCD); an edit writes the three words of each row and keeps the fourth.

[code: rust/bc2-scene/src/scene.rs:395-397] [code: rust/bc2-formats/src/dbxedit/check.rs:77-81]
[code: rust/bc2-formats/src/dbx/edit.rs:795-799] [test: rust/bc2-scene/tests/scene.rs::loads_a_level]
[test: rust/bc2-levelwrite/src/write.rs::move_delete_duplicate_set]
[data: MP_005 level bundle: 7,343 Transform complexes, fourth words all 0 on 4,562 and all CDCDCDCD on 2,781]

### TYPE-CPLX-3: Transform lists

Status: **observed**

A list of transforms (a model group's `Transforms`) is an `array` of kind 7 holding sixteen floats per placement,
in the order of the matching item list (`Assets`).

[data: the level corpus's 232 CompositeHavokAsset instances: Transforms has 16 values per Assets item in 232 of 232]
[code: rust/bc2-scene/src/scene.rs:547-560]

## 6. Arrays

### TYPE-ARR-1: Items are references or strings

Status: **observed**

An `item` is always a text element. With a `ref` attribute (and empty text) it is a reference element of a
reference array; without one its text is a string element of a string array.

[data: level corpus: 2,015,781 items with ref and empty text, 12,052 string items, none of another kind]

### TYPE-ARR-2: An empty array is an empty text element

Status: **observed**

An array with no elements is stored as an `array` element of kind 2 (text) with the empty string, not as an empty
container. A container array always has at least one child. Writers making an array empty must write this form;
readers must take a text `array` as empty.

[data: level corpus: 664,969 arrays of kind 2, all with empty text; 285,441 container arrays, all with 1 or more children]

### TYPE-ARR-3: Number arrays

Status: **observed**

An array of numbers (`ZOcclusionLookup`, `CompositeMeshPartNames`, `NeighbourLinks`, transform lists ...) is one
`array` element of kind 7: the count and size varints and the values (DBX-NUM). An empty number array has count 0.

[data: level corpus: 151,999 arrays of kind 7; MP_005: 0-count number arrays on 10 field names, CompositeMeshPartNames 1,656]

### TYPE-ARR-4: Arrays of structures

Status: **observed**

An array of structures holds unnamed `complex` children, one per element, in order. Event connections
(LEVEL-EVT-1), spawn data and member data are such arrays.

[data: level corpus: array>complex 1,134,815; complex elements without a name 1,134,815]

## 7. The schema

### TYPE-SCH-1: No class schema in the files

Status: **inferred**

A partition carries no class definitions: no type ids, no field list, no defaults. The class layouts live in the
executable. Members a class has but an instance does not store do not appear; in the level corpus every instance of
a class lists its members in the same order (the catalogue's `order`), so the data is written from the full
class.

[inferred: no tag or attribute names a class definition (TYPE-ELEM-1, TYPE-ATTR-1); the importer's catalogue builds field order, shapes and defaults per class from the partitions themselves, which would be unnecessary with a schema in the files]
[code: rust/bc2-convert/src/dbxcatalogue.rs:39-59]

### TYPE-SCH-2: The suite learns types from the data

Status: **verified**

The suite's schema (`bc2_levelwrite::schema`, the importer's `dbxcatalogue`) is built by walking partitions: per
(instance type, field path) the shapes seen (`complex-container`, `array-text`, `ref`, `field-text`, `num4x1`,
`k6` ...), whole / decimal counts for 4-byte values, scalar values and up to 400 texts. A field's type is then
text, reference, bool (only 0 / 1 seen), byte, int, float, vector, transform, array, complex, unknown number or
other numbers.

[code: rust/bc2-levelwrite/src/schema.rs:1-34] [test: rust/bc2-levelwrite/src/schema.rs::types_from_the_data]

## Open questions

- The class layouts: the executable's type registry (field offsets, defaults, enum tables) is not yet read; the
  suite's types are learnt from data, so a field no stock instance sets to a nonzero value has an unknown type.
- Whether the engine reads the scalar's first byte (always 1) or the container byte (always 0); both are kept.
- The meaning of `exportMode` and whether any value but `All` is accepted.
- Which Vec3 fields the engine reads the fourth word of (none is known to be read).
- Size-8 numbers: which fields, and whether they are doubles or 64-bit integers (68 values in the corpus).
- BF1943's partitions: the same vocabulary is assumed (the importer reads them with the same reader); not counted
  here.
