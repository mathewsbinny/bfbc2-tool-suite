//! DbxEdit's commands in process: a port of `bfbc2-dbx-tools/DbxEdit.cs` (retired in round GR4 with `DbxEditGUI`, the
//! window built on it, whose window is the map editor's DBX inspector now; FORMAT.md sections 3, 5-7) that gives
//! DbxEdit's output byte for byte - the same files, messages and exit codes - without starting .NET.
//!
//! | `DbxEdit.cs` | here |
//! |---|---|
//! | `Program` (`xml`, `roundtrip`, `edit`, `import`, `check`, `fixslots`) | [`run`], [`run_many`] ([`Outcome`]) |
//! | `DbxFile`, `Element`, `Varint`, `AtomicFile` | [`EditFile`], [`Element`], [`read_varint`], [`write_atomic`] |
//! | `XmlExport.Convert` / `ValueText` | [`convert`], [`value_text`] |
//! | `XmlImport`, `TypeBook` | [`import`], [`parse_xml`], [`ImportReport`] |
//! | `Editor` | [`editor`] |
//! | `LevelCheck`, `SlotFixer`, `PlacedObject` | [`level_check`], [`fix_slots`] |
//! | exceptions and `ToolLog.Explain` | [`Fault`] |
//! | `LevelDiff.cs` (`bfbc2-diff-workbench`, retired in round EX1) | [`leveldiff`] (`bfbc2.exe leveldiff`, the editor's Diff panel) |
//!
//! Why a model of its own, not [`crate::dbx::DbxFile`]: DbxEdit writes back what an edit does not touch exactly as
//! it was - the string table as stored (offsets, padding, duplicates) until a string is added or changed, the
//! header's kept word, each container's flag byte, bytes after the tree - where `dbx::write` writes a fresh table.
//!
//! Where .NET Framework's answer differs from Rust's, the port takes .NET's ([`net`]): number parsing (`-0` is +0,
//! 50 digits at most, `Infinity` / `NaN` spelled exactly), case-insensitive comparison, `Path.ChangeExtension`,
//! `Directory.GetFiles`' order, the byte order marks of `File.ReadAllText`. One known limit: .NET Framework's
//! `NumberToDouble` can be one unit off in the last place for numbers of more than 15 significant digits; the port
//! rounds correctly (see rust/BENCHMARKS.md, E3b, for how often that shows).
//!
//! A damaged file fails where DbxEdit's fails, with the exit code DbxEdit gives; the message names the same .NET
//! exception but may word its details differently. Nesting past [`MAX_DEPTH`] is refused (DbxEdit would overflow its
//! stack and die).

mod check;
mod cli;
pub mod editor;
mod export;
mod fault;
mod file;
mod import;
pub mod leveldiff;
pub mod net;

pub use check::{Finding, PlacedObject, SlotResult, fix_slots, level_check};
pub use cli::{Outcome, expand_commands, run, run_many, write_atomic};
pub use export::{convert, number, value_text};
pub use fault::{Fault, FaultKind, Result};
pub use file::{
    CONTAINER, EditFile, Element, MAX_DEPTH, NUMBERS, TEXT, Table, read_varint, varint_bytes, write_varint,
};
pub use import::{
    ImportReport, Node, TypeBook, encode_value, encode_value_rounded, import, parse_xml, prints_as_whole,
};
