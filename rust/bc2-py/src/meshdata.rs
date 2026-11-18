//! Round F0: mesh resource headers (`bc2_formats::meshdata`) for `bfbc2-console-convert/mesh.py` (`parse` / `write`)
//! and `meshset.py` (`parse` / `write`). The parsers build the reference's dicts (the same keys in the same order); the
//! writers read them. Each answers `None` where the reference raises, a value is a NaN, or a value is not one the port
//! reads exactly as the reference does (a name that is not `bytes`, a float that overflows a 32-bit float, a list of
//! another length): the Python then runs, so a result here is always the reference's and the errors stay its own.

use bc2_formats::meshdata::{self, Kind, Lod, MeshHeader, MeshPart, MeshSet, Part};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyTuple};

use crate::pathdata::{f32_of, items, u32_of};

fn floats<'py, const N: usize>(py: Python<'py>, v: &[f32; N]) -> PyResult<Bound<'py, PyTuple>> {
    PyTuple::new(py, v.iter().map(|&x| f64::from(x)))
}

/// `mesh.parse(b, big_endian)`'s dict, or `None` where the reference raises.
#[pyfunction]
pub fn mesh_parse<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
    big_endian: bool,
) -> PyResult<Option<Bound<'py, PyDict>>> {
    let Ok(m) = meshdata::parse_mesh(b.as_bytes(), big_endian) else {
        return Ok(None);
    };
    let d = PyDict::new(py);
    d.set_item("h0", m.h0)?;
    d.set_item("h1", PyBytes::new(py, &m.h1))?;
    let parts = PyList::empty(py);
    for p in &m.parts {
        let pd = PyDict::new(py);
        pd.set_item("name", PyBytes::new(py, &p.name))?;
        for (k, v) in ["triangles", "vertices", "first_index", "vertex_offset"]
            .iter()
            .zip(p.counts)
        {
            pd.set_item(k, v)?;
        }
        for (k, v) in ["stride", "k", "fa", "fb"].iter().zip(p.bytes) {
            pd.set_item(k, v)?;
        }
        pd.set_item("bones", PyList::new(py, &p.bones)?)?;
        parts.append(pd)?;
    }
    d.set_item("parts", parts)?;
    let lists = PyList::empty(py);
    for l in &m.lists {
        lists.append(PyList::new(py, l)?)?;
    }
    d.set_item("lists", lists)?;
    for (k, v) in ["vertex_bytes", "index_bytes", "wide_indices"].iter().zip(m.sizes) {
        d.set_item(k, v)?;
    }
    d.set_item("data", m.data)?;
    Ok(Some(d))
}

fn get<'py>(d: &Bound<'py, PyDict>, k: &str) -> Option<Bound<'py, PyAny>> {
    d.get_item(k).ok().flatten()
}

fn byte_string(o: &Bound<'_, PyAny>) -> Option<Vec<u8>> {
    o.cast::<PyBytes>().ok().map(|b| b.as_bytes().to_vec())
}

fn u8_of(o: &Bound<'_, PyAny>) -> Option<u8> {
    o.extract::<u8>().ok()
}

fn mesh_header(m: &Bound<'_, PyAny>) -> Option<MeshHeader> {
    let m = m.cast::<PyDict>().ok()?;
    let h1: [u8; 2] = byte_string(&get(m, "h1")?)?.try_into().ok()?;
    let parts = items(&get(m, "parts")?)?
        .iter()
        .map(|p| {
            let p = p.cast::<PyDict>().ok()?;
            let c = |k: &str| u32_of(&get(p, k)?);
            let b = |k: &str| u8_of(&get(p, k)?);
            Some(MeshPart {
                name: byte_string(&get(p, "name")?)?,
                counts: [c("triangles")?, c("vertices")?, c("first_index")?, c("vertex_offset")?],
                bytes: [b("stride")?, b("k")?, b("fa")?, b("fb")?],
                bones: items(&get(p, "bones")?)?
                    .iter()
                    .map(|x| x.extract::<u16>().ok())
                    .collect::<Option<_>>()?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let lists = items(&get(m, "lists")?)?
        .iter()
        .map(|l| items(l)?.iter().map(u8_of).collect::<Option<Vec<u8>>>())
        .collect::<Option<Vec<_>>>()?;
    Some(MeshHeader {
        h0: u32_of(&get(m, "h0")?)?,
        h1,
        parts,
        lists: lists.try_into().ok()?,
        sizes: [0; 3],
        data: 0,
    })
}

/// `mesh.write(m, vertices, indices)`, or `None` where the port leaves it to the reference.
#[pyfunction]
pub fn mesh_write<'py>(
    py: Python<'py>,
    m: &Bound<'py, PyAny>,
    vertices: &Bound<'py, PyAny>,
    indices: &Bound<'py, PyAny>,
) -> Option<Bound<'py, PyBytes>> {
    let header = mesh_header(m)?;
    let (v, i) = (vertices.cast::<PyBytes>().ok()?, indices.cast::<PyBytes>().ok()?);
    let out = meshdata::write_mesh(&header, v.as_bytes(), i.as_bytes()).ok()?;
    Some(PyBytes::new(py, &out))
}

/// `meshset.parse(b, kind, big_endian, flag)`'s dict, or `None` where the reference raises (an unknown kind too).
#[pyfunction]
#[pyo3(signature = (b, kind, big_endian, flag=None))]
pub fn meshset_parse<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
    kind: &str,
    big_endian: bool,
    flag: Option<&Bound<'py, PyAny>>,
) -> PyResult<Option<Bound<'py, PyDict>>> {
    let Some(k) = Kind::of(kind) else {
        return Ok(None);
    };
    let flag = match flag {
        Some(f) if !f.is_none() => Some(f.is_truthy()?),
        _ => None,
    };
    let Ok(s) = meshdata::parse_set(b.as_bytes(), k, big_endian, flag) else {
        return Ok(None);
    };
    let d = PyDict::new(py);
    d.set_item("a", s.a)?;
    d.set_item("z", s.z)?;
    d.set_item("lod_settings", PyBytes::new(py, &s.lod_settings))?;
    let lods = PyList::empty(py);
    for l in &s.lods {
        let ld = PyDict::new(py);
        ld.set_item("x", l.x)?;
        ld.set_item("shaders", l.shaders)?;
        let sets = PyList::empty(py);
        for (name, shaders) in &l.sets {
            let sh = PyList::empty(py);
            for x in shaders {
                sh.append(PyBytes::new(py, x))?;
            }
            sets.append((PyBytes::new(py, name), sh))?;
        }
        ld.set_item("sets", sets)?;
        ld.set_item("bounds", floats(py, &l.bounds)?)?;
        ld.set_item("size", l.size)?;
        ld.set_item("extra", l.extra)?;
        ld.set_item("flag", l.flag)?;
        if let Some((parts, masks)) = &l.parts {
            let pl = PyList::empty(py);
            for (h, name, bounds, transform) in parts {
                pl.append((h, PyBytes::new(py, name), floats(py, bounds)?, floats(py, transform)?))?;
            }
            ld.set_item("parts", pl)?;
            let ml = PyList::empty(py);
            for m in masks {
                ml.append(PyTuple::new(py, m)?)?;
            }
            ld.set_item("masks", ml)?;
        }
        if let Some((billboard, value)) = &l.tree {
            ld.set_item("billboard", PyBytes::new(py, billboard))?;
            ld.set_item("tree_value", value)?;
        }
        if let Some((bones, hashes)) = &l.bones {
            ld.set_item("bones", PyList::new(py, bones)?)?;
            ld.set_item("bone_hashes", PyList::new(py, hashes)?)?;
        }
        lods.append(ld)?;
    }
    d.set_item("lods", lods)?;
    Ok(Some(d))
}

fn float_array<const N: usize>(o: &Bound<'_, PyAny>) -> Option<[f32; N]> {
    let v = items(o)?;
    if v.len() != N {
        return None;
    }
    let mut out = [0f32; N];
    for (s, x) in out.iter_mut().zip(&v) {
        *s = f32_of(x)?;
    }
    Some(out)
}

fn u32_list(o: &Bound<'_, PyAny>) -> Option<Vec<u32>> {
    items(o)?.iter().map(u32_of).collect()
}

#[allow(clippy::many_single_char_names)] // the reference's names
fn lod(l: &Bound<'_, PyAny>, kind: Kind) -> Option<Lod> {
    let l = l.cast::<PyDict>().ok()?;
    let sets = items(&get(l, "sets")?)?
        .iter()
        .map(|s| {
            let v = items(s)?;
            let [name, shaders] = v.as_slice() else { return None };
            Some((
                byte_string(name)?,
                items(shaders)?.iter().map(byte_string).collect::<Option<Vec<_>>>()?,
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    let flag = if kind == Kind::Skinned {
        None
    } else {
        Some(u8_of(&get(l, "flag")?)?)
    };
    let parts = if matches!(kind, Kind::Composite | Kind::Tree) {
        let parts = items(&get(l, "parts")?)?
            .iter()
            .map(|p| -> Option<Part> {
                let v = items(p)?;
                let [h, name, bounds, transform] = v.as_slice() else {
                    return None;
                };
                Some((
                    u32_of(h)?,
                    byte_string(name)?,
                    float_array::<6>(bounds)?,
                    float_array::<12>(transform)?,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        let masks = items(&get(l, "masks")?)?
            .iter()
            .map(|m| {
                let v = items(m)?;
                let [a, b, c] = v.as_slice() else { return None };
                Some([
                    a.extract::<u64>().ok()?,
                    b.extract::<u64>().ok()?,
                    c.extract::<u64>().ok()?,
                ])
            })
            .collect::<Option<Vec<_>>>()?;
        Some((parts, masks))
    } else {
        None
    };
    let tree = if kind == Kind::Tree {
        Some((byte_string(&get(l, "billboard")?)?, u32_of(&get(l, "tree_value")?)?))
    } else {
        None
    };
    let bones = if kind == Kind::Skinned {
        Some((u32_list(&get(l, "bones")?)?, u32_list(&get(l, "bone_hashes")?)?))
    } else {
        None
    };
    Some(Lod {
        x: u8_of(&get(l, "x")?)?,
        shaders: u8_of(&get(l, "shaders")?)?,
        sets,
        bounds: float_array::<6>(&get(l, "bounds")?)?,
        size: u32_of(&get(l, "size")?)?,
        extra: u32_of(&get(l, "extra")?)?,
        flag,
        parts,
        tree,
        bones,
    })
}

/// `meshset.write(s, kind)`, or `None` where the port leaves it to the reference (an unknown kind too).
#[pyfunction]
pub fn meshset_write<'py>(py: Python<'py>, s: &Bound<'py, PyAny>, kind: &str) -> Option<Bound<'py, PyBytes>> {
    let k = Kind::of(kind)?;
    let s = s.cast::<PyDict>().ok()?;
    let set = MeshSet {
        a: u32_of(&get(s, "a")?)?,
        z: u8_of(&get(s, "z")?)?,
        lod_settings: byte_string(&get(s, "lod_settings")?)?,
        lods: items(&get(s, "lods")?)?
            .iter()
            .map(|l| lod(l, k))
            .collect::<Option<Vec<_>>>()?,
    };
    let out = meshdata::write_set(&set, k).ok()?;
    Some(PyBytes::new(py, &out))
}
