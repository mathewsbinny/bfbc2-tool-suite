//! Round F0: path data (`pathdata.hpd`, `bc2_formats::hpd`) and the runtime-mesh writer (`bc2_formats::kymesh::write`)
//! for `bfbc2-auto-navkit/tools/hpdtool.py` and `kymesh.py`.
//!
//! `hpd_parse` builds `hpdtool`'s own dataclasses (the classes are passed in): each object made with `__new__` and
//! its `__dict__` filled in field order, so it equals, prints and pickles as the reference's; since round RT3 it
//! reads every file (NaNs as CPython's `struct` reads them) and raises the reference's `ValueError` / `struct.error`
//! where it raised (`parse_reference` retired). `hpd_write` and `kymesh_write` read the reference's objects attribute
//! by attribute and answer `None` where the reference would raise, a value is a NaN, or a value is not one the port
//! reads exactly as `struct` does (a float that overflows a 32-bit float, a sequence of another length, a
//! `bytes`-like that is not `bytes` / `bytearray`): the Python writers (kept) then run, so a result here is always
//! the reference's and the errors stay its own.

use bc2_formats::hpd::{self, Edge, HpdFile, Node, Sector};
use bc2_formats::hpdlayout::c_double;
use bc2_formats::kymesh::{self, Area, BorderPortal, Cell, LinkObject, Mesh, NeighbourLink, Portal, TileItem, Vec3};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyByteArray, PyBytes, PyDict, PyList, PyString, PyTuple};
use pyo3::{IntoPyObjectExt, intern};

/// `struct.pack('<f', x)`'s value as a 32-bit float, or `None` where it raises (a finite value too large) or where
/// the port leaves the value to the reference (a NaN: its bits are the interpreter's).
pub(crate) fn pack_f32(x: f64) -> Option<f32> {
    let y = x as f32;
    if x.is_nan() || (y.is_infinite() && x.is_finite()) {
        None
    } else {
        Some(y)
    }
}

/// A float field as `struct`'s `f` reads it (`PyFloat_AsDouble`, as PyO3's `f64`).
pub(crate) fn f32_of(o: &Bound<'_, PyAny>) -> Option<f32> {
    pack_f32(o.extract::<f64>().ok()?)
}

/// An `I` field.
pub(crate) fn u32_of(o: &Bound<'_, PyAny>) -> Option<u32> {
    o.extract::<u32>().ok()
}

/// A list or tuple's items (the only sequences the port reads).
pub(crate) fn items<'py>(o: &Bound<'py, PyAny>) -> Option<Vec<Bound<'py, PyAny>>> {
    if let Ok(l) = o.cast::<PyList>() {
        Some(l.iter().collect())
    } else if let Ok(t) = o.cast::<PyTuple>() {
        Some(t.iter().collect())
    } else {
        None
    }
}

/// Exactly `N` floats from a list or tuple.
fn floats<const N: usize>(o: &Bound<'_, PyAny>) -> Option<[f32; N]> {
    let v = items(o)?;
    if v.len() != N {
        return None;
    }
    let mut out = [0f32; N];
    for (slot, x) in out.iter_mut().zip(&v) {
        *slot = f32_of(x)?;
    }
    Some(out)
}

fn attr<'py>(o: &Bound<'py, PyAny>, name: &Bound<'py, PyString>) -> Option<Bound<'py, PyAny>> {
    o.getattr(name).ok()
}

/// `bytes` or `bytearray` contents.
fn byte_vec(o: &Bound<'_, PyAny>) -> Option<Vec<u8>> {
    if let Ok(b) = o.cast::<PyBytes>() {
        Some(b.as_bytes().to_vec())
    } else {
        o.cast::<PyByteArray>().ok().map(PyByteArrayMethods::to_vec)
    }
}

/// Makes instances of a class without running its `__init__`: `cls.__new__(cls)` with the fields put in its
/// `__dict__` in order (a dataclass instance as its `__init__` leaves it).
struct Maker<'py> {
    cls: Bound<'py, PyAny>,
    new: Bound<'py, PyAny>,
    names: Vec<Bound<'py, PyString>>,
}

impl<'py> Maker<'py> {
    fn new(cls: &Bound<'py, PyAny>, names: &[&str]) -> PyResult<Self> {
        let py = cls.py();
        Ok(Self {
            cls: cls.clone(),
            new: cls.getattr(intern!(py, "__new__"))?,
            names: names.iter().map(|n| PyString::intern(py, n)).collect(),
        })
    }

    fn make(&self, values: &[Bound<'py, PyAny>]) -> PyResult<Bound<'py, PyAny>> {
        let py = self.cls.py();
        let obj = self.new.call1((&self.cls,))?;
        let d = PyDict::new(py);
        for (k, v) in self.names.iter().zip(values) {
            d.set_item(k, v)?;
        }
        obj.setattr(intern!(py, "__dict__"), d)?;
        Ok(obj)
    }
}

fn f64s<'py, const N: usize>(py: Python<'py>, v: &[f32; N]) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyTuple::new(py, v.iter().map(|&x| c_double(x)))?.into_any())
}

/// The reference's exception where `hpd::parse_exact` stops: its `ValueError`, or `struct.error` (round RT3).
fn exact_err(py: Python<'_>, e: &hpd::ExactError) -> PyErr {
    if e.is_struct() {
        match py.import("struct").and_then(|m| m.getattr("error")) {
            Ok(cls) => PyErr::from_type(
                cls.cast_into().unwrap_or_else(|_| py.get_type::<PyValueError>()),
                e.to_string(),
            ),
            Err(err) => err,
        }
    } else {
        PyValueError::new_err(e.to_string())
    }
}

/// `hpdtool.parse(data)` as the reference's objects (`node_cls`, `edge_cls`, `sector_cls`, `file_cls` are
/// `hpdtool.Node`, `Edge`, `Sector`, `HpdFile`); where the reference raised, its `ValueError` or `struct.error` with
/// its message, and a NaN read as CPython's `struct` reads it (round RT3: `parse_reference` was retired). Always
/// `Some` (the `Option` is the F0 signature's).
#[pyfunction]
pub fn hpd_parse<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyBytes>,
    node_cls: &Bound<'py, PyAny>,
    edge_cls: &Bound<'py, PyAny>,
    sector_cls: &Bound<'py, PyAny>,
    file_cls: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let bytes = data.as_bytes();
    let f = py.detach(|| hpd::parse_exact(bytes)).map_err(|e| exact_err(py, &e))?;
    let node = Maker::new(node_cls, &["id", "pos", "flags", "value", "a", "b", "c"])?;
    let edge = Maker::new(edge_cls, &["tag", "src", "dst"])?;
    let sector = Maker::new(
        sector_cls,
        &["id", "bounds", "reserved", "date", "time", "nodes", "edges", "mesh"],
    )?;
    let file = Maker::new(file_cls, &["header", "sectors"])?;
    let sectors = PyList::empty(py);
    for s in &f.sectors {
        let nodes = PyList::empty(py);
        for n in &s.nodes {
            nodes.append(node.make(&[
                n.id.into_bound_py_any(py)?,
                f64s(py, &n.pos)?,
                n.flags.into_bound_py_any(py)?,
                c_double(n.value).into_bound_py_any(py)?,
                n.a.into_bound_py_any(py)?,
                n.b.into_bound_py_any(py)?,
                n.c.into_bound_py_any(py)?,
            ])?)?;
        }
        let edges = PyList::empty(py);
        for e in &s.edges {
            edges.append(edge.make(&[
                e.tag.into_bound_py_any(py)?,
                e.src.into_bound_py_any(py)?,
                e.dst.into_bound_py_any(py)?,
            ])?)?;
        }
        sectors.append(sector.make(&[
            s.id.into_bound_py_any(py)?,
            f64s(py, &s.bounds)?,
            s.reserved.into_bound_py_any(py)?,
            s.date.into_bound_py_any(py)?,
            s.time.into_bound_py_any(py)?,
            nodes.into_any(),
            edges.into_any(),
            PyBytes::new(py, s.mesh).into_any(),
        ])?)?;
    }
    let header = PyTuple::new(py, f.header)?.into_any();
    Ok(Some(file.make(&[header, sectors.into_any()])?))
}

/// The attribute names `hpd_write` reads, interned once per process.
struct HpdNames {
    names: [Py<PyString>; 19],
}

static HPD_NAMES: PyOnceLock<HpdNames> = PyOnceLock::new();

const HPD_FIELDS: [&str; 19] = [
    "header", "sectors", "id", "bounds", "reserved", "date", "time", "nodes", "edges", "mesh", "pos", "flags", "value",
    "a", "b", "c", "tag", "src", "dst",
];

fn hpd_names(py: Python<'_>) -> &HpdNames {
    HPD_NAMES.get_or_init(py, || HpdNames {
        names: HPD_FIELDS.map(|n| PyString::intern(py, n).unbind()),
    })
}

/// One sector's values; the mesh is kept as the Python object's bytes.
struct SectorIn {
    head: (u32, [f32; 6], u32, u32, u32),
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    mesh: Vec<u8>,
}

fn sector_in(py: Python<'_>, s: &Bound<'_, PyAny>, n: &HpdNames) -> Option<SectorIn> {
    let a = |i: usize| attr(s, n.names.get(i)?.bind(py));
    let head = (
        u32_of(&a(2)?)?,
        floats::<6>(&a(3)?)?,
        u32_of(&a(4)?)?,
        u32_of(&a(5)?)?,
        u32_of(&a(6)?)?,
    );
    let mut nodes = Vec::new();
    for o in items(&a(7)?)? {
        let g = |i: usize| attr(&o, n.names.get(i)?.bind(py));
        nodes.push(Node {
            id: u32_of(&g(2)?)?,
            pos: floats::<3>(&g(10)?)?,
            flags: u32_of(&g(11)?)?,
            value: f32_of(&g(12)?)?,
            a: u32_of(&g(13)?)?,
            b: u32_of(&g(14)?)?,
            c: u32_of(&g(15)?)?,
        });
    }
    let mut edges = Vec::new();
    for o in items(&a(8)?)? {
        let g = |i: usize| attr(&o, n.names.get(i)?.bind(py));
        edges.push(Edge {
            tag: u32_of(&g(16)?)?,
            src: u32_of(&g(17)?)?,
            dst: u32_of(&g(18)?)?,
        });
    }
    Some(SectorIn {
        head,
        nodes,
        edges,
        mesh: byte_vec(&a(9)?)?,
    })
}

/// `hpdtool.write(f)` of the reference's `HpdFile`, or `None` where the port leaves it to the reference.
#[pyfunction]
pub fn hpd_write<'py>(py: Python<'py>, f: &Bound<'py, PyAny>) -> Option<Bound<'py, PyBytes>> {
    let n = hpd_names(py);
    let file = (|| {
        let header_items = items(&attr(f, n.names.first()?.bind(py))?)?;
        if header_items.len() != hpd::HEADER_WORDS {
            return None;
        }
        let mut header = [0u32; hpd::HEADER_WORDS];
        for (k, (h, v)) in header.iter_mut().zip(&header_items).enumerate() {
            if k != 2 {
                *h = u32_of(v)?;
            }
        }
        let sectors = items(&attr(f, n.names.get(1)?.bind(py))?)?
            .iter()
            .map(|s| sector_in(py, s, n))
            .collect::<Option<Vec<_>>>()?;
        Some((header, sectors))
    })()?;
    let (header, sectors) = file;
    let out = py.detach(|| {
        let hpd = HpdFile {
            header,
            sectors: sectors
                .iter()
                .map(|s| {
                    let (id, bounds, reserved, date, time) = s.head;
                    Sector {
                        id,
                        bounds,
                        reserved,
                        date,
                        time,
                        nodes: s.nodes.clone(),
                        edges: s.edges.clone(),
                        mesh: &s.mesh,
                    }
                })
                .collect(),
        };
        hpd::write(&hpd)
    });
    out.ok().map(|b| PyBytes::new(py, &b))
}

/// The attribute names `kymesh_write` reads.
const MESH_FIELDS: [&str; 34] = [
    "version",
    "flags",
    "loaded",
    "sector_id",
    "resolution",
    "pitch",
    "rect",
    "tiles_x",
    "tiles_z",
    "radius",
    "tiles",
    "areas",
    "links",
    "end_slot",
    "links_null_offset",
    "area",
    "a",
    "b",
    "tile_x",
    "tile_z",
    "layer",
    "max_y",
    "min_y",
    "portals",
    "border_portals",
    "d_lines",
    "outline",
    "cells",
    "end_row",
    "null_offsets",
    "p0",
    "p1",
    "target",
    "direction",
];

struct MeshNames {
    names: Vec<Py<PyString>>,
    runtime: Py<PyString>,
    points: Py<PyString>,
    header: Py<PyString>,
    refs: Py<PyString>,
    objects: Py<PyString>,
}

static MESH_NAMES: PyOnceLock<MeshNames> = PyOnceLock::new();

/// Reads the reference's `kymesh.Mesh` attribute by attribute.
struct MeshReader<'py> {
    py: Python<'py>,
    n: &'py MeshNames,
}

impl<'py> MeshReader<'py> {
    fn get(&self, o: &Bound<'py, PyAny>, field: &str) -> Option<Bound<'py, PyAny>> {
        let i = MESH_FIELDS.iter().position(|f| *f == field)?;
        attr(o, self.n.names.get(i)?.bind(self.py))
    }

    fn v3(o: &Bound<'py, PyAny>) -> Option<Vec3> {
        floats::<3>(o)
    }

    fn lines(o: &Bound<'py, PyAny>) -> Option<Vec<Vec<Vec3>>> {
        items(o)?
            .iter()
            .map(|l| items(l)?.iter().map(Self::v3).collect())
            .collect()
    }

    fn area(&self, a: &Bound<'py, PyAny>) -> Option<Area> {
        let portals = items(&self.get(a, "portals")?)?
            .iter()
            .map(|p| {
                Some(Portal {
                    p0: Self::v3(&self.get(p, "p0")?)?,
                    p1: Self::v3(&self.get(p, "p1")?)?,
                    target: self.get(p, "target")?.extract::<usize>().ok()?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let border_portals = items(&self.get(a, "border_portals")?)?
            .iter()
            .map(|p| {
                let rt = items(&attr(p, self.n.runtime.bind(self.py))?)?;
                let [r0, r1] = rt.as_slice() else { return None };
                Some(BorderPortal {
                    p0: Self::v3(&self.get(p, "p0")?)?,
                    p1: Self::v3(&self.get(p, "p1")?)?,
                    direction: Self::v3(&self.get(p, "direction")?)?,
                    runtime: (u32_of(r0)?, u32_of(r1)?),
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let cells = items(&self.get(a, "cells")?)?
            .iter()
            .map(|c| {
                Some(Cell {
                    direction: Self::v3(&self.get(c, "direction")?)?,
                    points: items(&attr(c, self.n.points.bind(self.py))?)?
                        .iter()
                        .map(Self::v3)
                        .collect::<Option<Vec<_>>>()?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        // the end row is written only after cells (the reference reads it only then)
        let end_row = if cells.is_empty() {
            [0.0; 3]
        } else {
            Self::v3(&self.get(a, "end_row")?)?
        };
        let nulls = self.get(a, "null_offsets")?;
        let nulls = nulls.cast::<PyDict>().ok()?;
        let mut null_offsets = Vec::new();
        for k in [0x20u32, 0x28, 0x30, 0x38, 0x40, 0x48] {
            if let Some(v) = nulls.get_item(k).ok()? {
                null_offsets.push((k, u32_of(&v)?));
            }
        }
        Some(Area {
            ids: [
                u32_of(&self.get(a, "sector_id")?)?,
                u32_of(&self.get(a, "tile_x")?)?,
                u32_of(&self.get(a, "tile_z")?)?,
                u32_of(&self.get(a, "layer")?)?,
            ],
            values: [
                f32_of(&self.get(a, "radius")?)?,
                f32_of(&self.get(a, "pitch")?)?,
                f32_of(&self.get(a, "max_y")?)?,
                f32_of(&self.get(a, "min_y")?)?,
            ],
            portals,
            border_portals,
            d_lines: Self::lines(&self.get(a, "d_lines")?)?,
            outline: Self::lines(&self.get(a, "outline")?)?,
            cells,
            end_row,
            null_offsets,
        })
    }

    fn mesh(&self, m: &Bound<'py, PyAny>) -> Option<Mesh> {
        let tiles = items(&self.get(m, "tiles")?)?
            .iter()
            .map(|t| {
                items(t)?
                    .iter()
                    .map(|it| {
                        Some(TileItem {
                            area: self.get(it, "area")?.extract::<usize>().ok()?,
                            a: f32_of(&self.get(it, "a")?)?,
                            b: f32_of(&self.get(it, "b")?)?,
                        })
                    })
                    .collect::<Option<Vec<_>>>()
            })
            .collect::<Option<Vec<_>>>()?;
        let areas = items(&self.get(m, "areas")?)?
            .iter()
            .map(|a| self.area(a))
            .collect::<Option<Vec<_>>>()?;
        let links = items(&self.get(m, "links")?)?
            .iter()
            .map(|l| {
                let objects = items(&attr(l, self.n.objects.bind(self.py))?)?
                    .iter()
                    .map(|o| {
                        let h = items(&attr(o, self.n.header.bind(self.py))?)?;
                        let [h0, h1, h2] = h.as_slice() else { return None };
                        let refs = items(&attr(o, self.n.refs.bind(self.py))?)?
                            .iter()
                            .map(|r| r.extract::<(usize, u64)>().ok())
                            .collect::<Option<Vec<_>>>()?;
                        Some(LinkObject {
                            header: [u32_of(h0)?, u32_of(h1)?, u32_of(h2)?],
                            refs,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(NeighbourLink {
                    sector_id: u32_of(&self.get(l, "sector_id")?)?,
                    objects,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let links_null_offset = if links.is_empty() {
            Some(u32_of(&self.get(m, "links_null_offset")?)?)
        } else {
            None
        };
        Some(Mesh {
            version: self.get(m, "version")?.extract::<u16>().ok()?,
            flags: self.get(m, "flags")?.extract::<u16>().ok()?,
            loaded: u32_of(&self.get(m, "loaded")?)?,
            sector_id: u32_of(&self.get(m, "sector_id")?)?,
            resolution: f32_of(&self.get(m, "resolution")?)?,
            pitch: f32_of(&self.get(m, "pitch")?)?,
            rect: floats::<4>(&self.get(m, "rect")?)?,
            tiles_x: u32_of(&self.get(m, "tiles_x")?)?,
            tiles_z: u32_of(&self.get(m, "tiles_z")?)?,
            radius: f32_of(&self.get(m, "radius")?)?,
            tiles,
            areas,
            links,
            end_slot: byte_vec(&self.get(m, "end_slot")?)?,
            links_null_offset,
        })
    }
}

/// The reference's `kymesh.Mesh` read into the port's, or `None` where the port leaves it to the reference.
pub(crate) fn mesh_of<'py>(py: Python<'py>, mesh: &Bound<'py, PyAny>) -> Option<Mesh> {
    let n = MESH_NAMES.get_or_init(py, || MeshNames {
        names: MESH_FIELDS.iter().map(|f| PyString::intern(py, f).unbind()).collect(),
        runtime: PyString::intern(py, "runtime").unbind(),
        points: PyString::intern(py, "points").unbind(),
        header: PyString::intern(py, "header").unbind(),
        refs: PyString::intern(py, "refs").unbind(),
        objects: PyString::intern(py, "objects").unbind(),
    });
    (MeshReader { py, n }).mesh(mesh)
}

/// `kymesh.write_mesh(mesh)` of the reference's `Mesh`, or `None` where the port leaves it to the reference.
#[pyfunction]
pub fn kymesh_write<'py>(py: Python<'py>, mesh: &Bound<'py, PyAny>) -> Option<Bound<'py, PyBytes>> {
    let m = mesh_of(py, mesh)?;
    py.detach(|| kymesh::write(&m)).ok().map(|b| PyBytes::new(py, &b))
}

#[cfg(test)]
mod tests {
    use super::pack_f32;

    #[test]
    fn floats_as_struct_packs_them() {
        assert_eq!(pack_f32(0.1), Some(0.1f32));
        assert_eq!(pack_f32(f64::INFINITY), Some(f32::INFINITY));
        assert_eq!(pack_f32(1e39), None);
        assert_eq!(pack_f32(f64::NAN), None);
        // just under the overflow threshold rounds to f32::MAX, as C's cast
        assert_eq!(pack_f32(f64::from(f32::MAX) * (1.0 + 1e-9)), Some(f32::MAX));
    }
}
