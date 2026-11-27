//! Round O2's `menu` group: `aptcheck`, `aptrefs` and `bc2loc` (`bfbc2-menu-tools`), with `menucommon`'s pieces in
//! [`common`].

use crate::pyproc::Tool;

mod aptcheck;
mod aptrefs;
mod bc2loc;
// menucommon's twin: the library crate bc2-menu (round O3)
use bc2_menu::common;
mod json;
// round OR5: Project OPFOR's menu build steps (aptpatch, menuart)
pub mod opfor;

/// `MenuToolError(msg)` as the command lines raise it (the library's [`bc2_menu::error::MenuError::Menu`], as the
/// Python's exception).
fn menu_err(msg: impl Into<String>) -> crate::pyproc::Fail {
    bc2_menu::error::MenuError::Menu(msg.into()).into()
}

/// The test switch under which the menu tools read an exe stand-in kept on disk without its `MZ` / `PE` signatures
/// as the program it stands for (round AV1; O2's build-795745 stand-in, which Microsoft Defender's machine-learning
/// scan blocked as a program). Read once per process, here at the command line's edge: the library's
/// `PeImage::open` has no test switch (audit AU5 ARC-10).
const STAND_IN_VAR: &str = "BFBC2_TEST_STAND_IN_EXE";

/// `PEImage(path)` as the menu tools open the game's exe: [`common::PeImage::open`], or under [`STAND_IN_VAR`]`=1`
/// the bytes with a stand-in's signatures put back ([`common::restore_stand_in`]) first.
fn open_exe(path: &str) -> bc2_menu::error::MenuResult<common::PeImage> {
    static STAND_IN: std::sync::LazyLock<bool> =
        std::sync::LazyLock::new(|| std::env::var_os(STAND_IN_VAR).is_some_and(|v| v == "1"));
    if *STAND_IN {
        let b = bc2_menu::error::read_file(path)?;
        common::PeImage::from_bytes(path, common::restore_stand_in(b))
    } else {
        common::PeImage::open(path)
    }
}

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
