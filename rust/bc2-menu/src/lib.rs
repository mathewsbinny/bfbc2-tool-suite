//! The menu tools' library in Rust (`bfbc2-menu-tools`): `menucommon`'s pieces ([`common`]: the game copy, movies
//! from files or bundles, BIGF members, OPFOR's patch format, the exe's data), used by `bfbc2.exe`'s `aptcheck`,
//! `aptrefs`, `bc2loc` and `aptgraft` (rounds O2-O3; `rust/BENCHMARKS.md` "O3").

#![allow(
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::missing_panics_doc,
    reason = "ports of Python's behaviour: each function's errors are the Python exception it names"
)]
#![allow(missing_docs, reason = "fields and functions named as the Python they port")]

pub mod aptpatch;
pub mod common;
pub mod error;
pub mod graft;
pub mod loc;
pub mod modeart;
pub mod pacific;
pub mod pil;
