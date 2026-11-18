//! Python bindings for the suite's Rust core: the module `bc2_native`, built by `rust/build_native.py` into
//! `core/` (README.md, "The Rust core").
//!
//! The bindings only convert: every format lives in `bc2-formats`. `core/bfbc2core/dbx.py` calls this module when
//! it is importable and falls back to its own Python reader otherwise (`BFBC2_PURE_PYTHON=1` forces the
//! fallback). Texture conversion and NavKit's grid walks have no Python twin since 2026-11-17: they need the module,
//! and `bfbc2core.native` uses it only when its `__source_hash__` matches the checkout's sources. The work runs with
//! the interpreter released, the batch functions on all cores.
//!
//! Errors are `ValueError`s with `bc2-formats`' message, as the Python reader raises them (and, for bundles,
//! `KeyError` for an entry that is not there and `OSError` for a file that cannot be read). The binding is tested
//! from Python (`core/tests/test_dbx_native.py`, `core/tests/test_fbrb_native.py`), where an interpreter exists.
//!
//! `core/bfbc2core/fbrb.py` calls the `fbrb_*` functions the same way: the reading, decompression and the batch
//! function's bundles run with the interpreter released. `fbrb_read` / `fbrb_read_many` go through the bundle
//! cache (round E3e, `fbrb_cache.rs`: `fbrb_cache_config`, `fbrb_cache_configure`, `fbrb_cache_stats`,
//! `fbrb_cache_clear`, and `fbrb_read_uncached`, the read without it).
//!
//! APT movies (`apt.rs`, round E3n): `apt_constants`, `apt_decode` / `apt_decode_many`, for
//! `bfbc2-console-readers/apt/apt.py`.
//!
//! DbxEdit's commands (`dbxedit.rs`): `dbxedit_run` / `dbxedit_run_many`, for `core/bfbc2core/dbxedit.py`; FbrbTool's
//! (`fbrbtool.rs`): `fbrbtool_run` / `fbrbtool_run_many`, for `core/bfbc2core/fbrbtool.py`.
//!
//! The writers (`writers.rs`): `dbx_write` (`dbx.write` of a Python tree), `fbrb_pack` / `fbrb_pack_many`
//! (FbrbTool's `pack`, `fbrb.pack`), `fbrb_unpack` (FbrbTool's `unpack`, `fbrb.unpack`) and `zlib_deflate`.
//!
//! Granny resources (`granny.rs`): `granny_header`, `granny_index`, `granny_root` / `granny_roots_many` and
//! `granny_to_pc` / `granny_to_pc_many`, for `bfbc2-anim-tools/granny.py`.
//!
//! The converter's (`convert.rs`): `texture_convert` / `texture_convert_many`, and the EA-XAS audio codec
//! (`xas_encode`, `xas_encode_many`, `xas_decode_frames`) that `bfbc2-console-convert/audio.py` needs;
//! `mesh_vertices`, `havok_convert_packfile` / `havok_verify_packfile` / `HavokClasses` (rounds E3g, E3o) for
//! `mesh.py` and `havok.py`.

mod apt;
mod convert;
mod culltree;
mod dbxedit;
mod fbrb_cache;
mod fbrbtool;
mod geometry;
mod granny;
mod kymesh;
mod navbuild;
mod navgraph;
mod navpath;
mod navtrace;
mod scene;
mod shaders;
mod writers;
mod xenos;

use bc2_formats::dbx::{self, DbxError, DbxTree};
use bc2_formats::fbrb::{self, FbrbError};
use pyo3::exceptions::{PyKeyError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyBytesMethods, PyList, PyString, PyTuple};

/// `bc2-formats`' error as Python's: a `ValueError` with the same message.
fn value_error(e: &DbxError) -> PyErr {
    PyValueError::new_err(e.to_string())
}

/// The bytes of each item of `items` (a sequence of `bytes`).
fn byte_slices<'a>(items: &'a [Bound<'_, PyBytes>]) -> Vec<&'a [u8]> {
    items.iter().map(PyBytesMethods::as_bytes).collect()
}

/// Builds the Python tree of a parsed file through the caller's `Element` and `DbxFile` classes, so the result is
/// exactly what `dbx.parse` returns (dataclass instances, mutable, deep-copyable). It reads the compact
/// [`DbxTree`] (no owned Rust tree in between) and walks it in file order without recursion: each container's
/// children list is made with it and filled as the walk passes its subtree.
struct Builder<'py> {
    py: Python<'py>,
    element: Bound<'py, PyAny>,
    strings: Vec<Bound<'py, PyString>>,
    empty_str: Bound<'py, PyString>,
    empty_bytes: Bound<'py, PyBytes>,
}

impl<'py> Builder<'py> {
    fn new(py: Python<'py>, element: &Bound<'py, PyAny>, tree: &DbxTree<'_>) -> Self {
        Self {
            py,
            element: element.clone(),
            strings: tree.strings().iter().map(|s| PyString::new(py, s)).collect(),
            empty_str: PyString::new(py, ""),
            empty_bytes: PyBytes::new(py, b""),
        }
    }

    fn s(&self, id: dbx::StrId) -> Bound<'py, PyString> {
        self.strings.get(id as usize).unwrap_or(&self.empty_str).clone()
    }

    fn file(&self, file_cls: &Bound<'py, PyAny>, tree: &DbxTree<'_>) -> PyResult<Bound<'py, PyAny>> {
        let strings = PyList::new(self.py, &self.strings)?;
        let roots = PyList::empty(self.py);
        // the levels being walked, innermost last: their remaining elements and the list they go to
        let mut stack = vec![(tree.roots(), roots.clone())];
        while let Some((level, list)) = stack.last_mut() {
            let Some(n) = level.next() else {
                stack.pop();
                continue;
            };
            let attributes = PyList::empty(self.py);
            for (k, v) in n.attribute_ids() {
                attributes.append(PyTuple::new(self.py, [self.s(k), self.s(v)])?)?;
            }
            let none = || self.py.None().into_bound(self.py);
            let children = n.is_container().then(|| PyList::empty(self.py));
            let (text, raw, kids) = match (&children, n.text_id()) {
                (Some(c), _) => (self.empty_str.clone(), self.empty_bytes.clone(), c.clone().into_any()),
                (None, Some(t)) => (self.s(t), self.empty_bytes.clone(), none()),
                (None, None) => (self.empty_str.clone(), PyBytes::new(self.py, n.raw()), none()),
            };
            list.append(
                self.element
                    .call1((self.s(n.tag_id()), n.kind(), attributes, text, raw, kids))?,
            )?;
            if let Some(c) = children {
                stack.push((n.children(), c));
            }
        }
        file_cls.call1((strings, roots))
    }
}

/// `dbx.parse(data)` through Rust: `element_cls(tag, kind, attributes, text, raw, children)` per element and
/// `file_cls(strings, roots)` for the file.
#[pyfunction]
fn dbx_parse<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyBytes>,
    element_cls: &Bound<'py, PyAny>,
    file_cls: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let bytes = data.as_bytes();
    let tree = py.detach(|| dbx::parse_tree(bytes)).map_err(|e| value_error(&e))?;
    Builder::new(py, element_cls, &tree).file(file_cls, &tree)
}

/// `dbx.to_xml(data)` through Rust.
#[pyfunction]
fn dbx_to_xml(py: Python<'_>, data: &Bound<'_, PyBytes>) -> PyResult<String> {
    let bytes = data.as_bytes();
    py.detach(|| dbx::to_xml(bytes)).map_err(|e| value_error(&e))
}

/// `dbx_parse` of many files: parsed on all cores, then built as Python trees in order. Each item is a file or the
/// `ValueError` it raised (returned, not raised).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
fn dbx_parse_many<'py>(
    py: Python<'py>,
    items: Vec<Bound<'py, PyBytes>>,
    element_cls: &Bound<'py, PyAny>,
    file_cls: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyList>> {
    let slices = byte_slices(&items);
    let parsed = py.detach(|| dbx::parse_tree_all(&slices));
    let out = PyList::empty(py);
    for r in parsed {
        match r {
            Ok(t) => out.append(Builder::new(py, element_cls, &t).file(file_cls, &t)?)?,
            Err(e) => out.append(value_error(&e).into_value(py))?,
        }
    }
    Ok(out)
}

/// `dbx_to_xml` of many files on all cores. Each item is the XML or the `ValueError` it raised (returned).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
fn dbx_to_xml_many<'py>(py: Python<'py>, items: Vec<Bound<'py, PyBytes>>) -> PyResult<Bound<'py, PyList>> {
    let slices = byte_slices(&items);
    let xml = py.detach(|| dbx::to_xml_all(&slices));
    let out = PyList::empty(py);
    for r in xml {
        match r {
            Ok(s) => out.append(s)?,
            Err(e) => out.append(value_error(&e).into_value(py))?,
        }
    }
    Ok(out)
}

/// Each file's instance types (`[i.attr('type') for i in dbx.parse(d).instances]`, `None` for an instance without
/// one) with, for each, whether its UTF-8 bytes occur in the file - parsed on all cores, no Python tree built
/// (effectport's emitter index, round E3g). Each item is `(types, present)` or the `ValueError` (returned; a file
/// with no partition is one too: the Python tree has no `roots[0]`).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
fn dbx_instance_types_many<'py>(py: Python<'py>, items: Vec<Bound<'py, PyBytes>>) -> PyResult<Bound<'py, PyList>> {
    use rayon::prelude::*;
    type Types = (Vec<Option<String>>, Vec<bool>);
    let slices = byte_slices(&items);
    let found: Vec<Result<Types, String>> = py.detach(|| {
        slices
            .par_iter()
            .map(|d| {
                if d.is_empty() || !dbx::is_binary(d) {
                    return Err("not a binary .dbx file".to_owned()); // dbx.parse's _check
                }
                let tree = dbx::parse_tree(d).map_err(|e| e.to_string())?;
                if tree.partition().is_none() {
                    return Err("no partition".to_owned());
                }
                let types: Vec<Option<String>> = tree.instances().map(|n| n.attr("type").map(str::to_owned)).collect();
                let present = types
                    .iter()
                    .map(|t| {
                        t.as_ref()
                            .is_some_and(|t| t.is_empty() || d.windows(t.len()).any(|w| w == t.as_bytes()))
                    })
                    .collect();
                Ok((types, present))
            })
            .collect()
    });
    let out = PyList::empty(py);
    for r in found {
        match r {
            Ok(t) => out.append(t)?,
            Err(e) => out.append(PyValueError::new_err(e).into_value(py))?,
        }
    }
    Ok(out)
}

/// `bc2-formats`' bundle error as `fbrb.py` raises it: `KeyError` for a missing entry (named as `keys` gives it),
/// `OSError` for a file that cannot be read, else `ValueError`, each naming the bundle as the Python reader does.
pub(crate) fn fbrb_error(e: FbrbError, path: &str, keys: &[String]) -> PyErr {
    match e {
        FbrbError::Io(io) => io.into(),
        FbrbError::NotBundle => PyValueError::new_err(format!("{path} is not an FbRB bundle")),
        FbrbError::MissingEntry { index } => {
            let name = keys.get(index).map_or("?", String::as_str);
            PyKeyError::new_err(format!("{name} is not an entry of {path}"))
        }
        FbrbError::PayloadEnds => {
            PyValueError::new_err(format!("{path}: the payload ends before entry data (damaged bundle)"))
        }
        other => PyValueError::new_err(format!("{path}: {other}")),
    }
}

/// `fbrb._header(path)` through Rust: `([entry_cls(name, type, flags, offset, size, size2), ...], zipped, payload
/// start)`. Only the header is read.
#[pyfunction]
fn fbrb_header<'py>(
    py: Python<'py>,
    path: &str,
    entry_cls: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyList>, bool, u64)> {
    let h = py
        .detach(|| fbrb::entries_path(path))
        .map_err(|e| fbrb_error(e, path, &[]))?;
    let out = PyList::empty(py);
    for e in &h.entries {
        out.append(entry_cls.call1((&e.name, &e.type_name, e.flags, e.offset, e.size, e.size2))?)?;
    }
    Ok((out, h.zipped, h.payload_start))
}

pub(crate) fn bytes_list<'py>(py: Python<'py>, data: &[Vec<u8>]) -> PyResult<Bound<'py, PyList>> {
    PyList::new(py, data.iter().map(|d| PyBytes::new(py, d)))
}

/// `fbrb.read(path, keys)` through Rust: the data of each entry, in the order of `keys` (`lowered`: each key's
/// `str.lower()`, which entry names are matched against).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
fn fbrb_read<'py>(
    py: Python<'py>,
    path: &str,
    keys: Vec<String>,
    lowered: Vec<String>,
) -> PyResult<Bound<'py, PyList>> {
    let data = py
        .detach(|| fbrb::read_path(path, &lowered))
        .map_err(|e| fbrb_error(e, path, &keys))?;
    bytes_list(py, &data)
}

/// `fbrb_read` of many `(path, keys, lowered)` items, one bundle per core. Each result is the list of the entries'
/// data or the exception it raised (returned, not raised).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
fn fbrb_read_many(py: Python<'_>, items: Vec<(String, Vec<String>, Vec<String>)>) -> PyResult<Bound<'_, PyList>> {
    let jobs: Vec<(&str, Vec<&str>)> = items
        .iter()
        .map(|(p, _, low)| (p.as_str(), low.iter().map(String::as_str).collect()))
        .collect();
    let results = py.detach(|| fbrb::read_many(&jobs));
    let out = PyList::empty(py);
    for (r, (path, keys, _)) in results.into_iter().zip(&items) {
        match r {
            Ok(data) => out.append(bytes_list(py, &data)?)?,
            Err(e) => out.append(fbrb_error(e, path, keys).into_value(py))?,
        }
    }
    Ok(out)
}

/// The number of worker threads the batch functions use (`RAYON_NUM_THREADS`, else every core).
#[pyfunction]
fn threads() -> usize {
    bc2_formats::threads()
}

/// The suite's Rust core (`rust/`): binary DBX parsing, XML and writing, used by `bfbc2core.dbx` when built; bundle
/// reading and packing, used by `bfbc2core.fbrb`; terrain, water and collision, used by `bfbc2core.terrain`, `water`
/// and `collision`.
#[pymodule]
mod bc2_native {
    #[pymodule_export]
    use super::{
        dbx_instance_types_many, dbx_parse, dbx_parse_many, dbx_to_xml, dbx_to_xml_many, fbrb_header, fbrb_read,
        fbrb_read_many, threads,
    };

    #[pymodule_export]
    use super::geometry::{
        collision_index, havok_index, havok_triangles, havok_triangles_files, terrain_heights, terrain_tail,
        water_mesh, water_shaders, water_visual,
    };

    #[pymodule_export]
    use super::scene::scene_load;

    #[pymodule_export]
    use super::dbxedit::{dbxedit_run, dbxedit_run_many};
    #[pymodule_export]
    use super::fbrb_cache::{
        fbrb_cache_clear, fbrb_cache_config, fbrb_cache_configure, fbrb_cache_stats, fbrb_read_uncached,
    };
    #[pymodule_export]
    use super::fbrbtool::{fbrbtool_gunzip, fbrbtool_run, fbrbtool_run_many};

    #[pymodule_export]
    use super::writers::{dbx_write, fbrb_pack, fbrb_pack_many, fbrb_unpack, zlib_deflate};

    #[pymodule_export]
    use super::shaders::{shaderdb_parse, shaderdb_write};

    #[pymodule_export]
    use super::culltree::{culltree_contains, culltree_dbx_files, culltree_elements};

    #[pymodule_export]
    use super::convert::{
        HavokClasses, havok_convert_packfile, havok_verify_packfile, mesh_vertices, texture_convert,
        texture_convert_many, xas_decode_frames, xas_encode, xas_encode_many,
    };

    #[pymodule_export]
    use super::apt::{AptLines, apt_constants, apt_decode, apt_decode_many};

    #[pymodule_export]
    use super::granny::{granny_header, granny_index, granny_root, granny_roots_many, granny_to_pc, granny_to_pc_many};

    #[pymodule_export]
    use super::navbuild::{
        NavGrid, NavLevel, navkit_centre_cells, navkit_float_ops, navkit_matched_flags, navkit_nearest_cells,
    };

    #[pymodule_export]
    use super::navgraph::{NavAreas, navkit_area_cells, navkit_round3};

    #[pymodule_export]
    use super::xenos::{
        XenosProgram, xenos_float_repr, xenos_microcode, xenos_parse, xenos_parse_many, xenos_translate,
        xenos_translate_many,
    };

    #[pymodule_export]
    use super::navpath::PathGraph;

    #[pymodule_export]
    use super::kymesh::{kymesh_parse, navkit_mesh_cells, navkit_navdata_areas, navkit_union_roots};

    #[pymodule_export]
    use super::navtrace::{navkit_gabriel, navkit_navsample_parse, navkit_roof_drop};

    /// The crate version, to tell builds apart.
    #[pymodule_export]
    const VERSION: &str = env!("CARGO_PKG_VERSION");

    use pyo3::prelude::*;

    /// `__version__` (the crates' version) and `__source_hash__`: the fingerprint of the Rust sources the module was
    /// built from (`bfbc2core.native.fingerprint`, passed by `rust/build_native.py` as `BC2_SOURCE_HASH`; empty for
    /// a build without it), which `bfbc2core.native` compares with the checkout's before using the module.
    #[pymodule_init]
    fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add("__version__", env!("CARGO_PKG_VERSION"))?;
        m.add("__source_hash__", option_env!("BC2_SOURCE_HASH").unwrap_or(""))
    }
}
