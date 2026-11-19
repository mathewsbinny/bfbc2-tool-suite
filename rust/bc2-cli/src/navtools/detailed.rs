//! `bfbc2 detailed <level> --offsets X,Z [--samples <bin>] [--score]`: NavKit's `tools\detailed.py`. The choice
//! itself (flagged objects' collision boxes through NumPy, stacked floors in the samples) stays the Python's: the
//! port answers the usage and what fails before it, as the script's uncaught exceptions (no Project OPFOR copy,
//! `--offsets` missing or not two ints), and hands the rest to the Python.

#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::format_push_string,
    clippy::type_complexity,
    reason = "a port of Python: indexes after their lengths are checked, ints as Python turns them into floats, == as Python compares floats, the Python's names and its functions' shape kept"
)]

use super::pycommon::{find_opfor_text, py_int};
use crate::pyfmt::{docstring, str_repr};
use crate::pyproc::{Fail, Py, PyErr, PyResult};

const SOURCE: &str = include_str!("../../../../bfbc2-auto-navkit/tools/detailed.py");

pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    if args.is_empty() {
        py.print(&docstring(SOURCE))?;
        return Ok(1);
    }
    // opts = dict(zip(argv[2:], argv[3:]))
    let rest = &args[1..];
    let mut offsets: Option<&str> = None;
    for w in rest.windows(2) {
        if w[0] == "--offsets" {
            offsets = Some(&w[1]);
        }
    }
    // opfor_root(): OPFOR_ROOT as it is, else paths.find_opfor (GameCopyNotFound uncaught)
    if std::env::var("OPFOR_ROOT").map_or(true, |v| v.is_empty())
        && let Err(m) = find_opfor_text(None, true)?
    {
        return Err(PyErr::new("bfbc2core.paths.GameCopyNotFound", m).into());
    }
    let Some(off) = offsets else {
        return Err(PyErr::new("KeyError", str_repr("--offsets")).into());
    };
    let parts: Vec<&str> = off.split(',').collect();
    for p in parts.iter().take(3) {
        py_int(p)?;
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
    Err(Fail::Python)
}
