//! Bindings for Granny resources (`bfbc2-anim-tools/granny.py`): `granny_header` / `granny_index` (the tables
//! `Granny.__init__` reads), `granny_root` / `granny_roots_many` (`Granny.root()`: the same dicts, lists, tuples,
//! numbers and strings), `granny_to_pc` / `granny_to_pc_many` (`to_pc`).
//!
//! Every error is a `ValueError`; `granny.py` then runs its Python code on that input, which raises its own
//! exception (or, where this port is stricter than it, gives its result). The work runs with the interpreter
//! released, the batch functions on all cores.

use std::collections::HashMap;

use bc2_formats::granny::{Granny, GrannyError, Object, Ref, Scalar, Text, Value};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyBytesMethods, PyDict, PyList, PyString, PyTuple};
use rayon::prelude::*;

/// A bone map: {old index: new index}.
type Bones = HashMap<u32, u32>;

fn value_error(e: &GrannyError) -> PyErr {
    PyValueError::new_err(e.to_string())
}

/// Latin-1 bytes as a Python string (`bytes.decode('latin-1')`).
fn latin1<'py>(py: Python<'py>, b: &[u8]) -> Bound<'py, PyString> {
    if b.is_ascii() {
        // ASCII is valid UTF-8
        PyString::new(py, std::str::from_utf8(b).unwrap_or_default())
    } else {
        PyString::new(py, &b.iter().map(|&c| char::from(c)).collect::<String>())
    }
}

/// Builds `Granny.root()`'s Python tree from a [`Value`] tree: each string of the table made once.
struct Builder<'py, 'a> {
    py: Python<'py>,
    g: &'a Granny<'a>,
    strings: Vec<Option<Bound<'py, PyString>>>,
    /// Each type's `__tref__` tuple, made once (the Python reader shares one tuple per type too).
    trefs: HashMap<Ref, Bound<'py, PyTuple>>,
    keys: [Bound<'py, PyString>; 7],
}

impl<'py, 'a> Builder<'py, 'a> {
    fn new(py: Python<'py>, g: &'a Granny<'a>) -> Self {
        let k = |s: &str| PyString::intern(py, s);
        Self {
            py,
            g,
            strings: vec![None; g.strings.len()],
            trefs: HashMap::new(),
            keys: [
                k("__at__"),
                k("__tref__"),
                k("__type__"),
                k("Flags"),
                k("Position"),
                k("Orientation"),
                k("ScaleShear"),
            ],
        }
    }

    fn text(&mut self, t: Text<'_>) -> Bound<'py, PyString> {
        match t {
            Text::Bytes(b) => latin1(self.py, b),
            Text::Missing(i) => PyString::new(self.py, &format!("#{i}")),
            Text::Index(i) => {
                let slot = self.strings.get_mut(i as usize);
                match slot {
                    Some(Some(s)) => s.clone(),
                    Some(slot) => {
                        let s = latin1(self.py, self.g.strings.get(i as usize).copied().unwrap_or_default());
                        *slot = Some(s.clone());
                        s
                    }
                    None => PyString::new(self.py, ""),
                }
            }
        }
    }

    fn scalar(&self, s: Scalar) -> PyResult<Bound<'py, PyAny>> {
        Ok(match s {
            Scalar::Int(i) => i.into_pyobject(self.py)?.into_any(),
            Scalar::Float(f) => f.into_pyobject(self.py)?.into_any(),
        })
    }

    fn floats(&self, v: &[f32]) -> PyResult<Bound<'py, PyTuple>> {
        PyTuple::new(self.py, v.iter().map(|&x| f64::from(x)))
    }

    fn object(&mut self, o: &Object<'_>) -> PyResult<Bound<'py, PyAny>> {
        let d = PyDict::new(self.py);
        d.set_item(&self.keys[0], (o.at.0, o.at.1))?;
        let tref = if let Some(t) = self.trefs.get(&o.tref) {
            t.clone()
        } else {
            let t = PyTuple::new(self.py, [o.tref.0, o.tref.1])?;
            self.trefs.insert(o.tref, t.clone());
            t
        };
        d.set_item(&self.keys[1], tref)?;
        for (name, v) in &o.fields {
            let k = self.text(*name);
            d.set_item(k, self.value(v)?)?;
        }
        if let Some(t) = &o.type_name {
            d.set_item(&self.keys[2], latin1(self.py, t))?;
        }
        Ok(d.into_any())
    }

    fn value(&mut self, v: &Value<'_>) -> PyResult<Bound<'py, PyAny>> {
        Ok(match v {
            Value::None => self.py.None().into_bound(self.py),
            Value::Scalar(s) => self.scalar(*s)?,
            Value::Text(t) => self.text(*t).into_any(),
            Value::Transform(flags, f) => {
                let d = PyDict::new(self.py);
                d.set_item(&self.keys[3], *flags)?;
                d.set_item(&self.keys[4], self.floats(f.get(0..3).unwrap_or_default())?)?;
                d.set_item(&self.keys[5], self.floats(f.get(3..7).unwrap_or_default())?)?;
                d.set_item(&self.keys[6], self.floats(f.get(7..16).unwrap_or_default())?)?;
                d.into_any()
            }
            Value::Ints(n) => PyList::new(self.py, n)?.into_any(),
            Value::Floats(n) => PyList::new(self.py, n)?.into_any(),
            Value::List(items) => {
                let mut v = Vec::with_capacity(items.len());
                for x in items {
                    v.push(self.value(x)?);
                }
                PyList::new(self.py, v)?.into_any()
            }
            Value::Object(o) => self.object(o)?,
        })
    }
}

/// `Granny(data)`'s tables: `(big_endian, strings_from, strings, gr2, root_type, root_obj, total, crc_from)`.
#[pyfunction]
pub fn granny_header<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>) -> PyResult<Bound<'py, PyTuple>> {
    let b = data.as_bytes();
    let g = py.detach(|| Granny::parse(b)).map_err(|e| value_error(&e))?;
    let strings = PyList::new(py, g.strings.iter().map(|s| latin1(py, s)))?;
    (
        g.endian.big,
        g.strings_from,
        strings,
        g.gr2,
        g.root_type,
        g.root_obj,
        g.total,
        g.crc_from,
    )
        .into_pyobject(py)
}

/// `Granny(data)`'s sections (`bytearray`s), their offsets in the resource and the relocation dict.
#[pyfunction]
pub fn granny_index<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>) -> PyResult<Bound<'py, PyTuple>> {
    let b = data.as_bytes();
    let g = py.detach(|| Granny::parse(b)).map_err(|e| value_error(&e))?;
    let sections = PyList::new(py, g.sections.iter().map(|s| PyByteArray::new(py, s)))?;
    // the relocations in file order (a dict built in that order is the Python reader's)
    let reloc = PyDict::new(py);
    for (k, o, ts, to) in reloc_rows(&g)? {
        reloc.set_item((k, o), (ts, to))?;
    }
    (sections, g.section_at.clone(), reloc).into_pyobject(py)
}

/// The relocations in the order the file lists them (section by section).
fn reloc_rows(g: &Granny<'_>) -> PyResult<Vec<(u32, u32, u32, u32)>> {
    let e = g.endian;
    let gg = g.data.get(g.gr2..).unwrap_or_default();
    let soff = g.crc_from - 32;
    let mut out = Vec::with_capacity(g.reloc.len());
    for k in 0..g.sections.len() as u64 {
        let row = 32 + soff + 44 * k;
        let roff = u64::from(e.u32(gg, row + 28).map_err(|x| value_error(&x))?);
        let n = e.u32(gg, row + 32).map_err(|x| value_error(&x))?;
        for r in 0..u64::from(n) {
            let at = roff + 12 * r;
            let w = |i: u64| e.u32(gg, at + 4 * i).map_err(|x| value_error(&x));
            out.push((k as u32, w(0)?, w(1)?, w(2)?));
        }
    }
    Ok(out)
}

/// `Granny(data).root(limit)`.
#[pyfunction]
#[pyo3(signature = (data, limit=64))]
pub fn granny_root<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>, limit: i64) -> PyResult<Bound<'py, PyAny>> {
    let b = data.as_bytes();
    let (g, tree) = py
        .detach(|| Granny::parse(b).and_then(|g| g.root(limit).map(|t| (g, t))))
        .map_err(|e| value_error(&e))?;
    Builder::new(py, &g).value(&tree)
}

/// `granny_root` of many resources: read on all cores, then built in order. Each item the tree or the
/// `ValueError` (returned, not raised).
#[pyfunction]
#[pyo3(signature = (items, limit=64))]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn granny_roots_many<'py>(
    py: Python<'py>,
    items: Vec<Bound<'py, PyBytes>>,
    limit: i64,
) -> PyResult<Bound<'py, PyList>> {
    let slices: Vec<&[u8]> = items.iter().map(PyBytesMethods::as_bytes).collect();
    let read = py.detach(|| {
        slices
            .par_iter()
            .map(|b| Granny::parse(b).and_then(|g| g.root(limit).map(|t| (g, t))))
            .collect::<Vec<_>>()
    });
    let out = PyList::empty(py);
    for r in &read {
        match r {
            Ok((g, t)) => out.append(Builder::new(py, g).value(t)?)?,
            Err(e) => out.append(value_error(e).into_value(py))?,
        }
    }
    Ok(out)
}

/// `to_pc(data, bones)`: the PC resource's bytes.
#[pyfunction]
#[pyo3(signature = (data, bones=None))]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a dict argument as an owned HashMap"
)]
pub fn granny_to_pc<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyBytes>,
    bones: Option<HashMap<u32, u32>>,
) -> PyResult<Bound<'py, PyBytes>> {
    let b = data.as_bytes();
    let out = py
        .detach(|| bc2_convert::granny::to_pc(b, bones.as_ref()))
        .map_err(|e| value_error(&e))?;
    Ok(PyBytes::new(py, &out))
}

/// `granny_to_pc` of many `(data, bones or None)` on all cores: each the bytes or the `ValueError`, returned.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn granny_to_pc_many<'py>(
    py: Python<'py>,
    items: Vec<(Bound<'py, PyBytes>, Option<Bones>)>,
) -> PyResult<Bound<'py, PyList>> {
    let jobs: Vec<bc2_convert::granny::Job<'_, std::hash::RandomState>> =
        items.iter().map(|(b, m)| (b.as_bytes(), m.as_ref())).collect();
    let results = py.detach(|| bc2_convert::granny::to_pc_all(&jobs));
    let out = PyList::empty(py);
    for r in results {
        match r {
            Ok(b) => out.append(PyBytes::new(py, &b))?,
            Err(e) => out.append(value_error(&e).into_value(py))?,
        }
    }
    Ok(out)
}
