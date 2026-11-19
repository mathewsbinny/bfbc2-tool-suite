//! Round O2's `menu` group: `aptcheck`, `aptrefs` and `bc2loc` (`bfbc2-menu-tools`), with `menucommon`'s pieces in
//! [`common`].

use crate::pyproc::Tool;

mod aptcheck;
mod aptrefs;
mod bc2loc;
mod common;
mod json;

/// The group's tools.
pub const TOOLS: &[Tool] = &[
    Tool {
        name: "aptcheck",
        script: "bfbc2-menu-tools\\aptcheck.py",
        log: Some("aptcheck"),
        run: aptcheck::run,
    },
    Tool {
        name: "aptrefs",
        script: "bfbc2-menu-tools\\aptrefs.py",
        log: Some("aptrefs"),
        run: aptrefs::run,
    },
    Tool {
        name: "bc2loc",
        script: "bfbc2-menu-tools\\bc2loc.py",
        log: Some("bc2loc"),
        run: bc2loc::run,
    },
];
