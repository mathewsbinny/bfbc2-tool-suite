//! Round `OR12` ("OPFOR in Rust"): the console Onslaught port's stages 11 - 13 (`console_cull`, `console_loading`,
//! `console_hud`) in the Rust core (`bc2_install::onslaught::{cull, loading, hud}`). OPFOR's scripts keep their
//! command lines, the install ledger's calls and the scratch clean-up.
//!
//! Each returns, as OR9's stages, `(writes, printed, fail, after)`: the game copy's writes in order (`(path, built
//! path)`), the lines printed before the ledger's commit, `None`, and no lines after. A failure prints the lines the
//! stage printed before it (as the Python did on its way) and raises `SystemExit` (a tool's failure or a refusal),
//! `OSError` or `ValueError`. `onsport_hud` adds a fifth item: each bundle's entry record for after the commit
//! (`(bundle, json text, [kept originals])`).

use bc2_install::onslaught::port::StageOut;
use bc2_install::onslaught::{OnsError, Source, cull, hud, loading};
use pyo3::exceptions::{PyOSError, PySystemExit, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

fn err(e: OnsError) -> PyErr {
    match e {
        OnsError::Tool(_) => PySystemExit::new_err(e.to_string()),
        OnsError::Io(m) => PyOSError::new_err(m),
        OnsError::Data(m) => PyValueError::new_err(m),
        OnsError::Exit(m) => PySystemExit::new_err(m),
    }
}

/// The stage's printed lines through Python's `print` (flushed), before its error is raised.
fn fail_after(py: Python<'_>, out: &StageOut, e: OnsError) -> PyErr {
    let printed = (|| -> PyResult<()> {
        let print = py.import("builtins")?.getattr("print")?;
        let kw = PyDict::new(py);
        kw.set_item("flush", true)?;
        for line in &out.printed {
            print.call((line,), Some(&kw))?;
        }
        Ok(())
    })();
    match printed {
        Ok(()) => err(e),
        Err(p) => p,
    }
}

fn writes_list<'py>(py: Python<'py>, out: &StageOut) -> PyResult<Bound<'py, PyList>> {
    let writes = PyList::empty(py);
    for (dst, src) in &out.writes {
        match src {
            Source::File(p) => writes.append((dst, p))?,
            Source::Bytes(b) => writes.append((dst, pyo3::types::PyBytes::new(py, b)))?,
        }
    }
    Ok(writes)
}

fn stage_out(py: Python<'_>, out: StageOut) -> PyResult<Bound<'_, PyTuple>> {
    let writes = writes_list(py, &out)?;
    (writes, out.printed, out.fail, out.after).into_pyobject(py)
}

/// `console_cull.build(lv)` (`pack` false: the writes are the edits to make, `(partition in the unpack, built
/// file)`, none when nothing is to add) or `apply`'s second half after the ledger's check (`pack` true: the bundle's
/// write). `cli`: `console_common.cli_command('culltree')`.
#[pyfunction]
pub fn onsport_cull<'py>(
    py: Python<'py>,
    root: &str,
    game: &str,
    lv: &str,
    cli: Vec<String>,
    pack: bool,
) -> PyResult<Bound<'py, PyTuple>> {
    let a = cull::Args {
        root: root.to_owned(),
        game: game.to_owned(),
        lv: lv.to_lowercase(),
        cli,
    };
    let mut out = StageOut::default();
    let r = py.detach(|| {
        if pack {
            cull::pack(&a, &mut out)
        } else {
            cull::build(&a, &mut out).map(|edits| {
                out.writes.extend(edits.into_iter().map(|(d, f)| (d, Source::File(f))));
            })
        }
    });
    match r {
        Ok(()) => stage_out(py, out),
        Err(e) => Err(fail_after(py, &out, e)),
    }
}

/// `console_loading.build(lv, write)`.
#[pyfunction]
pub fn onsport_loading<'py>(
    py: Python<'py>,
    root: &str,
    game: &str,
    lv: &str,
    write: bool,
) -> PyResult<Bound<'py, PyTuple>> {
    let a = loading::Args {
        root: root.to_owned(),
        game: game.to_owned(),
        lv: lv.to_lowercase(),
        write,
    };
    let mut out = StageOut::default();
    match py.detach(|| loading::build(&a, &mut out)) {
        Ok(()) => stage_out(py, out),
        Err(e) => Err(fail_after(py, &out, e)),
    }
}

/// `console_hud.apply(lv, write)` after the ledger's check; `sources`: per bundle (`ingame-00`, `loader-00`) the
/// file to build from and the file before this stage (or `None`).
#[pyfunction]
pub fn onsport_hud<'py>(
    py: Python<'py>,
    root: &str,
    game: &str,
    lv: &str,
    sources: Vec<(String, Option<String>)>,
    write: bool,
) -> PyResult<Bound<'py, PyTuple>> {
    let a = hud::Args {
        root: root.to_owned(),
        game: game.to_owned(),
        lv: lv.to_lowercase(),
        sources,
        write,
    };
    let mut out = StageOut::default();
    match py.detach(|| hud::build(&a, &mut out)) {
        Ok(records) => {
            let writes = writes_list(py, &out)?;
            let recs: Vec<(String, String, Vec<String>)> =
                records.into_iter().map(|r| (r.bundle, r.json, r.keeps)).collect();
            (writes, out.printed, out.fail, out.after, recs).into_pyobject(py)
        }
        Err(e) => Err(fail_after(py, &out, e)),
    }
}

/// `console_hud.patch_movie(movie, LOADING_SCREEN, tag)` (`--loading-screen`): the movie patched in place.
#[pyfunction]
pub fn onsport_hud_movie<'py>(py: Python<'py>, movie: &str, tag: &str) -> PyResult<Bound<'py, PyTuple>> {
    let mut out = StageOut::default();
    match py.detach(|| hud::loading_screen(movie, tag, &mut out)) {
        Ok(()) => stage_out(py, out),
        Err(e) => Err(fail_after(py, &out, e)),
    }
}
