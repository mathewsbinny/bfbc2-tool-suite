//! Levelwrite's integration tests in one test binary, `levelwrite` (round QW1, audit AU2 P5): each file of `tests/` is a
//! module here (`mod export_e3b;` is `tests/export_e3b.rs`), so the crate and its dependencies are linked into one
//! executable instead of one per file. A test's name is `<file>::<function>`:
//!
//! ```text
//! cargo test -p bc2-levelwrite --test levelwrite                       every test of the crate's files
//! cargo test -p bc2-levelwrite --test levelwrite export_e3b::                one file's tests (what `--test export_e3b` was)
//! cargo test -p bc2-levelwrite --test levelwrite export_e3b::<function> -- --exact   one test
//! ```
//!
//! The files' tests used to run in separate processes, one file after another; here they share one process, so
//! each test first takes `gate()`: at most [`GATE`] of them run at once (the real-level tests hold whole levels,
//! and a PC with 15 GB must not hold all of them together). A new file of `tests/` is a new `mod` line below.

use std::sync::{Condvar, Mutex, PoisonError};

mod auto_layout;
mod auto_layout_vehicles;
mod culltool_ct1;
mod df1;
mod export_e3b;
mod export_e3c;
mod export_e3d;
mod export_ex1;
mod export_rules;
mod export_rules_stage;
mod export_sp01;
mod export_xf1;
mod flag_texts;
mod gameplay_tools;
mod gameplay_xf1;
mod install_mapedit;
mod journal_kill;
mod model_group_members;
mod project_versions;
mod real_levels;
mod rush_stages;
mod terrain_edits;
mod terrain_layers;
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
