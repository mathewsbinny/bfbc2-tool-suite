//! `navkit.py apply` (rounds NK1, NK5, NK6): the side file's edits on DICE's file or a NavKit rebuild, in
//! `bc2_navbuild::apply` (the library the editor's AI workspace runs in process); here the lines printed, the errors
//! as the Python's exceptions (a hand-off gives the command line to the Python) and `--install`.

use bc2_navbuild::apply as navapply;

use super::build::{Print, root, run, tool};
use super::project::Project;
use crate::pyproc::{Py, PyResult};

/// `navkit.apply(pr, lv, navtune_path, out, install_after=...)`.
pub(super) fn apply(
    py: &mut Py,
    pr: &Project,
    lv: &str,
    (navtune_path, out): (Option<&str>, Option<&str>),
    install_after: bool,
    no_detail: bool,
) -> PyResult<()> {
    let mut sink = Print { py, err: None };
    let r = navapply::apply(&root(pr), lv, (navtune_path, out), run(no_detail), &mut sink, &tool);
    let out = sink.done(r)?;
    if install_after {
        super::driver::install(py, pr, lv, Some(&out))?;
    }
    Ok(())
}
