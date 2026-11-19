//! What the suite's Python command lines show of the Python runtime, for their Rust ports (`bc2-cli`'s `bfbc2.exe`,
//! rounds O2-O3; `rust/BENCHMARKS.md` "O3"): the exceptions they raise and report ([`err`]), the text forms of values
//! ([`fmt`]: `repr`, `format(x, 'g')`, `round`, `{:,}`, a module's `__doc__`) and `ntpath` / `str` / `dict` pieces
//! ([`py`]). A library so the ported tools' own libraries (`bc2-menu`, ...) can share them with the binary.

#![allow(
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::missing_panics_doc,
    reason = "ports of Python's behaviour: each function's errors are the Python exception it names"
)]
#![allow(missing_docs, reason = "fields named as the Python's attributes they port")]

pub mod err;
pub mod fmt;
pub mod pickle;
pub mod pickle_write;
pub mod py;
pub mod unprintable;
