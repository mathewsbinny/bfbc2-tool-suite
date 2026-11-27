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
//!
//! Round `OR10`: stages 5 - 7 (`onsport_vehicles`, `onsport_soldiers`, `onsport_shaders`), the same shape.

use std::collections::BTreeMap;

use bc2_install::onslaught::port::StageOut;
use bc2_install::onslaught::{self, Ctx, Entry, OnsError, Source, linux, loadset, mode, shaders, soldiers, vehicles};
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

/// `console_vehicles.build(lv)` (and `apply`'s pack) from the ledger's source bundle (stage 5).
#[pyfunction]
pub fn onsport_vehicles<'py>(
    py: Python<'py>,
    root: &str,
    game: &str,
    lv: &str,
    src_bundle: &str,
    write: bool,
) -> PyResult<Bound<'py, PyTuple>> {
    let a = vehicles::Args {
        root: root.to_owned(),
        game: game.to_owned(),
        lv: lv.to_lowercase(),
        src_bundle: src_bundle.to_owned(),
        write,
    };
    let out = py.detach(|| vehicles::build(&a)).map_err(err)?;
    stage_out(py, out)
}

/// `console_soldiers.build(lv)` (and `apply`'s pack) from the ledger's source bundle (stage 6); `keep_pc`, `only`,
/// `order_end`, `keep_guids`: the experiments' environment variables as OPFOR's script reads them.
#[pyfunction]
#[expect(
    clippy::too_many_arguments,
    reason = "the stage's arguments and its four experiments"
)]
#[pyo3(signature = (root, game, lv, src_bundle, write, keep_pc=Vec::new(), only=String::new(), order_end=false, keep_guids=false))]
pub fn onsport_soldiers<'py>(
    py: Python<'py>,
    root: &str,
    game: &str,
    lv: &str,
    src_bundle: &str,
    write: bool,
    keep_pc: Vec<String>,
    only: String,
    order_end: bool,
    keep_guids: bool,
) -> PyResult<Bound<'py, PyTuple>> {
    let a = soldiers::Args {
        root: root.to_owned(),
        game: game.to_owned(),
        lv: lv.to_lowercase(),
        src_bundle: src_bundle.to_owned(),
        write,
        keep_pc,
        only,
        order_end,
        keep_guids,
    };
    let out = py.detach(|| soldiers::build(&a)).map_err(err)?;
    stage_out(py, out)
}

/// `console_shaders.plan(lv)` / `apply`'s build (stage 7).
#[pyfunction]
pub fn onsport_shaders<'py>(
    py: Python<'py>,
    root: &str,
    game: &str,
    lv: &str,
    write: bool,
) -> PyResult<Bound<'py, PyTuple>> {
    let a = shaders::Args {
        root: root.to_owned(),
        game: game.to_owned(),
        lv: lv.to_lowercase(),
        write,
    };
    let out = py.detach(|| shaders::build(&a)).map_err(err)?;
    stage_out(py, out)
}
