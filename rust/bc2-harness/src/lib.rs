//! The map and bot test harness (`bfbc2-automated-map-and-bot-test-harness`) and its crash-reader tools in Rust,
//! round E4f.
//!
//! | Python | here |
//! |---|---|
//! | `testharness.ingame.sessions` (opfor.log sessions) | [`oplog::sessions`], [`oplog::sessions_of_file`] |
//! | `crashreader.log_before_crash` | [`oplog::log_before_crash`] |
//! | pefile's `PE(...).get_memory_mapped_image()` | [`pe::Image`] |
//! | capstone's 32-bit x86 decoding (text and operands) | [`x86`] |
//! | `crashreader.Image` (function starts, the code around an address, the call before a return address) | [`exe::Exe`] |
//! | `exeref.Exe` (strings, references, the window and its notes) | [`exe::Exe`] |
//! | `testharness.checks` / `classify` (reachability, joins, ground classes) | [`graph`] |
//!
//! Every piece gives the Python reference's answer or refuses (`None` / an error the binding turns into "run the
//! Python"): the reference keeps its error messages and the inputs the port does not reproduce.

pub mod exe;
pub mod graph;
pub mod oplog;
// round OR5: Project OPFOR's test run logic (opfor.ini's edits, the log's notes)
pub mod testrun;
/// The PE reader, now in `bc2-formats` (round QW2, audit AU2 A5: a file format, and the menu tools need it without
/// the harness); kept under its old path for this crate's callers.
pub use bc2_formats::pe;
pub mod x86;
mod x86_verified;
