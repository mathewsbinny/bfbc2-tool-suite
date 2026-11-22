//! The writers: `dbx.write` of a Python tree and FbrbTool's `pack` of a folder, through `bc2-formats`.
//!
//! `dbx_write` walks the Python tree `dbx.write` is given (its `Element` objects, duck-typed: `tag`, `kind`,
//! `attributes`, `text`, `raw`, `children`) straight into [`dbx::Writer`], with no Rust tree in between, reading
//! each element's fields in the order the retired Python writer read them, and writes the bytes it wrote (round F1a,
//! 2026-11-18: `dbx.write` has no other writer). It takes strings that are `str` (or a subclass), attributes as a
//! list or tuple of 2-tuples (or 2-lists) of `str`, children as a list, a tuple, `None` or any other iterable, a
//! value's bytes as anything `bytearray.extend` takes (`bytes`, `bytearray`, a buffer, an iterable of ints), and any
//! nesting depth (the walk keeps its own stack). What cannot be written raises with the reason: an empty tag (string
//! 0 reads back as the end of a container; the Python writer wrote it), more than 15 attributes, a kind outside
//! 0-15, an attribute that is not a (name, value) pair, a tag, attribute or text that is not a `str`.
//!
//! A parsed tree shares one `str` object per string of its file, so each string object's id is kept by the
//! object's address (the object held for the call, so the address cannot be reused): its text is read and hashed
//! once per object, not once per use.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::path::Path;

use bc2_formats::dbx::{self, CONTAINER, TEXT};
use bc2_formats::fbrb::{PackError, PackStats};
use pyo3::exceptions::{PyOSError, PyTypeError, PyValueError};
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyList, PyString, PyTuple};

/// A hash of an object's address: a multiply, its high half folded down (addresses are 16-byte aligned).
#[derive(Default, Clone, Copy)]
struct AddrHash(u64);

impl Hasher for AddrHash {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(8) ^ u64::from(b)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }

    fn write_usize(&mut self, v: usize) {
        let h = (v as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.0 = h ^ (h >> 32);
    }
}

/// The writer and the ids of the string objects met so far.
struct Out<'py> {
    w: dbx::Writer,
    ids: HashMap<usize, u32, BuildHasherDefault<AddrHash>>,
    held: Vec<Bound<'py, PyAny>>,
}

impl<'py> Out<'py> {
    /// The id of a `str` object (`what` names it in the `TypeError` for anything else).
    fn id(&mut self, o: Bound<'py, PyAny>, what: &str) -> PyResult<u32> {
        let key = o.as_ptr() as usize;
        if let Some(&i) = self.ids.get(&key) {
            return Ok(i);
        }
        let s = o
            .cast::<PyString>()
            .map_err(|_| {
                let t = o.get_type().name().map_or_else(|_| "?".to_owned(), |n| n.to_string());
                PyTypeError::new_err(format!("a DBX element's {what} must be a str, not {t}"))
            })?
            .to_str()?;
        let i = self.w.id(s);
        self.ids.insert(key, i);
        self.held.push(o);
        Ok(i)
    }
}

/// A list or tuple without copying its items, or the items of any other iterable (a generator, a string: what the
/// Python writer's `for` loops took).
enum Seq<'py> {
    List(Bound<'py, PyList>),
    Tuple(Bound<'py, PyTuple>),
    Items(Vec<Bound<'py, PyAny>>),
}

impl<'py> Seq<'py> {
    /// A list or tuple only (`what` names it in the `TypeError` for anything else).
    fn of(o: Bound<'py, PyAny>, what: &str) -> PyResult<Self> {
        match o.cast_into::<PyList>() {
            Ok(l) => Ok(Self::List(l)),
            Err(e) => match e.into_inner().cast_into::<PyTuple>() {
                Ok(t) => Ok(Self::Tuple(t)),
                Err(e) => {
                    let o = e.into_inner();
                    let t = o.get_type().name().map_or_else(|_| "?".to_owned(), |n| n.to_string());
                    Err(PyTypeError::new_err(format!("{what} must be a list or tuple, not {t}")))
                }
            },
        }
    }

    /// A list or tuple as it is, any other iterable's items (its error when it is not iterable).
    fn iterable(o: Bound<'py, PyAny>) -> PyResult<Self> {
        if o.is_instance_of::<PyList>() || o.is_instance_of::<PyTuple>() {
            return Self::of(o, "");
        }
        Ok(Self::Items(o.try_iter()?.collect::<PyResult<Vec<_>>>()?))
    }

    fn len(&self) -> usize {
        match self {
            Self::List(l) => l.len(),
            Self::Tuple(t) => t.len(),
            Self::Items(v) => v.len(),
        }
    }

    fn get(&self, i: usize) -> PyResult<Bound<'py, PyAny>> {
        match self {
            Self::List(l) => l.get_item(i),
            Self::Tuple(t) => t.get_item(i),
            Self::Items(v) => v
                .get(i)
                .cloned()
                .ok_or_else(|| PyValueError::new_err("children changed while being written")),
        }
    }
}

/// An element's head, attributes and value; for a container, its children (`None`: no children) after the open
/// byte, which the caller walks and closes.
fn emit_one<'py>(out: &mut Out<'py>, e: &Bound<'py, PyAny>) -> PyResult<Option<Seq<'py>>> {
    let py = e.py();
    let attrs = Seq::of(e.getattr(intern!(py, "attributes"))?, "a DBX element's attributes")?;
    let n = attrs.len();
    // as the Python writer: the attribute count, then the tag's string, then the kind
    if n > 15 {
        return Err(PyValueError::new_err(format!("{n} attributes (15 at most)")));
    }
    let tag = out.id(e.getattr(intern!(py, "tag"))?, "tag")?;
    let kind_value = e.getattr(intern!(py, "kind"))?;
    let kind = match kind_value.extract::<i64>()? {
        k @ 0..=15 => u8::try_from(k).unwrap_or_default(),
        k => return Err(PyValueError::new_err(format!("kind {k} does not fit 4 bits (0-15)"))),
    };
    out.w
        .element_id(tag, kind, n)
        .map_err(|err| PyValueError::new_err(err.to_string()))?;
    for i in 0..n {
        let pair = Seq::of(attrs.get(i)?, "an attribute")?;
        if pair.len() != 2 {
            return Err(PyValueError::new_err(format!(
                "an attribute must be a (name, value) pair, not {} items",
                pair.len()
            )));
        }
        let k = out.id(pair.get(0)?, "attribute name")?;
        let v = out.id(pair.get(1)?, "attribute value")?;
        out.w.attribute_ids(k, v);
    }
    match kind {
        CONTAINER => {
            out.w.open();
            let children = e.getattr(intern!(py, "children"))?;
            return if children.is_none() {
                Ok(Some(Seq::Items(Vec::new())))
            } else {
                Seq::iterable(children).map(Some)
            };
        }
        TEXT => {
            let t = out.id(e.getattr(intern!(py, "text"))?, "text")?;
            out.w.text_id(t);
        }
        _ => {
            let raw = e.getattr(intern!(py, "raw"))?;
            if let Ok(b) = raw.cast::<PyBytes>() {
                out.w.raw(b.as_bytes());
            } else if let Ok(b) = raw.cast::<PyByteArray>() {
                out.w.raw(&b.to_vec());
            } else {
                // what bytearray.extend takes (a buffer, an iterable of ints), with its error otherwise
                let b = PyByteArray::new(py, &[]);
                b.call_method1(intern!(py, "extend"), (raw,))?;
                out.w.raw(&b.to_vec());
            }
        }
    }
    Ok(None)
}

/// An element and everything below it, depth first, with the walk's own stack (any nesting depth).
fn emit<'py>(out: &mut Out<'py>, root: &Bound<'py, PyAny>) -> PyResult<()> {
    let mut stack: Vec<(Seq<'py>, usize)> = Vec::new();
    if let Some(kids) = emit_one(out, root)? {
        stack.push((kids, 0));
    }
    while let Some((kids, i)) = stack.last_mut() {
        if *i < kids.len() {
            let c = kids.get(*i)?;
            *i += 1;
            if let Some(k) = emit_one(out, &c)? {
                stack.push((k, 0));
            }
        } else {
            out.w.close();
            stack.pop();
        }
    }
    Ok(())
}

/// `dbx.write(f)`: `f` (a `DbxFile`: its `roots` are written) as a binary `.dbx`, the bytes the retired Python writer
/// wrote, or the reason it cannot be written (module doc).
#[pyfunction]
pub fn dbx_write<'py>(py: Python<'py>, file: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    let mut out = Out {
        w: dbx::Writer::new(),
        ids: HashMap::default(),
        held: Vec::new(),
    };
    let roots = Seq::iterable(file.getattr(intern!(py, "roots"))?)?;
    for i in 0..roots.len() {
        emit(&mut out, &roots.get(i)?)?;
    }
    let bytes = out.w.finish().map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(PyBytes::new(py, &bytes))
}

/// A pack error as Python's: `OSError` for a file that cannot be read or written, else `ValueError`.
fn pack_error(e: &PackError) -> PyErr {
    match e {
        PackError::Io { .. } => PyOSError::new_err(e.to_string()),
        _ => PyValueError::new_err(e.to_string()),
    }
}

fn stats(s: PackStats) -> (usize, u64) {
    (s.entries, s.payload_len)
}

/// FbrbTool's `pack <folder> <out>` through Rust: `(entries, payload bytes)`. Runs with the interpreter released;
/// streamed into the file (round S1: the payload is never held).
#[pyfunction]
pub fn fbrb_pack(py: Python<'_>, folder: &str, out: &str) -> PyResult<(usize, u64)> {
    py.detach(|| bc2_gamefs::bundles::pack_path(Path::new(folder), Path::new(out)))
        .map(stats)
        .map_err(|e| pack_error(&e))
}

/// FbrbTool's `unpack <bundle> <out>` through Rust (`bc2_tools::unpack_path`): `(entries, payload bytes,
/// *deleted* markers)`. A bundle FbrbTool refused raises `ValueError` (with the files it left behind), a file that
/// cannot be read or written `OSError`. Runs with the interpreter released.
#[pyfunction]
pub fn fbrb_unpack(py: Python<'_>, bundle: &str, out: &str) -> PyResult<(usize, u64, usize)> {
    py.detach(|| bc2_tools::unpack_path(Path::new(bundle), Path::new(out)))
        .map(|s| (s.entries, s.payload_len, s.deleted))
        .map_err(|e| match e {
            bc2_tools::UnpackError::Io { .. } => PyOSError::new_err(e.to_string()),
            _ => PyValueError::new_err(e.to_string()),
        })
}

/// `fbrb_pack` of many `(folder, out)` pairs, one bundle per core. Each result is `(entries, payload bytes)` or the
/// exception it raised (returned, not raised).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn fbrb_pack_many(py: Python<'_>, items: Vec<(String, String)>) -> PyResult<Bound<'_, PyList>> {
    let results = py.detach(|| bc2_gamefs::bundles::pack_many(&items));
    let out = PyList::empty(py);
    for r in results {
        match r {
            Ok(s) => out.append(stats(s))?,
            Err(e) => out.append(pack_error(&e).into_value(py))?,
        }
    }
    Ok(out)
}

/// `bc2_formats::zlib::deflate`: zlib's raw deflate at `level` (4-9: the lazy levels; `ValueError` for another),
/// the bytes Python's `zlib.compressobj(level, DEFLATED, -15, 8)` writes. Runs with the interpreter released.
#[pyfunction]
pub fn zlib_deflate<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>, level: u8) -> PyResult<Bound<'py, PyBytes>> {
    let bytes = data.as_bytes();
    let z = py
        .detach(|| bc2_formats::zlib::deflate(bytes, level))
        .ok_or_else(|| PyValueError::new_err(format!("zlib level {level}: only the lazy levels 4-9 are ported")))?;
    Ok(PyBytes::new(py, &z))
}
