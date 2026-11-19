//! The stock-MP AI layer (`bfbc2-stockmp-ai`) in Rust, round E4a: the level model (`levelmodel.py`), the authoring of
//! the AI layer into a level partition (`author.py`, on [`bc2_formats::dbx::edit::EditDbx`], the port of
//! `dbxbin.py`) and the heavy parts of the offline checks (`validate.py`: the structure, the changes to existing
//! instances, the references into other partitions, the Rush stages' instances, the folder index and the packed
//! bundle against the built files).
//!
//! Each piece gives the Python reference's answer or [`Refuse`]: wherever the reference raises (an `AuthorError`, a
//! `KeyError`, a value it cannot pack) or formats a value the port does not (a Python float's text), the port
//! refuses and the caller runs the reference, which then raises or answers by its own rules. So a result here is
//! always the reference's result, and the reference keeps every error message.
//!
//! | Python (`bfbc2-stockmp-ai`) | here |
//! |---|---|
//! | `dbxbin.new_guid` | [`guid::new_guid`] |
//! | `levelmodel.Model` | [`model::Model`] |
//! | `author.NumberKinds`, `author.Author` | [`author::NumberKinds`], [`author::author`] |
//! | `validate.check_structure` / `check_changes` / `check_external` / `check_stages` (their readings) | [`checks`] |
//! | `validate._folder_index`, `validate.check_packed` | [`files::folder_index`], [`files::packed_check`] |
//! | `pipeline.sha`, `pipeline.check_report`, `stockmp.installed_files` (round O3) | [`sha256`], [`install`] |

pub mod author;
pub mod checks;
pub mod files;
pub mod guid;
pub mod install;
pub mod model;
pub mod pytext;
pub mod sha256;

/// The Python reference decides: an input, or a path through it, where the port does not give the reference's
/// answer itself (the reason, for tests and logs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refuse(pub &'static str);

/// The result type of this crate.
pub type R<T> = Result<T, Refuse>;
