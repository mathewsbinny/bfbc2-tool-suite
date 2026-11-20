# The XML instance reader

Not a game format: the rules by which the suite's tools find instances and their references in the **XML form** of a
level partition (DbxEdit's export: [dbx.md](dbx.md)) without a full parse. Several tools (NavKit, the test
harness, the stock-MP AI tools) read placed objects, spawns and AI orders this way. `core/bfbc2core/xmlinst.py` and
`bc2_formats::xmlinst` implement them.

### XMLI-ORD-1: Attribute order is not fixed

Status: **verified**

In the XML of a partition an element's attributes may come in either order: DICE's files write
`<instance guid="..." type="...">` and `<field ref="..." name="...">`; Napisal's Onslaught levels write
`<instance type="..." guid="...">` and mostly `<field name="..." ref="...">`. A matcher must not depend on the order:
tools that did read one Onslaught level as having no placed objects, spawns or AI orders. One-attribute elements
(`<complex name=...>`, `<array name=...>`, `<item ref=...>`) are not affected.

[data: ONS_MP_002: 17,454 fields name-first, 118 ref-first (2026-10-20)]
[test: rust/bc2-formats/src/xmlinst.rs::either_attribute_order] [test: core/tests/test_xmlinst_native.py::quick]
[doc: core/bfbc2core/xmlinst.py:4-8]

### XMLI-INST-1: Instances

Status: **verified**

An instance is the text matched by `<instance\b([^>]*)>(.*?)</instance>` (the body lazy, across lines): its type and
guid are the `type` and `guid` attributes of the opening tag (the last of two attributes with one name wins; a
missing one is empty), its body everything up to the first `</instance>`. Instances are taken in document order, the
next search starting after the closing tag. With a type suffix only instances whose type ends with it are returned
(`Entity.ReferenceObjectData`, `.SoldierSpawnEntityData`).

[test: rust/bc2-formats/src/xmlinst.rs::either_attribute_order] [test: core/tests/test_xmlinst_native.py::quick] [test: core/tests/test_xmlinst_native.py::game] [code: rust/bc2-formats/src/xmlinst.rs:62-94]

### XMLI-REF-1: A field's reference

Status: **verified**

`field_ref(body, name)` is the `ref` of the first `<field ...>` (or `<field .../>`) element in the body whose `name`
is `name` and that has a `ref`; none when no such element exists. Attributes are `(\w+)="([^"]*)"` pairs.

[test: rust/bc2-formats/src/xmlinst.rs::either_attribute_order] [test: core/tests/test_xmlinst_native.py::quick] [code: rust/bc2-formats/src/xmlinst.rs:99-130] [code: rust/bc2-formats/src/xmlinst.rs:26-54]

### XMLI-REF-2: Word characters outside ASCII are decided by the reference

Status: **implemented**

Python's `\w` is Unicode-aware; where a match would depend on whether a non-ASCII character is a word character, the
Rust matcher declines and the Python patterns decide. The game's partitions are ASCII in tag and attribute names, so
this does not arise on game data.

[code: rust/bc2-formats/src/xmlinst.rs:10-21]

## Open questions

- This reader matches text, not XML: comments, CDATA or `>` inside attribute values would mislead it. DbxEdit's
  XML has none (it escapes `>` as `&gt;`), so the rules hold for the suite's own exports only.
