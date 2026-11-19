//! Round O4's `harness` group: the map and bot test harness's command line
//! (`bfbc2-automated-map-and-bot-test-harness\harness.py`): show, compare, list, import-log and ingame in Rust; `run`
//! handed to the Python. The harness never starts the game.

use crate::pyproc::Tool;

mod cli;
#[allow(
    dead_code,
    clippy::duplicate_mod,
    reason = "the navtools group's helpers, compiled here too so this group builds alone"
)]
#[path = "../navtools/pycommon.rs"]
mod pycommon;
mod runs;

/// The group's tools.
pub const TOOLS: &[Tool] = &[Tool {
    name: "harness",
    script: "bfbc2-automated-map-and-bot-test-harness\\harness.py",
    log: Some("harness"),
    run: cli::run,
}];
