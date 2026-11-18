//! Bindings for round E4e's pieces of `bc2-navbuild`: NavKit's tools around the build (`navvehicle`'s obstacle
//! distance, ...). Each returns `None` where the arrays do not fit together, and the Python reference then runs and
//! raises what it raises.

use bc2_navbuild::vehicle::{self, Obstacles, Surfaces};
use bc2_navbuild::{domains, levelscan, measures, offline};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::pybacked::PyBackedBytes;
use pyo3::types::{PyBytes, PyBytesMethods};
use rayon::prelude::*;

fn value_error(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(e.to_string())
}

fn chunks<const N: usize>(b: &[u8], what: &str) -> PyResult<Vec<[u8; N]>> {
    let (whole, rest) = b.as_chunks::<N>();
    if !rest.is_empty() {
        return Err(value_error(format!("{what}: {} bytes, not a multiple of {N}", b.len())));
    }
    Ok(whole.to_vec())
}

/// float32 values from little-endian bytes.
pub fn f32s(b: &[u8], what: &str) -> PyResult<Vec<f32>> {
    Ok(chunks::<4>(b, what)?.into_iter().map(f32::from_le_bytes).collect())
}

/// Column starts (int64, none negative) from little-endian bytes.
pub fn starts(b: &[u8]) -> PyResult<Vec<usize>> {
    chunks::<8>(b, "start")?
        .into_iter()
        .map(|b| usize::try_from(i64::from_le_bytes(b)).map_err(|_| value_error("a negative column start")))
        .collect()
}

fn bools(b: &[u8]) -> Vec<bool> {
    b.iter().map(|&v| v != 0).collect()
}

/// `navvehicle.obstacle_distance` from the arrays' bytes (column starts int64; heights, gradients and the extra
/// heights float32; candidates and solid one byte per surface), the offsets `[(d, a, b)]` in the reference's order,
/// the pitch and the band `lo`, `hi`: the distances as float32 bytes, or `None` (arrays that do not fit).
#[pyfunction]
#[pyo3(signature = (nx, nz, start, y, gx, gz, cand, solid, extra, offsets, pitch, lo, hi))]
#[expect(clippy::too_many_arguments, reason = "the reference's arguments, each an array")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn navkit_obstacle_distance<'py>(
    py: Python<'py>,
    nx: usize,
    nz: usize,
    start: &Bound<'py, PyBytes>,
    y: &Bound<'py, PyBytes>,
    gx: &Bound<'py, PyBytes>,
    gz: &Bound<'py, PyBytes>,
    cand: &Bound<'py, PyBytes>,
    solid: &Bound<'py, PyBytes>,
    extra: Option<&Bound<'py, PyBytes>>,
    offsets: Vec<(f64, i64, i64)>,
    pitch: f64,
    lo: f64,
    hi: f64,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    let start = starts(start.as_bytes())?;
    let (y, gx, gz) = (
        f32s(y.as_bytes(), "y")?,
        f32s(gx.as_bytes(), "gx")?,
        f32s(gz.as_bytes(), "gz")?,
    );
    let (cand, solid) = (bools(cand.as_bytes()), bools(solid.as_bytes()));
    let extra = extra.map(|e| f32s(e.as_bytes(), "extra")).transpose()?;
    let got = py.detach(|| {
        vehicle::obstacle_distance(
            &Surfaces {
                nx,
                nz,
                start: &start,
                y: &y,
                gx: &gx,
                gz: &gz,
            },
            &cand,
            &Obstacles {
                solid: &solid,
                extra: extra.as_deref(),
            },
            &offsets,
            pitch,
            lo as f32,
            hi as f32,
        )
    });
    Ok(got.map(|v| {
        let bytes: Vec<u8> = v.iter().flat_map(|f| f.to_le_bytes()).collect();
        PyBytes::new(py, &bytes)
    }))
}

fn usizes(b: &[u8], what: &str) -> PyResult<Vec<usize>> {
    chunks::<8>(b, what)?
        .into_iter()
        .map(|b| usize::try_from(i64::from_le_bytes(b)).map_err(|_| value_error(format!("{what}: a negative index"))))
        .collect()
}

fn f64_bytes<'py>(py: Python<'py>, v: &[f64]) -> Bound<'py, PyBytes> {
    let bytes: Vec<u8> = v.iter().flat_map(|f| f.to_le_bytes()).collect();
    PyBytes::new(py, &bytes)
}

/// A `navdomains.Domain`'s graph kept in the core (`navdomains.dijkstra` and `World.tables`' searches).
#[pyclass(frozen)]
pub struct DomainGraph {
    g: domains::Graph,
}

#[pymethods]
impl DomainGraph {
    /// The distances from `src` as float64 bytes (`navdomains.dijkstra`; targets: stop once each is reached), or
    /// `None` (src out of range).
    #[pyo3(signature = (src, targets=None))]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "PyO3 extracts a list argument as an owned Vec"
    )]
    fn dijkstra<'py>(&self, py: Python<'py>, src: usize, targets: Option<Vec<usize>>) -> Option<Bound<'py, PyBytes>> {
        let d = py.detach(|| self.g.dijkstra(src, targets.as_deref()))?;
        Some(f64_bytes(py, &d))
    }

    /// For each node of `nodes`, its distances to every node of `nodes` (float64 bytes per row), or `None`.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "PyO3 extracts a list argument as an owned Vec"
    )]
    fn tables<'py>(&self, py: Python<'py>, nodes: Vec<usize>) -> Option<Vec<Bound<'py, PyBytes>>> {
        let rows = py.detach(|| self.g.tables(&nodes))?;
        Some(rows.iter().map(|r| f64_bytes(py, r)).collect())
    }

    /// Nodes.
    fn __len__(&self) -> usize {
        self.g.len()
    }
}

/// A domain's graph from its adjacency as arrays (`start` int64 `n + 1` offsets; per entry the neighbour int64 and
/// the cost float64), or `None` where the reference must run (a cost negative or NaN, offsets that do not fit).
#[pyfunction]
pub fn navkit_domain_graph(
    start: &Bound<'_, PyBytes>,
    nbr: &Bound<'_, PyBytes>,
    cost: &Bound<'_, PyBytes>,
) -> PyResult<Option<DomainGraph>> {
    let start = usizes(start.as_bytes(), "start")?;
    let nbr = usizes(nbr.as_bytes(), "nbr")?;
    let cost: Vec<f64> = chunks::<8>(cost.as_bytes(), "cost")?
        .into_iter()
        .map(f64::from_le_bytes)
        .collect();
    if nbr.len() != cost.len() {
        return Ok(None);
    }
    Ok(domains::Graph::new(start, nbr.into_iter().zip(cost).collect()).map(|g| DomainGraph { g }))
}

/// One `.xml` file's record: (path relative to its base, `blueprint_flags`' tuple or `None`, the `Mesh` ref or `None`).
type XmlRow = (String, Option<(bool, bool, bool, bool, String)>, Option<String>);

/// `levelobjects`' scan of the bases (round E4e): per base, the `.xml` files with flags or a `Mesh` reference and
/// the `*_havok.res` files, relative to the base, in `glob`'s order; `None` when a file cannot be read.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn navkit_level_scan(py: Python<'_>, bases: Vec<String>) -> Option<Vec<(Vec<XmlRow>, Vec<String>)>> {
    let paths: Vec<std::path::PathBuf> = bases.iter().map(std::path::PathBuf::from).collect();
    let got = py.detach(|| levelscan::scan(&paths))?;
    Some(
        got.into_iter()
            .map(|(recs, havok)| (recs.into_iter().map(|r| (r.rel, r.flags, r.mesh)).collect(), havok))
            .collect(),
    )
}

/// A sector for `navkit_diff`: its mesh record, node positions (float64 x, y, z) and edges (int64 src, dst, tag).
type SectorIn = (PyBackedBytes, PyBackedBytes, PyBackedBytes);

fn words(b: &[u8]) -> Option<Vec<[u8; 8]>> {
    let (whole, rest) = b.as_chunks::<8>();
    rest.is_empty().then(|| whole.to_vec())
}

fn sector_of(sec: &SectorIn, cfg: &measures::DiffCfg, clip: Option<[f64; 4]>) -> Option<measures::Sector> {
    let pos: Vec<f64> = words(&sec.1)?.into_iter().map(f64::from_le_bytes).collect();
    let ed: Vec<i64> = words(&sec.2)?.into_iter().map(i64::from_le_bytes).collect();
    let (pos3, rest) = pos.as_chunks::<3>();
    let (ed3, rest2) = ed.as_chunks::<3>();
    if !rest.is_empty() || !rest2.is_empty() {
        return None;
    }
    let nodes = pos3.iter().map(|&[x, y, z]| (x, y, z)).collect();
    let edges = ed3
        .iter()
        .map(|&[src, dst, tag]| {
            Some((
                usize::try_from(src).ok()?,
                usize::try_from(dst).ok()?,
                u32::try_from(tag).ok()?,
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    measures::Sector::new(&sec.0, measures::Graph { nodes, edges }, cfg, clip)
}

/// One sector's `compare_sector` counts (name, value) and its unmatched cells (int64 i, j pairs) on each side.
type DiffRow<'py> = (Vec<(&'static str, i64)>, Bound<'py, PyBytes>, Bound<'py, PyBytes>);

fn pairs_bytes<'py>(py: Python<'py>, v: &[(i64, i64)]) -> Bound<'py, PyBytes> {
    let b: Vec<u8> = v
        .iter()
        .flat_map(|&(i, j)| i.to_le_bytes().into_iter().chain(j.to_le_bytes()))
        .collect();
    PyBytes::new(py, &b)
}

/// `navdiff.diff`'s per-sector work (round E4e): `compare_sector` of each reference / candidate pair (the reference's
/// cells clipped to `clip`), and the cell counts of the sectors only one file has (the reference's clipped) - every
/// sector parsed, sampled and compared on all cores. `None`: the Python runs (it raises where the core refuses).
#[pyfunction]
#[pyo3(signature = (ref_common, cand_common, ref_only, cand_only, clip, grid, y_offset, stack_tol, node_xz, node_y))]
#[expect(clippy::too_many_arguments, reason = "navdiff's module settings, each passed")]
#[expect(clippy::type_complexity, reason = "the rows as Python takes them")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn navkit_diff(
    py: Python<'_>,
    ref_common: Vec<SectorIn>,
    cand_common: Vec<SectorIn>,
    ref_only: Vec<SectorIn>,
    cand_only: Vec<SectorIn>,
    clip: Option<(f64, f64, f64, f64)>,
    grid: f64,
    y_offset: f64,
    stack_tol: f64,
    node_xz: f64,
    node_y: f64,
) -> Option<(Vec<DiffRow<'_>>, Vec<(usize, usize)>, Vec<usize>)> {
    if ref_common.len() != cand_common.len() {
        return None;
    }
    let cfg = measures::DiffCfg {
        grid,
        y_offset,
        stack_tol,
        node_xz,
        node_y,
    };
    let clip = clip.map(|c| [c.0, c.1, c.2, c.3]);
    let got = py.detach(|| {
        let load = |v: &Vec<SectorIn>, clip: Option<[f64; 4]>| -> Option<Vec<measures::Sector>> {
            v.par_iter().map(|s| sector_of(s, &cfg, clip)).collect()
        };
        let (rc, cc) = (load(&ref_common, clip)?, load(&cand_common, None)?);
        let (ro, co) = (load(&ref_only, clip)?, load(&cand_only, None)?);
        let pairs: Vec<(&measures::Sector, &measures::Sector)> = rc.iter().zip(&cc).collect();
        let rows = measures::compare_many(&pairs, &cfg)?;
        Some((
            rows,
            ro.iter().map(|s| (s.cell_count(), s.area_count())).collect::<Vec<_>>(),
            co.iter().map(measures::Sector::cell_count).collect::<Vec<_>>(),
        ))
    });
    got.map(|(rows, ro, co)| {
        let rows = rows
            .into_iter()
            .map(|r| (r.counts, pairs_bytes(py, &r.only_ref), pairs_bytes(py, &r.only_cand)))
            .collect();
        (rows, ro, co)
    })
}

/// `ordercheck.PathData`'s areas kept in the core for `on_area` (round E4e).
#[pyclass(frozen)]
pub struct OrderAreas {
    areas: Vec<measures::OrderArea>,
}

#[pymethods]
impl OrderAreas {
    /// `PathData.on_area((x, y, z), ytol)`.
    fn on_area(&self, x: f64, y: f64, z: f64, ytol: f64) -> bool {
        measures::on_area(&self.areas, (x, y, z), ytol)
    }

    /// `on_area` of many points (float64 x, y, z each): one byte per point.
    fn on_area_many<'py>(&self, py: Python<'py>, points: &Bound<'py, PyBytes>, ytol: f64) -> Bound<'py, PyBytes> {
        let pts: Vec<(f64, f64, f64)> = points
            .as_bytes()
            .as_chunks::<24>()
            .0
            .iter()
            .map(|c| {
                let f = |k: usize| {
                    c.get(k..k + 8)
                        .and_then(|x| x.try_into().ok())
                        .map_or(f64::NAN, f64::from_le_bytes)
                };
                (f(0), f(8), f(16))
            })
            .collect();
        let out: Vec<u8> = py.detach(|| {
            pts.par_iter()
                .map(|&p| u8::from(measures::on_area(&self.areas, p, ytol)))
                .collect()
        });
        PyBytes::new(py, &out)
    }

    /// Areas with a boundary.
    fn __len__(&self) -> usize {
        self.areas.len()
    }
}

/// `ordercheck.PathData.areas` of a file's mesh records (in sector order), or `None` (the Python makes them).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn navkit_order_areas(py: Python<'_>, meshes: Vec<PyBackedBytes>, y_offset: f64) -> Option<OrderAreas> {
    let refs: Vec<&[u8]> = meshes.iter().map(|m| &m[..]).collect();
    py.detach(|| measures::order_areas(&refs, y_offset))
        .map(|areas| OrderAreas { areas })
}

/// One class's figures: (D sides reference, candidate, matched, near), (F cells reference, candidate, matched).
type DfClassRow = ([i64; 4], [i64; 3]);

/// `navdf.compare`'s figures (round E4e), from the mesh records of both files (the candidate's ground made from its
/// own when `with_ground`): per class (all, normal, detailed), the median height difference, the shared-ground
/// (matched, total), both files' counts `[d_n, d_d, f_n, f_d]`. `None`: the Python runs.
#[pyfunction]
#[pyo3(signature = (ref_meshes, cand_meshes, clip, with_ground, cell_grid, y_offset, grid, dy))]
#[expect(
    clippy::too_many_arguments,
    reason = "navdf's and navdiff's module settings, each passed"
)]
#[expect(clippy::type_complexity, reason = "the figures as Python takes them")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn navkit_df(
    py: Python<'_>,
    ref_meshes: Vec<PyBackedBytes>,
    cand_meshes: Vec<PyBackedBytes>,
    clip: Option<(f64, f64, f64, f64)>,
    with_ground: bool,
    cell_grid: f64,
    y_offset: f64,
    grid: f64,
    dy: f64,
) -> Option<(Vec<DfClassRow>, Option<f64>, Option<(i64, i64)>, [[i64; 4]; 2])> {
    let r: Vec<&[u8]> = ref_meshes.iter().map(|m| &m[..]).collect();
    let c: Vec<&[u8]> = cand_meshes.iter().map(|m| &m[..]).collect();
    let clip = clip.map(|v| [v.0, v.1, v.2, v.3]);
    py.detach(|| {
        let ((fr, fc), ground) = rayon::join(
            || rayon::join(|| measures::df_features(&r, clip), || measures::df_features(&c, None)),
            || with_ground.then(|| measures::ground_of(&c, cell_grid, y_offset)),
        );
        let ground = match ground {
            Some(g) => Some(g?),
            None => None,
        };
        let fig = measures::df_compare(&fr?, &fc?, ground.as_ref(), grid, dy)?;
        Some((
            fig.classes.iter().map(|k| (k.d, k.f)).collect(),
            fig.median_dy,
            fig.shared,
            fig.counts,
        ))
    })
}

/// `navsample_offline.terrain_blocks`' test (round E4e): the height field (`size` x `size` float32 heights, its `min`
/// x / z and `cell`), the surfaces' x, z, y (float64 each) and the ring points' x, z offsets and lifts (float64): one
/// byte per surface (1: the terrain rises into the capsule), or `None` when the arrays do not fit.
#[pyfunction]
#[pyo3(signature = (heights, size, min_x, min_z, cell_x, cell_z, xs, zs, ys, ring_x, ring_z, lift))]
#[expect(
    clippy::too_many_arguments,
    reason = "the height field's values and the reference's arrays"
)]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn navkit_terrain_blocks<'py>(
    py: Python<'py>,
    heights: &Bound<'py, PyBytes>,
    size: usize,
    min_x: f64,
    min_z: f64,
    cell_x: f64,
    cell_z: f64,
    xs: &Bound<'py, PyBytes>,
    zs: &Bound<'py, PyBytes>,
    ys: &Bound<'py, PyBytes>,
    ring_x: Vec<f64>,
    ring_z: Vec<f64>,
    lift: Vec<f64>,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    let h = f32s(heights.as_bytes(), "heights")?;
    let f64s = |b: &Bound<'py, PyBytes>, what: &str| -> PyResult<Vec<f64>> {
        Ok(chunks::<8>(b.as_bytes(), what)?
            .into_iter()
            .map(f64::from_le_bytes)
            .collect())
    };
    let (x, z, y) = (f64s(xs, "xs")?, f64s(zs, "zs")?, f64s(ys, "ys")?);
    if x.len() != z.len() || x.len() != y.len() {
        return Ok(None);
    }
    let pts: Vec<(f64, f64, f64)> = x.into_iter().zip(z).zip(y).map(|((a, b), c)| (a, b, c)).collect();
    let field = offline::Field {
        heights: &h,
        size,
        min: (min_x, min_z),
        cell: (cell_x, cell_z),
    };
    let got = py.detach(|| offline::terrain_blocks(&field, &pts, &ring_x, &ring_z, &lift));
    Ok(got.map(|v| PyBytes::new(py, &v.iter().map(|&b| u8::from(b)).collect::<Vec<u8>>())))
}
