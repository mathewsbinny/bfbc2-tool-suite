//! The mesh viewer's Python API on its Rust port (round PR1b-2-rest, audit AU5 ARC-4): `meshview.Index` opens the
//! game copy's index here (`bc2_gamefs::meshindex`: its cache `meshview_index.bin` read, or the index built and
//! cached), and `load_mesh`, `mesh_surfaces`, `stack_models` and `render` are `bc2_convert::meshview`'s. The Python
//! (`bfbc2-mesh-viewer\meshview.py`) keeps the NumPy arrays and dicts its callers (Project OPFOR BFBC2's WW2 tools)
//! take, made from what these return.
//!
//! Round O3's `meshview_index(bundles)` (the tables for the Python's own pickle) went with the pickle.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use bc2_convert::meshview::{self, Image, Index, Part, render};
use bc2_gamefs::meshindex;
use pyo3::exceptions::{PyOSError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyString, PyTuple};

/// A game copy's mesh index (`meshview_open`).
#[pyclass(frozen, module = "bc2_native")]
pub struct MeshViewIndex {
    /// The bundle paths as the caller gave them (its `str` objects: each `where` value shares its bundle's).
    bundles: Vec<Py<PyString>>,
    /// The same as text, and each one's place.
    paths: Vec<String>,
    bundle_at: HashMap<String, usize>,
    /// The tables in the index's order.
    tables: meshindex::MeshIndex,
    /// The lookups.
    index: Index,
    /// Each bundle's partitions (`.dbx` entries), made when first asked.
    partitions: OnceLock<HashMap<usize, Vec<String>>>,
    built: bool,
}

/// `meshview_open(bundles, cache, say)`: the index of `bundles` (`meshview._bundles`' list) from the cache file
/// `cache` when it is this game copy's, else built (on all cores, under the cache's lock) and cached unless something
/// was left out; `say(text)` hears "indexing the game copy", what was left out and a cache not written.
///
/// # Errors
/// `OSError`: a bundle cannot be stat'ed or the lock cannot be taken.
#[pyfunction]
pub fn meshview_open(
    py: Python<'_>,
    bundles: Vec<Bound<'_, PyString>>,
    cache: &str,
    say: Bound<'_, PyAny>,
) -> PyResult<MeshViewIndex> {
    let paths: Vec<String> = bundles.iter().map(|b| b.to_string_lossy().into_owned()).collect();
    let say = say.unbind();
    let got = py
        .detach(|| {
            meshindex::load(&paths, std::path::Path::new(cache), &mut |s| {
                Python::attach(|py| {
                    let _ = say.call1(py, (s,));
                });
            })
        })
        .map_err(|e| PyOSError::new_err(e.to_string()))?;
    let index = py.detach(|| Index::from_tables(meshview::cache::tables(&got.index, &paths)));
    let mut bundle_at = HashMap::new();
    for (i, p) in paths.iter().enumerate() {
        bundle_at.entry(p.clone()).or_insert(i);
    }
    Ok(MeshViewIndex {
        bundles: bundles.into_iter().map(Bound::unbind).collect(),
        paths,
        bundle_at,
        tables: got.index,
        index,
        partitions: OnceLock::new(),
        built: got.built,
    })
}

/// A part as the Python's dict: `(name, surface, (positions float32 bytes, n), normals bytes or None, uvs bytes or
/// None, indices uint32 bytes, texture, image (w, h, rgb bytes) or None)`.
type PartTuple<'py> = Bound<'py, PyTuple>;

fn f32_bytes(v: impl Iterator<Item = f32>) -> Vec<u8> {
    v.flat_map(f32::to_le_bytes).collect()
}

fn part_tuple<'py>(py: Python<'py>, p: &Part, images: &mut Vec<(*const Image, Py<PyAny>)>) -> PyResult<PartTuple<'py>> {
    let opt_str =
        |s: Option<&str>| -> Py<PyAny> { s.map_or_else(|| py.None(), |s| PyString::new(py, s).into_any().unbind()) };
    let opt_bytes =
        |b: Option<Vec<u8>>| -> Py<PyAny> { b.map_or_else(|| py.None(), |b| PyBytes::new(py, &b).into_any().unbind()) };
    let pos = f32_bytes(p.positions.iter().flatten().copied());
    let nrm = p.normals.as_ref().map(|n| f32_bytes(n.iter().flatten().copied()));
    let uv = p.uvs.as_ref().map(|u| f32_bytes(u.iter().flatten().copied()));
    let idx: Vec<u8> = p.indices.iter().flatten().flat_map(|i| i.to_le_bytes()).collect();
    let image: Py<PyAny> = match &p.image {
        None => py.None(),
        Some(img) => {
            let key = Arc::as_ptr(img);
            if let Some((_, o)) = images.iter().find(|(k, _)| *k == key) {
                o.clone_ref(py)
            } else {
                let o = (img.w, img.h, PyBytes::new(py, &img.rgb))
                    .into_pyobject(py)?
                    .into_any()
                    .unbind();
                images.push((key, o.clone_ref(py)));
                o
            }
        }
    };
    PyTuple::new(
        py,
        [
            PyString::new(py, &p.name).into_any().unbind(),
            opt_str(p.surface.as_deref()),
            (PyBytes::new(py, &pos), p.positions.len())
                .into_pyobject(py)?
                .into_any()
                .unbind(),
            opt_bytes(nrm),
            opt_bytes(uv),
            PyBytes::new(py, &idx).into_any().unbind(),
            opt_str(p.texture.as_deref()),
            image,
        ],
    )
}

#[pymethods]
impl MeshViewIndex {
    /// The index was built now (not read from the cache).
    #[getter]
    fn built(&self) -> bool {
        self.built
    }

    /// The shader databases read in building it (0 when read from the cache).
    #[getter]
    fn databases(&self) -> usize {
        if self.built { self.tables.databases } else { 0 }
    }

    /// What was left out of the index (it was not cached).
    #[getter]
    fn skipped(&self) -> Vec<String> {
        self.tables.skipped.clone()
    }

    /// `(where, parts, entries)` as the Python's dicts, in the index's order: entry name (lower case) -> (bundle,
    /// entry name), part name -> vertex layout record (`bytes`; parts sharing a record share the object), database
    /// entry -> its texture names.
    fn tables<'py>(&self, py: Python<'py>) -> PyResult<(Bound<'py, PyDict>, Bound<'py, PyDict>, Bound<'py, PyDict>)> {
        let where_ = PyDict::new(py);
        for (key, bi, name) in &self.tables.where_ {
            let b = self
                .bundles
                .get(*bi)
                .ok_or_else(|| PyValueError::new_err("an index row names no bundle"))?;
            where_.set_item(
                key,
                PyTuple::new(py, [b.bind(py).clone().into_any(), PyString::new(py, name).into_any()])?,
            )?;
        }
        let empty = PyBytes::new(py, b"");
        let layouts: Vec<Bound<'py, PyBytes>> = self
            .tables
            .layouts
            .iter()
            .map(|l| {
                if l.is_empty() {
                    empty.clone()
                } else {
                    PyBytes::new(py, l)
                }
            })
            .collect();
        let parts = PyDict::new(py);
        for (key, id) in &self.tables.parts {
            let l = layouts
                .get(*id)
                .ok_or_else(|| PyValueError::new_err("a part names no layout"))?;
            parts.set_item(key, l)?;
        }
        let entries = PyDict::new(py);
        for (key, tex) in &self.tables.entries {
            entries.set_item(key, PyList::new(py, tex)?)?;
        }
        Ok((where_, parts, entries))
    }

    /// `where.get(name.lower())`: (bundle, entry name as the bundle holds it), or None.
    fn locate<'py>(&self, py: Python<'py>, name: &str) -> Option<(Bound<'py, PyString>, String)> {
        let (b, n) = self.index.locate(name)?;
        let bi = *self.bundle_at.get(b)?;
        Some((self.bundles.get(bi)?.bind(py).clone(), n.to_owned()))
    }

    /// The `.dbx` entry names the index finds in `bundle` (its partitions, read in one pass by the Python's cache).
    fn partitions_of(&self, bundle: &str) -> Vec<String> {
        let by = self.partitions.get_or_init(|| {
            let mut by: HashMap<usize, Vec<String>> = HashMap::new();
            for (_, bi, n) in &self.tables.where_ {
                if n.to_lowercase().ends_with(".dbx") {
                    by.entry(*bi).or_default().push(n.clone());
                }
            }
            by
        });
        self.bundle_at
            .get(bundle)
            .and_then(|bi| by.get(bi))
            .cloned()
            .unwrap_or_default()
    }

    /// `load_mesh(name, lod)`: `(parts, notes)` - each part a tuple (see the Python), the notes the parts skipped.
    ///
    /// # Errors
    /// `ValueError`: no stock bundle has the level's data, or it does not parse (the Python's `ViewError`).
    fn load_mesh<'py>(&self, py: Python<'py>, name: &str, lod: usize) -> PyResult<(Vec<PartTuple<'py>>, Vec<String>)> {
        let m = py
            .detach(|| meshview::load_mesh(&self.index, name, lod))
            .map_err(PyValueError::new_err)?;
        let mut images = Vec::new();
        let parts = m
            .parts
            .iter()
            .map(|p| part_tuple(py, p, &mut images))
            .collect::<PyResult<Vec<_>>>()?;
        Ok((parts, m.notes))
    }

    /// `mesh_surfaces(name)`: (surface name lower case, shader asset path), in the dict's order.
    fn mesh_surfaces(&self, py: Python<'_>, name: &str) -> Vec<(String, String)> {
        py.detach(|| meshview::mesh_surfaces(&self.index, name))
    }

    /// The bundles given, as text.
    #[getter]
    fn bundle_paths(&self) -> Vec<String> {
        self.paths.clone()
    }
}

/// Little-endian float32s.
fn floats(b: &[u8]) -> Vec<f32> {
    b.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect()
}

fn rows<T: Copy, const K: usize>(v: &[T]) -> Vec<[T; K]> {
    v.as_chunks::<K>().0.to_vec()
}

/// `stack_models(models)`'s offsets: each model's float32 positions (x, y, z rows) -> its offset.
///
/// # Errors
/// `ValueError`: a model without positions (NumPy's on an empty concatenation).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn meshview_stack(models: Vec<Vec<u8>>) -> PyResult<Vec<[f32; 3]>> {
    let pts: Vec<Vec<[f32; 3]>> = models.iter().map(|m| rows::<f32, 3>(&floats(m))).collect();
    if pts.iter().any(Vec::is_empty) {
        return Err(PyValueError::new_err("need at least one array to concatenate"));
    }
    let refs: Vec<&[[f32; 3]]> = pts.iter().map(Vec::as_slice).collect();
    Ok(render::stack_points(&refs))
}

/// One part for [`meshview_render`]: (positions float32 bytes, has normals, uvs float32 bytes or None, indices uint32
/// bytes, image (w, h, rgb bytes) or None).
type RenderPart = (Vec<u8>, bool, Option<Vec<u8>>, Vec<u8>, Option<(usize, usize, Vec<u8>)>);

/// `render(laid, size, view)`: the picture `(w, h, rgb bytes)` of the laid-out models, each `(offset, parts)`.
///
/// # Errors
/// `ValueError`: an unknown view, or an image whose bytes are not w x h x 3.
#[pyfunction]
pub fn meshview_render<'py>(
    py: Python<'py>,
    laid: Vec<([f32; 3], Vec<RenderPart>)>,
    size: usize,
    view: &str,
) -> PyResult<(usize, usize, Bound<'py, PyBytes>)> {
    let view = render::View::parse(view).ok_or_else(|| PyValueError::new_err(format!("{view:?}: no such view")))?;
    let mut models: Vec<(Vec<Part>, [f32; 3])> = Vec::with_capacity(laid.len());
    for (off, parts) in laid {
        let mut ps = Vec::with_capacity(parts.len());
        for (pos, normals, uvs, idx, image) in parts {
            let image = match image {
                Some((w, h, rgb)) => {
                    if Some(rgb.len()) != w.checked_mul(h).and_then(|n| n.checked_mul(3)) {
                        return Err(PyValueError::new_err("an image's bytes are not its width x height x 3"));
                    }
                    Some(Arc::new(Image { w, h, rgb }))
                }
                None => None,
            };
            let idx: Vec<u32> = idx.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect();
            ps.push(Part {
                name: String::new(),
                surface: None,
                shader: None,
                positions: rows::<f32, 3>(&floats(&pos)),
                normals: normals.then(Vec::new),
                uvs: uvs.map(|u| rows::<f32, 2>(&floats(&u))),
                indices: rows::<u32, 3>(&idx),
                texture: None,
                image,
            });
        }
        models.push((ps, off));
    }
    let img = py.detach(|| {
        let laid: Vec<render::Laid<'_>> = models.iter().map(|(ps, off)| (ps.iter().collect(), *off)).collect();
        render::render(&laid, size, view)
    });
    Ok((img.w, img.h, PyBytes::new(py, &img.rgb)))
}
