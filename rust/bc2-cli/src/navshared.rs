//! What round O4's NavKit groups share (navkit, navtools, harness): the suite folder the Python tools sit in,
//! NavKit's `find_opfor` (`bfbc2core.paths`, walking up from the suite) and `original_file` (`navkit.py`: DICE's
//! original of a game file behind OPFOR's backups and markers).

#![allow(clippy::float_cmp, reason = "the backups' times compared as Python compares floats")]

use bc2_navbuild::ntpath;

use crate::pyos;
use crate::pyproc::{Fail, PyErr, PyResult};

/// NavKit's level-driver library's answer (`bc2_navbuild`) as the command line's: the exception, or a hand-off. Here,
/// not in the navkit group, so navtools and harness build alone (CI's per-group clippy).
pub fn fail(f: ntpath::Fail) -> Fail {
    match f {
        ntpath::Fail::Raised(r) => Fail::Err(PyErr {
            kind: r.kind,
            msg: r.msg,
            filename: r.filename,
            what_to_do: None,
        }),
        ntpath::Fail::Handoff | ntpath::Fail::Sink => Fail::Python,
        ntpath::Fail::Cancelled => Fail::Err(PyErr::new("KeyboardInterrupt", "")),
    }
}

/// The suite folder the Python tools sit in (NavKit's `find_opfor` walks up from it).
pub fn suite() -> PyResult<String> {
    let s = crate::pyproc::suite_root()
        .into_os_string()
        .into_string()
        .map_err(|_| Fail::Python)?;
    pyos::abspath(&s)
}

/// `bfbc2core.paths.find_opfor(given, start=<the suite>)`: Ok(folder) or Err(GameCopyNotFound's text)
/// ([`bc2_gamefs::game::find_opfor`], walking up from the suite).
pub fn find_opfor_text(given: Option<&str>, env: bool) -> PyResult<Result<String, String>> {
    let suite = std::path::PathBuf::from(suite()?);
    Ok(
        bc2_gamefs::game::find_opfor(given, env, bc2_gamefs::game::Beside::Above(&[suite.as_path()]))
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| e.to_string()),
    )
}

/// `navkit.find_opfor(given)`: the folder, or `NavKitError` with `GameCopyNotFound`'s text (`kind` names the class as
/// the caller's traceback or report shows it).
pub fn find_opfor(given: Option<&str>, kind: &str) -> PyResult<String> {
    find_opfor_text(given, true)?.map_err(|m| Fail::Err(PyErr::new(kind, m)))
}

/// `navkit.original_file(p)`: DICE's original of a game file, or None. From the game copy's install ledger when an
/// owner holds the file there (round DF2: the bottom layer's prior, kept once by its sha256; None where an install
/// put the file where there was none); else by the markers of before the ledger (the earliest OPFOR backup).
/// Round NK6: `bc2_navbuild::inputs::original_file`, the level driver's.
pub fn original_file(p: &str) -> PyResult<Option<String>> {
    bc2_navbuild::inputs::original_file(p).map_err(fail)
}
