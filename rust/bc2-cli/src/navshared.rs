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

/// `bfbc2core.paths.walk_up(start)`: `<start>\project-opfor-bfbc2`, then the same in each folder above.
fn walk_up(start: &str) -> PyResult<Vec<String>> {
    let mut d = pyos::abspath(start)?;
    let mut out = vec![pyos::join2(&d, "project-opfor-bfbc2")?];
    loop {
        let up = pyos::dirname(&d)?;
        if up == d {
            break;
        }
        d = up;
        out.push(pyos::join2(&d, "project-opfor-bfbc2")?);
    }
    Ok(out)
}

/// The suite folder the Python tools sit in (NavKit's `find_opfor` walks up from it).
pub fn suite() -> PyResult<String> {
    let s = crate::pyproc::suite_root()
        .into_os_string()
        .into_string()
        .map_err(|_| Fail::Python)?;
    pyos::abspath(&s)
}

/// `bfbc2core.paths.find_opfor(given, start=<the suite>)`: Ok(folder) or Err(GameCopyNotFound's text).
pub fn find_opfor_text(given: Option<&str>, env: bool) -> PyResult<Result<String, String>> {
    let mut looked: Vec<String> = Vec::new();
    for c in [
        given.map(str::to_owned),
        if env { std::env::var("OPFOR_ROOT").ok() } else { None },
    ]
    .into_iter()
    .flatten()
    {
        if !c.is_empty() {
            looked.push(c);
        }
    }
    looked.extend(walk_up(&suite()?)?);
    for c in &looked {
        if pyos::isdir(&pyos::join2(c, "game")?) {
            return Ok(Ok(pyos::abspath(c)?));
        }
    }
    Ok(Err(format!(
        "Project OPFOR BFBC2 not found - pass --opfor <folder> or set OPFOR_ROOT (looked for a game\\ folder in: {})",
        looked.join(", ")
    )))
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
