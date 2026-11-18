//! Bindings for the BF1943 importer's Havok ports (round F2b): `bfbc2-bf1943-import/mopp.py` (`mopp_*`) and
//! `havokport.py` (`havokport_*`, `PortLayouts`), through its switch `importcore.py`.
//!
//! The convention: a function returns a 1-tuple `(result,)` when it answered, `None` where the port refuses (the
//! reference would raise something other than its own error, or meets a case not reproduced), and raises
//! `ValueError` with the reference's own error message (`MoppError`, `PortError`, `MaterialError` - the Python side
//! raises its class). Arguments it cannot take as the port needs them (a key that is not an int, a value past
//! 64 bits) raise `TypeError` / `OverflowError`, which the Python side also treats as a refusal.

use std::collections::{HashMap, HashSet};

use bc2_convert::dbxcatalogue::{self, Key};
use bc2_convert::havokport::{self, Layouts, Port};
use bc2_convert::mopp::{self, Mopp, Ray, Tri};
use bc2_convert::pyfmt::OrderedMap;
use bc2_formats::dbx::{self, DbxTree};
use pyo3::IntoPyObjectExt;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyFloat, PyInt, PyList, PySet, PyString, PyTuple};

/// `(value,)`, `None` for a refusal, `ValueError` for the reference's own error.
fn answer<'py, T: IntoPyObject<'py>>(
    py: Python<'py>,
    r: Result<T, Option<String>>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    match r {
        Ok(v) => Ok(Some(PyTuple::new(py, [v.into_bound_py_any(py)?])?)),
        Err(None) => Ok(None),
        Err(Some(m)) => Err(PyValueError::new_err(m)),
    }
}

fn of_mopp(e: Mopp) -> Option<String> {
    match e {
        Mopp::Error(m) => Some(m),
        Mopp::Refused => None,
    }
}

fn of_port(e: Port) -> Option<String> {
    match e {
        Port::Error(m) | Port::Material(m) => Some(m),
        Port::Refused => None,
    }
}

/// An exact Python `int` key as i64, or `None` (a bool or another type: the reference compares it otherwise).
fn int_key(k: &Bound<'_, PyAny>) -> Option<i64> {
    if k.is_exact_instance_of::<PyInt>() {
        k.extract().ok()
    } else {
        None
    }
}

/// How strictly numbers are taken: the ray casts add products with Python's `sum`, whose result depends on the
/// items' types (exact floats: a compensated sum; NumPy scalars: plain additions), so they take exact floats only;
/// the box queries do no sums and take floats, ints and NumPy float64 (the same doubles).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Num {
    ExactFloat,
    Double,
}

/// One number as the port takes it, or `None`.
fn num(x: &Bound<'_, PyAny>, how: Num) -> Option<f64> {
    if x.is_exact_instance_of::<PyFloat>() {
        return x.extract().ok();
    }
    if how == Num::Double {
        if x.is_exact_instance_of::<PyInt>() {
            let v: i64 = x.extract().ok()?;
            let small = i32::try_from(v).ok()?;
            return Some(f64::from(small));
        }
        let ty = x.get_type();
        if ty.name().ok()?.to_str().ok()? == "float64" && ty.module().ok()?.to_str().ok()? == "numpy" {
            return x.extract().ok();
        }
    }
    None
}

/// `N` numbers of a sequence as the port takes them, or `None`.
fn nums<const N: usize>(seq: &Bound<'_, PyAny>, how: Num) -> PyResult<Option<[f64; N]>> {
    let mut out = [0.0; N];
    let mut n = 0usize;
    for x in seq.try_iter()? {
        let Some(slot) = out.get_mut(n) else { return Ok(None) };
        let Some(v) = num(&x?, how) else { return Ok(None) };
        *slot = v;
        n += 1;
    }
    Ok((n == N).then_some(out))
}

/// `mopp.query_int(code, qmin, qmax)`: `(sorted keys,)`.
#[pyfunction]
pub fn mopp_query<'py>(
    py: Python<'py>,
    code: &Bound<'py, PyBytes>,
    qmin: [i64; 3],
    qmax: [i64; 3],
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let b = code.as_bytes();
    // the quantized box stays under 2^40 here (as `quantize` refuses beyond): larger values are left to Python
    if qmin.iter().chain(&qmax).any(|v| v.unsigned_abs() > 1 << 40) {
        return Ok(None);
    }
    let r = py.detach(|| mopp::query_int(b, qmin, qmax).map(|s| s.into_iter().collect::<Vec<i64>>()));
    answer(py, r.map_err(of_mopp))
}

/// `mopp.query(code, info, lo, hi)`: `(sorted keys,)`.
#[pyfunction]
pub fn mopp_query_box<'py>(
    py: Python<'py>,
    code: &Bound<'py, PyBytes>,
    info: &Bound<'py, PyAny>,
    lo: &Bound<'py, PyAny>,
    hi: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let (Some(info), Some(lo), Some(hi)) = (
        nums::<4>(info, Num::Double)?,
        nums::<3>(lo, Num::Double)?,
        nums::<3>(hi, Num::Double)?,
    ) else {
        return Ok(None);
    };
    let b = code.as_bytes();
    let r = py.detach(|| mopp::query(b, &info, &lo, &hi).map(|s| s.into_iter().collect::<Vec<i64>>()));
    answer(py, r.map_err(of_mopp))
}

/// `mopp.complete(code, info, boxes, tolerance)`: `((missed, mean),)`; boxes as the dict gives them.
#[pyfunction]
pub fn mopp_complete<'py>(
    py: Python<'py>,
    code: &Bound<'py, PyBytes>,
    info: &Bound<'py, PyAny>,
    boxes: &Bound<'py, PyDict>,
    tolerance: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let (Some(info), Some(tolerance)) = (nums::<4>(info, Num::Double)?, num(tolerance, Num::Double)) else {
        return Ok(None);
    };
    let mut rows = Vec::with_capacity(boxes.len());
    for (k, v) in boxes.iter() {
        let Some(k) = int_key(&k) else { return Ok(None) };
        let (lo, hi): (Bound<'py, PyAny>, Bound<'py, PyAny>) = v.extract()?;
        let (Some(lo), Some(hi)) = (nums::<3>(&lo, Num::Double)?, nums::<3>(&hi, Num::Double)?) else {
            return Ok(None);
        };
        rows.push((k, lo, hi));
    }
    let b = code.as_bytes();
    let r = py.detach(|| mopp::complete(b, &info, &rows, tolerance));
    answer(py, r.map_err(of_mopp))
}

/// A `{key: three points}` dict as the port takes it, in its order; `None` when a key is not an int or a triangle
/// not three points of three numbers.
fn tris_of(d: &Bound<'_, PyDict>) -> PyResult<Option<Vec<(i64, Tri)>>> {
    let mut out = Vec::with_capacity(d.len());
    for (k, v) in d.iter() {
        let Some(k) = int_key(&k) else { return Ok(None) };
        let mut tri: Tri = [[0.0; 3]; 3];
        let mut n = 0usize;
        for p in v.try_iter()? {
            let Some(slot) = tri.get_mut(n) else { return Ok(None) };
            let Some(p) = nums::<3>(&p?, Num::ExactFloat)? else {
                return Ok(None);
            };
            *slot = p;
            n += 1;
        }
        if n != 3 {
            return Ok(None);
        }
        out.push((k, tri));
    }
    Ok(Some(out))
}

/// `mopp.ray_cast(code, info, tris, origin, direction, length, step, tolerance)`: `((distance, key) or None,)`.
#[pyfunction]
#[expect(clippy::too_many_arguments, reason = "the reference's signature")]
pub fn mopp_ray_cast<'py>(
    py: Python<'py>,
    code: &Bound<'py, PyBytes>,
    info: &Bound<'py, PyAny>,
    tris: &Bound<'py, PyDict>,
    origin: &Bound<'py, PyAny>,
    direction: &Bound<'py, PyAny>,
    length: &Bound<'py, PyAny>,
    step: &Bound<'py, PyAny>,
    tolerance: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let e = Num::ExactFloat;
    let (Some(info), Some(origin), Some(direction)) = (
        nums::<4>(info, Num::Double)?,
        nums::<3>(origin, e)?,
        nums::<3>(direction, e)?,
    ) else {
        return Ok(None);
    };
    let (Some(length), Some(step), Some(tolerance)) = (num(length, e), num(step, e), num(tolerance, e)) else {
        return Ok(None);
    };
    let Some(rows) = tris_of(tris)? else { return Ok(None) };
    let map: HashMap<i64, Tri> = rows.into_iter().collect();
    let ray = Ray {
        origin,
        direction,
        length,
        step,
        tolerance,
    };
    let b = code.as_bytes();
    let r = py.detach(|| mopp::ray_cast(b, &info, &map, &ray));
    answer(py, r.map_err(of_mopp))
}

/// `mopp.ray_cast_brute(tris, origin, direction, length)`: `((distance, key) or None,)`.
#[pyfunction]
pub fn mopp_ray_cast_brute<'py>(
    py: Python<'py>,
    tris: &Bound<'py, PyDict>,
    origin: &Bound<'py, PyAny>,
    direction: &Bound<'py, PyAny>,
    length: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let e = Num::ExactFloat;
    let (Some(origin), Some(direction), Some(length)) =
        (nums::<3>(origin, e)?, nums::<3>(direction, e)?, num(length, e))
    else {
        return Ok(None);
    };
    let Some(rows) = tris_of(tris)? else { return Ok(None) };
    let r = py.detach(|| mopp::ray_cast_brute(&rows, &origin, &direction, length));
    answer(py, Ok::<_, Option<String>>(r))
}

/// The game's class layouts as `havokport.restructure` asks them (made once per class dict).
#[pyclass(frozen)]
pub struct PortLayouts {
    layouts: Layouts,
}

#[pymethods]
impl PortLayouts {
    /// `names`: every class name; `aabb_center`: per 5.5.0 bounding-box class its `aabbCenter` offset (`None`
    /// where the reference's `_member_offset` raises).
    #[new]
    fn new(names: HashSet<String>, aabb_center: HashMap<String, Option<u32>>) -> Self {
        Self {
            layouts: Layouts { names, aabb_center },
        }
    }
}

/// `havokport.restructure(b, template, classes, table)`: `(bytes,)`.
#[pyfunction]
#[pyo3(signature = (b, template, layouts, table=None))]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn havokport_restructure<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
    template: &Bound<'py, PyBytes>,
    layouts: &Bound<'py, PortLayouts>,
    table: Option<Vec<f64>>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let (b, t) = (b.as_bytes(), template.as_bytes());
    let l = &layouts.get().layouts;
    let r = py.detach(|| havokport::restructure(b, t, l, table.as_deref()));
    answer(py, r.map(|v| PyBytes::new(py, &v)).map_err(of_port))
}

/// `havokport.wrapper_run_table(b)`: `((offset, M, C, big) or None,)`.
#[pyfunction]
pub fn havokport_run_table<'py>(py: Python<'py>, b: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyTuple>>> {
    answer(py, havokport::wrapper_run_table(b.as_bytes()).map_err(of_port))
}

/// `havokport._run_table_to_pc(b)`: `(bytes,)`.
#[pyfunction]
pub fn havokport_run_table_to_pc<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let r = havokport::run_table_to_pc(b.as_bytes());
    answer(py, r.map(|v| PyBytes::new(py, &v)).map_err(of_port))
}

/// `havokport.shape_subparts_to_550(pc)`: `((bytes, swapped),)`.
#[pyfunction]
pub fn havokport_subparts<'py>(py: Python<'py>, pc: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let b = pc.as_bytes();
    let r = py.detach(|| havokport::shape_subparts_to_550(b));
    answer(py, r.map(|(v, n)| (PyBytes::new(py, &v), n)).map_err(of_port))
}

/// `convert`'s last two steps (`_run_table_to_pc`, then `shape_subparts_to_550`): `(bytes,)`.
#[pyfunction]
pub fn havokport_finish<'py>(py: Python<'py>, pc: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let b = pc.as_bytes();
    let r = py.detach(|| havokport::finish(b));
    answer(py, r.map(|v| PyBytes::new(py, &v)).map_err(of_port))
}

/// `havokport.ufloat8_table(exe)`: the table or `None`.
#[pyfunction]
pub fn havokport_ufloat8_table(py: Python<'_>, exe: &Bound<'_, PyBytes>) -> Option<Vec<f64>> {
    let b = exe.as_bytes();
    py.detach(|| havokport::ufloat8_table(b))
}

/// `havokport._wrapper(b)`: `((n, p, q, hashes, o),)`.
#[pyfunction]
pub fn havokport_wrapper<'py>(py: Python<'py>, b: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyTuple>>> {
    answer(py, havokport::wrapper(b.as_bytes()).map_err(of_port))
}

/// `havokport.part_materials(b)`: `((hashes, indices),)`.
#[pyfunction]
pub fn havokport_part_materials<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let r =
        havokport::part_materials(b.as_bytes()).map(|(h, i)| (h, i.into_iter().map(i64::from).collect::<Vec<i64>>()));
    answer(py, r.map_err(of_port))
}

/// `havokport.part_run(indices)`: `(bytes,)`.
#[pyfunction]
pub fn havokport_part_run<'py>(py: Python<'py>, indices: &Bound<'py, PyAny>) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let Some(ix) = exact_ints(indices)? else {
        return Ok(None);
    };
    answer(
        py,
        havokport::part_run(&ix).map(|v| PyBytes::new(py, &v)).map_err(of_port),
    )
}

/// The items of a sequence when every one is an exact `int` that fits 64 bits, else `None`.
fn exact_ints(seq: &Bound<'_, PyAny>) -> PyResult<Option<Vec<i64>>> {
    let mut out = Vec::new();
    for x in seq.try_iter()? {
        let x = x?;
        let Some(v) = int_key(&x) else { return Ok(None) };
        out.push(v);
    }
    Ok(Some(out))
}

/// `havokport.with_part_materials(b, indices, hashes)`: `(bytes,)`.
#[pyfunction]
pub fn havokport_with_part_materials<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
    indices: &Bound<'py, PyAny>,
    hashes: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let (Some(ix), Some(hs)) = (exact_ints(indices)?, exact_ints(hashes)?) else {
        return Ok(None);
    };
    let Ok(hs) = hs.into_iter().map(u32::try_from).collect::<Result<Vec<u32>, _>>() else {
        return Ok(None);
    };
    let r = havokport::with_part_materials(b.as_bytes(), &ix, &hs);
    answer(py, r.map(|v| PyBytes::new(py, &v)).map_err(of_port))
}

/// `havokport.havok_materials(b)`: `(names,)`.
#[pyfunction]
pub fn havokport_havok_materials<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    answer(py, havokport::havok_materials(b.as_bytes()).map_err(of_port))
}

/// `havokport.with_havok_materials(b, names)`: `(bytes,)`.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn havokport_with_havok_materials<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
    names: Vec<String>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let r = havokport::with_havok_materials(b.as_bytes(), &names);
    answer(py, r.map(|v| PyBytes::new(py, &v)).map_err(of_port))
}

/// `havokport.bodies(b)`: `([(name, [[index, ...], ...] or None)],)`.
#[pyfunction]
pub fn havokport_bodies<'py>(py: Python<'py>, b: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyTuple>>> {
    match havokport::bodies(b.as_bytes()) {
        Ok(v) => {
            let out = PyList::empty(py);
            for (name, mats) in v {
                out.append((name, mats))?;
            }
            Ok(Some(PyTuple::new(py, [out])?))
        }
        Err(e) => answer(py, Err::<(), _>(of_port(e))),
    }
}

/// `havokport.with_face_materials(b, changes)`: `(bytes,)`.
#[pyfunction]
pub fn havokport_with_face_materials<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
    changes: &Bound<'py, PyDict>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let mut rows = Vec::with_capacity(changes.len());
    for (k, v) in changes.iter() {
        let Some(bi) = int_key(&k) else { return Ok(None) };
        let mut lists = Vec::new();
        for sub in v.try_iter()? {
            let Some(vals) = exact_ints(&sub?)? else {
                return Ok(None);
            };
            lists.push(vals);
        }
        rows.push((bi, lists));
    }
    let r = havokport::with_face_materials(b.as_bytes(), &rows);
    answer(py, r.map(|v| PyBytes::new(py, &v)).map_err(of_port))
}

// ------------------------------------------------------------------------------------------------ dbxcatalogue

/// Builds the Python elements of one partition's kept subtrees, each element once (a prototype inside a kept
/// instance is the same object, as in the reference's one tree).
struct Elements<'py, 't, 'a> {
    py: Python<'py>,
    cls: &'t Bound<'py, PyAny>,
    tree: &'t DbxTree<'a>,
    built: HashMap<usize, Bound<'py, PyAny>>,
    strings: HashMap<u32, Bound<'py, PyString>>,
    empty: Bound<'py, PyBytes>,
}

impl<'py> Elements<'py, '_, '_> {
    /// One Python string per string of the file, as the reference's parse makes them (shared by every element).
    fn s(&mut self, id: u32) -> Bound<'py, PyString> {
        let (py, tree) = (self.py, self.tree);
        self.strings
            .entry(id)
            .or_insert_with(|| PyString::new(py, tree.str(id)))
            .clone()
    }

    fn of(&mut self, index: usize) -> PyResult<Bound<'py, PyAny>> {
        if let Some(e) = self.built.get(&index) {
            return Ok(e.clone());
        }
        let node = self
            .tree
            .node(index)
            .ok_or_else(|| PyValueError::new_err("catalogue: an element index past its tree"))?;
        let attributes = PyList::empty(self.py);
        for (k, v) in node.attribute_ids() {
            attributes.append((self.s(k), self.s(v)))?;
        }
        let children = if node.is_container() {
            let list = PyList::empty(self.py);
            for c in node.children() {
                list.append(self.of(c.index())?)?;
            }
            Some(list)
        } else {
            None
        };
        let empty = PyString::new(self.py, "");
        let (text, raw) = match (&children, node.text_id()) {
            (Some(_), _) => (empty, &[][..]),
            (None, Some(t)) => (self.s(t), &[][..]),
            (None, None) => (empty, node.raw()),
        };
        let tag = self.s(node.tag_id());
        let raw = if raw.is_empty() {
            self.empty.clone()
        } else {
            PyBytes::new(self.py, raw)
        };
        let e = self.cls.call1((tag, node.kind(), attributes, text, raw, children))?;
        self.built.insert(index, e.clone());
        Ok(e)
    }
}

/// One Python string per distinct text of the tables (names, types and values recur thousands of times: shared, the
/// catalogue's pickle stays small).
struct Interned<'py> {
    py: Python<'py>,
    map: HashMap<String, Bound<'py, PyString>>,
}

impl<'py> Interned<'py> {
    fn s(&mut self, v: &str) -> Bound<'py, PyString> {
        if let Some(x) = self.map.get(v) {
            return x.clone();
        }
        let x = PyString::new(self.py, v);
        self.map.insert(v.to_owned(), x.clone());
        x
    }
}

fn counters<'py>(
    py: Python<'py>,
    m: &OrderedMap<Key, OrderedMap<String, u64>>,
    st: &mut Interned<'py>,
) -> PyResult<Bound<'py, PyDict>> {
    let out = PyDict::new(py);
    for ((t, p), c) in m.iter() {
        let d = PyDict::new(py);
        for (k, v) in c.iter() {
            d.set_item(st.s(k), v)?;
        }
        out.set_item((st.s(t), p), d)?;
    }
    Ok(out)
}

/// `dbxcatalogue.build`'s pass in Rust: `jobs` is `[(bundle, [entry names in the index's order])]`; returns `(dict,)`
/// of the catalogue's tables (`partitions`, `classes`, `order`, `shapes`, `texts`, `numeric` as dicts of dicts in
/// the reference's insertion order - the caller makes the `Counter`s -, `protos`, `guids`, `primary`, `defaults`,
/// `others`, `problems`), the elements made with `element_cls`; `None` where the port refuses.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn catalogue_build<'py>(
    py: Python<'py>,
    jobs: Vec<(String, Vec<String>)>,
    element_cls: &Bound<'py, PyAny>,
    group: usize,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let Some(b) = py.detach(|| dbxcatalogue::build(&jobs, group)) else {
        return Ok(None);
    };
    // the kept elements, partition by partition
    let mut wanted: HashMap<usize, Vec<usize>> = HashMap::new();
    for (_, (_, (pi, ei))) in b.defaults.iter().chain(b.others.iter()) {
        wanted.entry(*pi).or_default().push(*ei);
    }
    for (_, (pi, ei)) in b.protos.iter() {
        wanted.entry(*pi).or_default().push(*ei);
    }
    let mut made: HashMap<(usize, usize), Bound<'py, PyAny>> = HashMap::new();
    let mut parts: Vec<&usize> = wanted.keys().collect();
    parts.sort_unstable();
    for pi in parts {
        let data = b.kept.get(pi).map_or(&[][..], Vec::as_slice);
        let tree = dbx::parse_tree(data).map_err(|e| PyValueError::new_err(e.to_string()))?;
        let mut el = Elements {
            py,
            cls: element_cls,
            tree: &tree,
            built: HashMap::new(),
            strings: HashMap::new(),
            empty: PyBytes::new(py, b""),
        };
        let mut idx = wanted.get(pi).cloned().unwrap_or_default();
        // instances first, so a prototype inside one is taken from it
        idx.sort_unstable();
        for ei in idx {
            made.insert((*pi, ei), el.of(ei)?);
        }
    }
    let get = |r: &(usize, usize)| -> PyResult<Bound<'py, PyAny>> {
        made.get(r)
            .cloned()
            .ok_or_else(|| PyValueError::new_err("catalogue: an element not made"))
    };
    let mut st = Interned {
        py,
        map: HashMap::new(),
    };
    let out = PyDict::new(py);
    out.set_item("partitions", b.partitions)?;
    let classes: Vec<Bound<'py, PyString>> = b.classes.iter().map(|c| st.s(c)).collect();
    out.set_item("classes", PySet::new(py, &classes)?)?;
    let order = PyDict::new(py);
    for (t, v) in b.order.iter() {
        let names: Vec<Bound<'py, PyString>> = v.iter().map(|n| st.s(n)).collect();
        order.set_item(st.s(t), names)?;
    }
    out.set_item("order", order)?;
    out.set_item("shapes", counters(py, &b.shapes, &mut st)?)?;
    out.set_item("texts", counters(py, &b.texts, &mut st)?)?;
    out.set_item("numeric", counters(py, &b.numeric, &mut st)?)?;
    let protos = PyDict::new(py);
    for ((t, p), r) in b.protos.iter() {
        protos.set_item((st.s(t), p), get(r)?)?;
    }
    out.set_item("protos", protos)?;
    let guids = PyDict::new(py);
    for (g, w) in b.guids.iter() {
        match w.as_slice() {
            [one] => guids.set_item(g, st.s(one))?,
            many => {
                let names: Vec<Bound<'py, PyString>> = many.iter().map(|n| st.s(n)).collect();
                guids.set_item(g, PyTuple::new(py, names)?)?;
            }
        }
    }
    out.set_item("guids", guids)?;
    let primary = PyDict::new(py);
    for (k, v) in b.primary.iter() {
        primary.set_item(st.s(k), v)?;
    }
    out.set_item("primary", primary)?;
    for (key, m) in [("defaults", &b.defaults), ("others", &b.others)] {
        let d = PyDict::new(py);
        for (t, (name, r)) in m.iter() {
            d.set_item(st.s(t), (st.s(name), get(r)?))?;
        }
        out.set_item(key, d)?;
    }
    out.set_item("problems", &b.problems)?;
    Ok(Some(PyTuple::new(py, [out])?))
}

/// `dbxcatalogue.exe_identifiers`' scan of the executable's bytes (the caller makes the frozenset).
#[pyfunction]
pub fn catalogue_exe_identifiers(py: Python<'_>, data: &Bound<'_, PyBytes>) -> Vec<String> {
    let d = data.as_bytes();
    py.detach(|| dbxcatalogue::exe_identifiers(d))
}
