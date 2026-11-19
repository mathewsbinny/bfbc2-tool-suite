//! Round O2's `crash` group: the crash reader's command lines, `exeref` and `crashreader`
//! (`bfbc2-automated-map-and-bot-test-harness\crash-reader`), over `bc2_harness` (round E4f).

use crate::pyproc::Tool;

mod common;
mod crashreader;
mod exeref;
mod xml;

/// The group's tools.
pub const TOOLS: &[Tool] = &[
    Tool {
        name: "exeref",
        script: "bfbc2-automated-map-and-bot-test-harness\\crash-reader\\exeref.py",
        log: Some("exeref"),
        run: exeref::run,
    },
    Tool {
        name: "crashreader",
        script: "bfbc2-automated-map-and-bot-test-harness\\crash-reader\\crashreader.py",
        log: Some("crashreader"),
        run: crashreader::run,
    },
];
