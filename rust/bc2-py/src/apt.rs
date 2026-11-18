//! Bindings for APT movies (`bfbc2-console-readers/apt/apt.py`, round E3n): `apt_constants` (`Movie.__init__`'s
//! constant list), `apt_decode` / `apt_decode_many` (`Movie.dump()`'s lines and `Movie.instructions()`' records).
//!
//! A movie the port refuses gives `None` (not an exception): `apt.py` then runs its own code on it, which gives the
//! reference's answer or raises the reference's error. The work runs with the interpreter released, the batch
//! function on all cores.
//!
//! With records, the result is `(lines, columns)`: `lines` an [`AptLines`] (the dump's text kept in Rust until
//! `Movie.dump_lines` is read: most callers of `instructions()` never read it) and `columns` 15 lists, one value per
//! record each: the 12 fields of `apt.Ins` in order (`at, op, name, arg, end, target, plist, items, func, index,
//! unknown, pool`), then `start, limit, depth`. `apt.py` zips them into `(Ins, start, limit, depth)` records in C
//! (`tuple.__new__(Ins, fields)`), which costs far less than building a tuple per record here. `func` is already
//! an instance of the `Func` class it passes.

use bc2_formats::apt::{self, Decoded, MovieRef, Record};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyBytesMethods, PyList, PyString};

/// `Movie.__init__`'s constants of a `.const` file, or `None` where the Python reader raises.
#[pyfunction]
pub fn apt_constants<'py>(py: Python<'py>, c: &Bound<'py, PyBytes>, big: bool) -> PyResult<Option<Bound<'py, PyList>>> {
    let b = c.as_bytes();
    match py.detach(|| apt::constants(b, big)) {
        Ok(v) => Ok(Some(PyList::new(py, v)?)),
        Err(_) => Ok(None),
    }
}

/// A movie's dump lines, held in Rust until asked for.
#[pyclass(frozen, module = "bc2_native")]
pub struct AptLines {
    lines: Vec<String>,
}

#[pymethods]
impl AptLines {
    /// The lines as a new list of `str`.
    fn list<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        PyList::new(py, &self.lines)
    }

    fn __len__(&self) -> usize {
        self.lines.len()
    }
}

/// A list of one value per record.
fn column<'py, T, F>(py: Python<'py>, recs: &[Record], f: F) -> PyResult<Bound<'py, PyList>>
where
    F: Fn(&Record) -> T,
    T: IntoPyObject<'py>,
{
    PyList::new(py, recs.iter().map(f))
}

/// The 15 columns of the records (module text).
fn columns<'py>(
    py: Python<'py>,
    recs: &[Record],
    func: &Bound<'py, PyAny>,
    names: &mut [Option<Bound<'py, PyString>>],
) -> PyResult<Bound<'py, PyAny>> {
    let empty = PyString::new(py, "");
    let mut name_col = Vec::with_capacity(recs.len());
    let mut arg_col = Vec::with_capacity(recs.len());
    let mut func_col = Vec::with_capacity(recs.len());
    for r in recs {
        let op = usize::from(r.ins.op);
        let name = match names.get_mut(op) {
            Some(Some(s)) => s.clone(),
            Some(slot) => {
                let s = PyString::new(py, &apt::display_name(r.ins.op));
                *slot = Some(s.clone());
                s
            }
            None => PyString::new(py, &apt::display_name(r.ins.op)),
        };
        name_col.push(name);
        arg_col.push(if r.ins.arg.is_empty() {
            empty.clone()
        } else {
            PyString::new(py, &r.ins.arg)
        });
        func_col.push(match &r.ins.func {
            Some(f) => Some(func.call1((&f.name, f.nparams, &f.extra, f.size, f.body))?),
            None => None,
        });
    }
    let items = PyList::empty(py);
    for r in recs {
        match &r.ins.items {
            Some(v) => items.append(PyList::new(py, v)?)?,
            None => items.append(py.None())?,
        }
    }
    let cols: [Bound<'py, PyAny>; 15] = [
        column(py, recs, |r| r.ins.at)?.into_any(),
        column(py, recs, |r| r.ins.op)?.into_any(),
        PyList::new(py, name_col)?.into_any(),
        PyList::new(py, arg_col)?.into_any(),
        column(py, recs, |r| r.ins.end)?.into_any(),
        column(py, recs, |r| r.ins.target)?.into_any(),
        column(py, recs, |r| r.ins.plist)?.into_any(),
        items.into_any(),
        PyList::new(py, func_col)?.into_any(),
        column(py, recs, |r| r.ins.index)?.into_any(),
        column(py, recs, |r| PyBool::new(py, r.ins.unknown).to_owned())?.into_any(),
        column(py, recs, |r| r.ins.pool)?.into_any(),
        column(py, recs, |r| r.start)?.into_any(),
        column(py, recs, |r| r.limit)?.into_any(),
        column(py, recs, |r| r.depth)?.into_any(),
    ];
    Ok(pyo3::types::PyTuple::new(py, cols)?.into_any())
}

/// The Python value of one decoded movie: its lines (a list) or `(AptLines, columns)`.
fn build<'py>(
    py: Python<'py>,
    d: Decoded,
    records: bool,
    func: &Bound<'py, PyAny>,
    names: &mut [Option<Bound<'py, PyString>>],
) -> PyResult<Bound<'py, PyAny>> {
    if !records {
        return Ok(PyList::new(py, &d.lines)?.into_any());
    }
    let cols = columns(py, &d.records, func, names)?;
    let lines = Bound::new(py, AptLines { lines: d.lines })?;
    Ok((lines, cols).into_pyobject(py)?.into_any())
}

/// `Movie.dump()` (records false: the lines) or `Movie.instructions()` (records true: `(AptLines, columns)`) of
/// one movie, or `None` for a movie the port hands to the Python reader.
#[pyfunction]
#[pyo3(signature = (name, data, big, movie, consts, records, func))]
#[expect(clippy::too_many_arguments, reason = "the fields apt.Movie keeps")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn apt_decode<'py>(
    py: Python<'py>,
    name: &str,
    data: &Bound<'py, PyBytes>,
    big: bool,
    movie: i64,
    consts: Vec<String>,
    records: bool,
    func: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let m = MovieRef {
        name,
        apt: data.as_bytes(),
        big,
        movie,
        consts: &consts,
    };
    match py.detach(|| apt::decode(m, records)) {
        Ok(d) => build(py, d, records, func, &mut vec![None; 256]).map(Some),
        Err(_) => Ok(None),
    }
}

/// One movie of `apt_decode_many`: `(name, data, big, movie, consts)`.
type MovieArg<'py> = (String, Bound<'py, PyBytes>, bool, i64, Vec<String>);

/// `apt_decode` of many movies `(name, data, big, movie, consts)`: decoded on all cores, built in order; `None`
/// for each movie the port hands to the Python reader.
#[pyfunction]
#[pyo3(signature = (movies, records, func))]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn apt_decode_many<'py>(
    py: Python<'py>,
    movies: Vec<MovieArg<'py>>,
    records: bool,
    func: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyList>> {
    let refs: Vec<MovieRef<'_>> = movies
        .iter()
        .map(|(name, data, big, movie, consts)| MovieRef {
            name,
            apt: data.as_bytes(),
            big: *big,
            movie: *movie,
            consts,
        })
        .collect();
    let decoded = py.detach(|| apt::decode_many(&refs, records));
    let mut names = vec![None; 256];
    let out = PyList::empty(py);
    for d in decoded {
        match d {
            Ok(d) => out.append(build(py, d, records, func, &mut names)?)?,
            Err(_) => out.append(py.None())?,
        }
    }
    Ok(out)
}
