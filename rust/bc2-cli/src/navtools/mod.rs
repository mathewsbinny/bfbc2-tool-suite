//! Round O4's `navtools` group: NavKit's other tool command lines (`bfbc2-auto-navkit\tools\`: graphstats, hpdcheck,
//! hpdview, detailed, navparity, navregress, `sample_all`). Each answers as its Python command line does where the Rust
//! core gives Python's exact answer, and hands the command line to the Python where it does not (matplotlib's
//! pictures, NavKit's builds and measures, the game launches of sampling).

use crate::pyproc::Tool;

pub(crate) mod detailed;
mod graphstats;
mod hpdcheck;
mod hpdview;
mod navparity;
mod navregress;
#[allow(dead_code)]
pub mod pycommon;
mod sample_all;

/// The group's tools.
pub const TOOLS: &[Tool] = &[
    Tool {
        name: "graphstats",
        script: "bfbc2-auto-navkit\\tools\\graphstats.py",
        log: None,
        run: graphstats::run,
    },
    Tool {
        name: "hpdcheck",
        script: "bfbc2-auto-navkit\\tools\\hpdcheck.py",
        log: None,
        run: hpdcheck::run,
    },
    Tool {
        name: "hpdview",
        script: "bfbc2-auto-navkit\\tools\\hpdview.py",
        log: Some("hpdview"),
        run: hpdview::run,
    },
    Tool {
        name: "detailed",
        script: "bfbc2-auto-navkit\\tools\\detailed.py",
        log: None,
        run: detailed::run,
    },
    Tool {
        name: "navparity",
        script: "bfbc2-auto-navkit\\tools\\navparity.py",
        log: Some("navparity"),
        run: navparity::run,
    },
    Tool {
        name: "navregress",
        script: "bfbc2-auto-navkit\\tools\\navregress.py",
        log: Some("navregress"),
        run: navregress::run,
    },
    Tool {
        name: "sample_all",
        script: "bfbc2-auto-navkit\\tools\\sample_all.py",
        log: None,
        run: sample_all::run,
    },
];
