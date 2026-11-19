//! Round O3's `graft` group: `bfbc2-menu-tools\aptgraft.py` on `bc2-menu` (`bc2_menu::graft`).

use crate::pyproc::Tool;

mod aptgraft;

/// The group's tools.
pub const TOOLS: &[Tool] = &[Tool {
    name: "aptgraft",
    script: "bfbc2-menu-tools\\aptgraft.py",
    log: Some("aptgraft"),
    run: aptgraft::run,
}];
