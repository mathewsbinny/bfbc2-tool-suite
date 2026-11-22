//! `navkit build` / `all` / `apply` (rounds NK5, NK6): NavKit's level driver is `bc2_navbuild::inputs` and
//! `bc2_navbuild::apply` (the build's inputs, the build, the apply), the library the editor's AI workspace and the
//! export's path-data step run in process; here only the command line's edges - the lines printed through `py`, the
//! in-process FbrbTool runs logged as the Python's tool runs are, the errors as the Python's exceptions (a hand-off
//! gives the command line to the Python).

use bc2_navbuild::inputs::{self, Out, Root, Run};
use bc2_navbuild::ntpath;

use super::project::Project;
use crate::pyproc::{Fail, Py, PyErr, PyResult};

/// The library's answer as the command line's: the exception, or a hand-off.
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

/// The project's folders as the library takes them.
pub fn root(pr: &Project) -> Root {
    Root {
        root: pr.root.clone(),
        game: pr.game.clone(),
        work: pr.work.clone(),
        samples_dir: None,
    }
}

/// An in-process `FbrbTool` run, logged as the Python's tool runs are.
pub fn tool(args: &[String]) -> i32 {
    let logp = crate::pylog::log_path("fbrbtool");
    let o = bc2_tools::fbrbtool::run(args, logp.as_deref());
    crate::pylog::append(logp.as_deref(), "fbrbtool", args, &o.log);
    o.code
}

/// The lines printed through `py`; a print that fails keeps its error.
pub struct Print<'a> {
    pub py: &'a mut Py,
    pub err: Option<Fail>,
}

impl Out for Print<'_> {
    fn line(&mut self, text: &str) -> ntpath::Res<()> {
        self.py.print(text).map_err(|e| {
            self.err.get_or_insert(e);
            ntpath::Fail::Sink
        })
    }
}

impl Print<'_> {
    /// The library's answer, a failed print's own error in place of [`ntpath::Fail::Sink`].
    pub fn done<T>(&mut self, r: ntpath::Res<T>) -> PyResult<T> {
        match r {
            Ok(v) => Ok(v),
            Err(ntpath::Fail::Sink) => Err(self.err.take().unwrap_or(Fail::Python)),
            Err(e) => Err(fail(e)),
        }
    }
}

/// `time.localtime()` of now as the sectors' build date and clock words.
pub fn run(no_detail: bool) -> Run {
    let now = crate::clock::now();
    Run {
        clock: bc2_navbuild::build::clock_words(
            u32::from(now.year),
            u32::from(now.month),
            u32::from(now.day),
            u32::from(now.hour),
            u32::from(now.minute),
            u32::from(now.second),
        ),
        no_detail,
    }
}

/// `pr.samples08(lv)`.
pub fn samples08(pr: &Project, lv: &str) -> PyResult<Option<String>> {
    root(pr).samples08(lv).map_err(fail)
}

/// `navkit.build(pr, lv, cfg=cfg)`.
pub fn build_level(py: &mut Py, pr: &Project, lv: &str, no_detail: bool) -> PyResult<()> {
    let mut out = Print { py, err: None };
    let r = inputs::build_level(&root(pr), lv, run(no_detail), &mut out, &tool);
    out.done(r)
}
