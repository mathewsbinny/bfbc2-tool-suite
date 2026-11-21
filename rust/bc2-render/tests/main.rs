//! The game-accurate renderer's integration tests in one test binary, `render` (round QW1, audit AU2 P5): each file of `tests/` is a
//! module here (`mod world;` is `tests/world.rs`), so the crate and its dependencies are linked into one
//! executable instead of one per file. A test's name is `<file>::<function>`:
//!
//! ```text
//! cargo test -p bc2-render --test render                       every test of the crate's files
//! cargo test -p bc2-render --test render world::                one file's tests (what `--test world` was)
//! cargo test -p bc2-render --test render world::<function> -- --exact   one test
//! ```
//!
//! The files' tests used to run in separate processes, one file after another; here they share one process, so
//! each test first takes `gate()`: at most [`GATE`] of them run at once (the real-level tests hold whole levels,
//! and a PC with 15 GB must not hold all of them together). A new file of `tests/` is a new `mod` line below.

use std::sync::{Condvar, Mutex, PoisonError};

// the examples' helpers (OPFOR found, a level placed), loaded once for the files that use them
#[path = "../examples/common/mod.rs"]
mod common;
mod live;
mod warehouse;
mod whole_terrain;
mod world;

/// At most this many of the binary's tests run at the same time.
const GATE: usize = 2;

static RUNNING: Mutex<usize> = Mutex::new(0);
static FREED: Condvar = Condvar::new();

/// A test's place among the [`GATE`] that may run at once, given back when it is dropped (a failing test's too).
pub(crate) struct Gate;

/// Waits until fewer than [`GATE`] tests run, then holds a place: every test's first statement.
pub(crate) fn gate() -> Gate {
    let mut running = RUNNING.lock().unwrap_or_else(PoisonError::into_inner);
    while *running >= GATE {
        running = FREED.wait(running).unwrap_or_else(PoisonError::into_inner);
    }
    *running += 1;
    Gate
}

impl Drop for Gate {
    fn drop(&mut self) {
        let mut running = RUNNING.lock().unwrap_or_else(PoisonError::into_inner);
        *running = running.saturating_sub(1);
        FREED.notify_one();
    }
}
