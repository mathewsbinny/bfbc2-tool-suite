//! Binary `.dbx` partitions (`bfbc2-dbx-tools/FORMAT.md`), ported from `core/bfbc2core/dbx.py`.
//!
//! | Python (`bfbc2core.dbx`) | here |
//! |---|---|
//! | `parse(data)`, `read(path)` | [`parse`], [`read`] |
//! | `to_xml(data)` | [`to_xml`] (identical text: `core/tests/test_dbx_native.py` checks every corpus file) |
//! | `write(f)`, `put_varint` | [`write`] ([`Writer`] for any tree, e.g. the Python one), [`put_varint`] |
//! | `value(e)`, `numbers(e)`, `floats(e)`, `_number_text` | [`value`], [`numbers`], [`floats`], [`number_text`] |
//! | `Element`, `DbxFile` | [`Element`] (strings are [`StrId`]s into [`DbxFile::strings`]), [`DbxFile`] |
//! | (read-only, no Python equivalent) | [`parse_tree`]: a [`DbxTree`] of [`Node`]s borrowing the file |
//! | `ValueError` / `DeletedEntry` | [`DbxError`] |
//!
//! [`parse_all`], [`parse_tree_all`] and [`to_xml_all`] read many files on all cores.
//!
//! Two trees: [`DbxFile`] is owned and editable (what [`write`] takes, and E1's scene and `levelwrite` build on);
//! [`DbxTree`] is read-only, a few allocations per file however large, its strings and values borrowed from the
//! file's bytes (what a reader of many files keeps: rust/BENCHMARKS.md). [`parse`] is [`parse_tree`] then
//! [`DbxTree::to_file`]: one reader for both.
//!
//! Where the reference has no answer the port refuses instead: a varint past 64 bits saturates (and then fails as
//! an index or count, as the reference's unbounded integer does), and a numbers element with more values than one
//! per byte of the file (and over 2^20) is [`DbxError::TooManyNumbers`] where the reference would build the list.

mod batch;
mod cursor;
mod error;
mod model;
mod number;
mod reader;
mod strings;
mod tree;
mod writer;
mod xml;

pub use batch::{parse_all, parse_tree_all, to_xml_all};
pub use error::{DbxError, Result};
pub use model::{Body, CONTAINER, DbxFile, Element, NUMBERS, StrId, TEXT};
pub use number::{WHOLE_NUMBER_FIELDS, floats, net_g, net_r_double, net_r_single, number_text, numbers, value};
pub use reader::{ReadError, parse, read};
pub use strings::{MAGIC, is_binary};
pub use tree::{DbxTree, Node, Siblings, parse_tree};
pub use writer::{Writer, put_varint, write};
pub use xml::to_xml;
