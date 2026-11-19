//! `bfbc2 sample_all ...`: NavKit's `tools\sample_all.py`. Sampling starts the game, so every run that gets that far
//! is handed to the Python (which starts it); the port answers what fails first, as the script's uncaught
//! exceptions: a `--pitch` or `--budget` Python cannot read, and no Project OPFOR game copy (`navkit.Project`).

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

use super::pycommon::{find_opfor, py_float, py_int};
use crate::pyos;
use crate::pyproc::{Fail, Py, PyErr, PyResult};

pub fn run(_py: &mut Py, args: &[String]) -> PyResult<i32> {
    // opts = dict(zip(argv[1:], argv[2:])): every argument with the one after it
    let mut opts: Vec<(&str, &str)> = Vec::new();
    for w in args.windows(2) {
        if let Some(slot) = opts.iter_mut().find(|(k, _)| *k == w[0]) {
            slot.1 = &w[1];
        } else {
            opts.push((&w[0], &w[1]));
        }
    }
    let get = |k: &str| opts.iter().find(|(o, _)| *o == k).map(|(_, v)| *v);
    if let Some(p) = get("--pitch") {
        py_float(p)?;
    }
    if let Some(b) = get("--budget") {
        py_int(b)?;
    }
    // navkit.Project(None): the folder, then its game exe
    let root = find_opfor(None, "navkit.NavKitError")?;
    let game = pyos::join2(&root, "game")?;
    if !pyos::isfile(&pyos::join2(&game, "BFBC2Game.exe")?) {
        return Err(PyErr::new(
            "navkit.NavKitError",
            format!("no game copy in {game} (run OPFOR's setup_game.bat first)"),
        )
        .into());
    }
    Err(Fail::Python)
}
