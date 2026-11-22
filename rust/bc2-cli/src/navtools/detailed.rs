//! `bfbc2 detailed <level> --offsets X,Z [--samples <bin>] [--score]`: NavKit's `tools\detailed.py`. Round O4 answered
//! the usage and what fails before the choice; round O5 makes the choice itself (`choose`: the sectors near a
//! `NeedsDetailedPathfinding` object's collision box, through the suite's Havok index and triangle reader; the stacked
//! floors of the 0.8 m samples, through NavKit's sample reader). The run prints one line at its end, so anything the
//! port does not read as the Python does hands the whole run to the Python: `--score` (DICE's choice, evaluation
//! only), objects of another shape, a packfile the reader refuses, a sample file it refuses.

#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::verbose_bit_mask,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::format_push_string,
    clippy::type_complexity,
    reason = "a port of Python: indexes after their lengths are checked, ints as Python turns them into floats, == as Python compares floats, the Python's names and its functions' shape kept"
)]

use std::collections::BTreeSet;

use bc2_navbuild::inputs;

use super::pycommon::{find_opfor_text, py_int};
use crate::pyfmt::{docstring, str_repr};
use crate::pyos::join;
use crate::pyproc::{Fail, Py, PyErr, PyResult};

const SOURCE: &str = include_str!("../../../../bfbc2-auto-navkit/tools/detailed.py");

/// `flagged_sectors(level_dir, common, xoff, zoff)` (round NK6: `bc2_navbuild::inputs`, the level driver's).
fn flagged_sectors(level_dir: &str, common: &[String], xoff: i64, zoff: i64) -> PyResult<BTreeSet<(i64, i64)>> {
    inputs::flagged_sectors(level_dir, common, xoff, zoff).map_err(crate::navkit::build::fail)
}

/// `stacked_columns(samples_path, xoff, zoff)`.
fn stacked_columns(path: &str, xoff: i64, zoff: i64) -> PyResult<std::collections::BTreeMap<(i64, i64), u64>> {
    inputs::stacked_columns(path, xoff, zoff).map_err(crate::navkit::build::fail)
}

pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    if args.is_empty() {
        py.print(&docstring(SOURCE))?;
        return Ok(1);
    }
    // opts = dict(zip(argv[2:], argv[3:])): the last pair of a key wins
    let rest = &args[1..];
    let mut offsets: Option<&str> = None;
    let mut samples: Option<&str> = None;
    for w in rest.windows(2) {
        if w[0] == "--offsets" {
            offsets = Some(&w[1]);
        }
        if w[0] == "--samples" {
            samples = Some(&w[1]);
        }
    }
    // opfor_root(): OPFOR_ROOT as it is, else paths.find_opfor (GameCopyNotFound uncaught)
    let root = match std::env::var("OPFOR_ROOT").ok().filter(|v| !v.is_empty()) {
        Some(r) => r,
        None => match find_opfor_text(None, true)? {
            Ok(r) => r,
            Err(m) => return Err(PyErr::new("bfbc2core.paths.GameCopyNotFound", m).into()),
        },
    };
    let Some(off) = offsets else {
        return Err(PyErr::new("KeyError", str_repr("--offsets")).into());
    };
    let parts: Vec<&str> = off.split(',').collect();
    let mut nums = Vec::new();
    for p in parts.iter().take(3) {
        nums.push(py_int(p)?);
    }
    if parts.len() < 2 {
        return Err(PyErr::new(
            "ValueError",
            format!("not enough values to unpack (expected 2, got {})", parts.len()),
        )
        .into());
    }
    if parts.len() > 2 {
        return Err(PyErr::new("ValueError", "too many values to unpack (expected 2)").into());
    }
    if rest.iter().any(|a| a == "--score") {
        return Err(Fail::Python); // DICE's choice (navparity.need_reference, hpdtool, kymesh): evaluation only
    }
    let lv = &args[0];
    if !lv.is_ascii() {
        return Err(Fail::Python);
    }
    let lv = lv.to_ascii_lowercase();
    let (xoff, zoff) = (nums[0], nums[1]);
    let l = join(&root, &["work", "levels", &lv])?;
    let common: Vec<String> = ["mp_common", "sp_common"]
        .iter()
        .map(|c| join(&root, &["work", "navkit", "common", c]))
        .collect::<PyResult<_>>()?;
    let flag = flagged_sectors(&l, &common, xoff, zoff)?;
    let stacked: BTreeSet<(i64, i64)> = match samples {
        // `if samples_path`: an empty argument is no samples
        Some(p) if !p.is_empty() => stacked_columns(p, xoff, zoff)?
            .into_iter()
            .filter(|(_, n)| *n >= inputs::STACKED_MIN)
            .map(|(k, _)| k)
            .collect(),
        _ => BTreeSet::new(),
    };
    let chosen = flag.union(&stacked).count();
    let only = stacked.difference(&flag).count();
    py.print(&format!(
        "{lv}: {chosen} detailed sectors ({{'flag': {}, 'stacked': {}, 'stacked only': {only}}})",
        flag.len(),
        stacked.len()
    ))?;
    Ok(0)
}
