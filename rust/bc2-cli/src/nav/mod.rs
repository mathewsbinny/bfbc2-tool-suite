//! Round O2's `nav` group: NavKit's `hpdtool` / `hkpack`, the console converter's `convert.py` and `aimanim`.

use crate::pyproc::Tool;

mod aimanim;

/// The group's tools.
pub const TOOLS: &[Tool] = &[Tool {
    name: "aimanim",
    script: "bfbc2-anim-tools\\aimanim.py",
    log: None,
    run: aimanim::run,
}];
