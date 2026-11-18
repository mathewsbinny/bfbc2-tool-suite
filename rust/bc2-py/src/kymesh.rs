//! Round E3o: NavKit's runtime-mesh reader (`kymesh.parse_mesh`) and `navdata.load`'s heavy parts in the Rust core.
//!
//! `kymesh_parse` gives the record as nested tuples that `kymesh.py` turns into its dataclasses; `None` where the
//! reference reader raises (or meets a NaN), so the Python reads it and the error stays the reference's.
//! `navkit_mesh_cells` is `navdata._cells`' work for a whole file: every sector's mesh parsed, each area's boundary
//! segments (`navdiff.segments`) sampled by `parity::area_cells` in parallel, the cells returned as flat arrays.
//! `navkit_union_roots` is `navdata._load`'s union-find over the route graph, step for step.

use bc2_formats::kymesh::{self, Area, Mesh, Vec3};
use bc2_navbuild::trace::P3;
use pyo3::prelude::*;
use pyo3::pybacked::PyBackedBytes;
use pyo3::types::{PyBytes, PyList, PyTuple};
use rayon::prelude::*;

const NULL: u32 = u32::MAX;

fn v3(py: Python<'_>, v: Vec3) -> PyResult<Bound<'_, PyTuple>> {
    (f64::from(v[0]), f64::from(v[1]), f64::from(v[2])).into_pyobject(py)
}

fn points<'py>(py: Python<'py>, pts: &[Vec3]) -> PyResult<Bound<'py, PyList>> {
    let out = PyList::empty(py);
    for &p in pts {
        out.append(v3(py, p)?)?;
    }
    Ok(out)
}

fn lines<'py>(py: Python<'py>, ls: &[Vec<Vec3>]) -> PyResult<Bound<'py, PyList>> {
    let out = PyList::empty(py);
    for l in ls {
        out.append(points(py, l)?)?;
    }
    Ok(out)
}

fn area<'py>(py: Python<'py>, a: &Area) -> PyResult<Bound<'py, PyTuple>> {
    let portals = PyList::empty(py);
    for p in &a.portals {
        portals.append((v3(py, p.p0)?, v3(py, p.p1)?, p.target))?;
    }
    let borders = PyList::empty(py);
    for b in &a.border_portals {
        borders.append((v3(py, b.p0)?, v3(py, b.p1)?, v3(py, b.direction)?, b.runtime))?;
    }
    let cells = PyList::empty(py);
    for c in &a.cells {
        cells.append((v3(py, c.direction)?, points(py, &c.points)?))?;
    }
    let [s, tx, tz, layer] = a.ids;
    let [radius, pitch, max_y, min_y] = a.values.map(f64::from);
    let head = (s, tx, tz, layer, radius, pitch, max_y, min_y).into_pyobject(py)?;
    let body = (
        portals,
        borders,
        lines(py, &a.d_lines)?,
        lines(py, &a.outline)?,
        cells,
        v3(py, a.end_row)?,
        a.null_offsets.clone(),
    )
        .into_pyobject(py)?;
    PyTuple::new(py, [head.into_any(), body.into_any()])
}

fn mesh<'py>(py: Python<'py>, m: &Mesh) -> PyResult<Bound<'py, PyTuple>> {
    let tiles = PyList::empty(py);
    for t in &m.tiles {
        let items = PyList::empty(py);
        for i in t {
            items.append((i.area, f64::from(i.a), f64::from(i.b)))?;
        }
        tiles.append(items)?;
    }
    let areas = PyList::empty(py);
    for a in &m.areas {
        areas.append(area(py, a)?)?;
    }
    let links = PyList::empty(py);
    for l in &m.links {
        let objs = PyList::empty(py);
        for o in &l.objects {
            let [h0, h1, h2] = o.header;
            objs.append(((h0, h1, h2), o.refs.clone()))?;
        }
        links.append((l.sector_id, objs))?;
    }
    let [r0, r1, r2, r3] = m.rect.map(f64::from);
    let head = (
        m.version,
        m.flags,
        m.loaded,
        m.sector_id,
        f64::from(m.resolution),
        f64::from(m.pitch),
        (r0, r1, r2, r3),
        m.tiles_x,
        m.tiles_z,
        f64::from(m.radius),
    )
        .into_pyobject(py)?;
    let body = (
        tiles,
        areas,
        links,
        PyBytes::new(py, &m.end_slot),
        m.links_null_offset.unwrap_or(NULL),
    )
        .into_pyobject(py)?;
    PyTuple::new(py, [head.into_any(), body.into_any()])
}

/// `kymesh.parse_mesh(data)` as `((version, flags, loaded, sector_id, resolution, pitch, rect, tiles_x, tiles_z,
/// radius), (tiles, areas, links, end_slot, links_null_offset))`, every area
/// `((sector_id, tile_x, tile_z, layer, radius, pitch, max_y, min_y), (portals, border_portals, d_lines, outline,
/// cells, end_row, null_offsets))`; `None` where the reference raises or a value is a NaN.
#[pyfunction]
pub fn kymesh_parse<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let bytes = data.as_bytes();
    match py.detach(|| kymesh::parse(bytes)) {
        Ok(m) => Ok(Some(mesh(py, &m)?)),
        Err(_) => Ok(None),
    }
}

/// `navdiff.segments(area)` in float64: the outline's consecutive point pairs, then the portals, then the border
/// portals.
fn segments(a: &Area) -> Vec<(P3, P3)> {
    let p = |v: Vec3| (f64::from(v[0]), f64::from(v[1]), f64::from(v[2]));
    let mut out = Vec::new();
    for l in &a.outline {
        out.extend(l.iter().zip(l.iter().skip(1)).map(|(&a, &b)| (p(a), p(b))));
    }
    out.extend(a.portals.iter().map(|q| (p(q.p0), p(q.p1))));
    out.extend(a.border_portals.iter().map(|q| (p(q.p0), p(q.p1))));
    out
}

/// `(ij, ys, area)` of `navdata._cells`: every area of every mesh (in order, numbered across the file) sampled by
/// `navdiff.area_cells` - `ij` int64 pairs, `ys` the float64 heights, `area` the int32 area numbers, cell after cell.
/// `None` when a mesh is left to the reference reader or a boundary is not all finite numbers (the Python path runs).
#[pyfunction]
#[allow(clippy::needless_pass_by_value)] // pyo3 hands the list over
pub fn navkit_mesh_cells(
    py: Python<'_>,
    meshes: Vec<PyBackedBytes>,
    pitch: f64,
    y_offset: f64,
) -> PyResult<Option<Bound<'_, PyTuple>>> {
    let got = py.detach(|| -> Option<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        let parsed = meshes
            .par_iter()
            .map(|m| kymesh::parse(m).ok())
            .collect::<Option<Vec<Mesh>>>()?;
        let segs: Vec<Vec<(P3, P3)>> = parsed.iter().flat_map(|m| m.areas.iter().map(segments)).collect();
        let finite = |p: &P3| p.0.is_finite() && p.1.is_finite() && p.2.is_finite();
        if !segs.iter().flatten().all(|(a, b)| finite(a) && finite(b)) {
            return None;
        }
        let cells: Vec<Vec<((i64, i64), f64)>> = segs
            .par_iter()
            .map(|s| bc2_navbuild::parity::area_cells(s, pitch, y_offset))
            .collect();
        let n: usize = cells.iter().map(Vec::len).sum();
        let (mut ij, mut ys, mut area) = (
            Vec::with_capacity(16 * n),
            Vec::with_capacity(8 * n),
            Vec::with_capacity(4 * n),
        );
        for (k, c) in cells.iter().enumerate() {
            let k = i32::try_from(k).ok()?;
            for &((i, j), y) in c {
                ij.extend_from_slice(&i.to_le_bytes());
                ij.extend_from_slice(&j.to_le_bytes());
                ys.extend_from_slice(&y.to_le_bytes());
                area.extend_from_slice(&k.to_le_bytes());
            }
        }
        Some((ij, ys, area))
    });
    match got {
        None => Ok(None),
        Some((ij, ys, area)) => Ok(Some(PyTuple::new(
            py,
            [PyBytes::new(py, &ij), PyBytes::new(py, &ys), PyBytes::new(py, &area)],
        )?)),
    }
}

/// `navdata._load`'s union-find, step for step: `parent = arange(n)`; for each edge `(a, b)` (int64 pairs) not
/// marked in `skip` (one byte per edge, nonzero: skipped), `ra, rb = find(a), find(b)` and `parent[ra] = rb` when
/// they differ; then `find(k)` for every k in order (path halving: `parent[a] = parent[parent[a]]`). The roots as
/// int64 bytes; a `ValueError` for a node out of range.
#[pyfunction]
#[pyo3(signature = (n, edges, skip=None))]
// every index is a node below n, checked before use; skip's length is checked
#[allow(
    clippy::many_single_char_names,
    clippy::needless_pass_by_value,
    clippy::indexing_slicing
)]
pub fn navkit_union_roots<'py>(
    py: Python<'py>,
    n: usize,
    edges: &Bound<'py, PyBytes>,
    skip: Option<&Bound<'py, PyBytes>>,
) -> PyResult<Bound<'py, PyBytes>> {
    let e = edges.as_bytes();
    let s = skip.map(PyBytesMethods::as_bytes);
    if e.len() % 16 != 0 || s.is_some_and(|s| s.len() != e.len() / 16) {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "edges: int64 pairs; skip: one byte per edge",
        ));
    }
    let roots = py.detach(|| -> Option<Vec<u8>> {
        let mut parent: Vec<usize> = (0..n).collect();
        let find = |parent: &mut Vec<usize>, mut a: usize| {
            while parent[a] != a {
                parent[a] = parent[parent[a]];
                a = parent[a];
            }
            a
        };
        for (k, pair) in e.as_chunks::<16>().0.iter().enumerate() {
            if s.is_some_and(|s| s[k] != 0) {
                continue;
            }
            let a = usize::try_from(i64::from_le_bytes(pair[..8].try_into().ok()?)).ok()?;
            let b = usize::try_from(i64::from_le_bytes(pair[8..].try_into().ok()?)).ok()?;
            if a >= n || b >= n {
                return None;
            }
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            if ra != rb {
                parent[ra] = rb;
            }
        }
        let mut out = Vec::with_capacity(8 * n);
        for k in 0..n {
            let r = find(&mut parent, k) as i64;
            out.extend_from_slice(&r.to_le_bytes());
        }
        Some(out)
    });
    match roots {
        Some(r) => Ok(PyBytes::new(py, &r)),
        None => Err(pyo3::exceptions::PyValueError::new_err(
            "an edge's node is out of range",
        )),
    }
}

/// Little-endian bytes of `values`.
fn le<T: Copy, const N: usize>(values: &[T], f: fn(T) -> [u8; N]) -> Vec<u8> {
    values.iter().flat_map(|&v| f(v)).collect()
}

/// The flat arrays of `navkit_navdata_areas`.
#[derive(Default)]
struct Flat {
    ints: Vec<i64>,
    floats: Vec<f64>,
    points: Vec<f32>,
    line_start: Vec<i64>,
    area_lines: Vec<i64>,
    portals: Vec<f64>,
    targets: Vec<i64>,
    area_portals: Vec<i64>,
    borders: Vec<f64>,
    area_borders: Vec<i64>,
}

fn flatten(meshes: &[Mesh]) -> Flat {
    let mut f = Flat::default();
    f.line_start.push(0);
    f.area_lines.push(0);
    f.area_portals.push(0);
    f.area_borders.push(0);
    let p64 = |v: Vec3| v.map(f64::from);
    for (s, m) in meshes.iter().enumerate() {
        for (n, a) in m.areas.iter().enumerate() {
            let [_, tx, tz, layer] = a.ids;
            f.ints
                .extend([s as i64, n as i64, i64::from(tx), i64::from(tz), i64::from(layer)]);
            let [_, pitch, max_y, min_y] = a.values;
            f.floats.extend([f64::from(pitch), f64::from(min_y), f64::from(max_y)]);
            for l in a.outline.iter().filter(|l| l.len() >= 2) {
                f.points.extend(l.iter().flatten());
                f.line_start.push((f.points.len() / 3) as i64);
            }
            f.area_lines.push(f.line_start.len() as i64 - 1);
            for p in &a.portals {
                f.portals.extend(p64(p.p0).into_iter().chain(p64(p.p1)));
                f.targets.push(p.target as i64);
            }
            f.area_portals.push(f.targets.len() as i64);
            for b in &a.border_portals {
                f.borders.extend(p64(b.p0).into_iter().chain(p64(b.p1)));
            }
            f.area_borders.push((f.borders.len() / 6) as i64);
        }
    }
    f
}

/// `navdata._load`'s areas of a whole file, as flat arrays (round E3o): every sector's mesh parsed, its areas in
/// order - `ints` int64 (sector index, area index, tile x, tile z, layer) per area, `floats` float64 (pitch, min y,
/// max y), the outline lines of two points or more as `points` (float32 x 3) cut by `line_start` (int64, lines + 1)
/// and given to the areas by `area_lines` (int64, areas + 1), the portals as float64 (p0, p1) with their `targets`
/// cut by `area_portals`, the border portals' (p0, p1) cut by `area_borders`. `None` where a mesh is left to the
/// reference reader.
#[pyfunction]
#[allow(clippy::needless_pass_by_value)] // pyo3 hands the list over
pub fn navkit_navdata_areas(py: Python<'_>, meshes: Vec<PyBackedBytes>) -> PyResult<Option<Bound<'_, PyTuple>>> {
    let got = py.detach(|| {
        let parsed = meshes
            .par_iter()
            .map(|m| kymesh::parse(m).ok())
            .collect::<Option<Vec<Mesh>>>()?;
        Some(flatten(&parsed))
    });
    let Some(f) = got else { return Ok(None) };
    let i64s = |v: &[i64]| PyBytes::new(py, &le(v, i64::to_le_bytes));
    let f64s = |v: &[f64]| PyBytes::new(py, &le(v, f64::to_le_bytes));
    Ok(Some(PyTuple::new(
        py,
        [
            i64s(&f.ints),
            f64s(&f.floats),
            PyBytes::new(py, &le(&f.points, f32::to_le_bytes)),
            i64s(&f.line_start),
            i64s(&f.area_lines),
            f64s(&f.portals),
            i64s(&f.targets),
            i64s(&f.area_portals),
            f64s(&f.borders),
            i64s(&f.area_borders),
        ],
    )?))
}
