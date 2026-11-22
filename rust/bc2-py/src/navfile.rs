//! Bindings for round NK2's `bc2_navbuild::graphfile`: a NavKit build's graphs to its path-data file (the shadow
//! islands, the order nodes' pruning, each sector's nodes and edges, the file) and a sector's neighbour links. The
//! Python (`navislands.prune` / `components` / `shadowing`, `navbuild.prune_order_nodes`, `finalize_graph`,
//! `write_file`'s assembly, `sector_links`) was retired.
//!
//! The graphs come as the build holds them: `[(sid, bounds, mesh, pts, pairs, origin)]`, `pts` `[(id, pos, ...)]`,
//! `pairs` `[(a, b, d)]`, `mesh` a `kymesh.Mesh` or its record's bytes. A pair past its sector's nodes raises the
//! reference's `IndexError`; a value the file cannot hold, a `ValueError`.

use std::collections::HashSet;

use bc2_formats::hpd;
use bc2_formats::kymesh;
use bc2_navbuild::graphfile::{self, FileSettings, GraphError, Layout, RouteNode, SectorGraph, Shadow, ShadowStats};
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList, PyTuple};
use rayon::prelude::*;

use crate::pathdata::{items, mesh_of};

fn value_error(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(e.to_string())
}

fn graph_error(e: &GraphError) -> PyErr {
    match e {
        GraphError::Index => PyIndexError::new_err(e.to_string()),
        GraphError::Refused(_) => value_error(e),
    }
}

/// Exactly `N` floats (at least `N` items: the first `N`) of a list or tuple.
fn f64s<const N: usize>(o: &Bound<'_, PyAny>, what: &str) -> PyResult<[f64; N]> {
    let v = items(o).ok_or_else(|| value_error(format!("{what}: not a list or tuple")))?;
    let mut out = [0f64; N];
    if v.len() < N {
        return Err(value_error(format!("{what}: {N} numbers needed")));
    }
    for (slot, x) in out.iter_mut().zip(&v) {
        *slot = x.extract::<f64>()?;
    }
    Ok(out)
}

/// A sector graph and its mesh (its record's bytes, or the `kymesh.Mesh` to write).
enum MeshIn {
    Bytes(Vec<u8>),
    Mesh(kymesh::Mesh),
}

/// One `(sid, bounds, mesh, pts, pairs, origin)`; `with_mesh`: the mesh read too (`None` where the port does not
/// write it).
fn graph_in(py: Python<'_>, graph: &Bound<'_, PyAny>, with_mesh: bool) -> PyResult<(SectorGraph, Option<MeshIn>)> {
    let fields = items(graph).ok_or_else(|| value_error("a sector graph is a tuple"))?;
    let [sid, bounds, mesh, pts, pair_list, origin] = fields.as_slice() else {
        return Err(value_error("a sector graph is (sid, bounds, mesh, pts, pairs, origin)"));
    };
    let nodes = items(pts)
        .ok_or_else(|| value_error("pts: not a list"))?
        .iter()
        .map(|node| {
            let node = node
                .cast::<PyTuple>()
                .map_err(|_| value_error("a route node is a tuple"))?;
            Ok(RouteNode {
                id: node.get_item(0)?.extract::<u32>()?,
                pos: f64s::<3>(&node.get_item(1)?, "a route node's position")?,
            })
        })
        .collect::<PyResult<Vec<_>>>()?;
    let pairs = items(pair_list)
        .ok_or_else(|| value_error("pairs: not a list"))?
        .iter()
        .map(|pair| {
            let pair = items(pair).ok_or_else(|| value_error("a pair is a tuple"))?;
            let (Some(from), Some(to)) = (pair.first(), pair.get(1)) else {
                return Err(value_error("a pair is (a, b, d)"));
            };
            Ok((from.extract::<usize>()?, to.extract::<usize>()?))
        })
        .collect::<PyResult<Vec<_>>>()?;
    let origin = if origin.is_none() {
        None
    } else {
        let [ox, oz] = f64s::<2>(origin, "a sector origin")?;
        Some((ox, oz))
    };
    let record = if !with_mesh {
        None
    } else if let Ok(bytes) = mesh.cast::<PyBytes>() {
        Some(MeshIn::Bytes(bytes.as_bytes().to_vec()))
    } else {
        mesh_of(py, mesh).map(MeshIn::Mesh)
    };
    Ok((
        SectorGraph {
            sid: sid.extract::<u32>()?,
            bounds: f64s::<6>(bounds, "a sector's bounds")?,
            nodes,
            pairs,
            origin,
        },
        record,
    ))
}

/// The shadow-island rule's settings `(min_nodes, xz, dy, anchor_xz, anchor_dy, y_offset, objects_only)`, its
/// anchors `[(x, y, z)]` and the ids of the nodes on terrain.
type ShadowIn<'py> = ((usize, f64, f64, f64, f64, f64, bool), Bound<'py, PyAny>, HashSet<u32>);

fn shadow_in(s: &ShadowIn<'_>) -> PyResult<(Shadow, Vec<[f64; 3]>, HashSet<u32>)> {
    let ((min_nodes, xz, dy, anchor_xz, anchor_dy, y_offset, objects_only), anchors, terrain) = s;
    let anchors = items(anchors)
        .ok_or_else(|| value_error("anchors: not a list"))?
        .iter()
        .map(|a| f64s::<3>(a, "an anchor"))
        .collect::<PyResult<Vec<_>>>()?;
    Ok((
        Shadow {
            min_nodes: *min_nodes,
            xz: *xz,
            dy: *dy,
            anchor_xz: *anchor_xz,
            anchor_dy: *anchor_dy,
            y_offset: *y_offset,
            objects_only: *objects_only,
        },
        anchors,
        terrain.clone(),
    ))
}

/// `(ids of the dropped islands' nodes, sorted; islands dropped; route islands before)`.
type ShadowOut = (Vec<u32>, usize, usize);

fn shadow_out(st: &ShadowStats) -> ShadowOut {
    let mut ids: Vec<u32> = st.ids.iter().copied().collect();
    ids.sort_unstable();
    (ids, st.islands, st.before)
}

/// `navislands.shadowing` as `prune` uses it: the dropped islands' node ids (sorted), the islands dropped and the
/// route islands before.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a tuple argument as an owned value"
)]
pub fn navkit_shadow_islands(py: Python<'_>, graphs: &Bound<'_, PyList>, shadow: ShadowIn<'_>) -> PyResult<ShadowOut> {
    let gs = graphs
        .iter()
        .map(|g| graph_in(py, &g, false).map(|x| x.0))
        .collect::<PyResult<Vec<_>>>()?;
    for g in &gs {
        if g.pairs.iter().any(|&(a, b)| a >= g.nodes.len() || b >= g.nodes.len()) {
            return Err(graph_error(&GraphError::Index));
        }
    }
    let (s, anchors, terrain) = shadow_in(&shadow)?;
    Ok(shadow_out(
        &py.detach(|| graphfile::shadow_islands(&gs, &anchors, &terrain, &s)),
    ))
}

/// The file's bytes, the shadow islands' answer (or `None`), the order nodes dropped `[(id, (x, y, z), size)]`, the
/// nodes and edges written.
type Written<'py> = (
    Bound<'py, PyBytes>,
    Option<ShadowOut>,
    Vec<(u32, (f64, f64, f64), usize)>,
    usize,
    usize,
);

/// `navislands.prune` (when `shadow` is given) and `navbuild.write_file`'s pruning, tables and file: `orders`
/// `(order ids, ORDER_MIN_ISLAND)` or `None`, `layout` `(SORT_NODES_BY_ID, BORDER_OWNER, dist_edge_max)`, the 36
/// header words, each sector's date and time. `None` when a mesh is one the port does not write (the caller then
/// passes every mesh's bytes, written by `kymesh.write_mesh`).
#[pyfunction]
#[pyo3(signature = (graphs, shadow, orders, layout, header, date, clock))]
#[expect(clippy::too_many_arguments, reason = "write_file's inputs")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn navkit_write_graphs<'py>(
    py: Python<'py>,
    graphs: &Bound<'py, PyList>,
    shadow: Option<ShadowIn<'py>>,
    orders: Option<(HashSet<u32>, usize)>,
    layout: (bool, bool, f64),
    header: Vec<u32>,
    date: u32,
    clock: u32,
) -> PyResult<Option<Written<'py>>> {
    let mut gs = Vec::with_capacity(graphs.len());
    let mut meshes_in = Vec::with_capacity(graphs.len());
    for g in graphs.iter() {
        let (sg, m) = graph_in(py, &g, true)?;
        let Some(m) = m else { return Ok(None) };
        gs.push(sg);
        meshes_in.push(m);
    }
    let header: [u32; hpd::HEADER_WORDS] = header
        .try_into()
        .map_err(|_| value_error(format!("{} header words needed", hpd::HEADER_WORDS)))?;
    let settings = FileSettings {
        shadow: shadow.as_ref().map(shadow_in).transpose()?,
        orders,
        layout: Layout {
            sort_by_id: layout.0,
            border_owner: layout.1,
            dist_edge_max: layout.2,
        },
        header,
        date,
        clock,
    };
    let got = py.detach(|| {
        let meshes = meshes_in
            .into_par_iter()
            .map(|m| match m {
                MeshIn::Bytes(b) => Some(b),
                MeshIn::Mesh(m) => kymesh::write(&m).ok(),
            })
            .collect::<Option<Vec<_>>>();
        meshes.map(|m| graphfile::write_file(&gs, &m, &settings))
    });
    let Some(got) = got else { return Ok(None) };
    let w = got.map_err(|e| graph_error(&e))?;
    let removed = w
        .removed
        .iter()
        .map(|r| (r.id, (r.pos[0], r.pos[1], r.pos[2]), r.size))
        .collect();
    Ok(Some((
        PyBytes::new(py, &w.data),
        w.shadow.as_ref().map(shadow_out),
        removed,
        w.nodes,
        w.edges,
    )))
}
