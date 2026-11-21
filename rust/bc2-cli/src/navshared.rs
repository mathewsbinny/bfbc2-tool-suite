//! What round O4's NavKit groups share (navkit, navtools, harness): the suite folder the Python tools sit in,
//! NavKit's `find_opfor` (`bfbc2core.paths`, walking up from the suite) and `original_file` (`navkit.py`: DICE's
//! original of a game file behind OPFOR's backups and markers).

#![allow(clippy::float_cmp, reason = "the backups' times compared as Python compares floats")]

use crate::pyos;
use crate::pyproc::{Fail, PyErr, PyResult};

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
pub fn original_file(p: &str) -> PyResult<Option<String>> {
    let path = std::path::Path::new(p);
    if let Some(game) = bc2_formats::gameledger::find_game(path) {
        let led = bc2_formats::gameledger::Ledger::new(&game);
        let rel = pyos::relpath(p, &game.to_string_lossy())?;
        let fail = |e: bc2_formats::gameledger::Error| Fail::Err(PyErr::new("LedgerError", e.to_string()));
        if !led.holders(&rel).map_err(fail)?.is_empty() {
            return Ok(led
                .original(&rel)
                .map_err(fail)?
                .map(|b| b.to_string_lossy().into_owned()));
        }
    }
    let (d, base) = pyos::split(p)?;
    let Ok(listing) = pyos::listdir(if d.is_empty() { "." } else { &d }) else {
        return Ok(None);
    };
    // {f.lower(): f}: the first position of a lower-case name, the last spelling
    let mut names: Vec<(String, String)> = Vec::new();
    for f in listing {
        let low = f.to_lowercase();
        if let Some(slot) = names.iter_mut().find(|(k, _)| *k == low) {
            slot.1 = f;
        } else {
            names.push((low, f));
        }
    }
    let pre = format!("{}.opfor-", base.to_lowercase());
    let ours: Vec<&(String, String)> = names.iter().filter(|(k, _)| k.starts_with(&pre)).collect();
    let has = |k: &str| ours.iter().any(|(o, _)| o == k);
    if has(&format!("{pre}new")) {
        let own = format!("{pre}orig");
        return Ok(match ours.iter().find(|(o, _)| *o == own) {
            Some((_, f)) => Some(pyos::join2(&d, f)?),
            None => None,
        });
    }
    if ours.iter().any(|(o, _)| {
        o.strip_suffix("-applied")
            .is_some_and(|stem| !has(&format!("{stem}-orig")))
    }) {
        return Ok(None);
    }
    let mut best: Option<((f64, bool), String)> = None;
    for (o, f) in &ours {
        if !o.ends_with("-orig") {
            continue;
        }
        let b = pyos::join2(&d, f)?;
        if !pyos::isfile(&b) {
            continue;
        }
        let key = (pyos::getmtime(&b)?, !b.to_lowercase().ends_with(".opfor-orig"));
        if best
            .as_ref()
            .is_none_or(|(k, _)| key.0 < k.0 || (key.0 == k.0 && !key.1 && k.1))
        {
            best = Some((key, b));
        }
    }
    if let Some((_, b)) = best {
        return Ok(Some(b));
    }
    Ok(pyos::isfile(p).then(|| p.to_owned()))
}
