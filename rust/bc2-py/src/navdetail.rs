//! Bindings for round NK3's `bc2_navbuild::detail`: NavKit's detail features of one area (the broken sides, the D
//! lines, the F cells) for `navdetail.py`, whose Python (`broken_sides` ... `_f_point`) was retired.
//!
//! The area comes as the Python holds it: `cells` a dict `{(i, j): height}` (its order is the order `eligible` and
//! `breakable` are asked in), `half` a dict `{((i, j), (di, dj)): height}` or `None`; `eligible(i, j, di, dj)` and
//! `breakable(a, b)` are the caller's callables (or `None`: every side), asked only for the sides that step more than
//! `step`, as the reference asked them.

use bc2_navbuild::detail::{self, Candidate, DetailSettings, Features, GridCell, Point, Side};
use pyo3::exceptions::PyKeyError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// `((i, j), (di, dj))` from Python.
fn side_of(o: &Bound<'_, PyAny>) -> PyResult<Side> {
    o.extract::<Side>()
}

fn cells_of(cells: &Bound<'_, PyDict>) -> PyResult<Vec<(GridCell, f64)>> {
    cells
        .iter()
        .map(|(k, v)| Ok((k.extract::<GridCell>()?, v.extract::<f64>()?)))
        .collect()
}

fn half_of(half: Option<&Bound<'_, PyDict>>) -> PyResult<Vec<(Side, f64)>> {
    half.map_or_else(
        || Ok(Vec::new()),
        |h| h.iter().map(|(k, v)| Ok((side_of(&k)?, v.extract::<f64>()?))).collect(),
    )
}

type PyPoint = (f64, f64, f64);

fn point(p: Point) -> PyPoint {
    let [x, y, z] = p;
    (x, y, z)
}

/// An F record as `(direction, points)`.
type PyFCell = (PyPoint, Vec<PyPoint>);

/// `(broken sides, D sides, x-F cells, z-F cells, D lines, F records)`, the sides and cells sorted.
type DetailOut = (
    Vec<Side>,
    Vec<Side>,
    Vec<GridCell>,
    Vec<GridCell>,
    Vec<Vec<PyPoint>>,
    Vec<PyFCell>,
);

fn out(f: Features) -> DetailOut {
    (
        f.broken.into_iter().collect(),
        f.d_sides.into_iter().collect(),
        f.fx.into_iter().collect(),
        f.fz.into_iter().collect(),
        f.lines
            .into_iter()
            .map(|l| l.into_iter().map(point).collect())
            .collect(),
        f.cells
            .into_iter()
            .map(|c| (point(c.direction), c.points.into_iter().map(point).collect()))
            .collect(),
    )
}

/// `navdetail.detail_features` (`navdetail.py`'s docstring): one area's broken sides, D sides, F cells of each axis,
/// D polylines (world points, owner on the right) and F records (`(direction, closed polygon)`: all `-x`, `+x`,
/// `-z`, `+z`). `cut_corners` / `chain_turns`: `navdetail.CUT_CORNERS` / `CHAIN_TURNS`.
#[pyfunction]
#[pyo3(signature = (cells, pitch, ox, oz, step, eligible=None, half=None, breakable=None, with_f=true,
                    cut_corners=true, chain_turns=true))]
#[expect(
    clippy::too_many_arguments,
    reason = "navdetail.detail_features' arguments and its two switches"
)]
pub fn navkit_detail_features(
    cells: &Bound<'_, PyDict>,
    pitch: f64,
    ox: f64,
    oz: f64,
    step: f64,
    eligible: Option<&Bound<'_, PyAny>>,
    half: Option<&Bound<'_, PyDict>>,
    breakable: Option<&Bound<'_, PyAny>>,
    with_f: bool,
    cut_corners: bool,
    chain_turns: bool,
) -> PyResult<DetailOut> {
    let s = DetailSettings {
        pitch,
        ox,
        oz,
        step,
        with_f,
        cut_corners,
        chain_turns,
    };
    let cells = cells_of(cells)?;
    let half = half_of(half)?;
    let breaks =
        |a: GridCell, b: GridCell| -> PyResult<bool> { breakable.map_or(Ok(true), |f| f.call1((a, b))?.is_truthy()) };
    let f = detail::detail_features(&cells, &half, &s, |c| match *c {
        Candidate::Side(((i, j), (di, dj))) => {
            if let Some(f) = eligible
                && !f.call1((i, j, di, dj))?.is_truthy()?
            {
                return Ok(false);
            }
            breaks((i, j), (i + di, j + dj))
        }
        Candidate::Leg(((i, j), (di, dj))) => breaks((i, j), (i + di, j + dj)),
    })?;
    Ok(out(f))
}

/// `navdetail.f_cells(fset, axis, cells, pitch, ox, oz)`: the F records of one axis (`'x'` or `'z'`) of the cells
/// `fset`, heights from `cells` (`{(i, j): height}`), as `(direction, closed polygon)`.
#[pyfunction]
pub fn navkit_detail_f_cells(
    fset: &Bound<'_, PyAny>,
    axis: &str,
    cells: &Bound<'_, PyDict>,
    pitch: f64,
    ox: f64,
    oz: f64,
) -> PyResult<Vec<PyFCell>> {
    let fset = fset
        .try_iter()?
        .map(|c| c?.extract::<GridCell>())
        .collect::<PyResult<_>>()?;
    let cells = cells_of(cells)?;
    let x = match axis {
        "x" => true,
        "z" => false,
        _ => return Err(PyKeyError::new_err(format!("('{axis}', -1)"))),
    };
    let s = DetailSettings {
        pitch,
        ox,
        oz,
        step: 0.0,
        with_f: true,
        cut_corners: true,
        chain_turns: true,
    };
    Ok(detail::f_records(&fset, x, &cells, &s)
        .into_iter()
        .map(|c| (point(c.direction), c.points.into_iter().map(point).collect()))
        .collect())
}
