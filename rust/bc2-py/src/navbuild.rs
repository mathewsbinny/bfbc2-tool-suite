//! Bindings for `bc2-navbuild`: `navbuild.Level` builds a `NavLevel` from its arrays and calls it for its grid
//! walks (`bfbc2-auto-navkit/tools/navbuild.py`; their Python was retired on 2026-11-17, so a `ValueError` here
//! reaches the caller); `NavGrid` and the cell picking serve the walkability rules and `build_areas`, whose Python
//! still runs instead on a `ValueError`.

use std::collections::HashSet;

use bc2_navbuild::rules::{Grid, LipTests, Tri};
use bc2_navbuild::{CellMap, Floats, Level, LevelData, NavError, Rule, pick, pymath};
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

fn starts(b: &[u8]) -> PyResult<Vec<usize>> {
    chunks::<8>(b, "start")?
        .into_iter()
        .map(|b| usize::try_from(i64::from_le_bytes(b)).map_err(|_| value_error("a negative column start")))
        .collect()
}

fn heights(b: &[u8]) -> PyResult<Vec<f64>> {
    Ok(chunks::<8>(b, "y")?.into_iter().map(f64::from_le_bytes).collect())
}

/// `navwalk.matched_flags(s, other)` from the arrays' bytes (column starts int64, heights float64, flags uint8):
/// the matched flags, one byte per surface of s.
#[pyfunction]
pub fn navkit_matched_flags<'py>(
    py: Python<'py>,
    cols: usize,
    start: &Bound<'py, PyBytes>,
    y: &Bound<'py, PyBytes>,
    other_start: &Bound<'py, PyBytes>,
    other_y: &Bound<'py, PyBytes>,
    other_flags: &Bound<'py, PyBytes>,
) -> PyResult<Bound<'py, PyBytes>> {
    let (s, ys, os, oy) = (
        starts(start.as_bytes())?,
        heights(y.as_bytes())?,
        starts(other_start.as_bytes())?,
        heights(other_y.as_bytes())?,
    );
    let flags = other_flags.as_bytes();
    let out = py
        .detach(|| bc2_navbuild::matched_flags(cols, &s, &ys, &os, &oy, flags))
        .map_err(value_error)?;
    Ok(PyBytes::new(py, &out))
}

/// `build_graph`'s centre cells: for each area's cells (`[(i, j, k)]`), the index of its centre cell.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn navkit_centre_cells(py: Python<'_>, areas: Vec<Vec<(i64, i64, i64)>>) -> PyResult<Vec<usize>> {
    py.detach(|| {
        areas
            .iter()
            .map(|cells| {
                let ij: Vec<(i64, i64)> = cells.iter().map(|c| (c.0, c.1)).collect();
                pick::centre_cell(&ij)
            })
            .collect::<Option<Vec<usize>>>()
    })
    .ok_or_else(|| value_error("an area without cells, or cells a float cannot hold"))
}

/// `build_graph`'s portal cells: for each `(cells, points)` (an area's `[(i, j, k)]`, its portals' midpoints
/// `[(x, z)]`), the index of the cell nearest each point, with the sector origin (ox, oz) and the pitch.
#[pyfunction]
#[expect(clippy::type_complexity, reason = "PyO3 extracts the lists as they are")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn navkit_nearest_cells(
    py: Python<'_>,
    groups: Vec<(Vec<(i64, i64, i64)>, Vec<(f64, f64)>)>,
    ox: f64,
    oz: f64,
    pitch: f64,
) -> PyResult<Vec<Vec<usize>>> {
    py.detach(|| {
        groups
            .iter()
            .map(|(cells, points)| {
                let ij: Vec<(i64, i64)> = cells.iter().map(|c| (c.0, c.1)).collect();
                pick::nearest_cells(&ij, ox, oz, pitch, points)
            })
            .collect::<Option<Vec<Vec<usize>>>>()
    })
    .ok_or_else(|| value_error("an area without cells, or cells a float cannot hold"))
}

/// CPython's float functions as the port computes them, for the parity tests: for float64 bytes `a`, `b` (as many
/// values each), `math.hypot(a, b)`, `a ** 2` and `math.degrees(math.atan(math.hypot(a, b)))`, float64 bytes each.
#[pyfunction]
pub fn navkit_float_ops<'py>(
    py: Python<'py>,
    a: &Bound<'py, PyBytes>,
    b: &Bound<'py, PyBytes>,
) -> PyResult<(Bound<'py, PyBytes>, Bound<'py, PyBytes>, Bound<'py, PyBytes>)> {
    let (xs, zs) = (heights(a.as_bytes())?, heights(b.as_bytes())?);
    if xs.len() != zs.len() {
        return Err(value_error("a and b differ in length"));
    }
    let hyp: Vec<f64> = xs.iter().zip(&zs).map(|(&x, &z)| pymath::hypot(x, z)).collect();
    let sqs: Vec<f64> = xs.iter().map(|&x| pymath::sq(x)).collect();
    let deg: Vec<f64> = xs.iter().zip(&zs).map(|(&x, &z)| pymath::slope_deg(x, z)).collect();
    Ok((f64_bytes(py, &hyp), f64_bytes(py, &sqs), f64_bytes(py, &deg)))
}

fn flags(b: &[u8]) -> Vec<bool> {
    b.iter().map(|&v| v != 0).collect()
}

fn bool_bytes<'py>(py: Python<'py>, v: &[bool]) -> Bound<'py, PyBytes> {
    let b: Vec<u8> = v.iter().map(|&x| u8::from(x)).collect();
    PyBytes::new(py, &b)
}

fn f64_bytes<'py>(py: Python<'py>, v: &[f64]) -> Bound<'py, PyBytes> {
    let b: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
    PyBytes::new(py, &b)
}

/// A sample run (`navsample.Samples`) for `navwalk.walkable`'s rules: column starts (int64), heights (`float32` or
/// `float64` as `y_f32` says, held as `float64` like `s.y.astype(float)`), pitch and grid origin.
#[pyclass(frozen)]
pub struct NavGrid {
    grid: Grid,
}

#[pymethods]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts an optional tuple argument as owned"
)]
impl NavGrid {
    #[new]
    #[expect(clippy::too_many_arguments, reason = "the run's arrays and settings")]
    #[pyo3(signature = (nx, nz, start, y, y_f32, pitch, min_x, min_z))]
    fn new(
        nx: usize,
        nz: usize,
        start: &Bound<'_, PyBytes>,
        y: &Bound<'_, PyBytes>,
        y_f32: bool,
        pitch: f64,
        min_x: f64,
        min_z: f64,
    ) -> PyResult<Self> {
        let ys = match floats(y.as_bytes(), y_f32, "y")? {
            Floats::F32(v) => v.into_iter().map(f64::from).collect(),
            Floats::F64(v) => v,
        };
        Ok(NavGrid {
            grid: Grid::new(nx, nz, starts(start.as_bytes())?, ys, pitch, min_x, min_z).map_err(value_error)?,
        })
    }

    /// `navwalk.headroom(s, ignore)`: float64 bytes.
    #[pyo3(signature = (ignore=None))]
    fn headroom<'py>(&self, py: Python<'py>, ignore: Option<&Bound<'py, PyBytes>>) -> PyResult<Bound<'py, PyBytes>> {
        let ig = ignore.map(|b| flags(b.as_bytes()));
        let out = py.detach(|| self.grid.headroom(ig.as_deref())).map_err(value_error)?;
        Ok(f64_bytes(py, &out))
    }

    /// `navwalk.stair_slope(s, mask, r, tol)`: (slope, gx, gz) as float64 bytes.
    fn stair_slope<'py>(
        &self,
        py: Python<'py>,
        mask: &Bound<'py, PyBytes>,
        r: i64,
        tol: f64,
    ) -> PyResult<(Bound<'py, PyBytes>, Bound<'py, PyBytes>, Bound<'py, PyBytes>)> {
        let m = flags(mask.as_bytes());
        let (deg, gx, gz) = py.detach(|| self.grid.stair_slope(&m, r, tol)).map_err(value_error)?;
        Ok((f64_bytes(py, &deg), f64_bytes(py, &gx), f64_bytes(py, &gz)))
    }

    /// `navwalk.narrow_lips(s, base, cand, slope, width, rise, flat)` with `lim` = `tan(radians(slope))` and `flat`
    /// as (the normals' y bytes, float32, `cos(radians(flat))`): one byte per surface.
    #[expect(clippy::too_many_arguments, reason = "the reference's arguments")]
    #[pyo3(signature = (base, cand, lim, width, rise=None, flat=None))]
    fn narrow_lips<'py>(
        &self,
        py: Python<'py>,
        base: &Bound<'py, PyBytes>,
        cand: &Bound<'py, PyBytes>,
        lim: f64,
        width: i64,
        rise: Option<f64>,
        flat: Option<(Bound<'py, PyBytes>, bool, f64)>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let (b, c) = (flags(base.as_bytes()), flags(cand.as_bytes()));
        let ny = flat
            .as_ref()
            .map(|(n, f32_, cos)| Ok::<_, PyErr>((floats(n.as_bytes(), *f32_, "normal")?, *cos)))
            .transpose()?;
        let tests = LipTests {
            rise,
            flat: ny.as_ref().map(|(n, cos)| (n, *cos)),
        };
        let out = py
            .detach(|| self.grid.narrow_lips(&b, &c, lim, width, tests))
            .map_err(value_error)?;
        Ok(bool_bytes(py, &out))
    }

    /// `walkable`'s landings around the `stair` surfaces among `cand`: one byte per surface.
    fn landing<'py>(
        &self,
        py: Python<'py>,
        stair: &Bound<'py, PyBytes>,
        cand: &Bound<'py, PyBytes>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let (s, c) = (flags(stair.as_bytes()), flags(cand.as_bytes()));
        let out = py.detach(|| self.grid.landing(&s, &c)).map_err(value_error)?;
        Ok(bool_bytes(py, &out))
    }

    /// `navwalk.near_objects(s, tris, grow, below, above)` with `tris` as float64 bytes (9 per triangle).
    fn near_objects<'py>(
        &self,
        py: Python<'py>,
        tris: &Bound<'py, PyBytes>,
        grow: f64,
        below: f64,
        above: f64,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let t = triangles(tris.as_bytes())?;
        let out = py
            .detach(|| self.grid.near_objects(&t, grow, below, above))
            .map_err(value_error)?;
        Ok(bool_bytes(py, &out))
    }

    /// `navwalk.on_objects(s, tris, tol)` for the triangles its bounding-box filter keeps (float64 bytes).
    fn on_objects<'py>(&self, py: Python<'py>, tris: &Bound<'py, PyBytes>, tol: f64) -> PyResult<Bound<'py, PyBytes>> {
        let t = triangles(tris.as_bytes())?;
        let out = py.detach(|| self.grid.on_objects(&t, tol)).map_err(value_error)?;
        Ok(bool_bytes(py, &out))
    }
}

/// Triangles from float64 bytes, nine values each.
fn triangles(b: &[u8]) -> PyResult<Vec<Tri>> {
    let v: Vec<f64> = chunks::<8>(b, "tris")?.into_iter().map(f64::from_le_bytes).collect();
    let (whole, rest) = v.as_chunks::<9>();
    if !rest.is_empty() {
        return Err(value_error("tris: not nine values per triangle"));
    }
    Ok(whole
        .iter()
        .map(|t| {
            let [x0, y0, z0, x1, y1, z1, x2, y2, z2] = *t;
            [[x0, y0, z0], [x1, y1, z1], [x2, y2, z2]]
        })
        .collect())
}

/// `navbuild.Level` in Rust: made from its arrays' bytes (little-endian; `start` int64, `keep` / `stair` /
/// `terrain` one byte each, `y` float64, `gx` / `gz` float32 or float64 as their flags say).
#[pyclass(frozen)]
pub struct NavLevel {
    level: Level,
    blocked: std::sync::RwLock<HashSet<(usize, usize)>>,
}

/// A surface index, or a `ValueError` for one outside the level.
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

    /// `navbuild.build_areas(level, gi0, gj0, start, tile, tiles)` over the sector window `window` (start, stop;
    /// step 1), bisected at `max_area` (`MAX_AREA` when `BISECT_AREAS`, else `None`): the areas as `(tile, cells)`.
    #[expect(clippy::too_many_arguments, reason = "the reference's arguments")]
    fn build_areas<'py>(
        &self,
        py: Python<'py>,
        gi0: i64,
        gj0: i64,
        window: (i64, i64),
        start: (i64, i64),
        tile: i64,
        tiles: i64,
        max_area: Option<i64>,
        rule_name: &str,
    ) -> PyResult<Bound<'py, PyList>> {
        let rule = rule(rule_name)?;
        let areas = py
            .detach(|| {
                self.level
                    .build_areas(gi0, gj0, window, start, tile, tiles, max_area, rule)
            })
            .map_err(value_error)?;
        let out = PyList::empty(py);
        for (t, cells) in areas {
            let cells: Vec<Bound<'py, PyTuple>> = cells
                .into_iter()
                .map(|cell| cell.into_pyobject(py))
                .collect::<PyResult<_>>()?;
            out.append((t, PyList::new(py, cells)?))?;
        }
        Ok(out)
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
