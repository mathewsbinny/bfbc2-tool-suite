//! Round `OR5` ("OPFOR in Rust", stage 5): Project OPFOR's console Onslaught import in the Rust core
//! (`bc2_install::onslaught`). OPFOR's `console_onslaught.py` keeps its command line and the ledger commit
//! (`console_common.install`): `onslaught_build` builds the level in the work folder, `onslaught_install_plan` the
//! bundles and the game copy's writes in their order.
//!
//! Python's shapes: the added files `{rel path: manifest line (str) or size (int)}`; the plan
//! `([(game path, built path (str) or bytes)], [printed line])`. A failure raises `SystemExit` with the message the
//! Python's `run()` gave (`failed: <command line>` and the tool's output), else `OSError` / `ValueError`.
//!
//! Round `OR9`: the console port's stages 2 - 4 (`onsport_linux`, `onsport_mode`, `onsport_loadset`): each returns
//! `(writes, printed, fail, after)` - the game copy's writes in order (as above), the lines printed before the ledger's
//! commit, the refusal to raise after printing them (or `None`), the lines printed after the commit.

use std::collections::BTreeMap;

use bc2_install::onslaught::port::StageOut;
use bc2_install::onslaught::{self, Ctx, Entry, OnsError, Source, deps, input, linux, loadset, markv, mode};
use pyo3::exceptions::{PyOSError, PySystemExit, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyTuple};

fn err(e: OnsError) -> PyErr {
    match e {
        OnsError::Tool(_) => PySystemExit::new_err(e.to_string()),
        OnsError::Io(m) => PyOSError::new_err(m),
        OnsError::Data(m) => PyValueError::new_err(m),
        OnsError::Exit(m) => PySystemExit::new_err(m),
    }
}

fn added_dict<'py>(py: Python<'py>, added: &onslaught::Added) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    for (k, v) in added {
        match v {
            Entry::Line(l) => d.set_item(k, l)?,
            Entry::Size(n) => d.set_item(k, n)?,
        }
    }
    Ok(d)
}

/// `console_onslaught.build(Ctx(root, xbox, ons, pc, work))`: the added files.
#[pyfunction]
pub fn onslaught_build<'py>(
    py: Python<'py>,
    root: &str,
    xbox: &str,
    ons: &str,
    pc: &str,
    work: &str,
) -> PyResult<Bound<'py, PyDict>> {
    let c = Ctx::new(root, xbox, ons, pc, work);
    let added = py.detach(|| onslaught::build(&c)).map_err(err)?;
    added_dict(py, &added)
}

/// `console_onslaught.install(c, added)` up to the ledger commit: (writes in order, printed lines).
#[pyfunction]
pub fn onslaught_install_plan<'py>(
    py: Python<'py>,
    root: &str,
    xbox: &str,
    ons: &str,
    pc: &str,
    work: &str,
    added: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyTuple>> {
    let mut a: onslaught::Added = BTreeMap::new();
    for (k, v) in added.iter() {
        let k: String = k.extract()?;
        let e = match v.extract::<String>() {
            Ok(l) => Entry::Line(l),
            Err(_) => Entry::Size(v.extract::<u64>()?),
        };
        a.insert(k, e);
    }
    let c = Ctx::new(root, xbox, ons, pc, work);
    let plan = py.detach(|| onslaught::install_plan(&c, &a)).map_err(err)?;
    let writes = PyList::empty(py);
    for (dst, src) in &plan.writes {
        match src {
            Source::File(p) => writes.append((dst, p))?,
            Source::Bytes(b) => writes.append((dst, PyBytes::new(py, b)))?,
        }
    }
    (writes, plan.printed).into_pyobject(py)
}

fn stage_out(py: Python<'_>, out: StageOut) -> PyResult<Bound<'_, PyTuple>> {
    let writes = PyList::empty(py);
    for (dst, src) in &out.writes {
        match src {
            Source::File(p) => writes.append((dst, p))?,
            Source::Bytes(b) => writes.append((dst, PyBytes::new(py, b)))?,
        }
    }
    (writes, out.printed, out.fail, out.after).into_pyobject(py)
}

/// `console_linux_base.build(lv, pc, linux_bundle, write)` after the ledger's source bundle (stage 2).
#[pyfunction]
#[expect(clippy::too_many_arguments, reason = "the stage's own arguments and its two folders")]
pub fn onsport_linux<'py>(
    py: Python<'py>,
    root: &str,
    game: &str,
    lv: &str,
    pc: &str,
    linux_bundle: &str,
    src_bundle: &str,
    write: bool,
) -> PyResult<Bound<'py, PyTuple>> {
    let a = linux::Args {
        root: root.to_owned(),
        game: game.to_owned(),
        lv: lv.to_lowercase(),
        pc: pc.to_lowercase(),
        linux_bundle: linux_bundle.to_owned(),
        src_bundle: src_bundle.to_owned(),
        write,
    };
    let out = py.detach(|| linux::build(&a)).map_err(err)?;
    stage_out(py, out)
}

/// `console_mode.plan(lv, console)` / `apply`'s build after the ledger's checks (stage 3).
#[pyfunction]
pub fn onsport_mode<'py>(
    py: Python<'py>,
    root: &str,
    game: &str,
    lv: &str,
    console: &str,
    write: bool,
) -> PyResult<Bound<'py, PyTuple>> {
    let a = mode::Args {
        root: root.to_owned(),
        game: game.to_owned(),
        lv: lv.to_lowercase(),
        console: console.to_owned(),
        write,
    };
    let out = py.detach(|| mode::build(&a)).map_err(err)?;
    stage_out(py, out)
}

/// `console_loadset.plan(lv)` / `apply`'s build (stage 4).
#[pyfunction]
pub fn onsport_loadset<'py>(
    py: Python<'py>,
    root: &str,
    game: &str,
    lv: &str,
    write: bool,
) -> PyResult<Bound<'py, PyTuple>> {
    let a = loadset::Args {
        root: root.to_owned(),
        game: game.to_owned(),
        lv: lv.to_lowercase(),
        write,
    };
    let out = py.detach(|| loadset::build(&a)).map_err(err)?;
    stage_out(py, out)
}

// ------------------------------------------------------------------------------------------------ round OR11

/// `console_markv.install(write)` up to the ledger (stage 8): `src_bundle` the ledger's source bundle, `pre_bundle` the
/// level before stage 5 (`before_path(bundle, 'vehicles:ons_mp_002')`).
#[pyfunction]
pub fn onsport_markv<'py>(
    py: Python<'py>,
    root: &str,
    game: &str,
    src_bundle: &str,
    pre_bundle: &str,
    write: bool,
) -> PyResult<Bound<'py, PyTuple>> {
    let a = markv::Args {
        root: root.to_owned(),
        game: game.to_owned(),
        src_bundle: src_bundle.to_owned(),
        pre_bundle: pre_bundle.to_owned(),
        write,
    };
    let out = py.detach(|| markv::build(&a)).map_err(err)?;
    stage_out(py, out)
}

/// `console_input.build(lv)` / `apply(lv, write)` up to the ledger (stage 9).
#[pyfunction]
pub fn onsport_input<'py>(
    py: Python<'py>,
    root: &str,
    game: &str,
    lv: &str,
    src_bundle: &str,
    write: bool,
) -> PyResult<Bound<'py, PyTuple>> {
    let a = input::Args {
        root: root.to_owned(),
        game: game.to_owned(),
        lv: lv.to_lowercase(),
        src_bundle: src_bundle.to_owned(),
        write,
    };
    let out = py.detach(|| input::build(&a)).map_err(err)?;
    stage_out(py, out)
}

/// `console_deps.build(lv)` / `apply(lv, write)` up to the ledger (stage 10).
#[pyfunction]
pub fn onsport_deps<'py>(
    py: Python<'py>,
    root: &str,
    game: &str,
    lv: &str,
    src_bundle: &str,
    write: bool,
) -> PyResult<Bound<'py, PyTuple>> {
    let a = deps::Args {
        root: root.to_owned(),
        game: game.to_owned(),
        lv: lv.to_lowercase(),
        src_bundle: src_bundle.to_owned(),
        write,
    };
    let out = py.detach(|| deps::build(&a)).map_err(err)?;
    stage_out(py, out)
}

/// `console_input.role_inputs(t, counterpart_xml)` with the PC gunner mapping: {seat GUID: (concept, mapping)}.
#[pyfunction]
#[pyo3(signature = (t, counterpart, gunner))]
pub fn onsport_role_inputs<'py>(
    py: Python<'py>,
    t: &str,
    counterpart: Option<&str>,
    gunner: &str,
) -> PyResult<Bound<'py, PyDict>> {
    let got = input::role_inputs(t, counterpart, gunner).map_err(err)?;
    let d = PyDict::new(py);
    for (g, v) in got {
        d.set_item(g, v)?;
    }
    Ok(d)
}

/// `console_input.seat_edits(part, t, want, roles)`: (the new XML, the number of edits, the lines for `roles`).
#[pyfunction]
pub fn onsport_seat_edits<'py>(
    py: Python<'py>,
    part: &str,
    t: &str,
    want: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyTuple>> {
    let mut w: Vec<(String, input::Input)> = Vec::new();
    for (k, v) in want.iter() {
        w.push((k.extract()?, v.extract()?));
    }
    let mut roles = Vec::new();
    let (new, n) = input::seat_edits(part, t, &w, &mut roles).map_err(err)?;
    (new, n, roles).into_pyobject(py)
}
