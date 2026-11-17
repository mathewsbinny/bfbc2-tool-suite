//! The bindings of the shader database reader and writer (`bfbc2-shader-tools/shaderdb.py` calls them when the
//! module is built): `bc2_formats::shaderdb`.
//!
//! The reader gives `ShaderDb._parse`'s sections as Python lists of the same objects (records as `bytes`, shaders
//! through the caller's `Shader` class, names as `str` decoded latin-1, groups as `(bytes, [int])`). The writer
//! takes the sections back; anything the Python writer would treat differently (an item that is not `bytes`, a name
//! that is not latin-1, a number out of its field's range) makes it return `None`, and `shaderdb.py` then writes
//! with its own code, which raises what it raises.

use std::borrow::Cow;

use bc2_formats::shaderdb::{self, Named, Shader, ShaderDb};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyBytesMethods, PyList, PyString, PyTuple};

/// A latin-1 name as Python's `bytes.decode('latin-1')`.
fn latin1<'py>(py: Python<'py>, b: &[u8]) -> Bound<'py, PyString> {
    match std::str::from_utf8(b) {
        Ok(s) if b.is_ascii() => PyString::new(py, s),
        _ => PyString::new(py, &b.iter().map(|&c| char::from(c)).collect::<String>()),
    }
}

fn records<'py>(py: Python<'py>, list: &[&[u8]]) -> PyResult<Bound<'py, PyList>> {
    PyList::new(py, list.iter().map(|r| PyBytes::new(py, r)))
}

fn shaders<'py>(py: Python<'py>, list: &[Shader<'_>], cls: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyList>> {
    let out = PyList::empty(py);
    let empty = PyBytes::new(py, b"");
    for s in list {
        let tail = if s.tail.is_empty() {
            empty.clone()
        } else {
            PyBytes::new(py, s.tail)
        };
        out.append(cls.call1((
            PyBytes::new(py, s.guid),
            PyBytes::new(py, s.code),
            s.param,
            s.binding,
            s.regs,
            tail,
        ))?)?;
    }
    Ok(out)
}

fn names<'py>(py: Python<'py>, list: &[Named<'_>]) -> PyResult<Bound<'py, PyList>> {
    PyList::new(
        py,
        list.iter()
            .map(|n| {
                PyTuple::new(
                    py,
                    [latin1(py, &n.name).into_any(), PyBytes::new(py, n.guid).into_any()],
                )
            })
            .collect::<PyResult<Vec<_>>>()?,
    )
}

/// `ShaderDb._parse(data, dx, where, big_endian)` through Rust: `(version, flag, params, bindings, regs, vs, ps,
/// programs, keys, layouts, entries, fragments, parts, groups)`, the shaders made with `shader_cls(guid, code, param,
/// binding, regs, tail)`. A file this layout does not read is a `ValueError` with the Python reader's message
/// (without its `where: ` prefix).
#[pyfunction]
pub fn shaderdb_parse<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyBytes>,
    dx: u32,
    big_endian: bool,
    shader_cls: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyTuple>> {
    let bytes = data.as_bytes();
    let db = py
        .detach(|| shaderdb::parse(bytes, dx, big_endian))
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    let groups = PyList::empty(py);
    for (g, idx) in &db.groups {
        groups.append((PyBytes::new(py, g), PyList::new(py, idx)?))?;
    }
    PyTuple::new(
        py,
        [
            db.version.into_pyobject(py)?.into_any(),
            db.flag.into_pyobject(py)?.into_any(),
            records(py, &db.params)?.into_any(),
            records(py, &db.bindings)?.into_any(),
            records(py, &db.regs)?.into_any(),
            shaders(py, &db.vs, shader_cls)?.into_any(),
            shaders(py, &db.ps, shader_cls)?.into_any(),
            records(py, &db.programs)?.into_any(),
            records(py, &db.keys)?.into_any(),
            records(py, &db.layouts)?.into_any(),
            names(py, &db.entries)?.into_any(),
            names(py, &db.fragments)?.into_any(),
            names(py, &db.parts)?.into_any(),
            groups.into_any(),
        ],
    )
}

/// The `bytes` items of a list, or `None` when one is not `bytes`.
fn bytes_items<'py>(list: &Bound<'py, PyAny>) -> Option<Vec<Bound<'py, PyBytes>>> {
    list.try_iter()
        .ok()?
        .map(|x| x.ok()?.cast_into::<PyBytes>().ok())
        .collect()
}

/// A shader's fields: `(guid, code, param, binding, regs, tail)`, or `None`.
type ShaderFields<'py> = (
    Bound<'py, PyBytes>,
    Bound<'py, PyBytes>,
    u32,
    u32,
    u32,
    Bound<'py, PyBytes>,
);

fn shader_items<'py>(list: &Bound<'py, PyAny>) -> Option<Vec<ShaderFields<'py>>> {
    let mut out = Vec::new();
    for s in list.try_iter().ok()? {
        let s = s.ok()?;
        let get = |n: &str| s.getattr(n).ok();
        out.push((
            get("guid")?.cast_into().ok()?,
            get("code")?.cast_into().ok()?,
            get("param")?.extract().ok()?,
            get("binding")?.extract().ok()?,
            get("regs")?.extract().ok()?,
            get("tail")?.cast_into().ok()?,
        ));
    }
    Some(out)
}

/// `(name, guid)` items: the name a `str` all of whose characters are latin-1, the GUID `bytes`.
fn name_items<'py>(list: &Bound<'py, PyAny>) -> Option<Vec<(Bound<'py, PyString>, Bound<'py, PyBytes>)>> {
    let mut out = Vec::new();
    for x in list.try_iter().ok()? {
        let (n, g): (Bound<'py, PyAny>, Bound<'py, PyAny>) = x.ok()?.extract().ok()?;
        out.push((n.cast_into().ok()?, g.cast_into().ok()?));
    }
    Some(out)
}

fn slices<'a>(l: &'a [Bound<'_, PyBytes>]) -> Vec<&'a [u8]> {
    l.iter().map(PyBytesMethods::as_bytes).collect()
}

fn shader<'a>(s: &'a ShaderFields<'_>) -> Shader<'a> {
    Shader {
        guid: s.0.as_bytes(),
        code: s.1.as_bytes(),
        param: s.2,
        binding: s.3,
        regs: s.4,
        tail: s.5.as_bytes(),
    }
}

fn named<'a>(l: &'a [(Bound<'_, PyString>, Bound<'_, PyBytes>)], s: &[&'a str]) -> Option<Vec<Named<'a>>> {
    l.iter()
        .zip(s)
        .map(|((_, g), n)| {
            Some(Named {
                name: latin1_bytes(n)?,
                guid: g.as_bytes(),
            })
        })
        .collect()
}

fn latin1_bytes(s: &str) -> Option<Cow<'_, [u8]>> {
    if s.is_ascii() {
        return Some(Cow::Borrowed(s.as_bytes()));
    }
    s.chars()
        .map(|c| u8::try_from(u32::from(c)).ok())
        .collect::<Option<Vec<u8>>>()
        .map(Cow::Owned)
}

/// `ShaderDb.to_bytes()` through Rust: the sections as the `ShaderDb` holds them. `None` when an item is of a kind
/// only the Python writer handles (see the module's notes).
#[pyfunction]
#[pyo3(signature = (version, flag, big_endian, params, bindings, regs, vs, ps, programs, keys, layouts, entries,
                    fragments, parts, groups))]
#[expect(
    clippy::too_many_arguments,
    reason = "the sections of a ShaderDb, as Python passes them"
)]
pub fn shaderdb_write<'py>(
    py: Python<'py>,
    version: &Bound<'py, PyAny>,
    flag: &Bound<'py, PyAny>,
    big_endian: bool,
    params: &Bound<'py, PyAny>,
    bindings: &Bound<'py, PyAny>,
    regs: &Bound<'py, PyAny>,
    vs: &Bound<'py, PyAny>,
    ps: &Bound<'py, PyAny>,
    programs: &Bound<'py, PyAny>,
    keys: &Bound<'py, PyAny>,
    layouts: &Bound<'py, PyAny>,
    entries: &Bound<'py, PyAny>,
    fragments: &Bound<'py, PyAny>,
    parts: &Bound<'py, PyAny>,
    groups: &Bound<'py, PyAny>,
) -> Option<Bound<'py, PyBytes>> {
    let version: u32 = version.extract().ok()?;
    let flag: u8 = flag.extract().ok()?;
    let lists = [params, bindings, regs, programs, keys, layouts].map(bytes_items);
    let [
        Some(params),
        Some(bindings),
        Some(regs),
        Some(programs),
        Some(keys),
        Some(layouts),
    ] = lists
    else {
        return None;
    };
    let (vs, ps) = (shader_items(vs)?, shader_items(ps)?);
    let (entries, fragments, parts) = (name_items(entries)?, name_items(fragments)?, name_items(parts)?);
    let (mut group_guids, mut group_idx) = (Vec::new(), Vec::new());
    for x in groups.try_iter().ok()? {
        let (g, idx): (Bound<'py, PyAny>, Vec<u16>) = x.ok()?.extract().ok()?;
        group_guids.push(g.cast_into::<PyBytes>().ok()?);
        group_idx.push(idx);
    }
    let strs: Vec<Vec<&str>> = [&entries, &fragments, &parts]
        .iter()
        .map(|l| l.iter().map(|(n, _)| n.to_str()).collect::<PyResult<Vec<_>>>())
        .collect::<PyResult<_>>()
        .ok()?;
    let [se, sf, sp] = [0, 1, 2].map(|i| strs.get(i).map_or_default(Vec::as_slice));
    let db = ShaderDb {
        big_endian,
        version,
        flag,
        params: slices(&params),
        bindings: slices(&bindings),
        regs: slices(&regs),
        vs: vs.iter().map(shader).collect(),
        ps: ps.iter().map(shader).collect(),
        programs: slices(&programs),
        keys: slices(&keys),
        layouts: slices(&layouts),
        entries: named(&entries, se)?,
        fragments: named(&fragments, sf)?,
        parts: named(&parts, sp)?,
        groups: slices(&group_guids).into_iter().zip(group_idx).collect(),
    };
    // written straight into the new bytes object (one copy of the records, not two)
    let size = shaderdb::written_size(&db);
    PyBytes::new_with(py, size, |buf| {
        py.detach(|| shaderdb::write_into(&db, buf));
        Ok(())
    })
    .ok()
}
