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
pub mod pe;
pub mod x86;
mod x86_verified;
