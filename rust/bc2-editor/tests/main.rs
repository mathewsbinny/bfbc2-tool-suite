//! The map editor's integration tests in one test binary, `editor` (round QW1, audit AU2 P5): each file of `tests/` is a
//! module here (`mod g2;` is `tests/g2.rs`), so the crate and its dependencies are linked into one
//! executable instead of one per file. A test's name is `<file>::<function>`:
//!
//! ```text
//! cargo test -p bc2-editor --test editor                       every test of the crate's files
//! cargo test -p bc2-editor --test editor g2::                one file's tests (what `--test g2` was)
//! cargo test -p bc2-editor --test editor g2::<function> -- --exact   one test
//! ```
//!
//! The files' tests used to run in separate processes, one file after another; here they share one process, so
//! each test first takes `gate()`: at most [`GATE`] of them run at once (the real-level tests hold whole levels,
//! and a PC with 15 GB must not hold all of them together). A new file of `tests/` is a new `mod` line below.

use std::sync::{Condvar, Mutex, PoisonError};

mod asset_browser;
mod e2b;
mod editing;
mod every_level;
mod ex1;
mod factions;
mod g2;
mod g3;
mod g4;
mod game_view;
mod game_view_live;
mod game_view_package;
mod game_view_r5d;
mod game_view_ww2;
mod gr1;
mod gr2;
mod inspect_parity;
mod inspector_parity;
mod nav_parity;
mod real_level_editing;
mod render;
mod simplified_view;
mod view_geometry;
mod view_toggles;
mod vw2;

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
