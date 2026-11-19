//! Round O2's `nav` group: NavKit's `hpdtool` / `hkpack`, the console converter's `convert.py` and `aimanim`.

use crate::pyproc::Tool;

mod aimanim;
mod common;
mod convert;
mod hkpack;
mod hpdtool;

/// The group's tools.
pub const TOOLS: &[Tool] = &[
    Tool {
        name: "aimanim",
        script: "bfbc2-anim-tools\\aimanim.py",
        log: None,
        run: aimanim::run,
    },
    Tool {
        name: "hpdtool",
        script: "bfbc2-auto-navkit\\tools\\hpdtool.py",
        log: Some("hpdtool"),
        run: hpdtool::run,
    },
    Tool {
        name: "hkpack",
        script: "bfbc2-auto-navkit\\tools\\hkpack.py",
        log: Some("hkpack"),
        run: hkpack::run,
    },
    Tool {
        name: "convert",
        script: "bfbc2-console-convert\\convert.py",
        log: Some("console-convert"),
        run: convert::run,
    },
];
