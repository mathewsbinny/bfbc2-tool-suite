//! The writers: `dbx.write` of a Python tree and FbrbTool's `pack` of a folder, through `bc2-formats`.
//!
//! `dbx_write` walks the Python tree `dbx.write` is given (its `Element` objects, duck-typed: `tag`, `kind`,
//! `attributes`, `text`, `raw`, `children`) straight into [`dbx::Writer`], with no Rust tree in between, reading
//! each element's fields in the order the Python writer reads them. It accepts only what the Python writer writes
//! the same way: strings that are `str`, attributes as a list or tuple of 2-tuples (or 2-lists) of `str`, children
//! as a list, tuple or `None`, a value's bytes as `bytes` or `bytearray`, a nesting depth up to [`MAX_DEPTH`].
//! Anything else (and every tree the Python writer refuses or writes differently: an empty tag, more than 15
//! attributes, a kind outside 0-15) raises here, and `dbx.write` then runs the Python writer, which gives its own
//! bytes or error: the result is the Python writer's in every case.
//!
//! A parsed tree shares one `str` object per string of its file, so each string object's id is kept by the
//! object's address (the object held for the call, so the address cannot be reused): its text is read and hashed
//! once per object, not once per use.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::path::Path;

use bc2_formats::dbx::{self, CONTAINER, TEXT};
use bc2_formats::fbrb::{self, PackError, PackStats};
use pyo3::exceptions::{PyOSError, PyTypeError, PyValueError};
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyList, PyString, PyTuple};

/// The deepest nesting `dbx_write` walks; past it the Python writer decides (its recursion limit, 1,000 frames by
/// default, is the same order; game files nest about 10 deep).
pub const MAX_DEPTH: usize = 500;

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
    /// The id of a `str` object (a `TypeError` for anything else, left to the Python writer).
    fn id(&mut self, o: Bound<'py, PyAny>) -> PyResult<u32> {
        let key = o.as_ptr() as usize;
        if let Some(&i) = self.ids.get(&key) {
            return Ok(i);
        }
        let s = o
            .cast::<PyString>()
            .map_err(|_| PyTypeError::new_err("not a str"))?
            .to_str()?;
        let i = self.w.id(s);
        self.ids.insert(key, i);
        self.held.push(o);
        Ok(i)
    }
}

/// A list or tuple (the shapes the Python writer's loops see the same way), without copying its items.
enum Seq<'py> {
    List(Bound<'py, PyList>),
    Tuple(Bound<'py, PyTuple>),
}

impl<'py> Seq<'py> {
    fn of(o: Bound<'py, PyAny>) -> PyResult<Self> {
        match o.cast_into::<PyList>() {
            Ok(l) => Ok(Self::List(l)),
            Err(e) => match e.into_inner().cast_into::<PyTuple>() {
                Ok(t) => Ok(Self::Tuple(t)),
                Err(_) => Err(PyTypeError::new_err("not a list or tuple")),
            },
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::List(l) => l.len(),
            Self::Tuple(t) => t.len(),
        }
    }

    fn get(&self, i: usize) -> PyResult<Bound<'py, PyAny>> {
        match self {
            Self::List(l) => l.get_item(i),
            Self::Tuple(t) => t.get_item(i),
        }
    }
}

fn emit<'py>(out: &mut Out<'py>, e: &Bound<'py, PyAny>, depth: usize) -> PyResult<()> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err("nested too deep for the Rust writer"));
    }
    let py = e.py();
    let attrs = Seq::of(e.getattr(intern!(py, "attributes"))?)?;
    let n = attrs.len();
    // as the Python writer: the attribute count, then the tag's string, then the kind
    if n > 15 {
        return Err(PyValueError::new_err("more than 15 attributes"));
    }
    let tag = out.id(e.getattr(intern!(py, "tag"))?)?;
    let kind: u8 = e.getattr(intern!(py, "kind"))?.extract()?;
    out.w
        .element_id(tag, kind, n)
        .map_err(|err| PyValueError::new_err(err.to_string()))?;
    for i in 0..n {
        let pair = Seq::of(attrs.get(i)?)?;
        if pair.len() != 2 {
            return Err(PyTypeError::new_err("an attribute that is not a pair"));
        }
        let k = out.id(pair.get(0)?)?;
        let v = out.id(pair.get(1)?)?;
        out.w.attribute_ids(k, v);
    }
    match kind {
        CONTAINER => {
            out.w.open();
            let children = e.getattr(intern!(py, "children"))?;
            if !children.is_none() {
                let c = Seq::of(children)?;
                for i in 0..c.len() {
                    emit(out, &c.get(i)?, depth + 1)?;
                }
            }
            out.w.close();
        }
        TEXT => {
            let t = out.id(e.getattr(intern!(py, "text"))?)?;
            out.w.text_id(t);
        }
        _ => {
            let raw = e.getattr(intern!(py, "raw"))?;
            if let Ok(b) = raw.cast::<PyBytes>() {
                out.w.raw(b.as_bytes());
            } else if let Ok(b) = raw.cast::<PyByteArray>() {
                out.w.raw(&b.to_vec());
            } else {
                return Err(PyTypeError::new_err("a value that is not bytes"));
            }
        }
    }
    Ok(())
}

/// `dbx.write(f)` through Rust: the bytes of the Python writer for `f` (a `DbxFile`: its `roots` are written), or
/// an exception for a tree this function leaves to the Python writer (module doc).
#[pyfunction]
pub fn dbx_write<'py>(py: Python<'py>, file: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    let mut out = Out {
        w: dbx::Writer::new(),
        ids: HashMap::default(),
        held: Vec::new(),
    };
    let roots = Seq::of(file.getattr(intern!(py, "roots"))?)?;
    for i in 0..roots.len() {
        emit(&mut out, &roots.get(i)?, 0)?;
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

/// FbrbTool's `pack <folder> <out>` through Rust: `(entries, payload bytes)`. Runs with the interpreter released.
#[pyfunction]
pub fn fbrb_pack(py: Python<'_>, folder: &str, out: &str) -> PyResult<(usize, u64)> {
    py.detach(|| fbrb::pack_path(Path::new(folder), Path::new(out)))
        .map(stats)
        .map_err(|e| pack_error(&e))
}

/// FbrbTool's `unpack <bundle> <out>` through Rust (`bc2_formats::fbrb::unpack_path`): `(entries, payload bytes,
/// *deleted* markers)`. A bundle the port leaves to FbrbTool.exe raises `ValueError` (nothing written), a file that
/// cannot be read or written `OSError`. Runs with the interpreter released.
#[pyfunction]
pub fn fbrb_unpack(py: Python<'_>, bundle: &str, out: &str) -> PyResult<(usize, u64, usize)> {
    py.detach(|| fbrb::unpack_path(Path::new(bundle), Path::new(out)))
        .map(|s| (s.entries, s.payload_len, s.deleted))
        .map_err(|e| match e {
            fbrb::UnpackError::Io { .. } => PyOSError::new_err(e.to_string()),
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
    let results = py.detach(|| fbrb::pack_many(&items));
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
