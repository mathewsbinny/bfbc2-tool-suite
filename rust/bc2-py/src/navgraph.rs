//! Bindings for round E3h: a stage's areas kept in the Rust core (`NavAreas`, made by `NavLevel.build_areas_many`
//! with `store=True`), so the tracing and the route graphs take them from there instead of from the Python's cell
//! lists, and `build_graph` whole for every sector of a stage at once, in parallel (`NavLevel.graph_many`). The
//! `NavLevel` methods call the helpers here. A `ValueError` / `TypeError` is raised before anything changed: the
//! Python reference runs instead.

use std::collections::HashSet;
use std::sync::RwLock;

use bc2_navbuild::sector::{self, GraphCfg, GraphJob};
use bc2_navbuild::trace::{self, TraceCfg};
use bc2_navbuild::{Cell, Level, NavError, Rule};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyList, PyTuple};
use rayon::prelude::*;

use crate::navtrace;

fn value_error(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(e.to_string())
}

fn rule(name: &str) -> PyResult<Rule> {
    Rule::from_name(name).ok_or_else(|| value_error(format!("unknown link rule {name:?}")))
}

/// A stage's areas in the Rust core: per sector (in the stage's order), per area, its cells `(i, j, surface)`.
#[pyclass(frozen)]
pub struct NavAreas {
    sectors: RwLock<Vec<Vec<Vec<Cell>>>>,
}

impl NavAreas {
    /// Areas from the core's own cells.
    pub fn new(sectors: Vec<Vec<Vec<Cell>>>) -> NavAreas {
        NavAreas {
            sectors: RwLock::new(sectors),
        }
    }

    fn read(&self) -> PyResult<std::sync::RwLockReadGuard<'_, Vec<Vec<Vec<Cell>>>>> {
        self.sectors.read().map_err(value_error)
    }
}

#[pymethods]
impl NavAreas {
    /// Puts each sector's areas in the stage's order: `orders[s]` lists sector s's area indices in their new
    /// order (a permutation).
    #[expect(
        clippy::needless_pass_by_value,
        reason = "PyO3 extracts a list argument as an owned Vec"
    )]
    fn reorder(&self, orders: Vec<Vec<usize>>) -> PyResult<()> {
        let mut g = self.sectors.write().map_err(value_error)?;
        if orders.len() != g.len() {
            return Err(value_error("an order for each sector"));
        }
        for (areas, order) in g.iter_mut().zip(&orders) {
            let mut seen = vec![false; areas.len()];
            if order.len() != areas.len()
                || order
                    .iter()
                    .any(|&q| seen.get_mut(q).is_none_or(|s| std::mem::replace(s, true)))
            {
                return Err(value_error("an order that is not a permutation of the sector's areas"));
            }
            let mut old = std::mem::take(areas);
            *areas = order
                .iter()
                .map(|&q| old.get_mut(q).map_or_else(Vec::new, std::mem::take))
                .collect();
        }
        Ok(())
    }

    fn __len__(&self) -> PyResult<usize> {
        Ok(self.read()?.len())
    }
}

/// Areas from the Python's cells (`[[[(i, j, k)]]]`: sectors, areas, cells), the surfaces checked.
#[allow(clippy::type_complexity)] // the Python's nested lists
pub fn areas_from(level: &Level, sectors: &[Vec<Vec<(i64, i64, i64)>>]) -> PyResult<NavAreas> {
    let s = sectors
        .iter()
        .map(|areas| {
            areas
                .iter()
                .map(|a| navtrace::cells_of(level, a))
                .collect::<PyResult<Vec<Vec<Cell>>>>()
        })
        .collect::<PyResult<Vec<_>>>()?;
    Ok(NavAreas::new(s))
}

/// `trace_edges` for every sector of the store, in parallel: `origins` the sectors' (gi0, gj0) in the store's order.
#[expect(clippy::too_many_arguments, reason = "the stage's settings")]
pub fn trace_edges_stored<'py>(
    py: Python<'py>,
    level: &Level,
    store: &NavAreas,
    origins: &[(i64, i64)],
    window: (i64, i64),
    y_offset: f64,
    rule_name: &str,
    symmetric: bool,
) -> PyResult<Bound<'py, PyList>> {
    let rule = rule(rule_name)?;
    let sectors = store.read()?;
    if origins.len() != sectors.len() {
        return Err(value_error("an origin for each sector"));
    }
    let got = py
        .detach(|| {
            sectors
                .par_iter()
                .zip(origins.par_iter())
                .map(|(cells, &(gi0, gj0))| {
                    let cfg = TraceCfg {
                        gi0,
                        gj0,
                        window,
                        y_offset,
                        rule,
                    };
                    trace::trace_edges(level, cells, &trace::area_of(cells), cfg, symmetric)
                })
                .collect::<Result<Vec<_>, NavError>>()
        })
        .map_err(value_error)?;
    let out = PyList::empty(py);
    for (per_area, one_way) in &got {
        out.append(navtrace::edges_to_py(py, per_area, one_way)?)?;
    }
    Ok(out)
}

/// `trace_emit` for several sectors of the store at once, in parallel: `jobs` = `[(sector, per_area, gi0, gj0, (ox,
/// oz))]`; per job the answer `NavLevel.trace_emit` gives.
pub fn trace_emit_many<'py>(
    py: Python<'py>,
    level: &Level,
    store: &NavAreas,
    jobs: &Bound<'py, PyList>,
    pitch: f64,
    y_offset: f64,
    rule_name: &str,
) -> PyResult<Bound<'py, PyList>> {
    let rule = rule(rule_name)?;
    let sectors = store.read()?;
    let mut prepared = Vec::with_capacity(jobs.len());
    for job in jobs.iter() {
        let (s, per_area, gi0, gj0, origin) = job.extract::<(usize, Bound<'py, PyList>, i64, i64, (f64, f64))>()?;
        let cells = sectors
            .get(s)
            .ok_or_else(|| value_error("a sector the store does not hold"))?;
        if per_area.len() < cells.len() {
            return Err(value_error("fewer edge sets than areas"));
        }
        let mut rows = navtrace::edges_from_py(&per_area)?;
        rows.truncate(cells.len());
        prepared.push((s, rows, gi0, gj0, origin));
    }
    let got: Vec<_> = py.detach(|| {
        prepared
            .into_par_iter()
            .map(|(s, rows, gi0, gj0, origin)| {
                let cfg = TraceCfg {
                    gi0,
                    gj0,
                    window: (0, 0),
                    y_offset,
                    rule,
                };
                let cells = sectors.get(s).map_or(&[][..], Vec::as_slice);
                trace::trace_emit(level, cells, rows, cfg, origin.0, origin.1, pitch)
            })
            .collect()
    });
    let out = PyList::empty(py);
    for em in &got {
        out.append(navtrace::emitted_to_py(py, em)?)?;
    }
    Ok(out)
}

/// A surface from the caller, checked.
fn surface(level: &Level, k: i64) -> PyResult<usize> {
    level.surface(k).map_err(value_error)
}

/// One sector's job from the Python's tuple: `(sector, gi0, gj0, (ox, oz), mids, borders [((x, y, z), k)], orders
/// [(x, y, z)], ladders [(k, k)], blocked [(k, k)])`.
fn job_of(level: &Level, t: &Bound<'_, PyAny>) -> PyResult<(usize, GraphJob)> {
    type Raw = (
        usize,
        i64,
        i64,
        (f64, f64),
        Vec<Vec<(f64, f64)>>,
        Vec<((f64, f64, f64), i64)>,
        Vec<(f64, f64, f64)>,
        Vec<(i64, i64)>,
        Vec<(i64, i64)>,
    );
    let (s, gi0, gj0, origin, mids, borders, orders, ladders, blocked) = t.extract::<Raw>()?;
    let borders = borders
        .into_iter()
        .map(|(p, k)| Ok((p, surface(level, k)?)))
        .collect::<PyResult<Vec<_>>>()?;
    let ladders = ladders
        .into_iter()
        .map(|(a, b)| Ok((surface(level, a)?, surface(level, b)?)))
        .collect::<PyResult<Vec<_>>>()?;
    let blocked: HashSet<(usize, usize)> = blocked
        .into_iter()
        .map(|(a, b)| Ok((surface(level, a)?, surface(level, b)?)))
        .collect::<PyResult<_>>()?;
    Ok((
        s,
        GraphJob {
            gi0,
            gj0,
            origin,
            mids,
            borders,
            orders,
            ladders,
            blocked,
        },
    ))
}

/// `build_graph` for several sectors of the store at once, in parallel. `jobs`: see [`job_of`]; `cfg`: `(pitch,
/// y_offset, window, dist_edge_max, rule, cover or None, gabriel, order_nodes, ORDER_NODE_MIN, (min_x, min_z))`.
/// Per job: (nodes `[(border index or -1, (x, y, z), k, i, j)]` in the reference's order, pairs `[(a, b, d)]`, the
/// indices of the order nodes).
#[expect(clippy::type_complexity, reason = "the build's settings as a tuple")]
pub fn graph_many<'py>(
    py: Python<'py>,
    level: &Level,
    store: &NavAreas,
    jobs: &Bound<'py, PyList>,
    cfg: (
        f64,
        f64,
        (i64, i64),
        f64,
        String,
        Option<f64>,
        bool,
        bool,
        f64,
        (f64, f64),
    ),
) -> PyResult<Bound<'py, PyList>> {
    let (pitch, y_offset, window, dist_edge_max, rule_name, cover, gabriel, order_nodes, order_node_min, grid_min) =
        cfg;
    let gcfg = GraphCfg {
        pitch,
        y_offset,
        window,
        dist_edge_max,
        rule: rule(&rule_name)?,
        cover,
        gabriel,
        order_nodes,
        order_node_min,
        grid_min,
    };
    let sectors = store.read()?;
    let prepared = jobs
        .iter()
        .map(|t| {
            let (s, job) = job_of(level, &t)?;
            if s >= sectors.len() {
                return Err(value_error("a sector the store does not hold"));
            }
            Ok((s, job))
        })
        .collect::<PyResult<Vec<_>>>()?;
    let got = py
        .detach(|| {
            prepared
                .par_iter()
                .map(|(s, job)| {
                    let areas = sectors.get(*s).map_or(&[][..], Vec::as_slice);
                    sector::build_graph(level, areas, job, &gcfg)
                })
                .collect::<Result<Vec<_>, NavError>>()
        })
        .map_err(value_error)?;
    let out = PyList::empty(py);
    for g in got {
        let nodes = PyList::empty(py);
        let mut orders = Vec::new();
        for (idx, n) in g.nodes.iter().enumerate() {
            let src = n.border.map_or(-1, |b| i64::try_from(b).unwrap_or(-1));
            let (i, j) = n.node.cell;
            nodes.append(PyTuple::new(
                py,
                [
                    src.into_pyobject(py)?.into_any(),
                    n.node.pos.into_pyobject(py)?.into_any(),
                    n.node.k.into_pyobject(py)?.into_any(),
                    i.into_pyobject(py)?.into_any(),
                    j.into_pyobject(py)?.into_any(),
                ],
            )?)?;
            if n.order {
                orders.push(idx);
            }
        }
        out.append((nodes, g.pairs, orders))?;
    }
    Ok(out)
}
