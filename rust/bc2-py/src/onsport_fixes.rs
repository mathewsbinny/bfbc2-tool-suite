//! Round `OR13` ("OPFOR in Rust"): the rest of the console Onslaught port - the Napisal-level fixes
//! (`apply_ai_template`, `apply_combat_squads`, `apply_sector_offsets`: `bc2_install::onslaught::{napisal, squads,
//! sectoroff}`), stage 14 (`console_icons`: `icons`), the shader audit (`console_shader_audit`: `audit`) and the
//! driver (`console_port`: `driver`). OPFOR's scripts keep their command lines, the install ledger's calls (is-top,
//! the check before the work, the install, the revert) and the scratch clean-up; these are the work between those
//! calls.
//!
//! Errors as the other stages': `SystemExit` (a tool's failure or a refusal), `OSError`, `ValueError`; a call that
//! printed on its way prints those lines first.

use bc2_install::onslaught::port::StageOut;
use bc2_install::onslaught::{OnsError, Source, audit, driver, icons, napisal, sectoroff, squads};
use pyo3::exceptions::{PyOSError, PySystemExit, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

fn err(e: OnsError) -> PyErr {
    match e {
        OnsError::Tool(_) => PySystemExit::new_err(e.to_string()),
        OnsError::Io(m) => PyOSError::new_err(m),
        OnsError::Data(m) => PyValueError::new_err(m),
        OnsError::Exit(m) => PySystemExit::new_err(m),
    }
}

/// The lines printed so far through Python's `print` (flushed), then the error.
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

/// `apply_ai_template.decide(lv)` and its line: `(path, loads, counts [(template, n)], found [(bundle, entry)],
/// line)`.
#[pyfunction]
pub fn onsport_aitemplate_decide<'py>(py: Python<'py>, game: &str, lv: &str) -> PyResult<Bound<'py, PyTuple>> {
    let d = py.detach(|| napisal::decide(game, lv)).map_err(err)?;
    let line = napisal::decision_line(lv, &d);
    (d.path, d.loads, d.counts, d.found, line).into_pyobject(py)
}

/// `apply_ai_template.sources()`: `([(path, file, manifest line)], donor level file, donor spawner GUID)`.
#[pyfunction]
pub fn onsport_aitemplate_sources(py: Python<'_>, root: &str, game: &str) -> PyResult<napisal::Sources> {
    py.detach(|| napisal::sources(root, game)).map_err(err)
}

/// `apply`'s bundle step after the ledger's check: `('already', None, 0, '')` or `('packed', file, spawners,
/// check words)`.
#[pyfunction]
#[allow(
    clippy::too_many_arguments,
    clippy::needless_pass_by_value,
    reason = "the Python loop's state; PyO3 extracts a tuple argument by value"
)]
pub fn onsport_aitemplate_bundle(
    py: Python<'_>,
    root: &str,
    lv: &str,
    k: usize,
    bundle: &str,
    entry: &str,
    path: &str,
    sources: napisal::Sources,
) -> PyResult<(String, Option<String>, usize, String)> {
    match py
        .detach(|| napisal::edit_bundle(root, lv, k, bundle, entry, path, &sources))
        .map_err(err)?
    {
        napisal::Swapped::Already => Ok(("already".into(), None, 0, String::new())),
        napisal::Swapped::Packed(p, n, chk) => Ok(("packed".into(), Some(p), n, chk)),
    }
}

/// `apply_sector_offsets.apply`'s first half for a level: `([(asset file, its path in the bundle)])`; the lines
/// printed (each kind's, as it was built).
#[pyfunction]
pub fn onsport_sectoroff_assets(py: Python<'_>, root: &str, game: &str, lv: &str) -> PyResult<Vec<(String, String)>> {
    let mut out = StageOut::default();
    let r = py.detach(|| sectoroff::assets(root, game, lv, &mut out));
    match r {
        Ok(a) => {
            let print = py.import("builtins")?.getattr("print")?;
            for line in &out.printed {
                print.call1((line,))?;
            }
            Ok(a)
        }
        Err(e) => Err(fail_after(py, &out, e)),
    }
}

/// `--plan`'s lines of a level.
#[pyfunction]
pub fn onsport_sectoroff_plan(py: Python<'_>, root: &str, game: &str, lv: &str) -> PyResult<Vec<String>> {
    py.detach(|| sectoroff::plan_lines(root, game, lv)).map_err(err)
}

/// `apply`'s bundle step after the ledger's check: the file to install, or `None` when the bundle holds the
/// assets already.
#[pyfunction]
#[allow(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn onsport_sectoroff_bundle(
    py: Python<'_>,
    root: &str,
    lv: &str,
    k: usize,
    bundle: &str,
    assets: Vec<(String, String)>,
) -> PyResult<Option<String>> {
    match py
        .detach(|| sectoroff::edit_bundle(root, lv, k, bundle, &assets))
        .map_err(err)?
    {
        sectoroff::Edited::Already => Ok(None),
        sectoroff::Edited::Packed(p) => Ok(Some(p)),
    }
}

/// `apply_combat_squads.apply`'s bundle step after the ledger's check: (the file to install, the lines printed
/// after the install).
#[pyfunction]
pub fn onsport_squads_bundle(py: Python<'_>, root: &str, k: usize, bundle: &str) -> PyResult<(String, Vec<String>)> {
    py.detach(|| squads::edit_bundle(root, k, bundle).map(|(p, c)| (p, squads::chosen_lines(&c))))
        .map_err(err)
}

/// `apply_combat_squads --plan`'s lines.
#[pyfunction]
pub fn onsport_squads_plan(py: Python<'_>, root: &str, game: &str) -> PyResult<Vec<String>> {
    py.detach(|| squads::plan_lines(root, game)).map_err(err)
}

/// Stage 14, `console_icons.apply(lv, write)` after the ledger's check: `sources` per bundle (`ingame-00`,
/// `loader-00`) the file to build from; `aptcheck` the command line of `bfbc2 aptcheck`. `(writes, printed, fail,
/// after)` as the other stages.
#[pyfunction]
pub fn onsport_icons<'py>(
    py: Python<'py>,
    root: &str,
    game: &str,
    lv: &str,
    sources: Vec<String>,
    aptcheck: Vec<String>,
    write: bool,
) -> PyResult<Bound<'py, PyTuple>> {
    let a = icons::Args {
        root: root.to_owned(),
        game: game.to_owned(),
        lv: lv.to_lowercase(),
        sources,
        aptcheck,
        write,
    };
    let mut out = StageOut::default();
    match py.detach(|| icons::build(&a, &mut out)) {
        Ok(()) => {
            let writes: Vec<(String, String)> = out
                .writes
                .iter()
                .filter_map(|(d, s)| match s {
                    Source::File(p) => Some((d.clone(), p.clone())),
                    Source::Bytes(_) => None,
                })
                .collect();
            (writes, out.printed, out.fail, out.after).into_pyobject(py)
        }
        Err(e) => Err(fail_after(py, &out, e)),
    }
}

/// `console_icons.staged_movies()`: {bundle (lower case): movie file}.
#[pyfunction]
pub fn onsport_icons_staged(root: &str, game: &str) -> std::collections::BTreeMap<String, String> {
    icons::staged_movies(root, game)
}

/// `console_icons.coverage(staged=...)`: [(level, movie or None, [vehicle Names], [Names without a label])].
#[pyfunction]
#[pyo3(signature = (game, staged = None))]
#[allow(
    clippy::type_complexity,
    clippy::needless_pass_by_value,
    reason = "the Python's tuples; PyO3 extracts a dict argument by value"
)]
pub fn onsport_icons_coverage(
    py: Python<'_>,
    game: &str,
    staged: Option<std::collections::BTreeMap<String, String>>,
) -> PyResult<Vec<(String, Option<String>, Vec<String>, Vec<String>)>> {
    let got = py.detach(|| icons::coverage(game, staged.as_ref())).map_err(err)?;
    Ok(got
        .into_iter()
        .map(|(lv, m, vn, lack)| (lv, m, vn.into_iter().collect(), lack))
        .collect())
}

/// `console_shader_audit.audit(lv, bundle)`: its lines printed; True when nothing is missing.
#[pyfunction]
#[pyo3(signature = (root, game, lv, bundle = None))]
pub fn onsport_audit(py: Python<'_>, root: &str, game: &str, lv: &str, bundle: Option<String>) -> PyResult<bool> {
    let a = audit::Args {
        root: root.to_owned(),
        game: game.to_owned(),
        lv: lv.to_lowercase(),
        bundle,
    };
    let mut out = StageOut::default();
    match py.detach(|| audit::audit(&a, &mut out)) {
        Ok(ok) => {
            let print = py.import("builtins")?.getattr("print")?;
            let kw = PyDict::new(py);
            kw.set_item("flush", true)?;
            for line in &out.printed {
                print.call((line,), Some(&kw))?;
            }
            Ok(ok)
        }
        Err(e) => Err(fail_after(py, &out, e)),
    }
}

/// `console_port.main`'s plan: `(the ledger stage names reverted first when installed, last first; stage 1 runs
/// (the backup check, the earlier registration dropped); [(stage, command line or None, the option it lacks)])`.
#[pyfunction]
#[pyo3(signature = (lv, start, xbox, linux, opfor))]
#[allow(clippy::type_complexity, reason = "the Python's tuples")]
pub fn onsport_driver_plan(
    lv: &str,
    start: Option<&str>,
    xbox: Option<&str>,
    linux: Option<&str>,
    opfor: &str,
) -> PyResult<(Vec<String>, bool, Vec<(u32, Option<Vec<String>>, Option<String>)>)> {
    let p = driver::plan(lv, start, xbox, linux, opfor).map_err(err)?;
    let runs = p
        .runs
        .into_iter()
        .map(|(n, r)| match r {
            Ok(a) => (n, Some(a), None),
            Err(o) => (n, None, Some(o)),
        })
        .collect();
    Ok((p.reverts, p.first, runs))
}

/// `package.mft`'s text without the level's earlier registration: `(text, blocks left out)`.
#[pyfunction]
pub fn onsport_driver_blocks(text: &str, lv: &str) -> (String, usize) {
    driver::without_blocks(text, lv)
}
