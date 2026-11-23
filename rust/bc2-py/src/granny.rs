//! Bindings for Granny resources (`bfbc2-anim-tools/granny.py`): the `Granny` class (`granny.Granny`: the
//! resource's bytes, its tables, `root()` / `read()` - the same dicts, lists, tuples, numbers and strings as the
//! Python reader's -, `field` / `array_at` / `put` / `to_bytes`: round PR1b-2-rest3), `granny_root` /
//! `granny_roots_many` (trees of many resources on all cores), `granny_to_pc` / `granny_to_pc_many` (`to_pc`).
//!
//! A resource the reader refuses is a `ValueError`; `granny.py` then runs its Python reader (`GrannyReference`) on
//! that input, which raises its own exception (or, where this port is stricter than it, gives its result). The
//! work runs with the interpreter released, the batch functions on all cores.

use std::collections::HashMap;

use bc2_convert::granny_edit;
use bc2_formats::granny::{Granny as Parsed, GrannyError, Object, Ref, Scalar, Text, Value};
use pyo3::exceptions::{PyIndexError, PyOverflowError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyBytesMethods, PyDict, PyList, PyString, PyTuple};

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
    g: &'a Parsed<'a>,
    strings: Vec<Option<Bound<'py, PyString>>>,
    /// Each type's `__tref__` tuple, made once (the Python reader shares one tuple per type too).
    trefs: HashMap<Ref, Bound<'py, PyTuple>>,
    keys: [Bound<'py, PyString>; 7],
}

impl<'py, 'a> Builder<'py, 'a> {
    fn new(py: Python<'py>, g: &'a Parsed<'a>) -> Self {
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

/// `struct.error(msg)`: the exception `struct.pack_into` raises.
fn struct_error(py: Python<'_>, msg: String) -> PyErr {
    match py.import("struct").and_then(|m| m.getattr("error")) {
        Ok(cls) => PyErr::from_type(cls.cast_into().unwrap_or_else(|_| py.get_type::<PyValueError>()), msg),
        Err(err) => err,
    }
}

/// The resource's bytes: a `bytes` or a `bytearray`.
fn bytes_of(data: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(b) = data.cast::<PyByteArray>() {
        return Ok(b.to_vec());
    }
    Ok(data.cast::<PyBytes>()?.as_bytes().to_vec())
}

/// A Granny resource held by the Rust core: `granny.Granny(b, where)` (round PR1b-2-rest3; the Python reader's
/// object, whose buffer and tables were Python's, is `granny.GrannyReference`, the twin). The tables are read once,
/// from the bytes as they were given; values written in place (`put`) go to the resource's own bytes, which every
/// later read (`root`, `read`, `sections`) and `to_bytes` take: a section is the resource's bytes at its place
/// (GRANNY-EDIT-2). `error` is the exception class its refusals raise (granny.py's `GrannyError`).
#[pyclass(module = "bc2_native")]
pub struct Granny {
    /// The resource as it is now.
    data: Vec<u8>,
    /// The bytes the tables were read from, kept from the first write on.
    read_from: Option<Vec<u8>>,
    /// Where it came from (the refusals' first words).
    #[pyo3(get, name = "where")]
    place: String,
    error: Py<PyAny>,
    big: bool,
    /// Where the string table starts (DICE's table is the words before it).
    #[pyo3(get)]
    strings_from: usize,
    /// Where the Granny file starts in the resource.
    #[pyo3(get)]
    gr2: usize,
    /// The root type (section, offset).
    #[pyo3(get)]
    root_type: Ref,
    /// The root object (section, offset).
    #[pyo3(get)]
    root_obj: Ref,
    /// The Granny file's size as its header gives it.
    #[pyo3(get)]
    total: u32,
    /// Where the CRC'd part starts in the Granny file.
    #[pyo3(get)]
    crc_from: u64,
    /// Each section's place in the resource and its length.
    section_at: Vec<u64>,
    section_len: Vec<usize>,
    strings: Py<PyList>,
    reloc: Option<Py<PyDict>>,
    sections: Option<Py<PyList>>,
}

impl Granny {
    /// The tables over the bytes as they are now.
    fn view(&self) -> PyResult<Parsed<'_>> {
        let mut g = Parsed::parse(self.read_from.as_deref().unwrap_or(&self.data)).map_err(|e| value_error(&e))?;
        if self.read_from.is_some() {
            g.over(&self.data);
        }
        Ok(g)
    }

    /// The `error` class raised with `<where>: <what>`.
    fn refuse(&self, py: Python<'_>, what: &str) -> PyErr {
        let msg = format!("{}: {what}", self.place);
        match self.error.bind(py).clone().cast_into() {
            Ok(cls) => PyErr::from_type(cls, msg),
            Err(_) => PyValueError::new_err(msg),
        }
    }

    fn pc_only(&self, py: Python<'_>) -> PyResult<()> {
        if self.big {
            return Err(self.refuse(
                py,
                "a big-endian (Xbox 360) resource is read only; writing is for PC files",
            ));
        }
        Ok(())
    }

    /// (section, offset) of member `name` of a read object (a dict with `__at__` and `__tref__`).
    fn field_of(&self, py: Python<'_>, g: &Parsed<'_>, obj: &Bound<'_, PyAny>, name: &str) -> PyResult<(u32, u64)> {
        let (sec, off): (u32, u64) = obj.get_item("__at__")?.extract()?;
        let tref: Ref = obj.get_item("__tref__")?.extract()?;
        let want: Option<Vec<u8>> = name.chars().map(|c| u8::try_from(u32::from(c)).ok()).collect();
        let found = match want {
            Some(w) => g.member_offset(tref, &w).map_err(|e| self.refuse(py, &e.0))?,
            None => None,
        };
        let Some(at) = found else {
            return Err(self.refuse(py, &format!("no member {name}")));
        };
        let off = off
            .checked_add(at)
            .ok_or_else(|| self.refuse(py, "an offset past 64 bits"))?;
        Ok((sec, off))
    }
}

#[pymethods]
impl Granny {
    /// Reads the resource's tables; a `ValueError` saying why where it is not one (granny.py then runs the Python
    /// reader, which raises its own exception or reads it).
    #[new]
    #[pyo3(signature = (data, r#where="file".to_owned(), error=None))]
    fn new(py: Python<'_>, data: &Bound<'_, PyAny>, r#where: String, error: Option<Py<PyAny>>) -> PyResult<Self> {
        let data = bytes_of(data)?;
        let g = py.detach(|| Parsed::parse(&data)).map_err(|e| value_error(&e))?;
        let strings = PyList::new(py, g.strings.iter().map(|s| latin1(py, s)))?.unbind();
        let section_len = g.sections.iter().map(|s| s.len()).collect();
        let (big, strings_from, gr2, root_type, root_obj, total, crc_from, section_at) = (
            g.endian.big,
            g.strings_from,
            g.gr2,
            g.root_type,
            g.root_obj,
            g.total,
            g.crc_from,
            g.section_at.clone(),
        );
        drop(g);
        Ok(Self {
            data,
            read_from: None,
            place: r#where,
            error: error.unwrap_or_else(|| py.get_type::<PyValueError>().into_any().unbind()),
            big,
            strings_from,
            gr2,
            root_type,
            root_obj,
            total,
            crc_from,
            section_at,
            section_len,
            strings,
            reloc: None,
            sections: None,
        })
    }

    /// `'<'` (PC, little-endian) or `'>'` (Xbox 360, big-endian).
    #[getter]
    fn e(&self) -> &'static str {
        if self.big { ">" } else { "<" }
    }

    /// The string table (`str`s, latin-1).
    #[getter]
    fn strings(&self, py: Python<'_>) -> Py<PyList> {
        self.strings.clone_ref(py)
    }

    /// The resource's bytes as they are now.
    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.data)
    }

    /// Each section's offset in the resource.
    #[getter]
    fn section_at(&self) -> Vec<u64> {
        self.section_at.clone()
    }

    /// Each section's bytes as they are now (`bytearray`s; made again after a write).
    #[getter]
    fn sections(&mut self, py: Python<'_>) -> PyResult<Py<PyList>> {
        if let Some(s) = &self.sections {
            return Ok(s.clone_ref(py));
        }
        let g = self.view()?;
        let s = PyList::new(py, g.sections.iter().map(|s| PyByteArray::new(py, s)))?.unbind();
        drop(g);
        self.sections = Some(s.clone_ref(py));
        Ok(s)
    }

    /// The relocations: {(section, offset): (target section, target offset)}, in the order the file lists them.
    #[getter]
    fn reloc(&mut self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        if let Some(r) = &self.reloc {
            return Ok(r.clone_ref(py));
        }
        let g = self.view()?;
        let reloc = PyDict::new(py);
        for (k, o, ts, to) in reloc_rows(&g)? {
            reloc.set_item((k, o), (ts, to))?;
        }
        drop(g);
        let r = reloc.unbind();
        self.reloc = Some(r.clone_ref(py));
        Ok(r)
    }

    /// The string of index `i`: the table's entry, else `#<i>`.
    fn name<'py>(&self, py: Python<'py>, i: i64) -> PyResult<Bound<'py, PyAny>> {
        let s = self.strings.bind(py);
        match usize::try_from(i) {
            Ok(k) if k < s.len() => s.get_item(k),
            _ => Ok(PyString::new(py, &format!("#{i}")).into_any()),
        }
    }

    /// `[(type, name, child ref or None, array size)]` of the type at `r`.
    fn members<'py>(&self, py: Python<'py>, r: Ref) -> PyResult<Bound<'py, PyList>> {
        let g = self.view()?;
        let ms = g.members(r).map_err(|e| self.refuse(py, &e.0))?;
        let mut b = Builder::new(py, &g);
        let rows: Vec<_> = ms
            .iter()
            .map(|m| (m.kind, b.text(g.name(m.name)), m.child, m.arr))
            .collect();
        PyList::new(py, rows)
    }

    /// A variant's type name.
    fn type_name<'py>(&self, py: Python<'py>, r: Ref) -> PyResult<Bound<'py, PyString>> {
        let g = self.view()?;
        Ok(latin1(py, &g.type_name(r).map_err(|e| self.refuse(py, &e.0))?))
    }

    /// The bytes of one object of the type at `r`.
    fn size(&self, py: Python<'_>, r: Ref) -> PyResult<u64> {
        self.view()?.size(r).map_err(|e| self.refuse(py, &e.0))
    }

    /// The object of type `tref` at `oref` (`None` for no object), as `root()` gives objects.
    #[pyo3(signature = (tref, oref, depth=0, limit=64))]
    fn read<'py>(
        &self,
        py: Python<'py>,
        tref: Option<Ref>,
        oref: Option<(u32, u64)>,
        depth: i64,
        limit: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let g = self.view()?;
        let v = g.read(tref, oref, depth, limit).map_err(|e| self.refuse(py, &e.0))?;
        Builder::new(py, &g).value(&v)
    }

    /// The object tree: dicts (`__at__`, `__tref__`, the members in order, `__type__` for a variant), lists,
    /// tuples, numbers and strings.
    #[pyo3(signature = (limit=64))]
    fn root<'py>(&self, py: Python<'py>, limit: i64) -> PyResult<Bound<'py, PyAny>> {
        let (data, read_from) = (&self.data[..], self.read_from.as_deref());
        let (g, tree) = py
            .detach(|| {
                let mut g = Parsed::parse(read_from.unwrap_or(data))?;
                if read_from.is_some() {
                    g.over(data);
                }
                let t = g.root(limit)?;
                Ok::<_, GrannyError>((g, t))
            })
            .map_err(|e| self.refuse(py, &e.0))?;
        Builder::new(py, &g).value(&tree)
    }

    /// (section, offset) of member `name` of a read object.
    fn field(&self, py: Python<'_>, obj: &Bound<'_, PyAny>, name: &str) -> PyResult<(u32, u64)> {
        let g = self.view()?;
        self.field_of(py, &g, obj, name)
    }

    /// (section, offset) of the elements of a reference-to-array member.
    fn array_at(&self, py: Python<'_>, obj: &Bound<'_, PyAny>, name: &str) -> PyResult<Ref> {
        let g = self.view()?;
        let (sec, off) = self.field_of(py, &g, obj, name)?;
        g.ptr(sec, off.saturating_add(4))
            .ok_or_else(|| self.refuse(py, &format!("{name} has no elements")))
    }

    /// Writes values at (section, offset) in place, packed as `struct.pack_into(fmt, ...)` packs them (a PC file:
    /// `fmt` starts with `<`); the place must lie inside the section.
    #[pyo3(signature = (at, fmt, *values))]
    fn put(&mut self, py: Python<'_>, at: (i64, i64), fmt: &str, values: &Bound<'_, PyTuple>) -> PyResult<()> {
        self.pc_only(py)?;
        let codes = struct_codes(py, fmt)?;
        let wanted = codes.iter().filter(|c| c.0 != b'x').count();
        if wanted != values.len() {
            return Err(struct_error(
                py,
                format!("pack_into expected {wanted} items for packing (got {})", values.len()),
            ));
        }
        let size: usize = codes.iter().map(|c| c.1).sum();
        let (sec, off) = at;
        let (len, sec_at) = usize::try_from(sec)
            .ok()
            .and_then(|k| Some((*self.section_len.get(k)?, *self.section_at.get(k)?)))
            .ok_or_else(|| PyIndexError::new_err("list index out of range"))?;
        let fits = usize::try_from(off)
            .ok()
            .filter(|&o| o.checked_add(size).is_some_and(|e| e <= len));
        let Some(o) = fits else {
            return Err(struct_error(
                py,
                format!(
                    "pack_into requires a buffer of at least {} bytes for packing {size} bytes at offset {off} \
                     (actual buffer size is {len})",
                    off.saturating_add(size as i64)
                ),
            ));
        };
        let mut bytes = Vec::with_capacity(size);
        let mut vals = values.iter();
        for &(c, _) in &codes {
            if c == b'x' {
                bytes.push(0);
                continue;
            }
            let v = vals.next().ok_or_else(|| struct_error(py, "too few items".into()))?;
            pack(py, c, &v, &mut bytes)?;
        }
        let pos = usize::try_from(sec_at).unwrap_or(usize::MAX).saturating_add(o);
        if self.data.get(pos..pos.saturating_add(size)).is_none() {
            return Err(struct_error(py, "a section past the resource's end".into()));
        }
        if self.read_from.is_none() {
            // the tables stay those of the bytes as they were read (GRANNY-EDIT-2)
            self.read_from = Some(self.data.clone());
        }
        if let Some(dst) = self.data.get_mut(pos..pos + size) {
            dst.copy_from_slice(&bytes);
        }
        self.sections = None;
        Ok(())
    }

    /// The resource with its Granny CRC recomputed (written into the resource too).
    #[expect(
        clippy::wrong_self_convention,
        reason = "granny.py's method name; it writes the CRC into the resource as the Python did"
    )]
    fn to_bytes<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.pc_only(py)?;
        let crc_from = usize::try_from(self.crc_from).unwrap_or(usize::MAX);
        let out = granny_edit::with_crc(&self.data, self.gr2, crc_from, self.total as usize)
            .ok_or_else(|| struct_error(py, "pack_into requires a buffer of at least 4 bytes for the CRC".into()))?;
        if self.read_from.is_none() {
            self.read_from = Some(self.data.clone());
        }
        self.data.clone_from(&out);
        self.sections = None;
        Ok(PyBytes::new(py, &out))
    }
}

/// The codes of a `put` format: (code, size) each, a count repeating one; `<` first (a PC file's values are
/// little-endian, GRANNY-EDIT-2).
fn struct_codes(py: Python<'_>, fmt: &str) -> PyResult<Vec<(u8, usize)>> {
    let Some(rest) = fmt.strip_prefix('<') else {
        return Err(struct_error(
            py,
            format!("{fmt:?}: a PC Granny file's values are written little-endian ('<' first)"),
        ));
    };
    let mut out = Vec::new();
    let mut count: Option<usize> = None;
    for c in rest.bytes() {
        if c.is_ascii_whitespace() {
            continue;
        }
        if c.is_ascii_digit() {
            let n = count
                .unwrap_or(0)
                .checked_mul(10)
                .and_then(|n| n.checked_add(usize::from(c - b'0')));
            count = Some(n.ok_or_else(|| struct_error(py, "total struct size too long".into()))?);
            continue;
        }
        let size = match c {
            b'x' | b'b' | b'B' | b'?' => 1,
            b'h' | b'H' => 2,
            b'i' | b'I' | b'l' | b'L' | b'f' => 4,
            b'q' | b'Q' | b'd' => 8,
            _ => {
                return Err(struct_error(
                    py,
                    format!(
                        "format character {:?} is not one a Granny value is written with",
                        char::from(c)
                    ),
                ));
            }
        };
        let n = count.take().unwrap_or(1);
        if n > 1 << 20 {
            return Err(struct_error(py, "total struct size too long".into()));
        }
        out.extend(std::iter::repeat_n((c, size), n));
    }
    if count.is_some() {
        return Err(struct_error(py, "repeat count given without format specifier".into()));
    }
    Ok(out)
}

/// One value packed little-endian as `struct` packs it.
fn pack(py: Python<'_>, c: u8, v: &Bound<'_, PyAny>, out: &mut Vec<u8>) -> PyResult<()> {
    let int = |lo: i128, hi: i128| -> PyResult<i128> {
        let n: i128 = v
            .extract()
            .map_err(|_| struct_error(py, "required argument is not an integer".into()))?;
        if n < lo || n > hi {
            return Err(struct_error(
                py,
                format!("'{}' format requires {lo} <= number <= {hi}", char::from(c)),
            ));
        }
        Ok(n)
    };
    let float = || -> PyResult<f64> {
        v.extract::<f64>()
            .map_err(|_| struct_error(py, "required argument is not a float".into()))
    };
    match c {
        b'?' => out.push(u8::from(v.is_truthy()?)),
        b'b' => out.extend((int(-128, 127)? as i8).to_le_bytes()),
        b'B' => out.extend((int(0, 255)? as u8).to_le_bytes()),
        b'h' => out.extend((int(-32_768, 32_767)? as i16).to_le_bytes()),
        b'H' => out.extend((int(0, 65_535)? as u16).to_le_bytes()),
        b'i' | b'l' => out.extend((int(i128::from(i32::MIN), i128::from(i32::MAX))? as i32).to_le_bytes()),
        b'I' | b'L' => out.extend((int(0, i128::from(u32::MAX))? as u32).to_le_bytes()),
        b'q' => out.extend((int(i128::from(i64::MIN), i128::from(i64::MAX))? as i64).to_le_bytes()),
        b'Q' => out.extend((int(0, i128::from(u64::MAX))? as u64).to_le_bytes()),
        b'f' => {
            let x = float()?;
            #[expect(clippy::cast_possible_truncation, reason = "PyFloat_Pack4: the float's nearest f32")]
            let y = x as f32;
            if y.is_infinite() && x.is_finite() {
                return Err(PyOverflowError::new_err("float too large to pack with f format"));
            }
            out.extend(y.to_le_bytes());
        }
        b'd' => out.extend(float()?.to_le_bytes()),
        _ => return Err(struct_error(py, "bad char in struct format".into())),
    }
    Ok(())
}

/// The relocations in the order the file lists them (section by section).
fn reloc_rows(g: &Parsed<'_>) -> PyResult<Vec<(u32, u32, u32, u32)>> {
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
        .detach(|| Parsed::parse(b).and_then(|g| g.root(limit).map(|t| (g, t))))
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
    let out = PyList::empty(py);
    crate::window::map(
        py,
        &slices,
        |b| b.len(),
        |b| Parsed::parse(b).and_then(|g| g.root(limit).map(|t| (g, t))),
        |r| match r {
            Ok((g, t)) => out.append(Builder::new(py, &g).value(&t)?),
            Err(e) => out.append(value_error(&e).into_value(py)),
        },
    )?;
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
    let out = PyList::empty(py);
    crate::window::map(
        py,
        &jobs,
        |(b, _)| b.len(),
        |(b, m)| bc2_convert::granny::to_pc(b, *m),
        |r| match r {
            Ok(b) => out.append(PyBytes::new(py, &b)),
            Err(e) => out.append(value_error(&e).into_value(py)),
        },
    )?;
    Ok(out)
}
