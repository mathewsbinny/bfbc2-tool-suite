//! `bfbc2 hpdview <pathdata.hpd> <sector> <out.png> ...`: NavKit's `tools\hpdview.py`. Its picture is matplotlib's
//! (Agg's rasteriser and PNG writer), so every drawing run is handed to the Python; the port answers what fails
//! before the drawing: too few arguments and a file that cannot be read, reported as `errors.guard` reports them.

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

use crate::pyproc::{Fail, Py, PyErr, PyResult, read_file};

pub fn run(_py: &mut Py, args: &[String]) -> PyResult<i32> {
    // path, which, out = sys.argv[1], sys.argv[2], sys.argv[3]
    if args.len() < 3 {
        return Err(PyErr::new("IndexError", "list index out of range").into());
    }
    read_file(&args[0])?;
    Err(Fail::Python)
}
