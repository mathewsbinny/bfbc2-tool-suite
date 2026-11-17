//! Bindings for `bc2-navbuild`: `navbuild.Level` builds a `NavLevel` from its arrays and calls it for its grid
//! walks (`bfbc2-auto-navkit/tools/navbuild.py`); any `ValueError` here makes it run its own Python code instead.

use std::collections::HashSet;

use bc2_navbuild::{CellMap, Floats, Level, LevelData, NavError, Rule};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyBytesMethods, PyDict, PyList, PySet, PyTuple};

fn value_error(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(e.to_string())
}

fn rule(name: &str) -> PyResult<Rule> {
    Rule::from_name(name).ok_or_else(|| value_error(format!("unknown link rule {name:?}")))
}

fn chunks<const N: usize>(b: &[u8], what: &str) -> PyResult<Vec<[u8; N]>> {
    let (whole, rest) = b.as_chunks::<N>();
    if !rest.is_empty() {
        return Err(value_error(format!("{what}: {} bytes, not a multiple of {N}", b.len())));
    }
    Ok(whole.to_vec())
}

fn floats(b: &[u8], f32_: bool, what: &str) -> PyResult<Floats> {
    Ok(if f32_ {
        Floats::F32(chunks::<4>(b, what)?.into_iter().map(f32::from_le_bytes).collect())
    } else {
        Floats::F64(chunks::<8>(b, what)?.into_iter().map(f64::from_le_bytes).collect())
    })
}

/// `navbuild.Level` in Rust: made from its arrays' bytes (little-endian; `start` int64, `keep` / `stair` /
/// `terrain` one byte each, `y` float64, `gx` / `gz` float32 or float64 as their flags say).
#[pyclass(frozen)]
pub struct NavLevel {
    level: Level,
    blocked: std::sync::RwLock<HashSet<(usize, usize)>>,
}

/// A surface index or an error the caller turns into its fallback.
fn surface(l: &Level, k: i64) -> PyResult<usize> {
    l.surface(k).map_err(value_error)
}

/// The build's `cell_ij` dict as [`CellMap`]: read with `dict[k]`, entered like `dict.setdefault`.
struct DictCells<'a, 'py>(&'a Bound<'py, PyDict>);

impl CellMap for DictCells<'_, '_> {
    fn get(&self, k: usize) -> Option<(i64, i64)> {
        self.0.get_item(k).ok().flatten()?.extract::<(i64, i64)>().ok()
    }

    fn set_default(&mut self, k: usize, cell: (i64, i64)) {
        if !self.0.contains(k).unwrap_or(true) {
            let _ = self.0.set_item(k, cell);
        }
    }
}

#[pymethods]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
impl NavLevel {
    #[new]
    #[expect(clippy::too_many_arguments, reason = "Level's arrays and settings")]
    #[allow(clippy::similar_names)] // gx / gz as Level names them
    #[pyo3(signature = (nx, nz, start, keep, y, gx, gx_f32, gz, gz_f32, stair, terrain, pitch, step, link_step))]
    fn new(
        nx: usize,
        nz: usize,
        start: &Bound<'_, PyBytes>,
        keep: &Bound<'_, PyBytes>,
        y: &Bound<'_, PyBytes>,
        gx: &Bound<'_, PyBytes>,
        gx_f32: bool,
        gz: &Bound<'_, PyBytes>,
        gz_f32: bool,
        stair: &Bound<'_, PyBytes>,
        terrain: &Bound<'_, PyBytes>,
        pitch: f64,
        step: f64,
        link_step: f64,
    ) -> PyResult<Self> {
        let start: Vec<usize> = chunks::<8>(start.as_bytes(), "start")?
            .into_iter()
            .map(|b| usize::try_from(i64::from_le_bytes(b)).map_err(|_| value_error("a negative column start")))
            .collect::<PyResult<_>>()?;
        let flags = |b: &Bound<'_, PyBytes>| b.as_bytes().iter().map(|&v| v != 0).collect::<Vec<bool>>();
        let data = LevelData {
            nx,
            nz,
            start,
            keep: flags(keep),
            y: match floats(y.as_bytes(), false, "y")? {
                Floats::F64(v) => v,
                Floats::F32(_) => Vec::new(),
            },
            gx: floats(gx.as_bytes(), gx_f32, "gx")?,
            gz: floats(gz.as_bytes(), gz_f32, "gz")?,
            stair: flags(stair),
            terrain: flags(terrain),
            pitch,
            step,
            link_step,
        };
        Ok(NavLevel {
            level: Level::new(data).map_err(value_error)?,
            blocked: std::sync::RwLock::default(),
        })
    }

    /// `Level.neighbour(k, gi, gj, di, dj, rule=rule)` (no `prefer`): the surface or `None`.
    fn neighbour(&self, k: i64, gi: i64, gj: i64, di: i64, dj: i64, rule_name: &str) -> PyResult<Option<usize>> {
        let k = surface(&self.level, k)?;
        Ok(self.level.neighbour(k, gi, gj, di, dj, None, rule(rule_name)?))
    }

    /// `navbuild.components(level, members, gi0, gj0)` with `members` as `[(k, i, j)]` in the dict's order: the
    /// components as lists of `(i, j, k)`.
    fn components<'py>(
        &self,
        py: Python<'py>,
        members: Vec<(i64, i64, i64)>,
        gi0: i64,
        gj0: i64,
        rule_name: &str,
    ) -> PyResult<Bound<'py, PyList>> {
        let rule = rule(rule_name)?;
        let m: Vec<(usize, i64, i64)> = members
            .iter()
            .map(|&(k, i, j)| Ok((surface(&self.level, k)?, i, j)))
            .collect::<PyResult<_>>()?;
        let comps = py.detach(|| self.level.components(&m, gi0, gj0, rule));
        let out = PyList::empty(py);
        for c in comps {
            let cells: Vec<Bound<'py, PyTuple>> = c
                .into_iter()
                .map(|cell| cell.into_pyobject(py))
                .collect::<PyResult<_>>()?;
            out.append(PyList::new(py, cells)?)?;
        }
        Ok(out)
    }

    /// `trace_edges`' neighbours: for each `(i, j, k)` of an area, the neighbours across +x, -x, +z, -z
    /// preferring the area's own surfaces; `None` where there is none.
    fn side_neighbours(
        &self,
        py: Python<'_>,
        cells: Vec<(i64, i64, i64)>,
        gi0: i64,
        gj0: i64,
        rule_name: &str,
    ) -> PyResult<Vec<Option<usize>>> {
        let rule = rule(rule_name)?;
        let c: Vec<(i64, i64, usize)> = cells
            .iter()
            .map(|&(i, j, k)| Ok((i, j, surface(&self.level, k)?)))
            .collect::<PyResult<_>>()?;
        Ok(py.detach(|| self.level.side_neighbours(&c, gi0, gj0, rule)))
    }

    /// The D sides `line_walkable` does not step across (`Level.blocked`): replaces the set held.
    fn set_blocked(&self, pairs: Vec<(i64, i64)>) -> PyResult<()> {
        let set: HashSet<(usize, usize)> = pairs
            .iter()
            .map(|&(a, b)| Ok((surface(&self.level, a)?, surface(&self.level, b)?)))
            .collect::<PyResult<_>>()?;
        *self.blocked.write().map_err(value_error)? = set;
        Ok(())
    }

    /// `navbuild.line_walkable(level, k0, a, b, gi0, gj0, k1)` over the D sides given to `set_blocked`.
    #[expect(clippy::too_many_arguments, reason = "the reference's arguments")]
    fn line_walkable(
        &self,
        k0: i64,
        a: (i64, i64),
        b: (i64, i64),
        gi0: i64,
        gj0: i64,
        k1: i64,
        rule_name: &str,
    ) -> PyResult<bool> {
        let (k0, k1) = (surface(&self.level, k0)?, surface(&self.level, k1)?);
        let blocked = self.blocked.read().map_err(value_error)?;
        Ok(self
            .level
            .line_walkable(k0, a, b, gi0, gj0, k1, &blocked, rule(rule_name)?))
    }

    /// `build_graph`'s search: from the surfaces `starts` to one in `targets` (a set), over the sector window
    /// `window` (start, stop), entering reached surfaces into `cells` (the build's `cell_ij`). The path, or `None`.
    #[expect(clippy::too_many_arguments, reason = "the reference's state")]
    fn search(
        &self,
        starts: Vec<i64>,
        targets: &Bound<'_, PySet>,
        cells: &Bound<'_, PyDict>,
        window: (i64, i64),
        gi0: i64,
        gj0: i64,
        rule_name: &str,
    ) -> PyResult<Option<Vec<usize>>> {
        let rule = rule(rule_name)?;
        let starts: Vec<usize> = starts
            .iter()
            .map(|&k| surface(&self.level, k))
            .collect::<PyResult<_>>()?;
        let wanted: HashSet<usize> = targets
            .iter()
            .map(|t| t.extract::<i64>().map(|k| usize::try_from(k).unwrap_or(usize::MAX)))
            .collect::<PyResult<_>>()?;
        let is_target = |n: usize| wanted.contains(&n);
        let mut map = DictCells(cells);
        self.level
            .search(&starts, &is_target, &mut map, window, gi0, gj0, rule)
            .map_err(|e: NavError| value_error(e))
    }

    /// `navwalk.walkable`'s flood: `kept` (one byte per surface) extended from the surfaces `queue` over the
    /// surfaces this level keeps (made with `keep` = the walkability rules' `ok`); the new `kept`.
    fn flood<'py>(
        &self,
        py: Python<'py>,
        kept: &Bound<'py, PyBytes>,
        queue: Vec<i64>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let mut out: Vec<bool> = kept.as_bytes().iter().map(|&v| v != 0).collect();
        if out.len() != self.level.len() {
            return Err(value_error(format!(
                "{} kept flags for {} surfaces",
                out.len(),
                self.level.len()
            )));
        }
        let q: Vec<usize> = queue
            .iter()
            .map(|&k| surface(&self.level, k))
            .collect::<PyResult<_>>()?;
        py.detach(|| self.level.flood(&mut out, q));
        let bytes: Vec<u8> = out.into_iter().map(u8::from).collect();
        Ok(PyBytes::new(py, &bytes))
    }

    /// `navwalk.islands_of` over the surfaces this level keeps: the island ids (int64, little-endian bytes) and the
    /// number of islands.
    fn islands<'py>(&self, py: Python<'py>) -> (Bound<'py, PyBytes>, usize) {
        let (comp, n) = py.detach(|| self.level.islands());
        let bytes: Vec<u8> = comp.iter().flat_map(|v| v.to_le_bytes()).collect();
        (PyBytes::new(py, &bytes), n)
    }
}
