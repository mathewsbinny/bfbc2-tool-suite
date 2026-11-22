# DBX: binary <-> tree <-> XML, and DbxEdit's edits

The suite reads a binary partition into a tree (`dbx::parse`), prints it as XML (`dbx::to_xml`, DbxEdit's `xml`),
writes a tree back (`dbx::write`, the editable tree `dbx::edit::EditDbx`, DbxEdit's model `dbxedit::file`), turns
XML back into binary against a reference (DbxEdit's `import`), and edits partitions in place with DbxEdit's
commands (`edit <in> <out> drop=... replace=... graft=...`, `dbxedit::cli::run`, `bfbc2core.dbxedit`).

The format is layer 1's (`formats/dbx.md`: DBX-WR-1, DBX-WR-2, DBX-IMP-1, DBX-IMP-2, DBX-XML-4); this file states
the transformations' contracts and the parity they are held to. The reference answers are those of the retired
Python reader and writer (round F1a) and of DbxEdit.exe (round R4), recorded before deletion
(`core/tests/dbx_golden*.json`, `dbxedit_golden*.json`).

## 1. Reading and printing

### XDBX-RD-1: Refusals, never a panic

Status: **verified**

`parse` and `to_xml` return a tree or an error for any bytes: a file under the minimum size or without the magic, a
string index out of range, a value cut short, a varint past 32 bits, a nesting past the limit or huge counts are
errors (DBX-HDR-1 to DBX-ELEM-7), every truncation of a valid file is an error or a tree, and no input panics.

[test: rust/bc2-formats/tests/robustness.rs::any_bytes_never_panic] [test: rust/bc2-formats/tests/robustness.rs::damaged_copies_never_panic]
[test: rust/bc2-formats/tests/robustness.rs::every_truncation_of_a_file_is_an_error_or_a_tree] [test: rust/bc2-formats/src/dbx/reader.rs::huge_counts_are_errors]
[test: rust/bc2-formats/tests/dbxedit_robustness.rs::deep_nesting_is_refused]

### XDBX-RD-2: The XML is the reference reader's

Status: **verified**

For every distinct `.dbx` of the corpus (48,120 files of OPFOR's `work\levels`, `work\navkit\common` and `work\ww2`)
the Rust reader gives the retired Python reader's XML text and element tree, and refuses the same files; on the
31,404 files DbxEdit's fingerprints cover, the XML is DbxEdit's.

[code: rust/bc2-formats/src/dbx/xml.rs:54] [code: rust/bc2-formats/src/dbx/batch.rs:31]
[test: core/tests/test_dbx_native.py::run_corpus] [test: core/tests/test_dbx_native.py::built_files]

## 2. Writing

### XDBX-WR-1: parse(write(t)) = t, with the reference writer's bytes

Status: **verified**

`dbx::write` of any tree it accepts reads back to the same tree (DBX-WR-1), and its bytes are the retired Python
writer's for every corpus file as read and after seeded random edits, for 2,000 random trees and for 41 edge trees;
a tree it cannot write (an element kind or value it has no encoding for) is an error naming the element, where the
Python writer raised one.

[code: rust/bc2-formats/src/dbx/writer.rs:268] [test: rust/bc2-formats/src/dbx/writer.rs::round_trip_keeps_the_tree]
[test: rust/bc2-formats/src/dbx/writer.rs::refuses_unwritable_elements] [test: rust/bc2-formats/tests/robustness.rs::write_then_read_keeps_the_tree]
[test: core/tests/test_dbx_write_native.py::edge_trees] [test: core/tests/test_dbx_write_native.py::random_trees]
[doc: rust/BENCHMARKS.md:331-333]

### XDBX-WR-2: The byte-keeping writers return an unchanged file exactly

Status: **verified**

`EditDbx::write` and DbxEdit's model write an unedited file back byte for byte (DBX-WR-2). After an edit, every byte
the edit did not touch is kept: string indexes, the header word, container bytes and the trailer; new strings are
appended. DbxEdit's `roundtrip` leaves files written by `dbx::write` unchanged too (3,000 corpus files checked).

[code: rust/bc2-formats/src/dbx/edit.rs:942] [test: rust/bc2-formats/src/dbx/edit.rs::unchanged_files_come_back]
[test: rust/bc2-formats/src/dbx/edit.rs::edits_and_clones] [test: core/tests/test_dbx_write_native.py::check_dbxedit]

## 3. XML back to binary

### XDBX-IMP-1: import(xml(f), like f) = f

Status: **verified**

DbxEdit's `import` of an unedited XML against its own binary as the reference gives that binary byte for byte; an
edited XML changes only the edited values (DBX-IMP-1). Without a reference the defaults of DBX-IMP-2 apply and the
result is a valid file with the XML's tree. The precondition is the reference: the XML alone does not carry value
kinds, number sizes or tiny floats (DBX-XML-4), so a tool that needs exact values reads the binary.

[test: rust/bc2-formats/tests/dbxedit_robustness.rs::built_files_round_trip_and_import_back] [test: rust/bc2-formats/tests/dbxedit_robustness.rs::any_text_imports_or_fails]
[test: core/tests/test_dbxedit.py::game] [test: core/tests/test_dbxedit.py::quick]

## 4. DbxEdit's edit commands

### XDBX-CMD-1: In process, DbxEdit.exe's answers

Status: **verified**

`dbxedit::cli::run` (the `bfbc2-cli dbxedit` command and `bfbc2core.dbxedit`) answers every command as DbxEdit.exe
did: the same files written byte for byte, the same messages and exit codes, for DbxEdit's own unit-test cases, the
usage, option and damaged-file errors, every edit fingerprint of the 795745 corpus and a recorded sample of 2,000
corpus files through `xml`, `roundtrip`, `import` and an edit battery.

[code: rust/bc2-tools/src/dbxedit.rs:91-109]
[test: core/tests/test_dbxedit.py::quick] [test: core/tests/test_dbxedit.py::golden_edits] [test: core/tests/test_dbxedit.py::game]

### XDBX-CMD-2: What each edit does

Status: **verified**

- `drop=<guid>` removes the instance; array items referring to it are removed, single references become null
  (REF-DEL-1);
- `dropfield=<name>` removes every non-instance element of that name; `set=<field>|<value>` sets every bool / byte
  field of that name;
- `replace=<old>|<new>` replaces a string in the table (every use at once);
- `graft=<source>|<guid>` copies an instance and every instance of the source it references locally that the file
  lacks, before a given instance or at the partition's end;
- `additem` inserts a reference into an instance's array in the shape of the array's own items.

An edit that names a guid or field the file does not have changes nothing and says so.

[code: rust/bc2-formats/src/dbxedit/editor.rs:121-130] [code: rust/bc2-formats/src/dbxedit/editor.rs:151-183] [code: rust/bc2-formats/src/dbxedit/editor.rs:251-271] [code: rust/bc2-formats/src/dbxedit/editor.rs:418-433]
[test: core/tests/test_dbxedit.py::quick] [test: core/tests/test_dbxedit.py::battery]

### XDBX-CMD-3: Output files are written atomically

Status: **verified**

DbxEdit writes each output beside its target (`<path>.tmp-<process>-<n>`), flushed, then moved over it, waiting out
a moment's lock; an error removes the temporary file and leaves the target as it was.

[code: rust/bc2-tools/src/dbxedit.rs:54-66] [test: rust/bc2-gamefs/src/atomic.rs::a_lasting_lock_is_returned] [test: rust/bc2-gamefs/src/atomic.rs::other_errors_are_not_retried]

## Open questions

- The class schema (TYPE-SCH-1): with it, an XML import could type new fields from the executable instead of a
  reference file or `--types`.
