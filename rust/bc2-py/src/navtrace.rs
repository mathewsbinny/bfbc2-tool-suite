//! Bindings for round E3c's pieces of `bc2-navbuild` (`trace`, `graph`): `trace_edges` / `trace_emit` (methods of
//! `NavLevel`, which call the helpers here), the roof rule's drop, the coverage nodes and the Gabriel pruning. The
//! Python reference runs instead on a `ValueError` / `TypeError` raised before anything was changed.

// cell indices become floats as Python's int -> float does (rounded to nearest, ties to even)
#![allow(clippy::cast_precision_loss)]

use bc2_navbuild::graph::{self, RoofCfg, RoofGrid};
use bc2_navbuild::route::Node;
use bc2_navbuild::trace::{EdgeKey, EdgeRows, Emitted, Kind, OneWay, Ordered, P3};
use bc2_navbuild::{Cell, FastMap, Level};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyBytesMethods, PyDict, PyList, PyTuple};

fn value_error(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(e.to_string())
}

/// An area's cells `[(i, j, k)]`, the surfaces checked against the level.
pub fn cells_of(level: &Level, cells: &[(i64, i64, i64)]) -> PyResult<Vec<Cell>> {
    cells
        .iter()
        .map(|&(i, j, k)| Ok((i, j, level.surface(k).map_err(value_error)?)))
        .collect()
}

/// The route nodes `[(id, pos, k, i, j)]` (`build_graph`'s `pts`) as surface, cell and position, the surfaces
/// checked.
pub fn nodes_of(level: &Level, pts: &Bound<'_, PyList>) -> PyResult<Vec<Node>> {
    pts.iter()
        .map(|p| {
            let (_id, pos, k, i, j) = p.extract::<(Bound<'_, PyAny>, (f64, f64, f64), i64, i64, i64)>()?;
            Ok(Node {
                k: level.surface(k).map_err(value_error)?,
                cell: (i, j),
                pos,
            })
        })
        .collect()
}

/// `area_of` (surface -> area index) from a dict, checked.
pub fn area_of_dict(level: &Level, d: &Bound<'_, PyDict>) -> PyResult<FastMap<usize, usize>> {
    let mut out = FastMap::default();
    out.reserve(d.len());
    for (k, v) in d.iter() {
        let k = level.surface(k.extract::<i64>()?).map_err(value_error)?;
        let v = usize::try_from(v.extract::<i64>()?).map_err(|_| value_error("a negative area index"))?;
        out.insert(k, v);
    }
    Ok(out)
}

/// The key tuple `(kind, target, axis, line, dir)` as `trace_edges` makes it.
fn key_tuple<'py>(py: Python<'py>, key: &EdgeKey) -> PyResult<Bound<'py, PyTuple>> {
    let (kind, target): (&Bound<'py, pyo3::types::PyString>, Bound<'py, PyAny>) = match key.kind {
        Kind::Portal(t) => (intern!(py, "B"), t.into_pyobject(py)?.into_any()),
        Kind::Border(di, dj) => (intern!(py, "C"), (di, dj).into_pyobject(py)?.into_any()),
        Kind::Outline => (intern!(py, "E"), py.None().into_bound(py)),
    };
    let axis = if key.x_axis { intern!(py, "x") } else { intern!(py, "z") };
    PyTuple::new(
        py,
        [
            kind.clone().into_any(),
            target,
            axis.clone().into_any(),
            key.line().into_pyobject(py)?.into_any(),
            key.dir.into_pyobject(py)?.into_any(),
        ],
    )
}

/// `trace_edges`' answer as the Python makes it: (per area a `defaultdict(list)` of rows `(along, y)`, one-way
/// candidates `(src, target, key, along)`).
pub fn edges_to_py<'py>(
    py: Python<'py>,
    per_area: &[EdgeRows],
    one_way: &[OneWay],
) -> PyResult<(Bound<'py, PyList>, Bound<'py, PyList>)> {
    let defaultdict = py
        .import(intern!(py, "collections"))?
        .getattr(intern!(py, "defaultdict"))?;
    let list_type = py.get_type::<PyList>();
    let out = PyList::empty(py);
    for edges in per_area {
        let d = defaultdict.call1((&list_type,))?;
        let dict = d.cast::<PyDict>()?;
        for (key, rows) in edges.iter() {
            let r = PyList::empty(py);
            for &(along, y) in rows {
                r.append((along, y))?;
            }
            dict.set_item(key_tuple(py, key)?, r)?;
        }
        out.append(d)?;
    }
    let ow = PyList::empty(py);
    for o in one_way {
        ow.append((o.src, o.target, key_tuple(py, &o.key)?, o.along))?;
    }
    Ok((out, ow))
}

/// An edge key from the Python's tuple; `None` for a key `trace_emit`'s port does not take.
fn key_of(t: &Bound<'_, PyAny>) -> Option<EdgeKey> {
    let (kind, target, axis, line, dir) = t.extract::<(String, Bound<'_, PyAny>, String, f64, i64)>().ok()?;
    let kind = match kind.as_str() {
        "B" => Kind::Portal(usize::try_from(target.extract::<i64>().ok()?).ok()?),
        "C" => {
            let (di, dj) = target.extract::<(i64, i64)>().ok()?;
            Kind::Border(di, dj)
        }
        "E" => Kind::Outline,
        _ => return None,
    };
    let x_axis = match axis.as_str() {
        "x" => true,
        "z" => false,
        _ => return None,
    };
    let l2 = line * 2.0;
    if !(l2.is_finite() && l2.fract() == 0.0 && l2.abs() < 9e15) {
        return None;
    }
    Some(EdgeKey {
        kind,
        x_axis,
        line2: l2 as i64,
        dir,
    })
}

/// Per area its edge rows from the Python's dicts (`trace_edges`' answer, changed by `drop_one_way` and the detail
/// pass); a `ValueError` for anything the port does not take.
pub fn edges_from_py(per_area: &Bound<'_, PyList>) -> PyResult<Vec<EdgeRows>> {
    let mut out = Vec::with_capacity(per_area.len());
    for d in per_area.iter() {
        let d = d
            .cast::<PyDict>()
            .map_err(|_| value_error("an area's edges are not a dict"))?;
        let mut pairs = Vec::with_capacity(d.len());
        for (k, rows) in d.iter() {
            let key = key_of(&k).ok_or_else(|| value_error("an edge key the port does not take"))?;
            let rows: Vec<(i64, f64)> = rows.extract()?;
            pairs.push((key, rows));
        }
        out.push(Ordered::from_pairs(pairs).ok_or_else(|| value_error("two edge keys the port takes as one"))?);
    }
    Ok(out)
}

fn p3(py: Python<'_>, p: P3) -> PyResult<Bound<'_, PyTuple>> {
    (p.0, p.1, p.2).into_pyobject(py)
}

/// `trace_emit`'s answer per area: (portals `[(p0, p1, target)]`, borders `[(p0, p1, direction, first, last,
/// line)]`, outline `[[p0, p1]]`); the caller makes the mesh's portal objects.
pub fn emitted_to_py<'py>(py: Python<'py>, areas: &[Emitted]) -> PyResult<Bound<'py, PyList>> {
    let out = PyList::empty(py);
    for a in areas {
        let portals = PyList::empty(py);
        for &(p0, p1, t) in &a.portals {
            portals.append((p3(py, p0)?, p3(py, p1)?, t))?;
        }
        let borders = PyList::empty(py);
        for b in &a.borders {
            let dir = (b.dir.0 as f64, 0.0, b.dir.1 as f64);
            borders.append((p3(py, b.p0)?, p3(py, b.p1)?, dir, b.first, b.last, b.line))?;
        }
        let outline = PyList::empty(py);
        for &(p0, p1) in &a.outline {
            outline.append(PyList::new(py, [p3(py, p0)?, p3(py, p1)?])?)?;
        }
        out.append((portals, borders, outline))?;
    }
    Ok(out)
}

fn flags(b: &[u8]) -> Vec<bool> {
    b.iter().map(|&v| v != 0).collect()
}

fn f64s(b: &[u8], what: &str) -> PyResult<Vec<f64>> {
    let (whole, rest) = b.as_chunks::<8>();
    if !rest.is_empty() {
        return Err(value_error(format!("{what}: not 8 bytes per value")));
    }
    Ok(whole.iter().map(|c| f64::from_le_bytes(*c)).collect())
}

fn i64s(b: &[u8], what: &str) -> PyResult<Vec<i64>> {
    let (whole, rest) = b.as_chunks::<8>();
    if !rest.is_empty() {
        return Err(value_error(format!("{what}: not 8 bytes per value")));
    }
    Ok(whole.iter().map(|c| i64::from_le_bytes(*c)).collect())
}

/// `navwalk.roof_islands` after its islands: the grid (column starts int64, heights float64), keep / terrain (one
/// byte per surface), the islands (int64, -1: not kept) and their count, the anchor surfaces and the rule's settings
/// (raise, share, near, `near_dy`). Returns (one byte per surface: dropped; the counts for `ROOF_STATS` - None, or
/// (islands, None) when no island was a candidate, or (islands, (dropped, surfaces))).
#[pyfunction]
#[expect(clippy::too_many_arguments, reason = "the grid, the islands and the rule's settings")]
#[expect(clippy::type_complexity, reason = "the counts as ROOF_STATS takes them")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn navkit_roof_drop<'py>(
    py: Python<'py>,
    nx: usize,
    nz: usize,
    start: &Bound<'py, PyBytes>,
    y: &Bound<'py, PyBytes>,
    keep: &Bound<'py, PyBytes>,
    terrain: &Bound<'py, PyBytes>,
    comp: &Bound<'py, PyBytes>,
    count: usize,
    anchors: Vec<i64>,
    origin: (f64, f64, f64),
    rule: (f64, f64, f64, f64),
) -> PyResult<(Bound<'py, PyBytes>, Option<(usize, Option<(usize, usize)>)>)> {
    let start: Vec<usize> = i64s(start.as_bytes(), "start")?
        .into_iter()
        .map(|v| usize::try_from(v).map_err(|_| value_error("a negative column start")))
        .collect::<PyResult<_>>()?;
    let (ys, kp, tr, cp) = (
        f64s(y.as_bytes(), "y")?,
        flags(keep.as_bytes()),
        flags(terrain.as_bytes()),
        i64s(comp.as_bytes(), "comp")?,
    );
    let anchors: Vec<usize> = anchors
        .iter()
        .map(|&k| usize::try_from(k).map_err(|_| value_error("a negative anchor surface")))
        .collect::<PyResult<_>>()?;
    let g = RoofGrid {
        nx,
        nz,
        start: &start,
        y: &ys,
        min_x: origin.0,
        min_z: origin.1,
        pitch: origin.2,
    };
    let cfg = RoofCfg {
        raise: rule.0,
        share: rule.1,
        near: rule.2,
        near_dy: rule.3,
    };
    let got = py
        .detach(|| graph::roof_drop(&g, &kp, &tr, &cp, count, &anchors, cfg))
        .ok_or_else(|| value_error("roof islands: arrays that do not fit the grid"))?;
    let bytes: Vec<u8> = got.out.iter().map(|&v| u8::from(v)).collect();
    Ok((PyBytes::new(py, &bytes), got.stats))
}

/// `navbuild.gabriel_prune(pts, pairs)` with the nodes' positions `[(x, y, z)]` and the pairs `[(a, b, d)]`: the
/// indices of the pairs kept, in the reference's order.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn navkit_gabriel(py: Python<'_>, pos: Vec<(f64, f64, f64)>, pairs: Vec<(i64, i64, f64)>) -> PyResult<Vec<usize>> {
    let pairs: Vec<(usize, usize, f64)> = pairs
        .iter()
        .map(|&(a, b, d)| match (usize::try_from(a), usize::try_from(b)) {
            (Ok(a), Ok(b)) => Ok((a, b, d)),
            _ => Err(value_error("a negative node index")),
        })
        .collect::<PyResult<_>>()?;
    py.detach(|| graph::gabriel_prune(&pos, &pairs))
        .ok_or_else(|| value_error("gabriel_prune: a pair naming no node, or a NaN length"))
}

/// `navbuild.add_cover_nodes`: the areas' cells (surfaces checked), the route nodes so far (`pts`, read for their
/// positions), `add` (`build_graph`'s: called with a cell's (i, j, k), may append to `pts`). A `ValueError` before the
/// first `add`; after it, a `RuntimeError` (the nodes have changed: the Python cannot run instead).
#[expect(clippy::too_many_arguments, reason = "the reference's arguments")]
pub fn cover_nodes(
    py: Python<'_>,
    level: &Level,
    areas: &[Vec<(i64, i64, i64)>],
    pts: &Bound<'_, PyList>,
    add: &Bound<'_, PyAny>,
    origin: (f64, f64),
    pitch: f64,
    y_offset: f64,
    cover: f64,
) -> PyResult<()> {
    let pos_of = |p: Bound<'_, PyAny>| -> PyResult<P3> { p.get_item(1)?.extract::<(f64, f64, f64)>() };
    let mut nodes: Vec<P3> = pts.iter().map(pos_of).collect::<PyResult<_>>()?;
    let mut cells = Vec::with_capacity(areas.len());
    let mut pos = Vec::with_capacity(areas.len());
    for a in areas {
        let c = cells_of(level, a)?;
        pos.push(
            c.iter()
                .map(|&(i, j, k)| {
                    (
                        origin.0 + i as f64 * pitch,
                        level.y(k) + y_offset,
                        origin.1 + j as f64 * pitch,
                    )
                })
                .collect::<Vec<P3>>(),
        );
        cells.push(a.clone());
    }
    let mut failed: Option<PyErr> = None;
    let mut call = |ai: usize, w: usize| -> Vec<P3> {
        if failed.is_some() {
            return Vec::new();
        }
        let Some(&cell) = cells.get(ai).and_then(|c| c.get(w)) else {
            return Vec::new();
        };
        let before = pts.len();
        let r = add.call1(cell).and_then(|_| {
            (before..pts.len())
                .map(|n| pos_of(pts.get_item(n)?))
                .collect::<PyResult<Vec<P3>>>()
        });
        match r {
            Ok(v) => v,
            Err(e) => {
                failed = Some(e);
                Vec::new()
            }
        }
    };
    let _ = py;
    graph::cover_nodes(&pos, &mut nodes, cover, &mut call);
    match failed {
        Some(e) => Err(PyRuntimeError::new_err(format!("add_cover_nodes: {e}"))),
        None => Ok(()),
    }
}

/// `navsample.load`'s records of a version 1 / 2 run: the data, where the records start, the columns and the
/// version. Returns (start int64, y float32, normal float32 (3 per surface), flags uint8, hit uint32 (3 per surface
/// in version 2), the columns read) - the arrays as little-endian bytearrays (writable, as NumPy's arrays of the
/// reference are).
#[pyfunction]
#[expect(clippy::type_complexity, reason = "the run's five arrays and a count")]
pub fn navkit_navsample_parse<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyBytes>,
    offset: usize,
    columns: usize,
    version: u32,
) -> PyResult<(
    Bound<'py, PyByteArray>,
    Bound<'py, PyByteArray>,
    Bound<'py, PyByteArray>,
    Bound<'py, PyByteArray>,
    Bound<'py, PyByteArray>,
    usize,
)> {
    let bytes = data.as_bytes();
    let r = py
        .detach(|| bc2_navbuild::navsample::parse_runs(bytes, offset, columns, version))
        .map_err(value_error)?;
    let le = |v: Vec<u8>| PyByteArray::new(py, &v);
    Ok((
        le(r.start.iter().flat_map(|v| v.to_le_bytes()).collect()),
        le(r.y.iter().flat_map(|v| v.to_le_bytes()).collect()),
        le(r.normal.iter().flat_map(|v| v.to_le_bytes()).collect()),
        PyByteArray::new(py, &r.flags),
        le(r.hit.iter().flat_map(|v| v.to_le_bytes()).collect()),
        r.read,
    ))
}
