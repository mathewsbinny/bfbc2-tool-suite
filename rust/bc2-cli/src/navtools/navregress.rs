//! `bfbc2 navregress ...`: NavKit's `tools\navregress.py`. A regression run builds every level (NavKit's build:
//! NumPy and Python stages around the Rust core) and measures it, so it is handed to the Python; the port answers
//! what comes before the first build: the arguments Python cannot read, no Project OPFOR copy, and a missing
//! reference run (exit code 2).

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

use super::navparity::{LEVELS, ONSLAUGHT, save_path};
use super::pycommon::{find_opfor, original_file, py_float};
use crate::pyos;
use crate::pyproc::{Fail, Py, PyErr, PyResult};

pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    // opt = lambda k, d=None: args[args.index(k) + 1] if k in args else d
    let opt = |k: &str| -> PyResult<Option<&str>> {
        match args.iter().position(|a| a == k) {
            None => Ok(None),
            Some(i) => args
                .get(i + 1)
                .map(|s| Some(s.as_str()))
                .ok_or_else(|| Fail::Err(PyErr::new("IndexError", "list index out of range"))),
        }
    };
    let root = find_opfor(opt("--opfor")?, "NavKitError")?;
    if opt("--levels")?.is_none_or(str::is_empty) {
        // the default levels: those with DICE path data (navparity.reference_of)
        for lv in LEVELS.iter().chain(ONSLAUGHT.iter()) {
            let p = pyos::join(
                &root,
                &[
                    "game",
                    "Output",
                    "win32",
                    "ai",
                    "levels",
                    lv,
                    "infantry",
                    "pathdata.hpd",
                ],
            )?;
            original_file(&p)?;
        }
    }
    let reference = opt("--reference")?.unwrap_or("regression").to_owned();
    if let Some(t) = opt("--tolerance")? {
        py_float(t)?;
    }
    let ref_path = save_path(&root, &reference)?;
    if pyos::isfile(&ref_path) || args.iter().any(|a| a == "--save") {
        return Err(Fail::Python);
    }
    py.print(&format!(
        "no reference run \"{reference}\" ({ref_path}); make one with: navregress.py --save {reference}"
    ))?;
    Ok(2)
}
